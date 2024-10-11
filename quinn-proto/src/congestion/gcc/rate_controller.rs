use crate::congestion::gcc::{
    overuse_detector::NetworkUsage,
    rate_calculator::{Bitrate, RateCalculator},
};

#[derive(Clone, Copy)]
enum State {
    Hold,
    Increase,
    Decrease,
}

impl State {
    fn transition(&mut self, signal: NetworkUsage) {
        *self = match *self {
            Self::Increase => match signal {
                NetworkUsage::Over => State::Decrease,
                NetworkUsage::Normal => *self,
                NetworkUsage::Under => State::Hold,
            },
            Self::Decrease => match signal {
                NetworkUsage::Over => *self,
                NetworkUsage::Normal => State::Hold,
                NetworkUsage::Under => State::Hold,
            },
            Self::Hold => match signal {
                NetworkUsage::Over => State::Decrease,
                NetworkUsage::Normal => *self,
                NetworkUsage::Under => State::Increase,
            },
        }
    }
}

pub(crate) struct RateController<'a> {
    state: State,
    rate_calculator: &'a RateCalculator,
    latest_decrease_rate: ExponentialMovingAverage,
}

impl<'a> RateController<'a> {
    fn new(rate_calculator: &'a RateCalculator) -> Self {
        Self {
            state: State::Increase,
            latest_decrease_rate: ExponentialMovingAverage::new(),
            rate_calculator,
        }
    }

    fn process(&mut self, bitrate: Bitrate, usage: NetworkUsage) {
        match usage {
            NetworkUsage::Normal => match self.state {
                State::Increase | State::Hold => {
                    self.state = State::Increase;
                }
                State::Decrease => {
                    self.latest_decrease_rate.update(bitrate.into());
                    // update EMA
                    self.state = State::Hold;
                }
            },
            NetworkUsage::Over => {
                self.state = State::Decrease;
            }
            NetworkUsage::Under => {
                self.state = State::Hold;
            }
        }
    }

    fn compute_increased_rate(&mut self, bitrate: Bitrate) -> Bitrate {
        if self.latest_decrease_rate.estimate_is_close(bitrate.into()) {
        } else {
        }
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
