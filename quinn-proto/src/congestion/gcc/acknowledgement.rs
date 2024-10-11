use std::time::Instant;
use time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Acknowledgement {
    pub(crate) packet_number: u64,
    pub(crate) size: u64,
    pub(crate) departure: Duration,
    pub(crate) arrival: Option<Duration>,
}

impl Acknowledgement {
    pub(crate) fn new(packet_number: u64, size: u64, departure: Duration) -> Self {
        Self {
            packet_number,
            size,
            departure,
            arrival: None,
        }
    }

    pub(crate) fn set_arrival(mut self, arrival: Duration) -> Self {
        self.arrival = Some(arrival);
        self
    }
}
