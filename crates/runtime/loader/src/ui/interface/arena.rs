//! The frame arena: mod texts the game reads for the next frames, each
//! frame's in its own buffer, with the game's label markup escaped.

use super::engine::UiText;

/// Frames a text copied into the arena stays readable.
const ARENA_FRAMES: usize = 3;
/// Bytes of text per frame.
const ARENA_BYTES: usize = 64 * 1024;

/// Appends `text` to `out` with each backslash doubled: the game's label
/// markup reads `\` with the character after it, and shows `\\` as one
/// backslash (client 26430625).
pub(super) fn escape(out: &mut Vec<u8>, text: &[u8]) {
    let mut from = 0;
    for at in memchr::memchr_iter(b'\\', text) {
        out.extend_from_slice(&text[from..=at]);
        out.push(b'\\');
        from = at + 1;
    }
    out.extend_from_slice(&text[from..]);
}

/// The bytes [`escape`] adds to `text`.
fn escaped_extra(text: &[u8]) -> usize {
    memchr::memchr_iter(b'\\', text).count()
}

/// Mod texts of the last frames, each frame's in its own buffer.
pub(super) struct Arena {
    buffers: Vec<Vec<u8>>,
    current: usize,
}

impl Arena {
    pub(super) const fn new() -> Self {
        Self {
            buffers: Vec::new(),
            current: 0,
        }
    }

    pub(super) fn next_frame(&mut self) {
        if self.buffers.is_empty() {
            self.buffers = (0..ARENA_FRAMES)
                .map(|_| Vec::with_capacity(ARENA_BYTES))
                .collect();
        }
        self.current = (self.current + 1) % ARENA_FRAMES;
        if let Some(buffer) = self.buffers.get_mut(self.current) {
            buffer.clear();
        }
    }

    /// `parts` joined, as a text the game may read for the next frames;
    /// empty once the frame's buffer is full; [`escape`]d.
    pub(super) fn text(&mut self, parts: &[&[u8]]) -> UiText {
        let Some(buffer) = self.buffers.get_mut(self.current) else {
            return UiText::EMPTY;
        };
        let len: usize = parts.iter().map(|p| p.len() + escaped_extra(p)).sum();
        if buffer.len() + len > buffer.capacity() {
            return UiText::EMPTY;
        }
        let start = buffer.len();
        for part in parts {
            escape(buffer, part);
        }
        // Within capacity, the buffer never moves; it is cleared only
        // `ARENA_FRAMES` frames later.
        buffer.get(start..).map_or(UiText::EMPTY, UiText::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arena_keeps_texts_until_its_buffer_comes_round() {
        let mut arena = Arena::new();
        arena.next_frame();
        let first = arena.text(&[b"enshwick/", b"lamps"]);
        assert_eq!(first.len, 14);
        // SAFETY: `first` points at 14 bytes the arena just copied into its
        // current buffer, which nothing clears before `next_frame`.
        let bytes = unsafe { core::slice::from_raw_parts(first.ptr, 14) };
        assert_eq!(bytes, b"enshwick/lamps");
        let big = vec![b'x'; ARENA_BYTES];
        assert_eq!(arena.text(&[&big]).len, 0);
        arena.next_frame();
        assert_eq!(arena.text(&[b"next"]).len, 4);
        // SAFETY: one frame on, the arena cleared only the next of its
        // `ARENA_FRAMES` buffers, so the 14 bytes of `first` are untouched.
        let again = unsafe { core::slice::from_raw_parts(first.ptr, 14) };
        assert_eq!(again, b"enshwick/lamps");
    }

    #[test]
    fn the_arena_doubles_each_backslash_so_the_game_shows_a_windows_path_whole() {
        let mut arena = Arena::new();
        arena.next_frame();
        let path = arena.text(&[br"world3-mod-data\enshwick", br"\owners.toml\"]);
        let want = br"world3-mod-data\\enshwick\\owners.toml\\";
        assert_eq!(usize::try_from(path.len), Ok(want.len()));
        // SAFETY: `path` points at `want.len()` bytes the arena just copied
        // into its current buffer, which nothing clears before `next_frame`.
        let bytes = unsafe { core::slice::from_raw_parts(path.ptr, want.len()) };
        assert_eq!(bytes, want);
        arena.next_frame();
        let half = vec![b'\\'; arena.buffers[arena.current].capacity() / 2 + 1];
        let fills = arena.text(&[&half]);
        assert_eq!(fills.len, 0, "backslashes that fit only undoubled");
    }
}
