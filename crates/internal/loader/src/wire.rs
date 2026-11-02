//! The wire format of the messages between a join mod's server part and its
//! client part: frames in chat lines, and the protocol's own control
//! messages.
//!
//! A frame is base64 of a small header and up to one line's worth of the
//! message; a message of up to [`MAX_MESSAGE`] bytes takes up to
//! [`MAX_PARTS`] frames. Everything here reads what another player's game
//! sent, so it never trusts a size or a count.

mod delivery;
mod envelope;
mod frame;
mod peer;

pub use delivery::{Drops, Outbox, strays};
pub use envelope::{
    ENVELOPE, MAX_HEADER, MAX_PAYLOAD, MAX_SEALED, ReadMessageError, is_executable, open, seal,
    sealed_size,
};
#[cfg(all(windows, test))]
pub(crate) use frame::Partial;
pub use frame::{Frame, assemble, frames, parse};
pub use peer::{DATA_AGAIN, Peer};

use enshwick::i18n::Language;

use crate::admission::Hello;
use crate::remote::Message;

/// The prefix of a frame line, both ways.
pub const PREFIX: &str = "/enshwick-net ";
/// The version of the frame protocol.
pub const PROTOCOL: u8 = 2;
pub use enshwick::net::MAX_MESSAGE;
/// The most frames one message takes.
pub const MAX_PARTS: usize = 8;
/// Message bytes in one frame: a line holds 1024 bytes, less the prefix,
/// as base64 of the header and the part.
pub const PART_BYTES: usize = 720;
/// The longest control message, in bytes, with its kind and version.
pub const MAX_CONTROL: usize = PART_BYTES * MAX_PARTS - 2;
/// The channel of the protocol's own frames.
pub const CONTROL: u32 = 0;
/// A control message from a client: its hello.
pub const HELLO: u8 = 1;
/// A control message from the server: the answer to a hello.
pub const WELCOME: u8 = 2;
/// A control message from the server: a notice for the client to show.
pub const NOTICE: u8 = 3;
/// A control message from the server: item data records.
pub const ITEM_DATA: u8 = 4;
/// A control message from the server: open the interface demo.
pub const UI_DEMO: u8 = 5;
/// Messages each peer may send per second.
pub const RATE: f64 = 20.0;
/// Messages each peer may send in one burst.
pub const BURST: f64 = 40.0;

/// Bit of a hello's flags: the mod list is whole.
const HELLO_COMPLETE: u8 = 1;
/// A hello's flags, host number and language, after its kind and version.
const HELLO_HEAD: usize = 1 + 8 + 4;

/// A control message's start: its kind and the protocol version.
///
/// # Examples
///
/// ```
/// use enshwick_loader::wire::{PROTOCOL, WELCOME, control};
///
/// assert_eq!(control(WELCOME), [WELCOME, PROTOCOL]);
/// ```
#[must_use]
pub fn control(kind: u8) -> Vec<u8> {
    vec![kind, PROTOCOL]
}

/// A client's hello, which names the join mods it runs and the language its
/// player reads the game in.
///
/// The control kind and version, flags, the host number `nonce` (0 unless
/// the client hosts the game), the language, then the mods,
/// `id@version,...`, cut after the last whole entry that fits.
///
/// # Examples
///
/// ```
/// use enshwick::i18n::Language;
/// use enshwick_loader::wire::{hello, read_hello};
///
/// let said = read_hello(&hello("you.zones@1.0.0", 9, Language::DE_DE)).ok_or("no hello")?;
/// assert_eq!((said.mods.as_str(), said.complete, said.nonce), ("you.zones@1.0.0", true, 9));
/// assert_eq!(said.language, Some(Language::DE_DE));
/// # Ok::<(), &str>(())
/// ```
#[must_use]
pub fn hello(mods: &str, nonce: u64, language: Language) -> Vec<u8> {
    let mut data = control(HELLO);
    let room = MAX_CONTROL - data.len() - HELLO_HEAD;
    let kept = if mods.len() <= room {
        mods
    } else {
        let cut = mods.as_bytes()[..=room]
            .iter()
            .rposition(|&b| b == b',')
            .unwrap_or(0);
        &mods[..cut]
    };
    data.push(if kept.len() == mods.len() {
        HELLO_COMPLETE
    } else {
        0
    });
    data.extend_from_slice(&nonce.to_le_bytes());
    data.extend_from_slice(&language.0.to_le_bytes());
    data.extend_from_slice(kept.as_bytes());
    data
}

/// What a hello says; `None` when `data` is no hello. A hello without its
/// head reads as a whole empty list, an unreadable list as empty.
///
/// # Examples
///
/// ```
/// use enshwick_loader::wire::{HELLO, WELCOME, control, read_hello};
///
/// let bare = read_hello(&control(HELLO)).ok_or("no hello")?;
/// assert_eq!((bare.mods.as_str(), bare.complete), ("", true));
/// assert_eq!(read_hello(&control(WELCOME)), None);
/// # Ok::<(), &str>(())
/// ```
#[must_use]
pub fn read_hello(data: &[u8]) -> Option<Hello> {
    let rest = data.strip_prefix(control(HELLO).as_slice())?;
    let Some((&flags, rest)) = rest.split_first() else {
        return Some(Hello {
            complete: true,
            ..Hello::default()
        });
    };
    let (nonce, rest) = rest.split_first_chunk::<8>().unwrap_or((&[0; 8], &[]));
    let (language, mods) = match rest.split_first_chunk::<4>() {
        Some((language, mods)) => {
            let language = Language(u32::from_le_bytes(*language));
            (Language::ALL.contains(&language).then_some(language), mods)
        }
        None => (None, rest),
    };
    Some(Hello {
        mods: core::str::from_utf8(mods).unwrap_or_default().to_owned(),
        complete: flags & HELLO_COMPLETE != 0,
        nonce: u64::from_le_bytes(*nonce),
        language,
    })
}

/// A control message's settings or actions message; `None` for any other
/// control message or another protocol version.
///
/// # Examples
///
/// ```
/// use enshwick_loader::remote::Message;
/// use enshwick_loader::wire::{PROTOCOL, remote_message};
///
/// let query = Message::Query { id: "you.zones".into() };
/// let mut data = vec![query.kind(), PROTOCOL];
/// data.extend(query.body());
/// assert_eq!(remote_message(&data), Some(query));
/// ```
#[must_use]
pub fn remote_message(data: &[u8]) -> Option<Message> {
    let (&kind, rest) = data.split_first()?;
    let (&version, body) = rest.split_first()?;
    if version != PROTOCOL {
        return None;
    }
    Message::read(kind, body)
}

/// A notice message for the player of entity `entity`: the control kind,
/// the entity's id and the text.
///
/// # Examples
///
/// ```
/// use enshwick_loader::wire::{NOTICE, control, notice, read_notice};
///
/// let data = notice(42, "Low fuel");
/// let rest = data.strip_prefix(control(NOTICE).as_slice()).unwrap();
/// assert_eq!(read_notice(rest), Some((42, "Low fuel")));
/// ```
#[must_use]
pub fn notice(entity: u32, text: &str) -> Vec<u8> {
    let mut data = control(NOTICE);
    data.extend_from_slice(&entity.to_le_bytes());
    data.extend_from_slice(text.as_bytes());
    data
}

/// The entity and text of a notice message, after its control kind; `None`
/// when the text is not UTF-8 or the entity is cut short.
///
/// # Examples
///
/// ```
/// use enshwick_loader::wire::read_notice;
///
/// assert_eq!(read_notice(b"\x07\0\0\0hi"), Some((7, "hi")));
/// assert_eq!(read_notice(b"\x07\0\0"), None);
/// assert_eq!(read_notice(b"\x07\0\0\0\xff"), None);
/// ```
#[must_use]
pub fn read_notice(bytes: &[u8]) -> Option<(u32, &str)> {
    let (entity, text) = bytes.split_first_chunk::<4>()?;
    Some((
        u32::from_le_bytes(*entity),
        core::str::from_utf8(text).ok()?,
    ))
}

/// The position a demo message carries: three little-endian `i64`.
///
/// # Examples
///
/// ```
/// use enshwick_loader::wire::demo_position;
///
/// let bytes: Vec<u8> = [1i64, -5, 7].iter().flat_map(|a| a.to_le_bytes()).collect();
/// assert_eq!(demo_position(&bytes), Some([1, -5, 7]));
/// assert_eq!(demo_position(&bytes[..16]), None);
/// ```
#[must_use]
pub fn demo_position(bytes: &[u8]) -> Option<[i64; 3]> {
    let (axes, _) = bytes.as_chunks::<8>();
    match axes {
        [x, y, z, ..] => Some([x, y, z].map(|a| i64::from_le_bytes(*a))),
        _ => None,
    }
}

/// Whether `text` is a frame line, which no chat shows.
///
/// # Examples
///
/// ```
/// use enshwick_loader::wire::is_frame;
///
/// assert!(is_frame(b"/enshwick-net AQ"));
/// assert!(!is_frame(b"hello"));
/// ```
#[must_use]
pub fn is_frame(text: &[u8]) -> bool {
    text.starts_with(PREFIX.as_bytes())
}

/// The frame a frame line carries, after its prefix.
///
/// # Examples
///
/// ```
/// use enshwick_loader::wire::frame_body;
///
/// assert_eq!(frame_body("/enshwick-net AQ "), Some("AQ"));
/// assert_eq!(frame_body("hello"), None);
/// ```
#[must_use]
pub fn frame_body(line: &str) -> Option<&str> {
    line.strip_prefix(PREFIX).map(str::trim)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hello_names_the_join_mods_within_its_frames() {
        let said = read_hello(&hello(
            "you.trading-post@0.3.0,zones@0.1.0",
            9,
            Language::UK_UA,
        ))
        .unwrap();
        assert_eq!(said.mods, "you.trading-post@0.3.0,zones@0.1.0");
        assert!(said.complete);
        assert_eq!(said.nonce, 9);
        assert_eq!(said.language, Some(Language::UK_UA));
        let unknown = read_hello(&hello("", 0, Language(99))).unwrap();
        assert_eq!((unknown.language, unknown.mods.as_str()), (None, ""));
        let bare = read_hello(&control(HELLO)).unwrap();
        assert_eq!((bare.mods.as_str(), bare.complete), ("", true));
        assert_eq!(bare.language, None);
        assert_eq!(read_hello(&control(WELCOME)), None);
        let many: Vec<String> = (0..1000).map(|i| format!("mod-{i:04}@1.0.0")).collect();
        let long = hello(&many.join(","), 0, Language::DE_DE);
        assert_eq!(read_hello(&long).unwrap().language, Some(Language::DE_DE));
        assert!(frames(CONTROL, 1, &long).is_some());
        let cut = read_hello(&long).unwrap();
        assert!(!cut.complete, "a cut list says so");
        assert!(
            cut.mods.split(',').all(|entry| entry.ends_with("@1.0.0")),
            "cut at an entry"
        );
    }

    #[test]
    fn a_demo_frame_carries_a_position_or_none() {
        let mut data = Vec::new();
        for axis in [1i64 << 32, -5, 7] {
            data.extend_from_slice(&axis.to_le_bytes());
        }
        assert_eq!(demo_position(&data), Some([1 << 32, -5, 7]));
        assert_eq!(demo_position(&data[..16]), None);
        assert_eq!(demo_position(&[]), None);
    }

    #[test]
    fn a_settings_message_needs_this_protocol_version() {
        let query = Message::Query {
            id: "you.zones".into(),
        };
        let mut data = vec![query.kind(), PROTOCOL + 1];
        data.extend(query.body());
        assert_eq!(remote_message(&data), None);
        assert_eq!(remote_message(&[query.kind()]), None);
    }
}
