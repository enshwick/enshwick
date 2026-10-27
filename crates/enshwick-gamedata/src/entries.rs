//! A table's entries: symbols and constants, and how each is found by its
//! pattern.

use enshwick::code::Pattern;

/// Where a symbol lies relative to its pattern's match.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::Locate;
///
/// // A call at offset 4 of the match: its rel32 at 5, the next instruction at 9.
/// let call = Locate::Rel { at: 5, end: 9 };
/// assert_ne!(call, Locate::At(0));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Locate {
    /// The symbol is the code this many bytes past the match's start.
    At(u64),
    /// The symbol is what a rel32 displacement in the match points at: a
    /// call or jump target, or a RIP-relative operand, as for data or for a
    /// function the game holds several identical copies of.
    Rel {
        /// The displacement's offset in the match.
        at: u64,
        /// The offset in the match where its instruction ends, which the
        /// displacement counts from.
        end: u64,
    },
}

/// A pinned engine function or address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    pub(crate) id: String,
    pub(crate) rva: u64,
    pub(crate) pattern: Option<Pattern>,
    pub(crate) locate: Locate,
}

impl Symbol {
    /// The symbolic id, such as `npc.lookup`.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The address relative to the image base.
    #[must_use]
    pub const fn rva(&self) -> u64 {
        self.rva
    }

    /// Code that finds the symbol on another build, and where the symbol
    /// lies from its match.
    #[must_use]
    pub const fn pattern(&self) -> Option<(&Pattern, Locate)> {
        match &self.pattern {
            Some(pattern) => Some((pattern, self.locate)),
            None => None,
        }
    }

    /// The symbol's address relative to the image base as its pattern finds
    /// it in `code`, the code section that starts at `code_rva`; `None`
    /// without a pattern or a unique match.
    #[must_use]
    pub fn find(&self, code: &[u8], code_rva: u64) -> Option<u64> {
        let pattern = self.pattern.as_ref()?;
        let start = u64::try_from(pattern.find_unique(code).ok()?).ok()?;
        let base = code_rva.checked_add(start)?;
        match self.locate {
            Locate::At(at) => base.checked_add(at),
            Locate::Rel { at, end } => {
                let disp = u32::try_from(read(code, start.checked_add(at)?, 4)?).ok()?;
                base.checked_add(end)?
                    .checked_add_signed(i64::from(i32::from_le_bytes(disp.to_le_bytes())))
            }
        }
    }
}

/// The `size` little-endian bytes of `code` at `at`, zero-extended.
pub(crate) fn read(code: &[u8], at: u64, size: u8) -> Option<u64> {
    let at = usize::try_from(at).ok()?;
    let bytes = code.get(at..at.checked_add(usize::from(size))?)?;
    let mut word = [0u8; 8];
    word.get_mut(..bytes.len())?.copy_from_slice(bytes);
    Some(u64::from_le_bytes(word))
}

/// Where a constant's pattern reads its value: the displacement or
/// immediate of the instruction that uses it.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::Operand;
///
/// let disp8 = Operand::new(3, 1).ok_or("a bad size")?;
/// assert_eq!((disp8.at(), disp8.size()), (3, 1));
/// assert_eq!(disp8.read(&[0x48, 0x8b, 0x41, 0x46], 0), Some(0x46));
/// let below_rbp = disp8.negative();
/// assert_eq!(below_rbp.read(&[0x48, 0x8d, 0x55, 0xa0], 0), Some(0x60));
/// assert_eq!(disp8.plus(8).read(&[0x48, 0x8b, 0x41, 0x46], 0), Some(0x4e));
/// // shl r9, 6: a size of 0x40 that only appears as a shift.
/// assert_eq!(disp8.shift().read(&[0x49, 0xc1, 0xe1, 0x06], 0), Some(0x40));
/// assert!(Operand::new(3, 3).is_none());
/// # Ok::<(), &str>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Operand {
    pub(crate) at: u64,
    pub(crate) size: u8,
    pub(crate) negative: bool,
    pub(crate) shift: bool,
    pub(crate) plus: i64,
}

impl Operand {
    /// The operand of `size` bytes, 1, 2, 4 or 8, `at` bytes past the
    /// match's start, read as it is.
    #[must_use]
    pub const fn new(at: u64, size: u8) -> Option<Self> {
        match size {
            1 | 2 | 4 | 8 => Some(Self {
                at,
                size,
                negative: false,
                shift: false,
                plus: 0,
            }),
            _ => None,
        }
    }

    /// The operand read as the value negated, for a frame slot the code
    /// reaches as `[rbp - value]`.
    #[must_use]
    pub const fn negative(self) -> Self {
        Self {
            negative: true,
            ..self
        }
    }

    /// The operand read as a shift count, the value being `1 << count`, for
    /// a size the code only multiplies by as `shl reg, count`.
    #[must_use]
    pub const fn shift(self) -> Self {
        Self {
            shift: true,
            ..self
        }
    }

    /// The operand read with `plus` added, for a value the code only reaches
    /// next to it, such as a size from the offset of the last field.
    #[must_use]
    pub const fn plus(self, plus: i64) -> Self {
        Self { plus, ..self }
    }

    /// Its offset in the match.
    #[must_use]
    pub const fn at(self) -> u64 {
        self.at
    }

    /// Its size in bytes.
    #[must_use]
    pub const fn size(self) -> u8 {
        self.size
    }

    /// Whether it is read negated.
    #[must_use]
    pub const fn is_negative(self) -> bool {
        self.negative
    }

    /// Whether it is read as a shift count.
    #[must_use]
    pub const fn is_shift(self) -> bool {
        self.shift
    }

    /// What is added to it.
    #[must_use]
    pub const fn addend(self) -> i64 {
        self.plus
    }

    /// The value it gives in the match at `start` in `code`: its bytes
    /// zero-extended, sign-extended and negated, or as a shift count, plus
    /// the addend.
    #[must_use]
    pub fn read(self, code: &[u8], start: u64) -> Option<u64> {
        let raw = read(code, start.checked_add(self.at)?, self.size)?;
        let value = if self.negative {
            let unused = 64 - u32::from(self.size) * 8;
            let signed = (raw << unused).cast_signed() >> unused;
            u64::try_from(signed.checked_neg()?).ok()?
        } else if self.shift {
            1u64.checked_shl(u32::try_from(raw).ok()?)?
        } else {
            raw
        };
        value.checked_add_signed(self.plus)
    }
}

/// A pinned engine constant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Constant {
    pub(crate) id: String,
    pub(crate) value: u64,
    pub(crate) pattern: Option<Pattern>,
    pub(crate) operand: Option<Operand>,
    pub(crate) via: Option<String>,
    pub(crate) within: Option<u32>,
    pub(crate) attribute: Option<String>,
    pub(crate) cell: Option<String>,
    pub(crate) carried: bool,
}

impl Constant {
    /// The symbolic id, such as `ecs.context_row`.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The value.
    #[must_use]
    pub const fn value(&self) -> u64 {
        self.value
    }

    /// Code that uses the value.
    #[must_use]
    pub const fn pattern(&self) -> Option<&Pattern> {
        self.pattern.as_ref()
    }

    /// Where the value lies in the pattern's match; `None` when the pattern
    /// fixes the value's bytes, so that a match only confirms it.
    #[must_use]
    pub const fn operand(&self) -> Option<Operand> {
        self.operand
    }

    /// The symbol in whose code the value is read, for a value only a
    /// function too short to find on its own holds: its pattern must match
    /// where the symbol starts, and the operand is read there.
    #[must_use]
    pub fn via(&self) -> Option<&str> {
        self.via.as_deref()
    }

    /// How many bytes from the [`Self::via`] symbol's start its pattern is
    /// searched in, for a value in the body of a function the game holds in
    /// identical copies: the pattern must match exactly once there. `None`
    /// when it must match where the symbol starts.
    #[must_use]
    pub const fn within(&self) -> Option<u32> {
        self.within
    }

    /// The name of the attribute whose slot in its set the value is; the
    /// game looks such slots up in its data at run time, so no code holds
    /// them and the loader reads them from the game's archive.
    #[must_use]
    pub fn attribute(&self) -> Option<&str> {
        self.attribute.as_deref()
    }

    /// The symbol of the data cell the game keeps the value in, a `u32`
    /// its code loads instead of holding it: a build without its own table
    /// reads the value from the cell where its symbol is found.
    #[must_use]
    pub fn cell(&self) -> Option<&str> {
        self.cell.as_deref()
    }

    /// Whether a build without its own table takes the value over; `false`
    /// for a value only this build holds, which such a build neither finds
    /// nor misses.
    #[must_use]
    pub const fn carried(&self) -> bool {
        self.carried
    }

    /// The value as its pattern finds it in `code`: read from the operand,
    /// or the pinned value when the pattern only confirms it; `None` without
    /// a pattern or a unique match, and for a value read [`Self::via`] a
    /// symbol.
    #[must_use]
    pub fn find(&self, code: &[u8]) -> Option<u64> {
        if self.via.is_some() {
            return None;
        }
        let start = u64::try_from(self.pattern.as_ref()?.find_unique(code).ok()?).ok()?;
        self.read(code, start)
    }

    /// The value in the code of the symbol at `start`: where the pattern
    /// matches at `start`, or exactly once in the [`Self::within`] bytes
    /// from it; read from the operand, or the pinned value when the pattern
    /// only confirms it. `None` without a pattern or such a match.
    #[must_use]
    pub fn find_at(&self, code: &[u8], start: u64) -> Option<u64> {
        let at = usize::try_from(start).ok()?;
        let pattern = self.pattern.as_ref()?;
        let found = match self.within {
            Some(within) => {
                let end = at
                    .checked_add(usize::try_from(within).ok()?)?
                    .min(code.len());
                let offset = pattern.find_unique(code.get(at..end)?).ok()?;
                start.checked_add(u64::try_from(offset).ok()?)?
            }
            None if pattern.matches(code.get(at..)?) => start,
            None => return None,
        };
        self.read(code, found)
    }

    fn read(&self, code: &[u8], start: u64) -> Option<u64> {
        match self.operand {
            Some(operand) => operand.read(code, start),
            None => Some(self.value),
        }
    }
}

#[cfg(test)]
mod proofs;
