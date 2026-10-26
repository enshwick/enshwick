//! Frames: a message cut into base64 chat lines and put back together.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

use super::peer::Peer;
use super::{MAX_PARTS, PART_BYTES, PREFIX, PROTOCOL};

/// A frame's header: protocol, channel, message id, part and part count.
const HEADER: usize = 1 + 4 + 2 + 1 + 1;

/// Parts of a message on their way in.
#[derive(Debug, Default)]
pub(crate) struct Partial {
    /// How many parts the message takes.
    pub(crate) total: u8,
    /// The parts that came, by index.
    pub(crate) parts: Vec<Option<Vec<u8>>>,
}

/// The frames of one message: base64 lines with the prefix; `None` for a
/// message longer than [`MAX_PARTS`] frames, which no peer takes.
///
/// # Examples
///
/// ```
/// use enshwick_loader::wire::{PREFIX, frames, parse};
///
/// let lines = frames(7, 1, b"data").ok_or("too long")?;
/// let frame = parse(lines[0].strip_prefix(PREFIX).ok_or("no prefix")?).ok_or("unreadable")?;
/// assert_eq!((frame.channel, frame.part.as_slice()), (7, &b"data"[..]));
/// # Ok::<(), &str>(())
/// ```
#[must_use]
pub fn frames(channel: u32, message: u16, data: &[u8]) -> Option<Vec<String>> {
    let parts: Vec<&[u8]> = if data.is_empty() {
        vec![&[]]
    } else {
        data.chunks(PART_BYTES).collect()
    };
    if parts.len() > MAX_PARTS {
        return None;
    }
    let total = u8::try_from(parts.len()).ok()?;
    let lines = parts
        .iter()
        .enumerate()
        .map(|(index, part)| {
            let mut bytes = Vec::with_capacity(HEADER + part.len());
            bytes.push(PROTOCOL);
            bytes.extend_from_slice(&channel.to_le_bytes());
            bytes.extend_from_slice(&message.to_le_bytes());
            bytes.push(u8::try_from(index).unwrap_or(u8::MAX));
            bytes.push(total);
            bytes.extend_from_slice(part);
            format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes))
        })
        .collect();
    Some(lines)
}

/// A frame's header and part.
///
/// # Examples
///
/// See [`frames`].
#[derive(Debug, PartialEq, Eq)]
pub struct Frame {
    /// The channel, [`CONTROL`](super::CONTROL) for the protocol's own.
    pub channel: u32,
    /// The message's id on its channel.
    pub message: u16,
    /// This part's index, below `total`.
    pub index: u8,
    /// How many parts the message takes, from 1 to [`MAX_PARTS`].
    pub total: u8,
    /// This part's bytes, at most [`PART_BYTES`].
    pub part: Vec<u8>,
}

/// The frame a line carries, after the prefix; `None` for a line that is no
/// frame of this protocol or whose header does not hold together.
///
/// # Examples
///
/// See [`frames`].
#[must_use]
pub fn parse(text: &str) -> Option<Frame> {
    let bytes = URL_SAFE_NO_PAD.decode(text.trim()).ok()?;
    if bytes.len() < HEADER || bytes[0] != PROTOCOL {
        return None;
    }
    let frame = Frame {
        channel: u32::from_le_bytes(bytes[1..5].try_into().ok()?),
        message: u16::from_le_bytes(bytes[5..7].try_into().ok()?),
        index: bytes[7],
        total: bytes[8],
        part: bytes[HEADER..].to_vec(),
    };
    let fits = frame.total >= 1
        && usize::from(frame.total) <= MAX_PARTS
        && frame.index < frame.total
        && frame.part.len() <= PART_BYTES;
    fits.then_some(frame)
}

/// Adds `frame` from `peer`; the whole message once its last part came.
///
/// # Examples
///
/// ```
/// use enshwick_loader::wire::{PREFIX, Peer, assemble, frames, parse};
///
/// let data = vec![7u8; 2000];
/// let mut peer = Peer::new();
/// let mut whole = None;
/// for line in frames(3, 1, &data).ok_or("too long")? {
///     let frame = parse(line.strip_prefix(PREFIX).ok_or("no prefix")?).ok_or("unreadable")?;
///     whole = assemble(&mut peer, frame);
/// }
/// assert_eq!(whole, Some((3, data)));
/// # Ok::<(), &str>(())
/// ```
pub fn assemble(peer: &mut Peer, frame: Frame) -> Option<(u32, Vec<u8>)> {
    let key = (frame.channel, frame.message);
    if frame.total == 1 {
        peer.partial.remove(&key);
        return Some((frame.channel, frame.part));
    }
    let partial = peer.partial.entry(key).or_default();
    if partial.total != frame.total {
        *partial = Partial {
            total: frame.total,
            parts: vec![None; usize::from(frame.total)],
        };
    }
    partial.parts[usize::from(frame.index)] = Some(frame.part);
    if partial.parts.iter().any(Option::is_none) {
        if peer.partial.len() > MAX_PARTS * 4 {
            peer.partial.clear();
        }
        return None;
    }
    let message = peer.partial.remove(&key)?;
    Some((
        frame.channel,
        message.parts.into_iter().flatten().flatten().collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::super::{CONTROL, MAX_CONTROL, MAX_MESSAGE, NOTICE, control, is_frame};
    use super::*;

    #[test]
    fn a_message_goes_out_in_frames_and_comes_back_whole() {
        let data: Vec<u8> = (0..2000u32).map(|i| (i % 251) as u8).collect();
        let lines = frames(7, 3, &data).unwrap();
        assert_eq!(lines.len(), 3);
        assert!(
            lines
                .iter()
                .all(|l| l.len() <= 1024 && is_frame(l.as_bytes()))
        );
        let mut peer = Peer::new();
        let mut whole = None;
        for line in lines.iter().rev() {
            let frame = parse(line.strip_prefix(PREFIX).unwrap()).unwrap();
            whole = assemble(&mut peer, frame);
        }
        assert_eq!(whole, Some((7, data)));
        assert!(peer.partial.is_empty());
    }

    #[test]
    fn the_largest_message_fits_its_frames_and_malformed_frames_are_dropped() {
        assert!(frames(1, 0, &[0; MAX_MESSAGE]).unwrap().len() <= MAX_PARTS);
        assert_eq!(
            frames(1, 0, &[0; PART_BYTES * MAX_PARTS]).unwrap().len(),
            MAX_PARTS
        );
        assert!(frames(1, 0, &[0; PART_BYTES * MAX_PARTS + 1]).is_none());
        let mut long = control(NOTICE);
        long.extend_from_slice(&[b'a'; MAX_CONTROL + 1]);
        assert!(frames(CONTROL, 1, &long).is_none());
        assert!(parse("not base64 !").is_none());
        assert!(parse(&URL_SAFE_NO_PAD.encode([9u8; HEADER])).is_none());
        let mut bad = vec![PROTOCOL, 1, 0, 0, 0, 0, 0, 3, 2];
        bad.push(0);
        assert!(parse(&URL_SAFE_NO_PAD.encode(&bad)).is_none());
    }
}
