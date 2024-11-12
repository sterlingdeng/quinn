use time::Duration;

const CHI: f64 = 0.001;
const Q: f64 = 0.001;

pub(crate) struct KalmanConfig {
    gain: f64,
    process_uncertainty: f64,
    estimate_error: f64,
    initial_estimate: Duration,
    measurement_uncertainty: f64,
}

impl Default for KalmanConfig {
    fn default() -> Self {
        Self {
            gain: 0.,
            initial_estimate: Duration::ZERO,
            process_uncertainty: Q,
            estimate_error: 0.1,
            measurement_uncertainty: 0.,
        }
    }
}

#[derive(Clone)]
pub(crate) struct Kalman {
    gain: f64,
    estimate: Duration,
    process_uncertainty: f64,
    estimate_error: f64,
    measurement_uncertainty: f64,
}

impl Kalman {
    pub(crate) fn new(cfg: KalmanConfig) -> Self {
        Self {
            gain: cfg.gain,
            estimate: cfg.initial_estimate,
            process_uncertainty: cfg.process_uncertainty,
            estimate_error: cfg.estimate_error,
            measurement_uncertainty: cfg.measurement_uncertainty,
        }
    }

    pub(crate) fn update_estimate(&mut self, measurement: Duration) {
        let z = measurement - self.estimate;
        let zms = z.whole_microseconds() as f64 / 1000.0;
        let alpha = (1.0 - CHI).powf(30.0 / (1000. * 5. * 1_000_000.));
        let root = self.measurement_uncertainty.sqrt();
        let root3 = 3. * root;

        self.measurement_uncertainty = if zms > root3 {
            (alpha * self.measurement_uncertainty + (1. - alpha) * root3.powf(2.)).max(1.)
        } else {
            (alpha * self.measurement_uncertainty + (1. - alpha) * zms.powf(2.)).max(1.)
        };

        let estimate_uncertainty = self.estimate_error + self.process_uncertainty;
        self.gain = estimate_uncertainty / (estimate_uncertainty + self.measurement_uncertainty);
        self.estimate += Duration::nanoseconds((self.gain * zms * 1_000_000.) as i64);
        self.estimate_error = (1. - self.gain) * estimate_uncertainty;
    }

    pub(crate) fn get_estimate(&self) -> Duration {
        self.estimate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kalman() {
        let cfg = KalmanConfig {
            initial_estimate: Duration::milliseconds(10),
            estimate_error: 100. * 100.,
            process_uncertainty: 0.15,
            measurement_uncertainty: 0.01,
            ..Default::default()
        };
        let mut k = Kalman::new(cfg);
        k.update_estimate(Duration::microseconds(50450));
        assert_eq!(Duration::nanoseconds(50449959), k.get_estimate());
    }
}
