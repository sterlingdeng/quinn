use std::collections::BTreeMap;

use time::Duration;

use crate::congestion::gcc::Acknowledgement;

pub(crate) type Bitrate = u32;

/// RateCalculator tracks the bitrate sent over the link within a duration `window`.
/// In the context of GCC and the RFC, this calculates the R_hat variable.
pub(crate) struct RateCalculator {
    window: Duration,
    packets: BTreeMap<u64, Acknowledgement>,
    size_bytes: u64,
}

impl RateCalculator {
    pub(crate) fn new(window: Duration) -> Self {
        Self {
            window,
            packets: Default::default(),
            size_bytes: 0,
        }
    }

    // update adds the Ack packet to the RateCalculator and evicts any Acks outside of the window.
    pub(crate) fn update(&mut self, ack: Acknowledgement) {
        self.size_bytes += ack.size;
        self.packets.insert(ack.packet_number, ack.clone());
        self.evict_old_packets();
    }

    fn evict_old_packets(&mut self) {
        if let Some(last_ack) = self.packets.last_key_value() {
            let last_arrival = last_ack.1.arrival;
            while let Some(first) = self.packets.first_key_value() {
                if first.1.arrival + self.window < last_arrival {
                    self.size_bytes -= first.1.size;
                    let _ = self.packets.pop_first();
                } else {
                    break;
                };
            }
        }
    }

    /// Returns the effective received bitrate during the last `window`.
    pub(crate) fn effective_bitrate(&self) -> Bitrate {
        if self.packets.is_empty() {
            return 0;
        }

        // Unwrap safety: Checked if empty above.
        let oldest = self.packets.first_key_value().unwrap();
        let newest = self.packets.last_key_value().unwrap();
        let duration = Duration::try_from(newest.1.arrival - oldest.1.arrival).ok();

        if let Some(dur) = duration {
            (self.size_bytes * 8 / dur.whole_seconds() as u64) as Bitrate
        } else {
            return 0;
        }
    }
}

// TODO(deng): TESTS!
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path() {
        let mut rc = RateCalculator::new(Duration::milliseconds(500));

        rc.update(Acknowledgement::new(0, 5, Duration::milliseconds(0)));
    }
}
