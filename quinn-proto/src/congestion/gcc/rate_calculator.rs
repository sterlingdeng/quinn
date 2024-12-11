use std::collections::VecDeque;
use tracing::warn;

use time::Duration;

use crate::congestion::gcc::{Acknowledgement, Bitrate};

// `N(i)` is the number of packets received the past T seconds and `L(j)` is
// the payload size of packet j.  A window between 0.5 and 1 second is
// RECOMMENDED.
const PACKETS_RECEIVED_DURATION_WINDOW: Duration = Duration::milliseconds(1000);

const MAX_COUNT: usize = 50;

pub(crate) struct RateCalculatorConfig {
    window: Duration,
    max_count: usize,
}

impl Default for RateCalculatorConfig {
    fn default() -> Self {
        Self {
            window: PACKETS_RECEIVED_DURATION_WINDOW,
            max_count: MAX_COUNT,
        }
    }
}

/// RateCalculator tracks the bitrate sent over the link within a duration `window`.
/// In the context of GCC and the RFC, this calculates the R_hat variable.
#[derive(Clone)]
pub(crate) struct RateCalculator {
    window_duration: Duration,
    max_count: usize,
    packets: VecDeque<Acknowledgement>,

    // size_bytes is the sum of the bytes between the interval left and right
    size_bytes: u64,
}

impl RateCalculator {
    pub(crate) fn new(config: RateCalculatorConfig) -> Self {
        Self {
            window_duration: config.window,
            max_count: config.max_count,
            packets: Default::default(),
            size_bytes: 0,
        }
    }

    // update adds the Ack packet to the RateCalculator and evicts any Acks outside of the window.
    pub(crate) fn add_ack(&mut self, ack: Acknowledgement) {
        self.packets.push_back(ack.clone());
        self.size_bytes += ack.size;
        self.update_window();
    }

    /// update_window
    /// The bitrate is calculated using a sliding window. The duration of the window is specified
    /// by the self.window_duration value. When acks with no arrival information is received, the
    /// last known calculation of the bitrate is used up until the last provided timestamp is older
    /// than the window duration.
    fn update_window(&mut self) {
        if self.packets.is_empty() {
            return;
        }

        if let Some(last_arrival) = self.packets.back().unwrap().arrival {
            while self.packets.len() > 1 {
                if let Some(earliest_arrival) = self.packets.front().unwrap().arrival {
                    if earliest_arrival + self.window_duration < last_arrival {
                        self.size_bytes -= self.packets.pop_front().unwrap().size;
                        continue;
                    }
                    break;
                } else {
                    self.size_bytes -= self.packets.pop_front().unwrap().size;
                }
            }
        } else {
            // In case ACK timestamps are never sent, we bound the size self.packets to
            // prevent unbounded memory usage.
            while self.packets.len() > self.max_count {
                self.size_bytes -= self.packets.pop_front().unwrap().size;
            }
        }
    }

    /// Returns the effective received bitrate during the last `window`.
    /// If the bitrate could not be calculated, None is returned.
    pub(crate) fn effective_bitrate(&self) -> Option<Bitrate> {
        if self.packets.len() <= 1 {
            return None;
        }

        // Unwrap safety 1: Checked if empty above
        // Unwrap safety 2: The front is guaranteed to contain an arrival.
        let oldest = self.packets.front().unwrap().arrival;
        let newest = self.packets.back().unwrap().arrival;

        if oldest.is_none() || newest.is_none() {
            return None;
        }

        // Unwrap safety: Check if empty above
        match Duration::try_from(newest.unwrap() - oldest.unwrap()) {
            Ok(dur) => {
                if dur.is_zero() {
                    None
                } else {
                    Some(((self.size_bytes * 8) as f64 / dur.as_seconds_f64()) as Bitrate)
                }
            }
            Err(e) => {
                warn!("failed to calculate effective bitrate: {}", e);
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path() {
        let mut rc = RateCalculator::new(RateCalculatorConfig {
            window: Duration::milliseconds(50),
            max_count: 30,
        });

        rc.add_ack(Acknowledgement::new(
            0,
            5,
            Duration::milliseconds(0),
            Some(Duration::milliseconds(10)),
        ));

        assert!(rc.effective_bitrate().is_none());

        rc.add_ack(Acknowledgement::new(
            1,
            5,
            Duration::milliseconds(0),
            Some(Duration::milliseconds(20)),
        ));

        assert!(rc.effective_bitrate().is_some());

        // (10 bytes * 8 bits / byte ) / (20ms - 10ms) * 1000 = 8000 bits/s
        assert_eq!(rc.effective_bitrate().unwrap(), 8000);

        // The next ack's arrival time will evict the first ack because of the window
        rc.add_ack(Acknowledgement::new(
            2,
            10,
            Duration::milliseconds(0),
            Some(Duration::milliseconds(61)),
        ));

        // (15 bytes * 8 bits / byte ) / (61ms - 20ms) * 1000 = 2926 bits/s
        assert_eq!(rc.effective_bitrate().unwrap(), 2926);

        // Adding an ACK without any arrival time will cause effective_bitrate to return None
        rc.add_ack(Acknowledgement::new(3, 10, Duration::milliseconds(0), None));

        assert!(rc.effective_bitrate().is_none());

        // Adding an ACK with an arrival time after will still take into account the Ack that previously did not contain a timestamp.
        rc.add_ack(Acknowledgement::new(
            4,
            10,
            Duration::milliseconds(0),
            Some(Duration::milliseconds(65)),
        ));

        // (35 bytes * 8 bits / byte ) / (65ms - 20ms) * 1000 = 6222 bits/s
        assert_eq!(rc.effective_bitrate().unwrap(), 6222);
    }

    #[test]
    fn start_with_empty_arrivals() {
        let mut rc = RateCalculator::new(RateCalculatorConfig {
            window: Duration::milliseconds(10),
            max_count: 30,
        });

        for i in 1..3 {
            rc.add_ack(Acknowledgement::new(i, 5, Duration::milliseconds(0), None));
        }

        assert!(rc.effective_bitrate().is_none());

        rc.add_ack(Acknowledgement::new(
            3,
            10,
            Duration::milliseconds(0),
            Some(Duration::milliseconds(1)),
        ));

        assert!(rc.effective_bitrate().is_none());

        rc.add_ack(Acknowledgement::new(
            4,
            10,
            Duration::milliseconds(0),
            Some(Duration::milliseconds(2)),
        ));

        // (20 * 8) / 1 * 1000 =  160,000

        assert_eq!(rc.effective_bitrate().unwrap(), 160_000);
    }

    #[test]
    fn same_arrival_time() {
        let mut rc = RateCalculator::new(RateCalculatorConfig {
            window: Duration::milliseconds(10),
            max_count: 30,
        });

        rc.add_ack(Acknowledgement::new(
            1,
            10,
            Duration::milliseconds(0),
            Some(Duration::milliseconds(1)),
        ));

        assert!(rc.effective_bitrate().is_none());

        rc.add_ack(Acknowledgement::new(
            2,
            10,
            Duration::milliseconds(0),
            Some(Duration::milliseconds(1)),
        ));

        // Can't divide by zero so we return None
        assert!(rc.effective_bitrate().is_none());
    }

    #[test]
    fn trim_below_max() {
        let mut rc = RateCalculator::new(RateCalculatorConfig {
            window: Duration::milliseconds(10),
            max_count: 3,
        });

        rc.add_ack(Acknowledgement::new(1, 10, Duration::milliseconds(0), None));
        rc.add_ack(Acknowledgement::new(2, 10, Duration::milliseconds(0), None));
        rc.add_ack(Acknowledgement::new(3, 10, Duration::milliseconds(0), None));
        assert_eq!(rc.packets.len(), 3);

        rc.add_ack(Acknowledgement::new(3, 10, Duration::milliseconds(0), None));
        assert_eq!(rc.packets.len(), 3);
    }
}
