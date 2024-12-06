use std::time::Instant;
use time::Duration;

use crate::congestion::gcc::{
    Bitrate, DEFAULT_INITIAL_BITRATE, DEFAULT_MAX_BITRATE, DEFAULT_MIN_BITRATE,
};

const WINDOW_INTERVAL: Duration = Duration::milliseconds(100);
const INCREASE_LOSS_THRESHOLD: f64 = 0.02;
const DECREASE_LOSS_THRESHOLD: f64 = 0.1;
const INCREASE_FACTOR: f64 = 1.05;

const TIME_THRESHOLD: Duration = Duration::milliseconds(200);

pub(crate) struct LossControllerConfig {
    window: Duration,
    time_threshold: Duration,
}

impl Default for LossControllerConfig {
    fn default() -> Self {
        Self {
            window: WINDOW_INTERVAL,
            time_threshold: TIME_THRESHOLD,
        }
    }
}

// since ack timestamps are not like TWCC in which it's grouped together nicely, we need to find a
// way to group the timestamps together
#[derive(Clone)]
pub(crate) struct LossController {
    last_update: Instant,
    last_increase: Instant,
    last_decrease: Instant,

    // The number of packets lost in the group.
    bytes_lost: u64,
    // The number of packets in the group.
    bytes_acked: u64,

    // The estimated bitrate determined by the loss controller.
    bitrate: Bitrate,

    average_loss: f64,

    window: Duration,
    time_threshold: Duration,
}

impl LossController {
    pub(crate) fn new(now: Instant, cfg: LossControllerConfig) -> Self {
        Self {
            last_update: now,
            last_increase: now,
            last_decrease: now,
            bytes_lost: 0,
            bytes_acked: 0,

            bitrate: DEFAULT_INITIAL_BITRATE,
            average_loss: 0.,
            window: cfg.window,
            time_threshold: cfg.time_threshold,
        }
    }

    pub(crate) fn add_bytes_acked(&mut self, bytes_acked: u64) {
        self.bytes_acked += bytes_acked;
    }

    pub(crate) fn add_bytes_lost(&mut self, bytes_lost: u64) {
        self.bytes_lost += bytes_lost;
    }

    pub(crate) fn calculate_loss_estimate(&mut self, now: Instant) -> Option<Bitrate> {
        let mut changed = false;

        // accumulate packets for WINDOW_INTERVAL. This is approx what TWCC does.
        if now >= self.last_update + self.window && self.bytes_lost + self.bytes_acked > 0 {
            if let Some(loss_ratio) = self.get_loss_ratio() {
                self.average_loss = self.compute_loss_average(
                    now.duration_since(self.last_update),
                    self.average_loss,
                    loss_ratio,
                );

                if self.get_average_loss() > DECREASE_LOSS_THRESHOLD
                    && now.duration_since(self.last_decrease) > self.time_threshold
                {
                    let factor = 1. - (0.5 * loss_ratio);
                    self.set_bitrate((self.bitrate as f64 * factor) as Bitrate);
                    self.last_decrease = now;
                    changed = true;
                } else if self.get_average_loss() < INCREASE_LOSS_THRESHOLD
                    && now.duration_since(self.last_increase) > self.time_threshold
                {
                    self.set_bitrate((self.bitrate as f64 * INCREASE_FACTOR) as Bitrate);
                    self.last_increase = now;
                    changed = true;
                }
            }

            self.last_update = now;
            self.bytes_acked = 0;
            self.bytes_lost = 0;
        }

        if changed {
            Some(self.bitrate)
        } else {
            None
        }
    }

    fn compute_loss_average(&mut self, delta: std::time::Duration, prev: f64, sample: f64) -> f64 {
        sample
            + (-1. * (delta.as_millis() as f64 / WINDOW_INTERVAL.whole_milliseconds() as f64)).exp()
                * (prev - sample)
    }

    pub(crate) fn get_bitrate(&self) -> Bitrate {
        self.bitrate
    }

    pub(crate) fn get_average_loss(&self) -> f64 {
        self.average_loss
    }

    // Returns None if a division by 0 were to occur if no bytes were sent or lost.
    pub(crate) fn get_loss_ratio(&self) -> Option<f64> {
        if self.bytes_lost + self.bytes_acked == 0 {
            None
        } else {
            Some(self.bytes_lost as f64 / (self.bytes_lost as f64 + self.bytes_acked as f64))
        }
    }

    fn set_bitrate(&mut self, bitrate: Bitrate) {
        self.bitrate = bitrate.clamp(DEFAULT_MIN_BITRATE, DEFAULT_MAX_BITRATE);
    }
}

/*
#[cfg(test)]
mod tests {
    use time::Duration;

    use super::*;

    #[test]
    fn first_packet_comes_after_window() {
        let t0 = Instant::now();
        let mut lc = LossController::new(
            t0,
            LossControllerConfig {
                window: Duration::milliseconds(100),
                ..Default::default()
            },
        );

        lc.update_loss_estimate(
            Acknowledgement::new(0, 5, Duration::milliseconds(0), None),
            t0 + std::time::Duration::from_millis(200),
        );
    }

    #[test]
    fn windows_properly() {
        let t0 = Instant::now();
        let mut lc = LossController::new(
            t0,
            LossControllerConfig {
                window: Duration::milliseconds(100),
                ..Default::default()
            },
        );

        assert!(lc
            .update_loss_estimate(
                Acknowledgement::new(0, 0, Duration::milliseconds(0), None),
                t0 + std::time::Duration::from_millis(50),
            )
            .is_none());

        assert!(lc
            .update_loss_estimate(
                Acknowledgement::new(1, 0, Duration::milliseconds(0), None),
                t0 + std::time::Duration::from_millis(75),
            )
            .is_none());

        assert!(lc
            .update_loss_estimate(
                Acknowledgement::new(2, 0, Duration::milliseconds(0), None),
                t0 + std::time::Duration::from_millis(99),
            )
            .is_none());

        assert_eq!(lc.next_pn, 3);
        assert_eq!(lc.packet_lost, 0);
        assert_eq!(lc.packets_total, 3);

        assert!(lc
            .update_loss_estimate(
                Acknowledgement::new(3, 0, Duration::milliseconds(0), None),
                t0 + std::time::Duration::from_millis(100),
            )
            .is_none());

        assert_eq!(lc.next_pn, 4);
        assert_eq!(lc.packet_lost, 0);
        assert_eq!(lc.packets_total, 1);

        assert_eq!(0., lc.get_average_loss());
    }

    #[test]
    fn increases_bitrate_by_factor() {
        let t0 = Instant::now();
        let mut lc = LossController::new(
            t0,
            LossControllerConfig {
                window: Duration::milliseconds(100),
                time_threshold: Duration::milliseconds(100),
            },
        );

        assert!(!lc
            .update_loss_estimate(
                Acknowledgement::new(0, 0, Duration::milliseconds(0), None),
                t0 + std::time::Duration::from_millis(50),
            )
            .is_none());

        assert!(!lc
            .update_loss_estimate(
                Acknowledgement::new(1, 0, Duration::milliseconds(0), None),
                t0 + std::time::Duration::from_millis(75),
            )
            .is_none());

        assert!(lc
            .update_loss_estimate(
                Acknowledgement::new(2, 0, Duration::milliseconds(0), None),
                t0 + std::time::Duration::from_millis(99),
            )
            .is_none());

        assert!(lc
            .update_loss_estimate(
                Acknowledgement::new(3, 0, Duration::milliseconds(0), None),
                t0 + std::time::Duration::from_millis(101),
            )
            .is_some());

        assert_eq!(0., lc.get_average_loss());
        assert_eq!(
            (DEFAULT_INITIAL_BITRATE as f64 * INCREASE_FACTOR) as Bitrate,
            lc.get_bitrate()
        );
    }

    #[test]
    fn ignores_out_of_order() {}
}
*/
