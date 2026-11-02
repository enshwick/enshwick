use std::collections::VecDeque;

use proptest::prelude::*;

use super::*;

const PACKET: usize = 1200;
const STEP: Duration = Duration::from_millis(50);

/// What the network does to one packet, from one byte of the test's input.
#[derive(Clone, Copy)]
enum Fate {
    Lost,
    Twice,
    Late(u8),
}

/// The fate of the `count`th packet; the count spreads a short input over
/// every fate at about the loss rate.
fn fate(byte: u8, count: u32, loss: u8) -> Fate {
    match (u32::from(byte) + count * 37) % 100 {
        n if n < u32::from(loss) => Fate::Lost,
        95.. => Fate::Twice,
        n => Fate::Late(u8::try_from(n % 4).unwrap()),
    }
}

/// Packets on their way, with the step they arrive at.
#[derive(Default)]
struct Wire {
    flying: Vec<(u32, Vec<u8>)>,
    fates: VecDeque<u8>,
    loss: u8,
    count: u32,
}

impl Wire {
    fn put(&mut self, step: u32, packet: &[u8]) {
        let byte = self.fates.pop_front().unwrap_or(50);
        self.fates.push_back(byte);
        self.count += 1;
        match fate(byte, self.count, self.loss) {
            Fate::Lost => {}
            Fate::Twice => {
                self.flying.push((step + 1, packet.to_vec()));
                self.flying.push((step + 2, packet.to_vec()));
            }
            Fate::Late(by) => self
                .flying
                .push((step + 1 + u32::from(by), packet.to_vec())),
        }
    }

    fn arrived(&mut self, step: u32) -> Vec<Vec<u8>> {
        let (now, later): (Vec<_>, Vec<_>) = self.flying.drain(..).partition(|(at, _)| *at <= step);
        self.flying = later;
        now.into_iter().map(|(_, packet)| packet).collect()
    }
}

/// Runs `messages` from `a` to `b` over two wires with `loss` percent lost;
/// what `b` delivered, in order, and whether `a` gave up.
fn carry(messages: &[Vec<u8>], fates: &[u8], loss: u8) -> (Vec<Vec<u8>>, bool) {
    let mut a = Link::new(PACKET).unwrap();
    let mut b = Link::new(PACKET).unwrap();
    let mut there = Wire {
        fates: fates.iter().copied().collect(),
        loss,
        ..Wire::default()
    };
    let mut back = Wire {
        fates: fates.iter().rev().copied().collect(),
        loss,
        ..Wire::default()
    };
    let start = Instant::now();
    let mut delivered = Vec::new();
    let mut waiting: VecDeque<&Vec<u8>> = messages.iter().collect();
    for step in 0..20_000u32 {
        let now = start + STEP * step;
        while let Some(message) = waiting.front() {
            if a.send(message).is_err() {
                break;
            }
            waiting.pop_front();
        }
        if !a.poll(now, 32, |p| there.put(step, p)) {
            return (delivered, true);
        }
        for packet in there.arrived(step) {
            b.receive(&packet, |m| delivered.push(m.to_vec())).unwrap();
        }
        assert!(b.poll(now, 32, |p| back.put(step, p)));
        for packet in back.arrived(step) {
            a.receive(&packet, |_| unreachable!("b sends no messages"))
                .unwrap();
        }
        if waiting.is_empty() && a.is_idle() && delivered.len() == messages.len() {
            break;
        }
    }
    (delivered, false)
}

#[test]
fn a_message_over_a_clean_wire_arrives_whole_in_its_fragments() {
    let message: Vec<u8> = (0..MAX_SEALED)
        .map(|i| u8::try_from(i % 251).unwrap())
        .collect();
    let (delivered, gone) = carry(&[message.clone(), Vec::new(), vec![9]], &[50], 0);
    assert!(!gone);
    assert_eq!(delivered, [message, Vec::new(), vec![9]]);
}

#[test]
fn a_peer_that_never_answers_is_given_up_on() {
    let mut link = Link::new(PACKET).unwrap();
    link.send(b"hello").unwrap();
    let start = Instant::now();
    let alive = (0..1000u32).all(|step| link.poll(start + STEP * step, 8, |_| {}));
    assert!(!alive);
}

#[test]
fn packets_past_a_bound_are_refused_and_change_nothing() {
    let mut link = Link::new(PACKET).unwrap();
    let mut packet = Vec::new();
    let head = |ack, flags| Head {
        version: LINK,
        ack,
        acks: 0,
        seq: 0,
        flags,
    };
    let long = write(&mut packet, &head(0, DATA | LAST), &[0; PACKET]).to_vec();
    assert_eq!(link.receive(&long, |_| {}), Err(ReadPacketError::TooLarge));
    let unsent = write(&mut packet, &head(5, 0), &[]).to_vec();
    assert_eq!(
        link.receive(&unsent, |_| {}),
        Err(ReadPacketError::Unreadable)
    );
    assert_eq!(
        link.receive(&[LINK + 1], |_| {}),
        Err(ReadPacketError::Version(2))
    );
    assert!(link.is_idle() && !link.ack_due);
}

#[test]
fn a_message_gathered_past_the_largest_is_refused() {
    let mut link = Link::new(PACKET).unwrap();
    let mut packet = Vec::new();
    let fragment = vec![1; PACKET - MAX_LINK_HEADER];
    let mut refused = false;
    for seq in 0..=u32::try_from(MAX_SEALED / fragment.len()).unwrap() + 1 {
        let head = Head {
            version: LINK,
            ack: 0,
            acks: 0,
            seq,
            flags: DATA,
        };
        let bytes = write(&mut packet, &head, &fragment).to_vec();
        refused |= link.receive(&bytes, |_| panic!("no last fragment came"))
            == Err(ReadPacketError::TooLarge);
    }
    assert!(refused);
}

#[test]
fn a_full_backlog_refuses_more() {
    let mut link = Link::new(MAX_LINK_HEADER + 1).unwrap();
    assert!(link.send(&[0; MAX_SEALED + 1]).is_err());
    while link.send(&[0; 100]).is_ok() {}
    assert!(link.backlog() <= BACKLOG && link.backlog() > BACKLOG - 100);
}

proptest! {
    #[test]
    fn every_message_arrives_once_and_in_order_over_a_lossy_wire(
        messages in proptest::collection::vec(
            proptest::collection::vec(any::<u8>(), 0..=MAX_SEALED),
            1..12,
        ),
        fates in proptest::collection::vec(any::<u8>(), 1..64),
        loss in 0u8..30,
    ) {
        let (delivered, gone) = carry(&messages, &fates, loss);
        prop_assert!(!gone);
        prop_assert_eq!(delivered, messages);
    }

    #[test]
    fn any_packet_is_read_within_the_bounds_or_refused(
        ours in any::<bool>(),
        mut packet in proptest::collection::vec(any::<u8>(), 0..PACKET + 8),
    ) {
        if ours {
            packet.insert(0, LINK);
        }
        let mut link = Link::new(PACKET).unwrap();
        let _read = link.receive(&packet, |message| assert!(message.len() <= MAX_SEALED));
        prop_assert!(link.poll(Instant::now(), 4, |p| assert!(p.len() <= PACKET)));
    }
}
