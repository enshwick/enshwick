//! Where a rolled loot slot keeps the item and the stack's count, read by
//! name from the build's reflection.

use enshwick_gamedata::reflect::Reflection;

/// The reflected type of a rolled loot slot.
pub(crate) const SLOT: &str = "keen::LootSlotInfo";
/// The slot's item id, a `u32`.
const ITEM: &str = "itemId";
/// The slot's stack count, a `u16`.
const COUNT: &str = "stackSize";

/// Where a rolled loot slot keeps its item id and its stack's count, and
/// how large the slot is, on the running build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SlotFields {
    /// The item id, a `u32`.
    pub(crate) item: usize,
    /// The stack's count, a `u16`.
    pub(crate) count: usize,
    /// The slot's size.
    pub(crate) size: usize,
}

/// Why the slot's fields do not read: the entry, and the reason.
pub(crate) type Refusal = (String, &'static str);

impl SlotFields {
    /// Reads the fields from the build's reflection.
    ///
    /// # Errors
    ///
    /// The field or the type that is not reflected, or a field that would
    /// run past the slot.
    pub(crate) fn reflect(reflection: &Reflection<'_>) -> Result<Self, Refusal> {
        let not_reflected = |entry: String| (entry, "is not reflected on this build");
        let field = |name: &str| {
            reflection
                .field_offset(SLOT, name)
                .ok()
                .and_then(|at| usize::try_from(at).ok())
                .ok_or_else(|| not_reflected(format!("{SLOT}.{name}")))
        };
        let size = reflection
            .type_info(SLOT)
            .ok()
            .and_then(|info| usize::try_from(info.size()).ok())
            .ok_or_else(|| not_reflected(SLOT.to_owned()))?;
        Self::checked(field(ITEM)?, field(COUNT)?, size)
    }

    /// The fields at `item` and `count` in a slot of `size` bytes.
    ///
    /// # Errors
    ///
    /// A field whose value would run past the slot.
    fn checked(item: usize, count: usize, size: usize) -> Result<Self, Refusal> {
        for (name, at, width) in [
            (ITEM, item, size_of::<u32>()),
            (COUNT, count, size_of::<u16>()),
        ] {
            if at.checked_add(width).is_none_or(|end| end > size) {
                return Err((format!("{SLOT}.{name}"), "runs past the slot"));
            }
        }
        Ok(Self { item, count, size })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_inside_the_slot_are_kept() {
        assert_eq!(
            SlotFields::checked(0, 4, 16),
            Ok(SlotFields {
                item: 0,
                count: 4,
                size: 16
            })
        );
    }

    #[test]
    fn a_field_that_runs_past_the_slot_is_named() {
        assert_eq!(
            SlotFields::checked(0, 15, 16),
            Err((
                "keen::LootSlotInfo.stackSize".to_owned(),
                "runs past the slot"
            ))
        );
        assert_eq!(
            SlotFields::checked(13, 4, 16),
            Err(("keen::LootSlotInfo.itemId".to_owned(), "runs past the slot"))
        );
        assert_eq!(
            SlotFields::checked(usize::MAX, 4, 16).map_err(|(entry, _)| entry),
            Err("keen::LootSlotInfo.itemId".to_owned())
        );
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
                Ok(SlotFields {
                    item: 0,
                    count: 4,
                    size: 16
                }),
                "{var}"
            );
        }
    }
}
