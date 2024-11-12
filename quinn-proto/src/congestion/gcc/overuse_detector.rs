use std::time::Instant;
use time::Duration;

use crate::congestion::gcc::adaptive_threshold::AdaptiveThreshold;

// Table 1 Recommened Values
const INIT_VAL: Duration = Duration::microseconds(125);
const OVERUSE_TIME_TH: Duration = Duration::milliseconds(10);

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum NetworkUsage {
    Normal,
    Over,
    Under,
}

#[derive(Clone)]
pub(crate) struct OveruseDetector {
    adaptive_threshold: AdaptiveThreshold,

    increasing_duration: Duration,

    // last_estimate is used to fulfill the following condition in the spec:
    // > However, if m(i) < m(i-1), over-use will not be signaled even if all the above
    // > conditions are met.
    last_estimate: Duration,

    last_use_detector_update: Instant,
    usage: NetworkUsage,
}

impl OveruseDetector {
    pub(crate) fn new(adaptive_threshold: AdaptiveThreshold) -> Self {
        Self {
            adaptive_threshold,
            increasing_duration: Duration::ZERO,
            last_use_detector_update: Instant::now(),
            last_estimate: Duration::ZERO,
            usage: NetworkUsage::Normal,
        }
    }

    pub(crate) fn get_usage(&self) -> NetworkUsage {
        self.usage
    }

    pub(crate) fn process_estimate(&mut self, estimate: Duration, now: Instant) -> NetworkUsage {
        let (usage_th, estimate) = self.adaptive_threshold.compare_threshold(estimate, now);
        let delta = now - self.last_use_detector_update;
        self.last_use_detector_update = now;

        let usage = match usage_th {
            NetworkUsage::Over => {
                self.increasing_duration += delta;

                if self.increasing_duration > OVERUSE_TIME_TH && estimate > self.last_estimate {
                    return NetworkUsage::Over;
                }
                self.usage
            }
            NetworkUsage::Normal | NetworkUsage::Under => {
                self.increasing_duration = Duration::ZERO;
                usage_th
            }
        };
        self.last_estimate = estimate;
        self.usage = usage;

        usage
    }
}
