use crate::congestion::gcc::acknowledgement::Acknowledgement;
use crate::congestion::gcc::prefilter::Prefilter;

pub(crate) struct DelayController {
    prefilter: Prefilter,
    // arrival time filter
    //
    // adaptivethreshold
    // overuse_detector
    // rate_controller
}

impl DelayController {
    pub(crate) fn process_packet(&mut self, pkt: Acknowledgement) {}
}
