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
    pub(crate) overuse_detector: OveruseDetector,
    rate_controller: RateController,

    pub(crate) last_inter_delay_variation: time::Duration,
}

impl DelayController {
    pub(crate) fn new() -> Self {
        Self {
            prefilter: Prefilter::new(PrefilterConfig::default()),
            arrival_filter: Kalman::new(KalmanConfig::default()),
            overuse_detector: OveruseDetector::new(AdaptiveThreshold::new()),
            rate_controller: RateController::new(),
            last_inter_delay_variation: time::Duration::ZERO,
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

        let inter_delay_variation = match packet_group_pair.inter_delay_variation() {
            None => {
                trace!("measurement is none");
                return None;
            }
            Some(v) => v,
        };
        self.last_inter_delay_variation = inter_delay_variation;

        self.arrival_filter.update_estimate(inter_delay_variation);
        let estimated_delay = self.arrival_filter.get_estimate();

        let network_usage = self.overuse_detector.process_estimate(estimated_delay, now);

        let ret = self
            .rate_controller
            .update(effective_bitrate, network_usage, rtt, now);

        ret
    }

    pub(crate) fn get_target_bitrate(&self) -> Bitrate {
        self.rate_controller.target_bitrate()
    }

    pub(crate) fn get_state(&self) -> State {
        self.rate_controller.state()
    }

    pub(crate) fn get_usage(&self) -> NetworkUsage {
        self.overuse_detector.get_usage()
    }
}
