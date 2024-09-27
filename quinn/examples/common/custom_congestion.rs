use proto::congestion::{Controller, ControllerFactory, Cubic, CubicConfig};
use std::sync::Arc;
use std::time::Instant;

use tracing::{info, info_span};

pub struct TestCubicWrapperFactory {}

impl ControllerFactory for TestCubicWrapperFactory {
    fn build(self: Arc<Self>, now: Instant, current_mtu: u16) -> Box<dyn Controller> {
        let cc = Arc::new(CubicConfig::default());
        let controller = TestCubicWrapper {
            last_packet: None,
            controller: cc.build(now, current_mtu),
        };
        Box::new(controller)
    }
}

pub struct TestCubicWrapper {
    last_packet: Option<LastPacket>,
    controller: Box<dyn Controller>,
}

#[derive(Debug, Clone)]
struct LastPacket {
    number: u64,
    sent: Instant,
    received: Option<Instant>,
}

impl Clone for TestCubicWrapper {
    fn clone(&self) -> Self {
        let cloned_controller = self.controller.clone_box();
        Self {
            last_packet: self.last_packet.clone(),
            controller: cloned_controller,
        }
    }
}

impl Controller for TestCubicWrapper {
    fn on_congestion_event(
        &mut self,
        now: Instant,
        sent: Instant,
        is_persistent_congestion: bool,
        lost_bytes: u64,
    ) {
        self.controller
            .on_congestion_event(now, sent, is_persistent_congestion, lost_bytes)
    }

    fn on_mtu_update(&mut self, new_mtu: u16) {
        self.controller.on_mtu_update(new_mtu);
    }
    fn window(&self) -> u64 {
        self.controller.window()
    }
    fn clone_box(&self) -> Box<dyn Controller> {
        Box::new(self.clone())
    }
    fn initial_window(&self) -> u64 {
        self.controller.initial_window()
    }
    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        Box::new(self)
    }

    // Provided methods

    fn on_ack_packet(
        &mut self,
        pn: u64,
        _now: Instant,
        sent: Instant,
        received: Option<Instant>,
        _bytes: u64,
        _app_limited: bool,
        _rtt: &proto::RttEstimator,
    ) {
        let span = info_span!("[cc] on_ack_packet", "pn" = pn);
        let _guard = span.enter();
        if let Some(recv) = received {
            info!("~0.5RTT={}", recv.duration_since(sent).as_millis());

            if let Some(lp) = self.last_packet.as_ref() {
                if let Some(last_recv) = lp.received {
                    info!(
                        "receiver interpacket delay = {}",
                        recv.duration_since(last_recv).as_millis()
                    )
                }
            }
        }
        self.last_packet = Some(LastPacket {
            number: pn,
            sent,
            received,
        })
    }
}
