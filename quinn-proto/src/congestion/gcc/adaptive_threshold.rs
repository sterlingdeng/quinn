use crate::congestion::gcc::overuse_detector::NetworkUsage;
use std::time::Instant;

use time::Duration;

// From Table 1 Recommended Values
const K_U: f64 = 0.01;
const K_D: f64 = 0.00018;

// It is also RECOMMENDED to clamp del_var_th(i) to the range [6, 600]
const MIN_THRESHOLD: Duration = Duration::milliseconds(6);
const MAX_THRESHOLD: Duration = Duration::milliseconds(600);

const INITIAL_DEL_VAR_TH: Duration = Duration::microseconds(12500);

#[derive(Clone)]
pub(crate) struct AdaptiveThreshold {
    threshold: Duration,
    overuse_coeff_up: f64,
    overuse_coeff_down: f64,
    min: Duration,
    max: Duration,
    last_update: Option<Instant>,
    num_deltas: i64,
}

impl AdaptiveThreshold {
    pub(crate) fn new() -> Self {
        Self {
            threshold: INITIAL_DEL_VAR_TH,
            overuse_coeff_down: K_D,
            overuse_coeff_up: K_U,
            min: MIN_THRESHOLD,
            max: MAX_THRESHOLD,
            last_update: None,
            num_deltas: 0,
        }
    }

    //
    pub(crate) fn compare_threshold(
        &mut self,
        estimate: Duration,
        now: Instant,
    ) -> (NetworkUsage, Duration) {
        const MAX_DELTAS: i64 = 60;
        self.num_deltas += 1;
        if self.num_deltas < 2 {
            return (NetworkUsage::Normal, estimate);
        }

        // Not in the spec but exists on other implementations.
        // The purpose of this seems to gradually increase impact of the estimate as more history
        // is gathered.
        let amplified_estimate = Duration::nanoseconds(
            estimate.whole_nanoseconds() as i64 * i64::min(self.num_deltas, MAX_DELTAS),
        );

        let usage = if amplified_estimate > self.threshold {
            NetworkUsage::Over
        } else if amplified_estimate.whole_nanoseconds() < -self.threshold.whole_nanoseconds() {
            NetworkUsage::Under
        } else {
            NetworkUsage::Normal
        };

        self.update_threshold(amplified_estimate, now);

        (usage, amplified_estimate)
    }

    fn update_threshold(&mut self, estimate: Duration, now: Instant) {
        if self.last_update.is_none() {
            self.last_update = Some(now);
        }

        let abs_estimate = estimate.abs();
        // Moreover, del_var_th(i) SHOULD NOT be updated if this condition
        // holds: |m(i)| - del_var_th(i) > 15
        if abs_estimate > self.threshold + Duration::milliseconds(15) {
            self.last_update = Some(now);
            return;
        }

        let k = if abs_estimate < self.threshold {
            K_U
        } else {
            K_D
        };

        // Not sure where this comes from but is found in pion and gstreamer implementations.
        const MAX_TIME_DELTAS: Duration = Duration::milliseconds(100);
        let time_delta = Duration::try_from(now - self.last_update.unwrap())
            .unwrap()
            .min(MAX_TIME_DELTAS);

        let d = abs_estimate - self.threshold;

        // del_var_th(i) = del_var_th(i-1) + (t(i)-t(i-1)) * K(i) * (|m(i)|-del_var_th(i-1))
        let add = k * d.whole_milliseconds() as f64 * time_delta.whole_milliseconds() as f64;
        self.threshold += Duration::nanoseconds((add * 1_000_000.) as i64);

        self.threshold = self.threshold.clamp(MIN_THRESHOLD, MAX_THRESHOLD);
        self.last_update = Some(now);
    }
}

#[cfg(test)]
mod tests {}
