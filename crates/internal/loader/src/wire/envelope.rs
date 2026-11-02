//! The envelope of a message on Enshwick's own socket of the game's
//! connection: a postcard header naming the envelope version and the
//! channel, then the payload.
//!
//! The game's message socket keeps each message apart with its length, so
//! the envelope needs no length or parts. Everything [`open`] reads comes
//! from another player's game: it checks every size, borrows the payload
//! and allocates nothing.

use serde::{Deserialize, Serialize};

/// The version of the envelope, its first byte.
pub const ENVELOPE: u8 = 1;
/// The largest payload one message carries, in bytes.
pub const MAX_PAYLOAD: usize = 8192;
/// The longest header: the version and a channel id as a postcard varint.
pub const MAX_HEADER: usize = 1 + 5;
/// The longest sealed message, in bytes.
pub const MAX_SEALED: usize = MAX_HEADER + MAX_PAYLOAD;

#[derive(Serialize, Deserialize)]
struct Header {
    version: u8,
    channel: u32,
}

/// Why a received message was refused.
///
/// # Examples
///
/// ```
/// use enshwick_loader::wire::{ReadMessageError, open};
///
/// assert_eq!(open(&[]), Err(ReadMessageError::Unreadable));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReadMessageError {
    /// The message is over [`MAX_SEALED`] bytes, or its payload over
    /// [`MAX_PAYLOAD`].
    #[error("a message over {MAX_PAYLOAD} bytes of payload")]
    TooLarge,
    /// The header does not read.
    #[error("an unreadable message header")]
    Unreadable,
    /// Another envelope version.
    #[error("envelope version {0}")]
    Version(u8),
    /// The payload starts like executable code.
    #[error("a payload that starts like executable code")]
    Executable,
}

/// Whether `payload` starts like executable code, which Enshwick never
/// sends or takes.
///
/// # Examples
///
/// ```
/// use enshwick_loader::wire::is_executable;
///
/// assert!(is_executable(b"\x7fELF\x02"));
/// assert!(!is_executable(b"\x02hello"));
/// ```
#[must_use]
pub fn is_executable(payload: &[u8]) -> bool {
    enshwick::net::looks_like_code(payload)
}

/// Whether `payload` may travel at all: within [`MAX_PAYLOAD`] and not
/// starting like executable code.
///
/// # Examples
///
/// ```
/// use enshwick_loader::wire::{MAX_PAYLOAD, sealable};
///
/// assert!(sealable(b"\x02hello"));
/// assert!(!sealable(b"\0asm\x01\0\0\0"));
/// assert!(!sealable(&[0; MAX_PAYLOAD + 1]));
/// ```
#[must_use]
pub fn sealable(payload: &[u8]) -> bool {
    payload.len() <= MAX_PAYLOAD && !is_executable(payload)
}

/// The channel and payload of a received message, the payload borrowed
/// from `message`.
///
/// # Errors
///
/// A [`ReadMessageError`] for a message too long, unreadable, of another
/// envelope version or with an executable payload.
///
/// # Examples
///
/// ```
/// use enshwick_loader::wire::{MAX_SEALED, open, seal};
///
/// let mut out = [0; MAX_SEALED];
/// let size = seal(&mut out, 7, b"data").ok_or("too long")?;
/// assert_eq!(open(&out[..size]), Ok((7, &b"data"[..])));
/// # Ok::<(), &str>(())
/// ```
pub fn open(message: &[u8]) -> Result<(u32, &[u8]), ReadMessageError> {
    if message.len() > MAX_SEALED {
        return Err(ReadMessageError::TooLarge);
    }
    match message.first() {
        Some(&ENVELOPE) => {}
        Some(&version) => return Err(ReadMessageError::Version(version)),
        None => return Err(ReadMessageError::Unreadable),
    }
    let (header, payload) =
        postcard::take_from_bytes::<Header>(message).map_err(|_| ReadMessageError::Unreadable)?;
    if payload.len() > MAX_PAYLOAD {
        return Err(ReadMessageError::TooLarge);
    }
    if is_executable(payload) {
        return Err(ReadMessageError::Executable);
    }
    Ok((header.channel, payload))
}

/// How many bytes [`seal`] writes for a payload of `payload` bytes on
/// `channel`, to size the game's send buffer.
///
/// # Examples
///
/// ```
/// use enshwick_loader::wire::sealed_size;
///
/// assert_eq!(sealed_size(7, 4), 6);
/// assert_eq!(sealed_size(u32::MAX, 0), 6);
/// ```
#[must_use]
pub const fn sealed_size(channel: u32, payload: usize) -> usize {
    let varint = match channel {
        0..0x80 => 1,
        0x80..0x4000 => 2,
        0x4000..0x20_0000 => 3,
        0x20_0000..0x1000_0000 => 4,
        _ => 5,
    };
    1 + varint + payload
}

/// Writes the message of `payload` on `channel` into `out`; the bytes
/// written, or `None` when `out` is too short, the payload over
/// [`MAX_PAYLOAD`] or executable.
///
/// # Examples
///
/// See [`open`].
#[must_use]
pub fn seal(out: &mut [u8], channel: u32, payload: &[u8]) -> Option<usize> {
    if !sealable(payload) {
        return None;
    }
    let header = Header {
        version: ENVELOPE,
        channel,
    };
    let head = postcard::to_slice(&header, out).ok()?.len();
    let size = head + payload.len();
    out.get_mut(head..size)?.copy_from_slice(payload);
    Some(size)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn a_payload_that_starts_like_code_is_refused_both_ways() {
        let mut out = [0; MAX_SEALED];
        let mut pe = vec![0u8; 0x48];
        pe[..2].copy_from_slice(b"MZ");
        pe[0x3c] = 0x40;
        pe[0x40..0x44].copy_from_slice(b"PE\0\0");
        assert!(!is_executable(b"MZ, a string and no PE header"));
        let starts: [&[u8]; 4] = [&pe, b"\x7fELF", b"\0asm", b"\x1bLua"];
        for start in starts {
            assert_eq!(seal(&mut out, 1, start), None);
            let mut message = vec![ENVELOPE, 1];
            message.extend_from_slice(start);
            assert_eq!(open(&message), Err(ReadMessageError::Executable));
        }
    }

    #[test]
    fn sizes_past_the_bounds_are_refused() {
        let mut out = vec![0; MAX_SEALED + 1];
        assert!(seal(&mut out, u32::MAX, &[0; MAX_PAYLOAD]).is_some());
        assert_eq!(seal(&mut out, 1, &[0; MAX_PAYLOAD + 1]), None);
        assert_eq!(seal(&mut out[..3], 1, b"data"), None);
        let mut long = vec![ENVELOPE, 1];
        long.resize(2 + MAX_PAYLOAD + 1, 0);
        assert_eq!(open(&long), Err(ReadMessageError::TooLarge));
        assert_eq!(open(&[0; MAX_SEALED + 1]), Err(ReadMessageError::TooLarge));
        assert_eq!(open(&[ENVELOPE + 1, 1]), Err(ReadMessageError::Version(2)));
        assert_eq!(open(&[ENVELOPE, 0x80]), Err(ReadMessageError::Unreadable));
    }

    proptest! {
        #[test]
        fn a_sealed_message_opens_to_its_channel_and_payload(
            channel in any::<u32>(),
            payload in proptest::collection::vec(any::<u8>(), 0..=MAX_PAYLOAD),
        ) {
            prop_assume!(!is_executable(&payload));
            let mut out = [0; MAX_SEALED];
            let size = seal(&mut out, channel, &payload).unwrap();
            prop_assert_eq!(size, sealed_size(channel, payload.len()));
            prop_assert_eq!(open(&out[..size]), Ok((channel, payload.as_slice())));
        }

        #[test]
        fn any_bytes_open_within_the_bounds_or_are_refused(
            ours in any::<bool>(),
            mut message in proptest::collection::vec(any::<u8>(), 0..64),
        ) {
            if ours {
                message.insert(0, ENVELOPE);
            }
            if let Ok((_, payload)) = open(&message) {
                prop_assert!(payload.len() <= MAX_PAYLOAD);
                prop_assert!(!is_executable(payload));
            }
        }
    }
}
