use std::time::Instant;
use time::Duration;

use crate::congestion::gcc::adaptive_threshold::AdaptiveThreshold;

use tracing::field::Value;

// Table 1 Recommened Values
const INIT_VAL: Duration = Duration::microseconds(125);
const OVERUSE_TIME_TH: Duration = Duration::milliseconds(10);

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum NetworkUsage {
    Normal,
    Over,
    Under,
}

impl NetworkUsage {
    pub(crate) fn string(&self) -> String {
        match self {
            NetworkUsage::Normal => String::from("normal"),
            NetworkUsage::Over => String::from("over"),
            NetworkUsage::Under => String::from("under"),
        }
    }
}

#[derive(Clone)]
pub(crate) struct OveruseDetector {
    pub adaptive_threshold: AdaptiveThreshold,

    increasing_duration: Duration,
    increasing_counter: u64,

    // last_estimate is used to fulfill the following condition in the spec:
    // > However, if m(i) < m(i-1), over-use will not be signaled even if all the above
    // > conditions are met.
    pub last_estimate: Duration,

    last_use_detector_update: Instant,
    usage: NetworkUsage,
}

impl OveruseDetector {
    pub(crate) fn new(adaptive_threshold: AdaptiveThreshold) -> Self {
        Self {
            adaptive_threshold,
            increasing_duration: Duration::ZERO,
            increasing_counter: 0,
            last_use_detector_update: Instant::now(),
            last_estimate: Duration::ZERO,
            usage: NetworkUsage::Normal,
        }
    }

    pub(crate) fn get_usage(&self) -> NetworkUsage {
        self.usage
    }

    pub(crate) fn process_estimate(&mut self, estimate: Duration, now: Instant) -> NetworkUsage {
        let (usage_th, amp_estimate) = self.adaptive_threshold.compare_threshold(estimate, now);
        let delta = now - self.last_use_detector_update;
        self.last_use_detector_update = now;

        let usage = match usage_th {
            NetworkUsage::Over => {
                self.increasing_duration += delta;
                self.increasing_counter += 1;

                if self.increasing_duration > OVERUSE_TIME_TH
                    && self.increasing_counter > 1
                    && amp_estimate > self.last_estimate
                {
                    NetworkUsage::Over
                } else {
                    self.usage
                }
            }
            NetworkUsage::Normal | NetworkUsage::Under => {
                self.increasing_duration = Duration::ZERO;
                self.increasing_counter = 0;
                usage_th
            }
        };
        self.last_estimate = amp_estimate;
        self.usage = usage;

        usage
    }
}
