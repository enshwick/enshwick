//! Where a rolled loot slot keeps the item and the stack's count, read by
//! name from the build's reflection.

use enshwick_gamedata::reflect::Reflection;

/// The reflected type of a rolled loot slot.
const SLOT: &str = "keen::LootSlotInfo";
/// The slot's item id, a `u32`.
const ITEM: &str = "keen::LootSlotInfo.itemId";
/// The slot's stack count, a `u16`.
const COUNT: &str = "keen::LootSlotInfo.stackSize";

/// Where a rolled loot slot keeps its item id and its stack's count on the
/// running build, both inside the slot's reflected size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SlotFields {
    /// The item id, a `u32`.
    pub(crate) item: usize,
    /// The stack's count, a `u16`.
    pub(crate) count: usize,
}

/// Why the slot's fields do not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum SlotError {
    /// The type or one of its fields is not in the build's reflection.
    #[error("is not reflected")]
    NotReflected(&'static str),
    /// A field's value would run past the slot.
    #[error("runs past the slot")]
    PastSlot(&'static str),
}

impl SlotError {
    /// The reflected entry that failed.
    pub(crate) fn entry(self) -> &'static str {
        match self {
            Self::NotReflected(entry) | Self::PastSlot(entry) => entry,
        }
    }
}

impl SlotFields {
    /// Reads the fields from the build's reflection.
    ///
    /// # Errors
    ///
    /// The type or field that is not reflected, or a field that would run
    /// past the slot.
    pub(crate) fn reflect(reflection: &Reflection<'_>) -> Result<Self, SlotError> {
        let field = |entry: &'static str| {
            let name = entry.rsplit('.').next().unwrap_or(entry);
            reflection
                .field_offset(SLOT, name)
                .ok()
                .and_then(|at| usize::try_from(at).ok())
                .ok_or(SlotError::NotReflected(entry))
        };
        let size = reflection
            .type_info(SLOT)
            .ok()
            .and_then(|info| usize::try_from(info.size()).ok())
            .ok_or(SlotError::NotReflected(SLOT))?;
        Self::checked(field(ITEM)?, field(COUNT)?, size)
    }

    /// The fields at `item` and `count` in a slot of `size` bytes.
    ///
    /// # Errors
    ///
    /// A field whose value would run past the slot.
    fn checked(item: usize, count: usize, size: usize) -> Result<Self, SlotError> {
        for (entry, at, width) in [
            (ITEM, item, size_of::<u32>()),
            (COUNT, count, size_of::<u16>()),
        ] {
            if at.checked_add(width).is_none_or(|end| end > size) {
                return Err(SlotError::PastSlot(entry));
            }
        }
        Ok(Self { item, count })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_inside_the_slot_are_kept_up_to_its_last_byte() {
        assert_eq!(
            SlotFields::checked(0, 4, 16),
            Ok(SlotFields { item: 0, count: 4 })
        );
        assert_eq!(
            SlotFields::checked(12, 14, 16),
            Ok(SlotFields {
                item: 12,
                count: 14
            })
        );
    }

    #[test]
    fn a_field_that_runs_past_the_slot_is_named() {
        let past = SlotFields::checked(0, 15, 16);
        assert_eq!(past, Err(SlotError::PastSlot(COUNT)));
        let why = past.unwrap_err();
        assert_eq!(why.entry(), "keen::LootSlotInfo.stackSize");
        assert_eq!(why.to_string(), "runs past the slot");
        assert_eq!(
            SlotFields::checked(13, 4, 16),
            Err(SlotError::PastSlot(ITEM))
        );
        assert_eq!(
            SlotFields::checked(usize::MAX, 4, 16),
            Err(SlotError::PastSlot(ITEM))
        );
    }

    #[test]
    fn a_missing_entry_is_named_as_not_reflected() {
        let why = SlotError::NotReflected(SLOT);
        assert_eq!(why.entry(), "keen::LootSlotInfo");
        assert_eq!(why.to_string(), "is not reflected");
    }

    /// In each executable `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` name (each
    /// skipped without it), the slot's fields read by name where the loot
    /// rate has always found them.
    #[test]
    fn the_slot_fields_read_by_name_on_each_executable() {
        for var in ["ENSH_SERVER_EXE", "ENSH_CLIENT_EXE"] {
            let Ok(path) = std::env::var(var) else {
                continue;
            };
            let bytes = std::fs::read(path).unwrap();
            let image = enshwick_gamedata::reflect::Image::parse(
                &bytes,
                enshwick_gamedata::reflect::Layout::File,
                None,
            )
            .unwrap();
            let reflection = Reflection::scan(&image);
            assert_eq!(
                SlotFields::reflect(&reflection),
                Ok(SlotFields { item: 0, count: 4 }),
                "{var}"
            );
            assert_eq!(
                reflection
                    .type_info(SLOT)
                    .map(enshwick_gamedata::reflect::TypeInfo::size)
                    .ok(),
                Some(16),
                "{var}"
            );
        }
    }
}
