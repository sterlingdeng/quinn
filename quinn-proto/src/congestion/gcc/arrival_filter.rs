use std::time::Duration;

const CHI: f64 = 0.001;
const Q: f64 = 0.001;

pub(crate) struct Kalman {
    gain: f64,
    estimate: Duration,
    process_uncertainty: f64,
    estimate_error: f64,
    measurement_uncertainty: f64,

    disable_measurement_uncertainty_update: bool,
}

impl Kalman {
    fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    fn update_estimate(&mut self, measurement: Duration) {
        let z = measurement - self.estimate;
        let zms = z.as_micros() as f64 / 1000.0;
        let alpha = (1.0 - CHI).powf(30.0 / (1000. * 5. * 1_000_000.));
        let root = self.measurement_uncertainty.sqrt();
        let root3 = 3. * root;

        self.measurement_uncertainty = if zms > root3 {
            (alpha * self.measurement_uncertainty + (1. - alpha) * root3.powf(2.)).max(1.)
        } else {
            (alpha * self.measurement_uncertainty + (1. - alpha) * zms.powf(2.)).max(1.)
        };

        let estimate_uncertainty = self.estimate_error + Q;
        self.gain = estimate_uncertainty / (estimate_uncertainty + self.measurement_uncertainty);
        self.estimate += Duration::from_nanos((self.gain * zms * 1_000_000.) as u64);
        self.estimate_error = (1. - self.gain) * estimate_uncertainty;
    }

    fn estimate(&self) -> Duration {
        self.estimate
    }
}

impl Default for Kalman {
    fn default() -> Self {
        Self {
            gain: 0.0,
            estimate: Duration::default(),
            process_uncertainty: 1e-3,
            estimate_error: 0.1,
            measurement_uncertainty: 0.0,
            disable_measurement_uncertainty_update: false,
        }
    }
}
