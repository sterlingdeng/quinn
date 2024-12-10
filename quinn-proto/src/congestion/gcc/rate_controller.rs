use crate::congestion::gcc::{
    overuse_detector::NetworkUsage, Bitrate, DEFAULT_MAX_BITRATE, DEFAULT_MIN_BITRATE,
};

use std::time::{Duration, Instant};

use tracing::trace;

// Minimal duration between 2 updates on the lost based rate controller
const DELAY_UPDATE_INTERVAL: Duration = Duration::from_millis(100);

const BETA: f64 = 0.85;

#[derive(Clone, Copy, Debug)]
pub(crate) enum State {
    Hold,
    Increase,
    Decrease,
}

impl State {
    pub(crate) fn string(&self) -> String {
        match self {
            State::Hold => String::from("hold"),
            State::Increase => String::from("increase"),
            State::Decrease => String::from("decrease"),
        }
    }
}

#[derive(Clone)]
pub(crate) struct RateController {
    state: State,
    latest_decrease_rate_ema: ExponentialMovingAverage,

    last_increase_time: Option<Instant>,
    last_decrease_time: Instant,

    // The target bitrate calculated for the delay controller.
    target: Bitrate,
}

impl RateController {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Increase,
            latest_decrease_rate_ema: ExponentialMovingAverage::new(),
            last_increase_time: None,
            last_decrease_time: Instant::now(),
            target: 2_048_000,
        }
    }

    pub(crate) fn target_bitrate(&self) -> Bitrate {
        self.target
    }

    pub(crate) fn state(&self) -> State {
        self.state
    }

    pub(crate) fn update(
        &mut self,
        // effective_bitrate is the measured bitrate by the peer.
        effective_bitrate: Option<Bitrate>,
        usage: NetworkUsage,
        rtt: Duration,
        now: Instant,
    ) -> Option<Bitrate> {
        if effective_bitrate.is_none() {
            return None;
        }
        match self.calculate_bitrate(effective_bitrate.unwrap(), usage, rtt, now) {
            Some(bitrate) => {
                self.set_target_bitrate(bitrate);
                Some(bitrate)
            }
            None => None,
        }
    }

    fn calculate_bitrate(
        &mut self,
        // effective_bitrate is the measured bitrate by the peer.
        effective_bitrate: Bitrate,
        usage: NetworkUsage,
        rtt: Duration,
        now: Instant,
    ) -> Option<Bitrate> {
        match usage {
            NetworkUsage::Normal => match self.state {
                State::Increase | State::Hold => {
                    self.state = State::Increase;
                    if let Some(new_bitrate) =
                        self.compute_increased_rate(effective_bitrate, rtt, now)
                    {
                        return Some(new_bitrate);
                    }
                }
                State::Decrease => {
                    self.latest_decrease_rate_ema
                        .update(effective_bitrate.into());
                    self.state = State::Hold;
                }
            },
            NetworkUsage::Over => {
                // Decrease the rate because of over use.
                if now - self.last_decrease_time > DELAY_UPDATE_INTERVAL {
                    let new_bitrate = self.compute_decreased_rate(effective_bitrate);
                    self.latest_decrease_rate_ema
                        .update(effective_bitrate as f64);
                    self.last_decrease_time = now;

                    self.state = State::Decrease;
                    return Some(new_bitrate);
                }
            }
            NetworkUsage::Under => {
                self.state = State::Hold;
            }
        }
        None
    }

    fn compute_decreased_rate(&self, effective_bitrate: Bitrate) -> Bitrate {
        trace!("rate controller decrease");
        let target = BETA * effective_bitrate as f64;
        target as Bitrate
    }

    fn compute_increased_rate(
        &mut self,
        effective_bitrate: Bitrate,
        rtt: Duration,
        now: Instant,
    ) -> Option<Bitrate> {
        let time_since_last_update_ms = match self.last_increase_time {
            None => 0.,
            Some(prev) => {
                if now - prev < DELAY_UPDATE_INTERVAL {
                    return None;
                }

                Duration::try_from(now - prev).unwrap().as_millis() as f64
            }
        };

        self.last_increase_time = Some(now);
        // NOTE: additive increase in GCC is coupled with concepts from video
        if self
            .latest_decrease_rate_ema
            .estimate_is_close(effective_bitrate.into())
        {
            // Additive increase
            let bits_per_frame = self.target as f64 / 30.0; // 30 frames per second
            let packets_per_frame = f64::ceil(bits_per_frame / (1200. * 8.));
            let avg_packet_size_bits = bits_per_frame / packets_per_frame;

            let rtt_ms = rtt.as_millis() as f64;
            let response_time_ms = 100. + rtt_ms;
            let alpha = 0.5 * f64::min(1.0, time_since_last_update_ms / response_time_ms);

            let threshold_on_effective_bitrate = 1.5 * effective_bitrate as f64;
            let increase = f64::max(
                1000.,
                f64::min(
                    alpha * avg_packet_size_bits,
                    f64::max(threshold_on_effective_bitrate - self.target as f64, 160.0),
                ),
            );

            return Some((self.target as f64 + increase) as Bitrate);
        } else {
            // Multiplicative increase
            let eta = 1.08_f64.powf(f64::min(time_since_last_update_ms / 1000., 1.0));
            let new_rate = eta * self.target as f64;
            let max = 1.5 * effective_bitrate as f64;

            /*
            trace!(
                effective_bitrate,
                eta,
                new_rate,
                max,
                self.target,
                "CIR: multiplicative increase"
            );
            */

            if new_rate > max && new_rate > self.target as f64 {
                Some(max as Bitrate)
            } else if new_rate < self.target as f64 {
                None
            } else {
                //trace!(new_rate, "using new_rate");
                Some(new_rate as Bitrate)
            }
        }
    }

    // Sets the target bitrate after clamping between a min and a max. Returns the clamped bitrate.
    fn set_target_bitrate(&mut self, target: Bitrate) -> Bitrate {
        self.target = target.clamp(DEFAULT_MIN_BITRATE, DEFAULT_MAX_BITRATE);
        self.target
    }
}

// It is RECOMMENDED to measure this average and standard deviation with an exponential moving average with the smoothing factor 0.95
const EMA_ALPHA: f64 = 0.95;

const STD_DEV_CLOSE: f64 = 3.;

/// ExponentialMovingAverage tracks the exponential moving average statistics of the estimated
/// bitrate of the times the system is in the DECREASE state. The statistics are tracked and used
/// to determine whether to perform a multiplicative increase if we are 'far' from the mean, or
/// additive increase if we are 'close'. See the function `estimate_is_close`.
///
/// https://datatracker.ietf.org/doc/html/draft-ietf-rmcat-gcc-02#section-5.5

#[derive(Clone)]
pub(crate) struct ExponentialMovingAverage {
    pub(crate) avg: f64,
    pub(crate) std_dev: f64,
    pub(crate) variance: f64,

    alpha: f64, // smoothing factor
    std_dev_is_close: f64,
}

impl ExponentialMovingAverage {
    pub(crate) fn new() -> Self {
        Self {
            avg: 0.0,
            std_dev: 0.0,
            variance: 0.0,
            alpha: EMA_ALPHA,
            std_dev_is_close: STD_DEV_CLOSE,
        }
    }

    // https://web.archive.org/web/20181222175223/http://people.ds.cam.ac.uk/fanf2/hermes/doc/antiforgery/stats.pdf
    // Section 9 - Exponentially Weighted mean and variance
    // pdf: Incremental calculation of weighted mean and variance
    pub(crate) fn update(&mut self, v: f64) {
        if self.avg == 0.0 {
            self.avg = v
        } else {
            let x = v - self.avg;
            self.avg += self.alpha * x;
            self.variance = (1.0 - self.alpha) * (self.variance + self.alpha * x * x);
            self.std_dev = self.variance.sqrt();
        }
    }

    /// This functions checks if the provided value is considered 'close', where 'close' is defined
    /// as 'x' std deviations around the mean. In the RFC, 'x' is 3.
    pub(crate) fn estimate_is_close(&self, value: f64) -> bool {
        let lower = self.avg - self.std_dev_is_close * self.std_dev;
        let upper = self.avg + self.std_dev_is_close * self.std_dev;
        (lower..upper).contains(&value)
    }
}
