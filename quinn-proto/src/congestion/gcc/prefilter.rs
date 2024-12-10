use std::fmt;

use crate::congestion::gcc::Acknowledgement;

use time::Duration;

const BURST_TIME: Duration = Duration::milliseconds(5);

#[derive(PartialEq, Eq, Clone)]
pub(crate) struct PacketGroup {
    acks: Vec<Acknowledgement>,
    departure: Duration,
    // This is different than GCC because it's possible to not include the arrival time for a
    // packet.
    arrival: Option<Duration>,
}

impl PacketGroup {
    pub(crate) fn inter_group_delay_variation(&self, other: &Self) -> Option<Duration> {
        if other.arrival.is_some() && self.arrival.is_some() {
            Some(
                (self.arrival.unwrap() - other.arrival.unwrap())
                    - (self.departure - other.departure),
            )
        } else {
            None
        }
    }
}

impl Default for PacketGroup {
    fn default() -> Self {
        Self {
            acks: Default::default(),
            departure: Duration::ZERO,
            arrival: None,
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

    fn inter_arrival_time_pkt(&self, next_pkt: &Acknowledgement) -> Option<Duration> {
        if let (Some(next), Some(current)) = (next_pkt.arrival, self.arrival) {
            Some(next - current)
        } else {
            None
        }
    }

    fn inter_departure_time_pkt(&self, next_pkt: &Acknowledgement) -> Duration {
        next_pkt.departure - self.departure
    }

    fn inter_delay_variation_pkt(&self, next_pkt: &Acknowledgement) -> Option<Duration> {
        if let Some(inter_arrival) = self.inter_arrival_time_pkt(next_pkt) {
            Some(inter_arrival - self.inter_departure_time_pkt(next_pkt))
        } else {
            None
        }
    }
}

impl fmt::Debug for PacketGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("arrival_group")
            .field("acks_len", &self.acks.len())
            .finish()
    }
}

pub(crate) struct PrefilterConfig {
    burst_time: Duration,
}

impl Default for PrefilterConfig {
    fn default() -> Self {
        Self {
            burst_time: BURST_TIME,
        }
    }
}

/// Prefilter implements the pre-filter as described in Section 5.2 of the GCC IETF doc.
/// The goal of prefilter is to merge together groups of packets that arrive in a burst.

#[derive(Clone)]
pub(crate) struct Prefilter {
    burst_time: Duration,

    group: PacketGroup,
    prev_group: Option<PacketGroup>,
}

#[derive(Debug, PartialEq)]
pub(crate) struct PacketGroupPair {
    pub(crate) prev: PacketGroup,
    pub(crate) curr: PacketGroup,
}

impl PacketGroupPair {
    pub(crate) fn inter_delay_variation(&self) -> Option<Duration> {
        self.curr.inter_group_delay_variation(&self.prev)
    }
}

impl Prefilter {
    pub(crate) fn new(cfg: PrefilterConfig) -> Self {
        Self {
            group: PacketGroup::default(),
            burst_time: cfg.burst_time,
            prev_group: None,
        }
    }

    /// add_packet adds an acknowledgement to the prefilter.
    /// When enough acknowledgements are added such that two distinct packet groups are formed,
    /// the PacketGroupPair is returned with the previous and current packet group.
    pub(crate) fn add_packet(&mut self, next: Acknowledgement) -> Option<PacketGroupPair> {
        if self.group.acks.is_empty() {
            self.group.add(next);
            return None;
        }

        if next.arrival < self.group.acks.last().unwrap().arrival {
            // Ignore out of order arrivals.
            return None;
        }

        // The pre-filtering merges together groups of packets that arrive in a
        // burst.  Packets are merged in the same group if one of these two
        // conditions holds:

        if next.departure >= self.group.departure {
            // A sequence of packets which are sent within a burst_time interval
            // constitute a group.
            if self.group.inter_departure_time_pkt(&next) < self.burst_time {
                self.group.add(next);
                return None;
            }

            if next.arrival.is_none() || self.group.arrival.is_none() {
                // This deviates because in GCC because in QUIC, it's possible for the Ack time to get dropped.
                // If the arrival time does not exist, we add it to the group because we don't want
                // to lose the fact an ACK did occur for a packet.
                // TODO: Review this
                // We need to flush it out at some point. What if the peer ends up never sending
                // timestamps?
                self.group.add(next);
                return None;
            }

            // after it reaches a certain amount of packets, we should flush it out to the loss
            // controller so at least we have control of the loss controller

            // A Packet which has an inter-arrival time less than burst_time and
            // an inter-group delay variation d(i) less than 0 is considered
            // being part of the current group of packets.
            // Unwrap safety: the existance of both next.arrival and group.arrival is checked
            // above.
            if self.group.inter_arrival_time_pkt(&next).unwrap() < self.burst_time
                && self.group.inter_delay_variation_pkt(&next).unwrap() < Duration::ZERO
            {
                self.group.add(next);
                return None;
            }

            let prev = self.prev_group.take();
            let curr = self.group.clone();

            self.prev_group = Some(curr.clone());

            self.group = PacketGroup::default();
            self.group.add(next);

            return prev.map(|prev| PacketGroupPair { prev, curr });
        }

        None
    }
}

#[cfg(test)]
mod prefilter_test {
    use super::*;

    #[test]
    fn creates_a_group() {
        let mut pf = Prefilter::new(PrefilterConfig {
            burst_time: Duration::milliseconds(5),
        });

        let acks = vec![
            Acknowledgement {
                packet_number: 0,
                size: 0,
                departure: Duration::milliseconds(0),
                arrival: Some(Duration::milliseconds(1)),
            },
            // this triggers a new group
            Acknowledgement {
                packet_number: 1,
                size: 0,
                departure: Duration::seconds(1),
                arrival: Some(Duration::seconds(2)),
            },
            // this triggers a new group
            Acknowledgement {
                packet_number: 2,
                size: 0,
                departure: Duration::seconds(2),
                arrival: Some(Duration::seconds(3)),
            },
        ];

        let mut got = vec![];
        for ack in acks.iter() {
            got.push(pf.add_packet(ack.clone()));
        }

        let expect = vec![
            None,
            None,
            Some(PacketGroupPair {
                prev: PacketGroup {
                    acks: vec![acks[0]],
                    departure: acks[0].departure,
                    arrival: acks[0].arrival,
                },
                curr: PacketGroup {
                    acks: vec![acks[1]],
                    departure: acks[1].departure,
                    arrival: acks[1].arrival,
                },
            }),
        ];

        assert_eq!(2, got.len());
        assert_eq!(expect, got);
    }

    #[test]
    fn merge_within_sent_burst() {
        let mut pf = Prefilter::new(PrefilterConfig {
            burst_time: Duration::milliseconds(5),
        });

        let acks = vec![
            Acknowledgement {
                packet_number: 0,
                size: 0,
                departure: Duration::milliseconds(0),
                arrival: Some(Duration::milliseconds(15)),
            },
            Acknowledgement {
                packet_number: 1,
                size: 0,
                departure: Duration::milliseconds(4),
                arrival: Some(Duration::milliseconds(20)),
            },
            // this triggers a new group
            Acknowledgement {
                packet_number: 2,
                size: 0,
                departure: Duration::seconds(1),
                arrival: Some(Duration::seconds(1)),
            },
            // this triggers a new group
            Acknowledgement {
                packet_number: 3,
                size: 0,
                departure: Duration::seconds(2),
                arrival: Some(Duration::seconds(2)),
            },
        ];

        let mut got = vec![];
        for ack in acks.iter() {
            got.push(pf.add_packet(ack.clone()));
        }

        let expect = vec![
            None,
            None,
            None,
            Some(PacketGroupPair {
                prev: PacketGroup {
                    acks: vec![acks[0], acks[1]],
                    departure: acks[0].departure,
                    arrival: acks[1].arrival,
                },
                curr: PacketGroup {
                    acks: vec![acks[2]],
                    departure: acks[2].departure,
                    arrival: acks[2].arrival,
                },
            }),
        ];

        assert_eq!(3, got.len());
        assert_eq!(expect, got);
    }

    #[test]
    fn merge_within_inter_arrival_burst() {
        let mut pf = Prefilter::new(PrefilterConfig {
            burst_time: Duration::milliseconds(5),
        });

        let acks = vec![
            Acknowledgement {
                packet_number: 0,
                size: 0,
                departure: Duration::milliseconds(0),
                arrival: Some(Duration::milliseconds(15)),
            },
            Acknowledgement {
                packet_number: 1,
                size: 0,
                departure: Duration::milliseconds(6),
                arrival: Some(Duration::milliseconds(16)),
            },
            // this triggers a new group
            Acknowledgement {
                packet_number: 2,
                size: 0,
                departure: Duration::seconds(1),
                arrival: Some(Duration::seconds(1)),
            },
            // this triggers a new group
            Acknowledgement {
                packet_number: 3,
                size: 0,
                departure: Duration::seconds(2),
                arrival: Some(Duration::seconds(2)),
            },
        ];

        let mut got = vec![];
        for ack in acks.iter() {
            got.push(pf.add_packet(ack.clone()));
        }

        let expect = vec![
            None,
            None,
            None,
            Some(PacketGroupPair {
                prev: PacketGroup {
                    acks: vec![acks[0], acks[1]],
                    departure: acks[0].departure,
                    arrival: acks[1].arrival,
                },
                curr: PacketGroup {
                    acks: vec![acks[2]],
                    departure: acks[2].departure,
                    arrival: acks[2].arrival,
                },
            }),
        ];

        assert_eq!(3, got.len());
        assert_eq!(expect, got);
    }
}
