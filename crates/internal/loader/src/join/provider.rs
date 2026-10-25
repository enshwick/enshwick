//! Finds the online provider's join in the provider's function table and
//! checks that it is the function the build table names, before any hook
//! goes in.
//!
//! No reflection or stable boundary names the provider, so its table and
//! join come from the build table, by address or by pattern. The join is
//! hooked only where the provider's table holds it in the pinned slot, or,
//! on a build without a pinned slot, in exactly one slot, and where its
//! first bytes are the pinned ones; anything else leaves the check off.

use core::ops::Range;

/// The build table's id of the online provider's join.
pub const JOIN_ID: &str = "net.join_game";
/// The build table's id of the online provider's function table.
pub const TABLE_ID: &str = "net.provider_vtable";
/// The build table's id of the join's slot in that table.
pub const SLOT_ID: &str = "net.provider_join_slot";
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
/// use enshwick::join::provider::Error;
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
    /// The provider's table or the join cannot be read.
    #[error("the provider's table at {table:#x} holds no function")]
    Unreadable {
        /// The table's address.
        table: u64,
    },
    /// The provider's table ends before the pinned slot.
    #[error("the provider's table ends after {slots} slots, before the pinned slot {slot}")]
    Short {
        /// The pinned slot.
        slot: usize,
        /// The slots the table holds.
        slots: usize,
    },
    /// The pinned slot holds another function.
    #[error("slot {slot} of the provider's table holds {found:#x}, not the join at {join:#x}")]
    Slot {
        /// The pinned slot.
        slot: usize,
        /// The function the slot holds.
        found: u64,
        /// The join's address.
        join: u64,
    },
    /// No slot holds the join, on a build without a pinned slot.
    #[error("the provider's table holds the join at {join:#x} in no slot")]
    NotFound {
        /// The join's address.
        join: u64,
    },
    /// Two slots hold the join, on a build without a pinned slot.
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
/// use enshwick::join::provider::Pins;
///
/// let pins = Pins { table: 0x1_41db_2f28, join: 0x1_4108_9230, slot: Some(16), bytes: None };
/// assert_eq!(pins.slot, Some(16));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pins {
    /// The provider's function table.
    pub table: u64,
    /// The join.
    pub join: u64,
    /// The join's slot in the table; `None` on a build whose table found the
    /// rest by patterns.
    pub slot: Option<usize>,
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
    /// use enshwick::join::provider::{Error, Pins};
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
            table: at(TABLE_ID)?,
            join: at(JOIN_ID)?,
            slot: constant(SLOT_ID).and_then(|slot| usize::try_from(slot).ok()),
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

/// Checks the pinned join against the game, reading eight bytes at an
/// address with `read`, whose code section spans `code`: its first bytes
/// must be the pinned ones and the provider's table must hold it in the
/// pinned slot, or in exactly one slot when none is pinned. Returns the
/// join to hook.
///
/// # Errors
///
/// An [`Error`] naming what did not match.
///
/// # Examples
///
/// ```
/// use enshwick::join::provider::{Pins, verify};
///
/// let (table, join, bytes) = (0x9000, 0x4000, 0x5340_u64);
/// let read = |va| match va {
///     0x9000 => Some(0x2000),
///     0x9008 => Some(join),
///     0x4000 => Some(bytes),
///     _ => None,
/// };
/// let pins = Pins { table, join, slot: Some(1), bytes: Some(bytes) };
/// assert_eq!(verify(&pins, read, 0x1000..0x8000), Ok(join));
/// ```
pub fn verify(
    pins: &Pins,
    read: impl Fn(u64) -> Option<u64>,
    code: Range<u64>,
) -> Result<u64, Error> {
    let expected = pins.bytes.ok_or(Error::NoBytes)?;
    let join = pins.join;
    let found = read(join).ok_or(Error::Unreadable { table: pins.table })?;
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
    if let Some(slot) = pins.slot {
        return match slots.get(slot) {
            Some(&function) if function == join => Ok(join),
            Some(&found) => Err(Error::Slot { slot, found, join }),
            None => Err(Error::Short {
                slot,
                slots: slots.len(),
            }),
        };
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

    fn pins(slot: Option<usize>, bytes: Option<u64>) -> Pins {
        Pins {
            table: TABLE,
            join: JOIN,
            slot,
            bytes,
        }
    }

    #[test]
    fn the_pinned_slot_holding_the_join_passes() {
        let read = memory(&[0x1100, 0x1200, JOIN, 0x1300]);
        assert_eq!(verify(&pins(Some(2), Some(BYTES)), read, CODE), Ok(JOIN));
    }

    #[test]
    fn a_pinned_slot_holding_another_function_turns_the_check_off() {
        let read = memory(&[0x1100, 0x1200, 0x1300, JOIN]);
        assert_eq!(
            verify(&pins(Some(2), Some(BYTES)), read, CODE),
            Err(Error::Slot {
                slot: 2,
                found: 0x1300,
                join: JOIN
            })
        );
    }

    #[test]
    fn a_pinned_slot_past_the_table_turns_the_check_off() {
        let read = memory(&[0x1100, JOIN]);
        assert_eq!(
            verify(&pins(Some(5), Some(BYTES)), read, CODE),
            Err(Error::Short { slot: 5, slots: 2 })
        );
    }

    #[test]
    fn without_a_pinned_slot_the_one_slot_holding_the_join_is_taken() {
        let read = memory(&[0x1100, 0x1200, JOIN]);
        assert_eq!(verify(&pins(None, Some(BYTES)), read, CODE), Ok(JOIN));
    }

    #[test]
    fn without_a_pinned_slot_no_or_two_slots_holding_the_join_turn_it_off() {
        let none = memory(&[0x1100, 0x1200]);
        assert_eq!(
            verify(&pins(None, Some(BYTES)), none, CODE),
            Err(Error::NotFound { join: JOIN })
        );
        let two = memory(&[JOIN, 0x1200, JOIN]);
        assert_eq!(
            verify(&pins(None, Some(BYTES)), two, CODE),
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
            verify(&pins(Some(0), Some(0x1234)), read, CODE),
            Err(Error::Bytes {
                join: JOIN,
                found: BYTES,
                expected: 0x1234
            })
        );
        assert_eq!(
            verify(&pins(Some(0), None), memory(&[JOIN]), CODE),
            Err(Error::NoBytes)
        );
    }

    #[test]
    fn an_unreadable_table_turns_the_check_off() {
        let read = |va| (va == JOIN).then_some(BYTES);
        assert_eq!(
            verify(&pins(Some(0), Some(BYTES)), read, CODE),
            Err(Error::Unreadable { table: TABLE })
        );
    }

    /// On the client `ENSH_CLIENT_EXE` names (skipped without it), its own
    /// table's pins pass against the executable, and every client table
    /// carried to it by patterns, without a pinned slot, finds the same join
    /// in the provider's table.
    #[test]
    fn the_client_s_tables_pass_on_its_executable() {
        let Ok(path) = std::env::var("ENSH_CLIENT_EXE") else {
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        let image = enshwick_loader_reflect::Image::parse(
            &bytes,
            enshwick_loader_reflect::Layout::File,
            None,
        )
        .unwrap();
        let tables = crate::builds::load();
        let fingerprint = image.fingerprint().unwrap();
        let table = enshwick_loader_builds::exact(
            &tables,
            enshwick_loader_builds::Side::Client,
            fingerprint,
        )
        .unwrap()
        .resolve();
        let base = image.base();
        let text = image.range(image.section(".text").unwrap());
        let read = |va| image.u64(va);
        let pinned =
            Pins::from_table(|id| table.symbol(id), |id| table.constant(id), base).unwrap();
        let join = base + table.symbol(JOIN_ID).unwrap();
        assert_eq!(verify(&pinned, read, text.clone()), Ok(join));
        let code = image.read(text.start, text.end - text.start).unwrap();
        for other in tables
            .iter()
            .filter(|t| t.side() == enshwick_loader_builds::Side::Client)
        {
            let carried = other.fall_back(code, text.start - base);
            let by_patterns =
                Pins::from_table(|id| carried.symbol(id), |id| carried.constant(id), base).unwrap();
            assert_eq!(by_patterns.slot, None, "{}", other.name());
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
