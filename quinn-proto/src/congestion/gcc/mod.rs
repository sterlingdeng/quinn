use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use super::{Controller, ControllerFactory, BASE_DATAGRAM_SIZE};

mod acknowledgement;
use acknowledgement::Acknowledgement;
use delay_controller::DelayController;
use loss_controller::{LossController, LossControllerConfig};
use overuse_detector::NetworkUsage;
use rate_calculator::{RateCalculator, RateCalculatorConfig};

use rate_controller::State;
use tracing::{trace, trace_span, warn};

mod adaptive_threshold;

mod arrival_filter;

mod delay_controller;

mod overuse_detector;

mod rate_controller;

mod rate_calculator;

mod loss_controller;

mod prefilter;

pub(crate) type Bitrate = u32;

fn human_kbits<T: Into<f64>>(bits: T) -> String {
    format!("{:.2}kb", (bits.into() / 1_000.))
}

pub(crate) const DEFAULT_INITIAL_BITRATE: Bitrate = 500_000;
pub(crate) const DEFAULT_MIN_BITRATE: Bitrate = 5_000;
pub(crate) const DEFAULT_MAX_BITRATE: Bitrate = 1_000_000_000; // 1 Gbps

const INITIAL_WINDOW: u64 = 10 * BASE_DATAGRAM_SIZE;

#[derive(Debug)]
enum ControllerType {
    Delay,
    Loss,
}

/// Gcc implements the Google Congestion Control algorithm.
#[derive(Clone)]
pub struct Gcc {
    rate_calculator: RateCalculator,
    loss_controller: LossController,
    pub(crate) delay_controller: DelayController,

    last_pn: u64,
    epoch: Instant,

    mtu: u64,
    window: u64,
    last_update: Instant,

    target_bitrate: Bitrate,
    delay_bitrate: Bitrate,
    loss_bitrate: Bitrate,
    report_stats: bool,
}

impl Gcc {
    fn new(mtu: u16, now: Instant, report_stats: bool) -> Self {
        Self {
            mtu: mtu as u64,
            last_pn: 0,
            rate_calculator: RateCalculator::new(RateCalculatorConfig::default()),
            loss_controller: LossController::new(now, LossControllerConfig::default()),
            delay_controller: DelayController::new(),
            epoch: now,
            target_bitrate: DEFAULT_INITIAL_BITRATE,
            delay_bitrate: DEFAULT_INITIAL_BITRATE,
            loss_bitrate: DEFAULT_INITIAL_BITRATE,
            last_update: now,
            window: INITIAL_WINDOW,
            report_stats,
        }
    }

    fn set_bitrate(&mut self, bitrate: Bitrate, controller_type: ControllerType) {
        match controller_type {
            ControllerType::Delay => {
                self.delay_bitrate = bitrate.clamp(DEFAULT_MIN_BITRATE, DEFAULT_MAX_BITRATE);
            }
            ControllerType::Loss => {
                self.loss_bitrate = bitrate.clamp(DEFAULT_MIN_BITRATE, DEFAULT_MAX_BITRATE);
            }
        }

        // If delay is never updated, then loss is clamped by delay
        let target_bitrate = Bitrate::min(self.delay_bitrate, self.loss_bitrate)
            .clamp(DEFAULT_MIN_BITRATE, DEFAULT_MAX_BITRATE);

        self.target_bitrate = target_bitrate;
    }

    fn calculate_window(target_bitrate: Bitrate, rtt: Duration) -> u64 {
        // The window is the number of packets allowed in flight.
        ((target_bitrate as f32 * rtt.as_secs_f32()) as f32) as u64
    }

    fn stats(&self) -> Stats {
        Stats {
            delay_ctrl_network_usage: self.delay_controller.get_usage(),
            delay_ctrl_state: self.delay_controller.get_state(),
            delay_ctrl_bitrate: self.delay_controller.get_target_bitrate(),
            loss_ctrl_bitrate: self.loss_controller.get_bitrate(),
            loss_ctrl_packet_loss: self.loss_controller.get_loss_ratio(),
            loss_ctrl_avg_loss: self.loss_controller.get_average_loss(),
            window: self.window,
            effective_bitrate: self.rate_calculator.effective_bitrate(),
            gcc_estimated_bitrate: self.target_bitrate,
            overuse_detector_threshold: self
                .delay_controller
                .overuse_detector
                .adaptive_threshold
                .threshold
                .whole_milliseconds(),
            overuse_detector_estimate: self
                .delay_controller
                .overuse_detector
                .last_estimate
                .whole_milliseconds(),
        }
    }

    /// Returns the target bitrate of GCC.
    pub fn target_bitrate(&self) -> Bitrate {
        self.target_bitrate
    }

    fn minimum_window(&self) -> u64 {
        2 * self.mtu
    }
}

impl Controller for Gcc {
    fn on_mtu_update(&mut self, new_mtu: u16) {
        self.mtu = new_mtu as u64;
        self.window = self.window.max(self.minimum_window());
    }

    fn window(&self) -> u64 {
        self.window
    }

    fn clone_box(&self) -> Box<dyn Controller> {
        Box::new(self.clone())
    }

    fn initial_window(&self) -> u64 {
        INITIAL_WINDOW
    }

    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self
    }

    fn on_congestion_event(
        &mut self,
        _now: Instant,
        _sent: Instant,
        _is_persistent_congestion: bool,
        lost_bytes: u64,
    ) {
        self.loss_controller.add_bytes_lost(lost_bytes);
    }

    fn on_ack_timestamped(
        &mut self,
        pn: u64,
        now: std::time::Instant,
        sent: std::time::Instant,
        received: Option<Duration>,
        bytes: u64,
        _app_limited: bool,
        rtt: &crate::RttEstimator,
    ) {
        // pn can be repeated or out of order
        if pn <= self.last_pn {
            return;
        }

        let departure = match sent.duration_since(self.epoch).try_into() {
            Err(e) => {
                warn!(
                    "failed to convert departure from std::time::Duration to time::Duration: {}",
                    e
                );
                return;
            }
            Ok(v) => v,
        };

        let arrival = match received {
            Some(dur) => {
                match dur.try_into() {
                    Ok(arrival) => Some(arrival),
                    Err(e) => {
                        warn!("failed to convert arrival from std::time::Duration to time::Duration: {}", e);
                        None
                    }
                }
            }
            None => None,
        };

        let ack = Acknowledgement::new(pn, bytes, departure, arrival);
        self.rate_calculator.add_ack(ack);

        if let Some(delay_estimate) = self.delay_controller.process_packet(
            ack,
            self.rate_calculator.effective_bitrate(),
            rtt.get(),
            now,
        ) {
            trace!(delay_estimate, "delay estimate: bitrate");
            self.set_bitrate(delay_estimate, ControllerType::Delay);
        }

        if let Some(loss_estimate) = self.loss_controller.calculate_loss_estimate(now) {
            //trace!(loss_estimate, "loss estimate: bitrate");
            self.set_bitrate(loss_estimate, ControllerType::Loss)
        }

        self.loss_controller.add_bytes_acked(bytes);

        self.window = Self::calculate_window(self.target_bitrate, rtt.get());

        if false {
            trace!(
                target_bitrate = self.target_bitrate,
                effective_bitrate = self.rate_calculator.effective_bitrate(),
                window = self.window,
                rtt = rtt.get().as_secs_f32(),
                "gcc output calculation",
            );
        }

        self.last_pn = pn;
        self.last_update = now;
        if true && self.report_stats {
            let stats = self.stats();
            let _span = trace_span!("stats").entered();
            trace!(
                usage = stats.delay_ctrl_network_usage.string(),
                state = stats.delay_ctrl_state.string(),
                delay_ctrl_bitrate = stats.delay_ctrl_bitrate,
                loss_ctrl_bitrate = stats.loss_ctrl_bitrate,
                //loss_ctrl_packet_loss = packet_loss,
                average_loss = stats.loss_ctrl_avg_loss,
                window = stats.window,
                rtt = rtt.get().as_millis(),
                //last_pn = stats.last_pn,
                measurement = stats.effective_bitrate.map(|v| human_kbits(v)),
                estimate = human_kbits(stats.gcc_estimated_bitrate),
                overuse_detector_estimate = stats.overuse_detector_estimate,
                overuse_detector_threshold = stats.overuse_detector_threshold,
                idv = self
                    .delay_controller
                    .last_inter_delay_variation
                    .whole_milliseconds(),
            );
        }
    }
}

/// GccConfig is the factory to produce GCC instances.
pub struct GccConfig {
    report_stats: bool,
}

impl GccConfig {
    pub fn new(report_stats: bool) -> Self {
        Self { report_stats }
    }
}

impl ControllerFactory for GccConfig {
    fn build(self: Arc<Self>, now: Instant, current_mtu: u16) -> Box<dyn Controller> {
        Box::new(Gcc::new(current_mtu, now, self.report_stats))
    }
}

#[derive(Debug, Clone)]
struct Stats {
    delay_ctrl_network_usage: NetworkUsage,
    delay_ctrl_state: State,
    delay_ctrl_bitrate: Bitrate,

    loss_ctrl_bitrate: Bitrate,
    loss_ctrl_packet_loss: Option<f64>,
    loss_ctrl_avg_loss: f64,

    window: u64,
    effective_bitrate: Option<Bitrate>,
    gcc_estimated_bitrate: Bitrate,
    overuse_detector_threshold: i128,
    overuse_detector_estimate: i128,
}

#[cfg(test)]
mod test {
    use crate::congestion::BASE_DATAGRAM_SIZE;

    #[test]
    fn blah() {
        assert_eq!(
            10 * BASE_DATAGRAM_SIZE,
            14720.clamp(2 * BASE_DATAGRAM_SIZE, 10 * BASE_DATAGRAM_SIZE)
        );
    }
}
