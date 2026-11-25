//! The control message that carries a server mod's sound to a client: a
//! start or a change of one, for the mod named by the hash of its id.
//!
//! After the control kind and version: the op (0 a start, 1 a change), the
//! mod's id hash and the sound's id, then for a start the container, the
//! place, the position as three `f32` metres, the volume and the fade in
//! milliseconds, and for a change its kind, volume, fade and position.
//! Every number is little-endian.

use enshwick_core::abi::{SoundPlay, SoundSteer};
use enshwick_core::content::Guid;

use super::{SOUND, control};

/// The longest sound message, in bytes.
pub const MAX_SOUND: usize = 64;

const PLAY: u8 = 0;
const STEER: u8 = 1;
/// Op, mod hash and sound id.
const HEAD: usize = 1 + 4 + 8;
/// Container, place, position, volume, fade.
const PLAY_BODY: usize = 16 + 1 + 12 + 4 + 4;
/// Kind, volume, fade, position.
const STEER_BODY: usize = 1 + 4 + 4 + 12;

/// What a sound message carries.
#[derive(Clone, Copy, Debug)]
pub enum SoundOp {
    /// A start: the place is [`SoundPlay::SCREEN`], [`SoundPlay::AT`] or
    /// [`SoundPlay::MUSIC`], since the server's entities name none of the
    /// client's.
    Play(SoundPlay),
    /// A change of a sound the server sent before.
    Steer(SoundSteer),
}

/// A sound message: the mod's id hash, the sound's id on the server and
/// what to do.
#[derive(Clone, Copy, Debug)]
pub struct SoundMessage {
    /// The FNV-1a hash of the mod's id.
    pub owner: u32,
    /// The sound's id, which the server counts per mod.
    pub id: u64,
    /// What to do.
    pub op: SoundOp,
}

/// The largest `f32` below 2^31 metres, the edge of
/// [`in_fixed_range`](enshwick_core::abi::in_fixed_range): `f32` rounds the
/// last 64 metres below 2^31 up to it, which the client refuses.
const EDGE: f64 = 2_147_483_520.0;

/// `at` as the message's `f32` metres, a place the server took kept one
/// the client takes.
fn narrow(at: [f64; 3]) -> [f32; 3] {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a millimetre at 4 km is all a sound's place needs"
    )]
    at.map(|axis| axis.clamp(-EDGE, EDGE) as f32)
}

/// `message` as a control message, at most [`MAX_SOUND`] bytes. A place or
/// kind past a byte is sent as 255, which the client refuses.
///
/// # Examples
///
/// ```
/// use enshwick_core::abi::SoundPlay;
/// use enshwick_loader::base::wire::{MAX_SOUND, SoundMessage, SoundOp, read_sound, sound};
///
/// let start = SoundMessage { owner: 7, id: 1, op: SoundOp::Play(SoundPlay { place: SoundPlay::AT, at: [1.0, 2.0, 3.0], ..SoundPlay::EMPTY }) };
/// let data = sound(&start);
/// assert!(data.len() <= MAX_SOUND);
/// let read = read_sound(&data[2..]).ok_or("unread")?;
/// assert!(matches!(read.op, SoundOp::Play(play) if play.at == [1.0, 2.0, 3.0]));
/// # Ok::<(), &str>(())
/// ```
#[must_use]
pub fn sound(message: &SoundMessage) -> Vec<u8> {
    let byte = |value: u32| u8::try_from(value).unwrap_or(u8::MAX);
    let mut data = control(SOUND);
    data.push(match message.op {
        SoundOp::Play(_) => PLAY,
        SoundOp::Steer(_) => STEER,
    });
    data.extend_from_slice(&message.owner.to_le_bytes());
    data.extend_from_slice(&message.id.to_le_bytes());
    let put_at = |data: &mut Vec<u8>, at: [f64; 3]| {
        for axis in narrow(at) {
            data.extend_from_slice(&axis.to_le_bytes());
        }
    };
    match message.op {
        SoundOp::Play(play) => {
            data.extend_from_slice(&play.container.0);
            data.push(byte(play.place));
            put_at(&mut data, play.at);
            data.extend_from_slice(&play.volume.to_le_bytes());
            data.extend_from_slice(&play.fade_in_ms.to_le_bytes());
        }
        SoundOp::Steer(steer) => {
            data.push(byte(steer.kind));
            data.extend_from_slice(&steer.volume.to_le_bytes());
            data.extend_from_slice(&steer.fade_ms.to_le_bytes());
            put_at(&mut data, steer.at);
        }
    }
    data
}

/// Reads `N` bytes at `at` of `bytes`, which the caller sized.
fn take<const N: usize>(bytes: &[u8], at: usize) -> [u8; N] {
    let mut out = [0; N];
    out.copy_from_slice(&bytes[at..at + N]);
    out
}

/// Three `f32` metres at `at` of `bytes`, which the caller sized.
fn position(bytes: &[u8], at: usize) -> [f64; 3] {
    [0, 4, 8].map(|step| f64::from(f32::from_le_bytes(take(bytes, at + step))))
}

/// A sound message after its control kind and version, or `None` for one
/// of the wrong length or an unknown op. Its numbers are not checked: the
/// client's start and change refuse what is out of bounds.
///
/// # Examples
///
/// ```
/// use enshwick_loader::base::wire::read_sound;
///
/// assert!(read_sound(&[0; 13]).is_none());
/// assert!(read_sound(&[9; 50]).is_none());
/// ```
#[must_use]
pub fn read_sound(bytes: &[u8]) -> Option<SoundMessage> {
    if bytes.len() < HEAD {
        return None;
    }
    let owner = u32::from_le_bytes(take(bytes, 1));
    let id = u64::from_le_bytes(take(bytes, 5));
    let body = &bytes[HEAD..];
    let op = match bytes[0] {
        PLAY if body.len() == PLAY_BODY => SoundOp::Play(SoundPlay {
            container: Guid(take(body, 0)),
            place: u32::from(body[16]),
            at: position(body, 17),
            volume: f32::from_le_bytes(take(body, 29)),
            fade_in_ms: u32::from_le_bytes(take(body, 33)),
            ..SoundPlay::EMPTY
        }),
        // A server changes only the one sound it names, never the client
        // part's own sounds.
        STEER if body.len() == STEER_BODY && u32::from(body[0]) != SoundSteer::STOP_ALL => {
            SoundOp::Steer(SoundSteer {
                kind: u32::from(body[0]),
                volume: f32::from_le_bytes(take(body, 1)),
                fade_ms: u32::from_le_bytes(take(body, 5)),
                at: position(body, 9),
                ..SoundSteer::EMPTY
            })
        }
        _ => return None,
    };
    Some(SoundMessage { owner, id, op })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_start_and_a_change_read_back_as_they_were_sent() {
        let play = SoundPlay {
            container: Guid([3; 16]),
            place: SoundPlay::MUSIC,
            at: [-4.5, 1e3, 0.25],
            volume: 0.5,
            fade_in_ms: 1500,
            ..SoundPlay::EMPTY
        };
        let data = sound(&SoundMessage {
            owner: 0xdead_beef,
            id: u64::MAX,
            op: SoundOp::Play(play),
        });
        assert_eq!(data.len(), 2 + HEAD + PLAY_BODY);
        let read = read_sound(&data[2..]).expect("a start");
        assert_eq!((read.owner, read.id), (0xdead_beef, u64::MAX));
        let SoundOp::Play(got) = read.op else {
            panic!("a start")
        };
        assert_eq!(
            (got.container, got.place, got.at, got.volume, got.fade_in_ms),
            (
                play.container,
                play.place,
                play.at,
                play.volume,
                play.fade_in_ms
            )
        );
        let steer = SoundSteer {
            kind: SoundSteer::POSITION,
            volume: 0.1,
            fade_ms: 9,
            at: [1.0, 2.0, 3.0],
            ..SoundSteer::EMPTY
        };
        let data = sound(&SoundMessage {
            owner: 1,
            id: 2,
            op: SoundOp::Steer(steer),
        });
        assert!(data.len() <= MAX_SOUND);
        let SoundOp::Steer(got) = read_sound(&data[2..]).expect("a change").op else {
            panic!("a change")
        };
        assert_eq!(
            (got.kind, got.volume, got.fade_ms, got.at),
            (steer.kind, steer.volume, steer.fade_ms, steer.at)
        );
    }

    #[test]
    fn a_place_the_server_took_near_the_fixed_point_s_edge_reads_as_one_the_client_takes() {
        let near = 2_147_483_600.0;
        #[expect(clippy::cast_possible_truncation, reason = "the rounding under test")]
        let rounded = near as f32;
        assert_eq!(rounded, 2_147_483_648.0, "f32 rounds it up to 2^31");
        let at = [near, -near, 0.0];
        let play = SoundPlay {
            place: SoundPlay::AT,
            at,
            ..SoundPlay::EMPTY
        };
        assert!(play.in_bounds());
        let data = sound(&SoundMessage {
            owner: 1,
            id: 1,
            op: SoundOp::Play(play),
        });
        let SoundOp::Play(got) = read_sound(&data[2..]).expect("a start").op else {
            panic!("a start")
        };
        assert!(got.in_bounds(), "{:?}", got.at);
        assert_eq!(got.at, [EDGE, -EDGE, 0.0]);
        let steer = SoundSteer {
            kind: SoundSteer::POSITION,
            at,
            ..SoundSteer::EMPTY
        };
        let data = sound(&SoundMessage {
            owner: 1,
            id: 2,
            op: SoundOp::Steer(steer),
        });
        let SoundOp::Steer(got) = read_sound(&data[2..]).expect("a change").op else {
            panic!("a change")
        };
        assert!(got.in_bounds(), "{:?}", got.at);
    }

    #[test]
    fn a_place_past_a_byte_goes_as_one_the_client_refuses() {
        let play = SoundPlay {
            place: 300,
            ..SoundPlay::EMPTY
        };
        let data = sound(&SoundMessage {
            owner: 1,
            id: 1,
            op: SoundOp::Play(play),
        });
        let SoundOp::Play(got) = read_sound(&data[2..]).expect("a start").op else {
            panic!("a start")
        };
        assert!(!got.in_bounds());
    }

    #[test]
    fn a_change_that_stops_every_sound_of_the_mod_is_not_read() {
        let data = sound(&SoundMessage {
            owner: 1,
            id: 1,
            op: SoundOp::Steer(SoundSteer {
                kind: SoundSteer::STOP_ALL,
                ..SoundSteer::EMPTY
            }),
        });
        assert!(read_sound(&data[2..]).is_none());
    }

    #[test]
    fn a_cut_or_padded_message_is_not_read() {
        let data = sound(&SoundMessage {
            owner: 1,
            id: 1,
            op: SoundOp::Steer(SoundSteer::EMPTY),
        });
        assert!(read_sound(&data[2..data.len() - 1]).is_none());
        let mut padded = data[2..].to_vec();
        padded.push(0);
        assert!(read_sound(&padded).is_none());
    }
}
