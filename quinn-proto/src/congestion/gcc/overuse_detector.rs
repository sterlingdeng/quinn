use std::time::Instant;
use time::Duration;

use crate::congestion::gcc::{adaptive_threshold::AdaptiveThreshold, arrival_filter::Kalman};

// Table 1 Recommened Values
const INIT_VAL: Duration = Duration::microseconds(125);
const OVERUSE_TIME_TH: Duration = Duration::milliseconds(10);

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum NetworkUsage {
    Normal,
    Over,
    Under,
}

pub(crate) struct OveruseDetector {
    adaptive_threshold: AdaptiveThreshold,

    increasing_duration: Duration,
    increasing_count: u64,
    last_use_detector_update: Instant,
    last_estimate: Duration,
    last_usage: NetworkUsage,
}

impl OveruseDetector {
    fn process_estimate(&mut self, estimate: Duration, now: Instant) -> NetworkUsage {
        let (usage_th, estimate) = self.adaptive_threshold.compare_threshold(estimate, now);
        let delta = now - self.last_use_detector_update;
        self.last_use_detector_update = now;

        let usage = match usage_th {
            NetworkUsage::Over => {
                self.increasing_duration += delta;
                self.increasing_count += 1;

                if self.increasing_duration > OVERUSE_TIME_TH
                    && self.increasing_count > 1
                    && estimate > self.last_estimate
                {
                    return NetworkUsage::Over;
                }
                self.last_usage
            }
            NetworkUsage::Normal | NetworkUsage::Under => {
                self.increasing_count = 0;
                self.increasing_duration = Duration::ZERO;
                usage_th
            }
        };
        self.last_estimate = estimate;
        self.last_usage = usage;

        usage
    }
}
