use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use super::{Controller, ControllerFactory, BASE_DATAGRAM_SIZE};

mod acknowledgement;
use acknowledgement::Acknowledgement;
use delay_controller::DelayController;
use loss_controller::{LossController, LossControllerConfig};
use rate_calculator::{RateCalculator, RateCalculatorConfig};

use tracing::warn;

mod adaptive_threshold;

mod arrival_filter;

mod delay_controller;

mod overuse_detector;

mod rate_controller;

mod rate_calculator;

mod loss_controller;

mod prefilter;

pub(crate) type Bitrate = u32;
pub(crate) const DEFAULT_INITIAL_BITRATE: Bitrate = 2_048_000; // 10 kbps
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
    delay_controller: DelayController,

    next_pn: u64,
    epoch: Instant,

    mtu: u16,
    window: u64,
    last_update: Instant,

    estimated_bitrate: Bitrate,
    delay_bitrate: Bitrate,
    loss_bitrate: Bitrate,
}

impl Gcc {
    fn new(mtu: u16, now: Instant) -> Self {
        Self {
            mtu,
            next_pn: 0,
            rate_calculator: RateCalculator::new(RateCalculatorConfig::default()),
            loss_controller: LossController::new(now, LossControllerConfig::default()),
            delay_controller: DelayController::new(),
            epoch: now,
            estimated_bitrate: DEFAULT_INITIAL_BITRATE,
            delay_bitrate: DEFAULT_INITIAL_BITRATE,
            loss_bitrate: DEFAULT_INITIAL_BITRATE,
            last_update: now,
            window: INITIAL_WINDOW,
        }
    }

    fn set_bitrate(&mut self, bitrate: Bitrate, controller_type: ControllerType) {
        // let prev = Bitrate::min(self.delay_bitrate, self.loss_bitrate);

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

        self.estimated_bitrate = target_bitrate;
    }

    fn calculate_window(target_bitrate: Bitrate, rtt: Duration) -> u64 {
        // The window is the number of packets allowed in flight.
        ((target_bitrate as f32 * rtt.as_secs_f32()) as f32) as u64
    }
}

impl Controller for Gcc {
    fn on_mtu_update(&mut self, new_mtu: u16) {
        self.mtu = new_mtu;
    }

    fn window(&self) -> u64 {
        self.window.max(3 * self.mtu as u64)
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
        now: std::time::Instant,
        sent: std::time::Instant,
        is_persistent_congestion: bool,
        lost_bytes: u64,
    ) {
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
        // pn can be repeated
        // pn can be out of order
        if pn < self.next_pn {
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
            println!("delay estimate: bitrate: {}", delay_estimate);
            self.set_bitrate(delay_estimate, ControllerType::Delay);
        }

        if let Some(loss_estimate) = self.loss_controller.update_loss_estimate(ack, now) {
            println!("loss estimate: bitrate: {}", loss_estimate);
            self.set_bitrate(loss_estimate, ControllerType::Loss)
        }

        self.window = Self::calculate_window(self.estimated_bitrate, rtt.get());
        println!(
            "calculating --> mtu: {}, target_bitrate: {}, rtt: {:?}s, window: {}",
            self.mtu,
            self.estimated_bitrate,
            rtt.get().as_secs_f32(),
            self.window
        );

        self.next_pn = pn + 1;
        self.last_update = now;
    }
}

/// GccConfig is the factory to produce GCC instances.
pub struct GccConfig {}

impl GccConfig {
    pub fn new() -> Self {
        Self {}
    }
}

impl ControllerFactory for GccConfig {
    fn build(self: Arc<Self>, now: Instant, current_mtu: u16) -> Box<dyn Controller> {
        Box::new(Gcc::new(current_mtu, now))
    }
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
