use time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Acknowledgement {
    pub(crate) packet_number: u64,
    // size in bytes
    pub(crate) size: u64,
    pub(crate) departure: Duration,
    // The arrival time is best effort and may not be included.
    pub(crate) arrival: Option<Duration>,
}

impl Acknowledgement {
    pub(crate) fn new(
        packet_number: u64,
        size: u64,
        departure: Duration,
        arrival: Option<Duration>,
    ) -> Self {
        Self {
            packet_number,
            size,
            departure,
            arrival,
        }
    }
}
