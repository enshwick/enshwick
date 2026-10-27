//! Finds the online provider's join in the provider's function table and
//! checks that it is the function the build table names, before any hook
//! goes in.
//!
//! No reflection or stable boundary names the provider, so its table and
//! join come from the build table, by address or by pattern. The join is
//! hooked only where the provider's table holds it in exactly one slot and
//! its first bytes are the pinned ones; anything else leaves the check off.

use core::ops::Range;

use crate::pinned;

/// The build table's id of the join's first eight bytes.
pub const BYTES_ID: &str = "net.join_game.bytes";
/// The most slots read from the provider's table.
pub const MAX_SLOTS: usize = 64;

/// Why the join check stays off: what in the game did not match the build
/// table.
///
/// # Examples
///
/// ```
/// use enshwick_loader::join::provider::Error;
///
/// let error = Error::NotFound { join: 0x1_4108_9230 };
/// assert!(error.to_string().contains("no slot"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The build table lacks an entry the check needs.
    #[error("the build table has no `{0}`")]
    Missing(&'static str),
    /// The build table pins no first bytes for the join.
    #[error("the build table pins no bytes for the join")]
    NoBytes,
    /// The join's first bytes are not the pinned ones.
    #[error("the code at the join {join:#x} is {found:#018x}, not the pinned {expected:#018x}")]
    Bytes {
        /// The join's address.
        join: u64,
        /// The bytes found there.
        found: u64,
        /// The bytes the build table pins.
        expected: u64,
    },
    /// The provider's table holds no function in the game's code.
    #[error("the provider's table at {table:#x} holds no function")]
    Unreadable {
        /// The table's address.
        table: u64,
    },
    /// The join's first bytes cannot be read.
    #[error("the join at {join:#x} cannot be read")]
    JoinUnreadable {
        /// The join's address.
        join: u64,
    },
    /// The game's image is not found in the process.
    #[error("the game's image is not found")]
    NoImage,
    /// The game's code section is not found in its image.
    #[error("the game's code section is not found")]
    NoCode,
    /// No slot holds the join.
    #[error("the provider's table holds the join at {join:#x} in no slot")]
    NotFound {
        /// The join's address.
        join: u64,
    },
    /// Two slots hold the join.
    #[error("the provider's table holds the join in slots {first} and {second}")]
    Twice {
        /// The first slot.
        first: usize,
        /// The second slot.
        second: usize,
    },
}

/// What the build table pins for the join, as addresses in the process.
///
/// # Examples
///
/// ```
/// use enshwick_loader::join::provider::Pins;
///
/// let pins = Pins { table: 0x1_41db_2f28, join: 0x1_4108_9230, bytes: None };
/// assert_eq!(pins.bytes, None);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pins {
    /// The provider's function table.
    pub table: u64,
    /// The join.
    pub join: u64,
    /// The join's first eight bytes.
    pub bytes: Option<u64>,
}

impl Pins {
    /// Reads the pins from a build table's `symbol` and `constant` lookups,
    /// relative to the image at `base`.
    ///
    /// # Errors
    ///
    /// [`Error::Missing`] when the table lacks the provider's table or the
    /// join.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::join::provider::{Error, Pins};
    ///
    /// let none = Pins::from_table(|_| None, |_| None, 0x1_4000_0000);
    /// assert_eq!(none, Err(Error::Missing("net.provider_vtable")));
    /// ```
    pub fn from_table(
        symbol: impl Fn(&str) -> Option<u64>,
        constant: impl Fn(&str) -> Option<u64>,
        base: u64,
    ) -> Result<Self, Error> {
        let at = |id: &'static str| {
            symbol(id)
                .and_then(|rva| base.checked_add(rva))
                .ok_or(Error::Missing(id))
        };
        Ok(Self {
            table: at(pinned::net::PROVIDER_VTABLE.id())?,
            join: at(pinned::net::JOIN_GAME.id())?,
            bytes: constant(BYTES_ID),
        })
    }
}

/// The functions in the provider's table at `table`, slot by slot while a
/// slot holds an address in `code`, at most [`MAX_SLOTS`].
fn slots(read: impl Fn(u64) -> Option<u64>, table: u64, code: &Range<u64>) -> Vec<u64> {
    (0..MAX_SLOTS)
        .map_while(|slot| {
            let at = table.checked_add(u64::try_from(slot * 8).ok()?)?;
            read(at).filter(|function| code.contains(function))
        })
        .collect()
}

/// Checks the pinned join against the game, reading eight bytes at an address
/// with `read`, whose code section spans `code`.
///
/// Its first bytes must be the pinned ones and the provider's table must hold
/// it in exactly one slot, the slot of this build.
///
/// Returns the join to hook.
///
/// # Errors
///
/// An [`Error`] naming what did not match.
///
/// # Examples
///
/// ```
/// use enshwick_loader::join::provider::{Pins, verify};
///
/// let (table, join, bytes) = (0x9000, 0x4000, 0x5340_u64);
/// let read = |va| match va {
///     0x9000 => Some(0x2000),
///     0x9008 => Some(join),
///     0x4000 => Some(bytes),
///     _ => None,
/// };
/// let pins = Pins { table, join, bytes: Some(bytes) };
/// assert_eq!(verify(&pins, read, 0x1000..0x8000), Ok(join));
/// ```
pub fn verify(
    pins: &Pins,
    read: impl Fn(u64) -> Option<u64>,
    code: Range<u64>,
) -> Result<u64, Error> {
    let expected = pins.bytes.ok_or(Error::NoBytes)?;
    let join = pins.join;
    let found = read(join).ok_or(Error::JoinUnreadable { join })?;
    if found != expected {
        return Err(Error::Bytes {
            join,
            found,
            expected,
        });
    }
    let slots = slots(&read, pins.table, &code);
    if slots.is_empty() {
        return Err(Error::Unreadable { table: pins.table });
    }
    let mut holding = slots
        .iter()
        .enumerate()
        .filter(|&(_, &function)| function == join)
        .map(|(slot, _)| slot);
    match (holding.next(), holding.next()) {
        (Some(_), None) => Ok(join),
        (Some(first), Some(second)) => Err(Error::Twice { first, second }),
        (None, _) => Err(Error::NotFound { join }),
    }
}

/// Checks the pinned join in the running game as [`verify`] does, reading
/// eight bytes at a time and only inside `image`, the game's image, whose
/// code section spans `code`.
///
/// # Errors
///
/// An [`Error`] naming what did not match; an address outside `image` reads
/// as unreadable.
///
/// # Safety
///
/// - Every byte of `image` is mapped readable and stays so for the call.
/// - `image.start` carries provenance the game or the caller exposed.
/// - No other thread writes the words read during the call.
pub unsafe fn verify_mapped(
    pins: &Pins,
    image: Range<u64>,
    code: Range<u64>,
) -> Result<u64, Error> {
    let read = |va: u64| {
        let end = va.checked_add(8)?;
        if va < image.start || end > image.end {
            return None;
        }
        let address = usize::try_from(va).ok()?;
        // SAFETY: `va..end` lies inside `image`, checked just before, and the
        // caller guarantees every byte of `image` mapped readable for the call.
        Some(unsafe { crate::memory::read::<u64>(address, 0) })
    };
    verify(pins, read, code)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CODE: Range<u64> = 0x1000..0x9000;
    const TABLE: u64 = 0xa000;
    const JOIN: u64 = 0x4000;
    const BYTES: u64 = 0xec83_4857_5655_5340;

    /// Memory with a provider table of `slots` at `TABLE`, ended by a
    /// non-code word, and `BYTES` at `JOIN`.
    fn memory(slots: &[u64]) -> impl Fn(u64) -> Option<u64> {
        let slots = slots.to_vec();
        move |va| {
            if va == JOIN {
                return Some(BYTES);
            }
            let index = usize::try_from(va.checked_sub(TABLE)? / 8).ok()?;
            Some(slots.get(index).copied().unwrap_or(0x21))
        }
    }

    fn pins(bytes: Option<u64>) -> Pins {
        Pins {
            table: TABLE,
            join: JOIN,
            bytes,
        }
    }

    #[test]
    fn the_one_slot_holding_the_join_is_taken_wherever_it_is() {
        for slot in 0..4 {
            let mut slots = vec![0x1100, 0x1200, 0x1300, 0x1400];
            slots[slot] = JOIN;
            let read = memory(&slots);
            assert_eq!(verify(&pins(Some(BYTES)), read, CODE), Ok(JOIN), "{slot}");
        }
    }

    #[test]
    fn no_or_two_slots_holding_the_join_turn_the_check_off() {
        let none = memory(&[0x1100, 0x1200]);
        assert_eq!(
            verify(&pins(Some(BYTES)), none, CODE),
            Err(Error::NotFound { join: JOIN })
        );
        let two = memory(&[JOIN, 0x1200, JOIN]);
        assert_eq!(
            verify(&pins(Some(BYTES)), two, CODE),
            Err(Error::Twice {
                first: 0,
                second: 2
            })
        );
    }

    #[test]
    fn other_code_at_the_join_turns_the_check_off() {
        let read = memory(&[JOIN]);
        assert_eq!(
            verify(&pins(Some(0x1234)), read, CODE),
            Err(Error::Bytes {
                join: JOIN,
                found: BYTES,
                expected: 0x1234
            })
        );
        assert_eq!(
            verify(&pins(None), memory(&[JOIN]), CODE),
            Err(Error::NoBytes)
        );
    }

    #[test]
    fn an_unreadable_join_names_the_join() {
        let read = |va| (va != JOIN).then_some(0x21);
        assert_eq!(
            verify(&pins(Some(BYTES)), read, CODE),
            Err(Error::JoinUnreadable { join: JOIN })
        );
    }

    /// Memory laid out as a game image: the join's bytes, then the
    /// provider's table holding the join in slot 0, ended by a non-code word.
    #[test]
    fn the_join_is_read_in_mapped_memory_only_inside_the_image() {
        let memory: Box<[u64; 4]> = Box::new([BYTES, 0, 0, 0x21]);
        let start = memory.as_ptr().expose_provenance() as u64;
        let mut memory = memory;
        memory[2] = start;
        let image = start..start + 32;
        let pins = Pins {
            table: start + 16,
            join: start,
            bytes: Some(BYTES),
        };
        // SAFETY: `image` spans the 32 bytes of `memory`, whose provenance
        // `start` exposed, alive and unwritten until the end of the test.
        let inside = unsafe { verify_mapped(&pins, image.clone(), start..start + 8) };
        assert_eq!(inside, Ok(start));
        // SAFETY: `start + 8..start + 32` lies inside the 32 bytes of `memory`,
        // whose provenance `start` exposed, alive and unwritten until the end
        // of the test.
        let outside = unsafe { verify_mapped(&pins, start + 8..start + 32, start..start + 8) };
        assert_eq!(outside, Err(Error::JoinUnreadable { join: start }));
    }

    #[test]
    fn an_unreadable_table_turns_the_check_off() {
        let read = |va| (va == JOIN).then_some(BYTES);
        assert_eq!(
            verify(&pins(Some(BYTES)), read, CODE),
            Err(Error::Unreadable { table: TABLE })
        );
    }

    /// On the client `ENSH_CLIENT_EXE` names (skipped without it), its own
    /// table's pins pass against the executable, and every client table
    /// carried to it by patterns finds the same join in the provider's table.
    #[test]
    fn the_client_s_tables_pass_on_its_executable() {
        let Ok(path) = std::env::var("ENSH_CLIENT_EXE") else {
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        let image = enshwick_gamedata::reflect::Image::parse(
            &bytes,
            enshwick_gamedata::reflect::Layout::File,
            None,
        )
        .unwrap();
        let tables = crate::builds::load();
        let fingerprint = image.fingerprint().unwrap();
        let table = enshwick_gamedata::exact(&tables, enshwick_gamedata::Side::Client, fingerprint)
            .unwrap()
            .resolve();
        let base = image.base();
        let text = image.range(image.section(".text").unwrap());
        let read = |va| image.u64(va);
        let pinned =
            Pins::from_table(|id| table.symbol(id), |id| table.constant(id), base).unwrap();
        let join = base + table.symbol(pinned::net::JOIN_GAME.id()).unwrap();
        assert_eq!(verify(&pinned, read, text.clone()), Ok(join));
        let code = image.read(text.start, text.end - text.start).unwrap();
        for other in tables
            .iter()
            .filter(|t| t.side() == enshwick_gamedata::Side::Client)
        {
            let carried = other.fall_back(code, text.start - base, |rva| image.u32(base + rva));
            let by_patterns =
                Pins::from_table(|id| carried.symbol(id), |id| carried.constant(id), base).unwrap();
            assert_eq!(by_patterns.join, join, "{}", other.name());
            assert_eq!(
                verify(&by_patterns, read, text.clone()),
                Ok(join),
                "{}",
                other.name()
            );
        }
    }
}
