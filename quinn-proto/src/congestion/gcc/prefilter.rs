use crate::congestion::gcc::Acknowledgement;
use std::{fmt, time::Instant};

use time::Duration;

const BURST_TIME: Duration = Duration::milliseconds(5);

#[derive(PartialEq, Eq, Clone)]
pub(crate) struct PacketGroup {
    acks: Vec<Acknowledgement>,
    departure: Duration,
    arrival: Duration,
}

impl Default for PacketGroup {
    fn default() -> Self {
        Self {
            acks: Default::default(),
            departure: Duration::ZERO,
            arrival: Duration::ZERO,
        }
    }
}

impl PacketGroup {
    fn add(&mut self, ack: Acknowledgement) {
        if self.acks.is_empty() {
            self.departure = ack.departure;
            self.arrival = ack.arrival;
            self.acks.push(ack);
            return;
        }

        if ack.arrival > self.arrival {
            self.arrival = ack.arrival;
        }
        self.acks.push(ack);
    }

    fn inter_arrival_time_pkt(&self, next_pkt: &Acknowledgement) -> Duration {
        next_pkt.arrival - self.arrival
    }

    fn inter_departure_time_pkt(&self, next_pkt: &Acknowledgement) -> Duration {
        next_pkt.departure - self.departure
    }

    fn inter_delay_variation_pkt(&self, next_pkt: &Acknowledgement) -> Duration {
        self.inter_arrival_time_pkt(next_pkt) - self.inter_departure_time_pkt(next_pkt)
    }
}

impl fmt::Debug for PacketGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("arrival_group")
            .field("acks_len", &self.acks.len())
            .finish()
    }
}

/// Prefilter implements the pre-filter as described in Section 5.2 of the GCC IETF doc.
/// The goal of prefilter is to merge together groups of packets that arrive in a burst.
pub(crate) struct Prefilter {
    burst_time: Duration,

    group: PacketGroup,
}

impl Default for Prefilter {
    fn default() -> Self {
        Self {
            burst_time: BURST_TIME,
            group: Default::default(),
        }
    }
}

impl Prefilter {
    fn new(burst_time: Duration) -> Self {
        Self {
            group: PacketGroup::default(),
            burst_time,
        }
    }

    fn add_packet(&mut self, next: Acknowledgement) -> Option<PacketGroup> {
        if self.group.acks.is_empty() {
            self.group.add(next);
            return None;
        }

        if next.arrival < self.group.acks.last().unwrap().arrival {
            // Ignore out of order arrivals.
            return None;
        }

        //The pre-filtering merges together groups of packets that arrive in a
        // burst.  Packets are merged in the same group if one of these two
        // conditions holds:

        if next.departure >= self.group.departure {
            // A sequence of packets which are sent within a burst_time interval
            // constitute a group.
            if self.group.inter_departure_time_pkt(&next) < self.burst_time {
                self.group.add(next);
                return None;
            }

            // A Packet which has an inter-arrival time less than burst_time and
            // an inter-group delay variation d(i) less than 0 is considered
            // being part of the current group of packets.
            if self.group.inter_arrival_time_pkt(&next) < self.burst_time
                && self.group.inter_delay_variation_pkt(&next) < Duration::ZERO
            {
                self.group.add(next);
                return None;
            }

            let ret = self.group.clone();

            self.group = PacketGroup::default();
            self.group.add(next);

            return Some(ret);
        }

        None
    }
}

#[cfg(test)]
mod prefilter_test {
    use super::*;

    #[test]
    fn creates_a_group() {
        let mut pf = Prefilter::new(Duration::milliseconds(5));

        let acks = vec![
            Acknowledgement {
                packet_number: 0,
                size: 0,
                departure: Duration::milliseconds(0),
                arrival: Duration::milliseconds(1),
            },
            // this triggers a new group
            Acknowledgement {
                packet_number: 1,
                size: 0,
                departure: Duration::seconds(1),
                arrival: Duration::seconds(1),
            },
        ];

        let mut got = vec![];
        for ack in acks.iter() {
            got.push(pf.add_packet(ack.clone()));
        }

        let expect = vec![
            None,
            Some(PacketGroup {
                acks: vec![acks[0]],
                departure: acks[0].departure,
                arrival: acks[0].arrival,
            }),
        ];

        assert_eq!(2, got.len());
        assert_eq!(expect, got);
    }

    #[test]
    fn merge_within_sent_burst() {
        let mut pf = Prefilter::new(Duration::milliseconds(5));

        let acks = vec![
            Acknowledgement {
                packet_number: 0,
                size: 0,
                departure: Duration::milliseconds(0),
                arrival: Duration::milliseconds(15),
            },
            Acknowledgement {
                packet_number: 1,
                size: 0,
                departure: Duration::milliseconds(4),
                arrival: Duration::milliseconds(20),
            },
            // this triggers a new group
            Acknowledgement {
                packet_number: 2,
                size: 0,
                departure: Duration::seconds(1),
                arrival: Duration::seconds(1),
            },
        ];

        let mut got = vec![];
        for ack in acks.iter() {
            got.push(pf.add_packet(ack.clone()));
        }

        let expect = vec![
            None,
            None,
            Some(PacketGroup {
                acks: vec![acks[0], acks[1]],
                departure: acks[0].departure,
                arrival: acks[1].arrival,
            }),
        ];

        assert_eq!(3, got.len());
        assert_eq!(expect, got);
    }

    #[test]
    fn merge_within_inter_arrival_burst() {
        let mut pf = Prefilter::new(Duration::milliseconds(5));

        let acks = vec![
            Acknowledgement {
                packet_number: 0,
                size: 0,
                departure: Duration::milliseconds(0),
                arrival: Duration::milliseconds(15),
            },
            Acknowledgement {
                packet_number: 1,
                size: 0,
                departure: Duration::milliseconds(6),
                arrival: Duration::milliseconds(16),
            },
            // this triggers a new group
            Acknowledgement {
                packet_number: 2,
                size: 0,
                departure: Duration::seconds(1),
                arrival: Duration::seconds(1),
            },
        ];

        let mut got = vec![];
        for ack in acks.iter() {
            got.push(pf.add_packet(ack.clone()));
        }

        let expect = vec![
            None,
            None,
            Some(PacketGroup {
                acks: vec![acks[0], acks[1]],
                departure: acks[0].departure,
                arrival: acks[1].arrival,
            }),
        ];

        assert_eq!(3, got.len());
        assert_eq!(expect, got);
    }
}
