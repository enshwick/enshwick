//! Text the loader hands the game as the answer to a text lookup: mods'
//! texts, notices and the interface's own.

/// The game's hash of a text view: FNV-1a over 64 bits, each byte
/// sign-extended, as the text format of client 26430625 computes it.
pub(crate) const fn hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut i = 0;
    while i < bytes.len() {
        #[expect(clippy::cast_possible_wrap, reason = "the game sign-extends each byte")]
        let byte = bytes[i] as i8;
        #[expect(clippy::cast_sign_loss, reason = "the game sign-extends each byte")]
        let wide = byte as i64 as u64;
        hash = (hash ^ wide).wrapping_mul(0x100_0000_01b3);
        i += 1;
    }
    hash
}

/// UTF-8 text the loader hands the game as a text lookup's answer: its
/// address and length.
///
/// The bytes stay readable at that address for as long as the game may keep
/// the view; their maker says how. A view may outlive its text: a notice
/// slot is written over in place while the game may still hold an older
/// view of it, which then reads the slot's newer text (or a mix of the two
/// cut short at the older length) but never freed memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Handed {
    at: *const u8,
    len: usize,
    /// The bytes' [`hash`].
    hash: u64,
}

impl Handed {
    /// The view of `len` bytes at `at`, whose [`hash`] is `hash`.
    pub(crate) const fn new(at: *const u8, len: usize, hash: u64) -> Self {
        Self { at, len, hash }
    }

    /// Where the text starts.
    pub(crate) const fn at(self) -> *const u8 {
        self.at
    }

    /// The text's length in bytes.
    pub(crate) const fn bytes(self) -> usize {
        self.len
    }

    /// The text's [`hash`].
    pub(crate) const fn hash(self) -> u64 {
        self.hash
    }

    /// The view of a text that lives for the process.
    pub(crate) const fn of(text: &'static str) -> Self {
        Self::new(text.as_ptr(), text.len(), hash(text.as_bytes()))
    }
}
