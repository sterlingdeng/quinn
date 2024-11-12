use std::time::{Duration, Instant};

use crate::congestion::gcc::{
    acknowledgement::Acknowledgement,
    adaptive_threshold::AdaptiveThreshold,
    arrival_filter::Kalman,
    overuse_detector::{NetworkUsage, OveruseDetector},
    prefilter::{Prefilter, PrefilterConfig},
    rate_controller::{RateController, State},
    Bitrate,
};

use tracing::trace;

use super::arrival_filter::KalmanConfig;

#[derive(Clone)]
pub(crate) struct DelayController {
    prefilter: Prefilter,
    arrival_filter: Kalman,
    overuse_detector: OveruseDetector,
    rate_controller: RateController,
}

impl DelayController {
    pub(crate) fn new() -> Self {
        Self {
            prefilter: Prefilter::new(PrefilterConfig::default()),
            arrival_filter: Kalman::new(KalmanConfig::default()),
            overuse_detector: OveruseDetector::new(AdaptiveThreshold::new()),
            rate_controller: RateController::new(),
        }
    }

    pub(crate) fn process_packet(
        &mut self,
        ack: Acknowledgement,
        effective_bitrate: Option<Bitrate>,
        rtt: Duration,
        now: Instant,
    ) -> Option<Bitrate> {
        let packet_group_pair = match self.prefilter.add_packet(ack) {
            None => return None,
            Some(v) => v,
        };

        let measurement = match packet_group_pair.inter_delay_variation() {
            None => {
                trace!("measurement is none");
                return None;
            }
            Some(v) => v,
        };

        self.arrival_filter.update_estimate(measurement);
        let estimated_bitrate = self.arrival_filter.get_estimate();

        let network_usage = self
            .overuse_detector
            .process_estimate(estimated_bitrate, now);

        self.rate_controller
            .update(effective_bitrate, network_usage, rtt, now)
    }

    pub(crate) fn get_target_bitrate(&self) -> Bitrate {
        self.rate_controller.target_bitrate()
    }

    pub(crate) fn get_state(&self) -> State {
        self.rate_controller.state()
    }

    pub(crate) fn usage(&self) -> NetworkUsage {
        self.overuse_detector.get_usage()
    }
}
