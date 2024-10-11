use super::{Controller, ControllerFactory};

mod acknowledgement;
use acknowledgement::Acknowledgement;

mod adaptive_threshold;

mod arrival_filter;

mod delay_controller;

mod overuse_detector;

mod rate_controller;

mod rate_calculator;

mod prefilter;

/// Gcc implements the Google Congestion Control algorithm.
pub struct Gcc {
    //delay_controller: delay_controller::DelayController,
}

impl Gcc {
    fn new() -> Self {
        Self {}
    }
}

impl Controller for Gcc {
    fn on_mtu_update(&mut self, new_mtu: u16) {
        todo!();
    }

    fn window(&self) -> u64 {
        todo!();
    }

    fn clone_box(&self) -> Box<dyn Controller> {
        todo!();
    }

    fn initial_window(&self) -> u64 {
        todo!();
    }

    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        todo!();
    }

    fn on_congestion_event(
        &mut self,
        now: std::time::Instant,
        sent: std::time::Instant,
        is_persistent_congestion: bool,
        lost_bytes: u64,
    ) {
        todo!();
    }

    fn on_ack_packet(
        &mut self,
        pn: u64,
        _now: std::time::Instant,
        sent: std::time::Instant,
        received: Option<std::time::Instant>,
        bytes: u64,
        _app_limited: bool,
        _rtt: &crate::RttEstimator,
    ) {
    }
}

/// GccConfig is the factory to produce GCC instances.
pub struct GccConfig {}

impl ControllerFactory for GccConfig {
    fn build(
        self: std::sync::Arc<Self>,
        now: std::time::Instant,
        current_mtu: u16,
    ) -> Box<dyn Controller> {
        todo!();
    }
}
