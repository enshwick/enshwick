//! The slots of attributes in their sets, read from the game's data. The
//! game's attribute container lists every set's attributes in slot order,
//! with their names, and the game looks slots up there at run time, so no
//! code holds them (game build 1269579). The build tables name each slot
//! they pin by its attribute; the loader reads the slot at start, so the
//! slots hold on a build without a table, and the pinned values serve as a
//! cross-check.

use rustc_hash::FxHashMap;

/// The qualified name of the container's type, whose fnv1a32 hash keys it
/// in the archive.
const CONTAINER: &str = "keen::AttributeContainerResource";

/// Where the fields of the attribute resources sit, from the game's
/// reflection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SlotLayout {
    /// `AttributeContainerResource.attributes`.
    pub sets: usize,
    /// `BaseAttributeResource` size.
    pub set_size: usize,
    /// `BaseAttributeResource.ids`.
    pub ids: usize,
    /// `BaseAttributeResource.debugNames`.
    pub names: usize,
}

fn word(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// A blob array or string at `field`: where it starts and its count.
fn array(bytes: &[u8], field: usize) -> Option<(usize, usize)> {
    let offset = usize::try_from(word(bytes, field)?).ok()?;
    let count = usize::try_from(word(bytes, field + 4)?).ok()?;
    Some((field.checked_add(offset)?, count))
}

/// Every attribute's slot in its set, by name, from `resource`, the bytes
/// of the game's attribute container; `None` when they do not read. A name
/// that more than one set lists is left out, and so is a set whose names do
/// not match its ids one for one.
pub(crate) fn slots<'a>(
    layout: &SlotLayout,
    resource: &'a [u8],
) -> Option<FxHashMap<&'a str, usize>> {
    let (first, count) = array(resource, layout.sets)?;
    let mut slots = FxHashMap::default();
    let mut twice = Vec::new();
    for set in 0..count {
        let at = first.checked_add(set.checked_mul(layout.set_size)?)?;
        let (_, ids) = array(resource, at.checked_add(layout.ids)?)?;
        let (names_at, names) = array(resource, at.checked_add(layout.names)?)?;
        if ids != names {
            continue;
        }
        for slot in 0..names {
            let (text, len) = array(resource, names_at.checked_add(slot.checked_mul(8)?)?)?;
            let name = core::str::from_utf8(resource.get(text..text.checked_add(len)?)?).ok()?;
            if slots.insert(name, slot).is_some() {
                twice.push(name);
            }
        }
    }
    for name in twice {
        slots.remove(name);
    }
    Some(slots)
}

/// Every attribute's slot in its set, by name, from the attribute
/// container in `archive`, with the layout `reflection` gives.
///
/// # Errors
///
/// Why the slots do not read: the game does not reflect the container, or
/// the archive holds no single container that reads.
pub fn read(
    archive: &crate::archive::Archive,
    reflection: &enshwick_loader_reflect::Reflection<'_>,
) -> Result<FxHashMap<String, usize>, &'static str> {
    let field = |ty: &str, name: &str| {
        let offset = reflection.field_offset(ty, name).ok()?;
        usize::try_from(offset).ok()
    };
    let layout = (|| {
        let set = "keen::BaseAttributeResource";
        Some(SlotLayout {
            sets: field(CONTAINER, "attributes")?,
            set_size: usize::try_from(reflection.type_info(set).ok()?.size()).ok()?,
            ids: field(set, "ids")?,
            names: field(set, "debugNames")?,
        })
    })()
    .ok_or("the game does not reflect its attribute container")?;
    let keys = archive.keys_of(enshwick_loader_reflect::fnv1a32(CONTAINER.as_bytes()));
    let resource = match keys.as_slice() {
        [key] => archive.resource(key),
        _ => None,
    };
    archive.trim();
    let found = resource
        .as_deref()
        .and_then(|r| slots(&layout, r))
        .ok_or("the game's attribute container does not read")?;
    Ok(found
        .into_iter()
        .map(|(name, slot)| (name.to_owned(), slot))
        .collect())
}

/// Reads the slot of each attribute the build's table names from the
/// game's archive in `dir` and gives it to the build; a pinned value that
/// differs is logged and replaced, since the game reads the slot from the
/// same data.
#[cfg(windows)]
pub(crate) fn supply(
    build: &mut crate::builds::KnownBuild,
    reflection: &enshwick_loader_reflect::Reflection<'_>,
    dir: &std::path::Path,
    side: enshwick_loader_api::abi::Side,
) {
    let wanted = build.attributes().to_vec();
    if wanted.is_empty() {
        return;
    }
    let Some(archive) = crate::archive::open_game(dir, side) else {
        return;
    };
    let slots = match read(archive, reflection) {
        Ok(slots) => slots,
        Err(why) => {
            crate::log!(WARN, "{why}: attribute slots are the build table's");
            return;
        }
    };
    let mut found = 0;
    for (id, name) in &wanted {
        let Some(slot) = slots.get(name).and_then(|&s| u64::try_from(s).ok()) else {
            crate::log!(WARN, "the game's data has no attribute {name}, for {id}");
            continue;
        };
        found += 1;
        if let Some(pinned) = build.supply(id, slot)
            && pinned != slot
        {
            crate::log!(
                WARN,
                "{id}: the game's data puts {name} in slot {slot}, the build table in {pinned}; the game's data holds"
            );
        }
    }
    crate::log!(
        INFO,
        "{found} of {} attribute slots read from the game's data",
        wanted.len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The layout on build 1269579.
    const LAYOUT: SlotLayout = SlotLayout {
        sets: 0,
        set_size: 28,
        ids: 4,
        names: 0x14,
    };

    /// A container of `sets`, each a list of names; ids are the names'
    /// indices, or one fewer for a set marked `short`.
    fn container(sets: &[(&[&str], bool)]) -> Vec<u8> {
        let put = |b: &mut Vec<u8>, at: usize, v: usize| {
            b[at..at + 4].copy_from_slice(&u32::try_from(v).unwrap().to_le_bytes());
        };
        let mut bytes = vec![0u8; 8 + sets.len() * 28];
        put(&mut bytes, 0, 8);
        put(&mut bytes, 4, sets.len());
        for (i, (names, short)) in sets.iter().enumerate() {
            let set = 8 + i * 28;
            let ids = bytes.len();
            let id_count = names.len() - usize::from(*short);
            bytes.resize(ids + id_count * 4, 0);
            put(&mut bytes, set + 4, ids - (set + 4));
            put(&mut bytes, set + 8, id_count);
            let strings = bytes.len();
            bytes.resize(strings + names.len() * 8, 0);
            put(&mut bytes, set + 0x14, strings - (set + 0x14));
            put(&mut bytes, set + 0x18, names.len());
            for (j, name) in names.iter().enumerate() {
                let text = bytes.len();
                bytes.extend_from_slice(name.as_bytes());
                let field = strings + j * 8;
                put(&mut bytes, field, text - field);
                put(&mut bytes, field + 4, name.len());
            }
        }
        bytes
    }

    #[test]
    fn slots_are_places_in_their_sets() {
        let bytes = container(&[
            (&["Health", "Health_Min", "Health_Max"], false),
            (&["Fuel", "Fuel_Max"], false),
            (&["Odd", "Odd_Max"], true),
            (&["Twice", "Fuel_Max"], false),
        ]);
        let found = slots(&LAYOUT, &bytes).unwrap();
        assert_eq!(found.get("Health_Max"), Some(&2));
        assert_eq!(found.get("Health"), Some(&0));
        assert_eq!(found.get("Twice"), Some(&0));
        assert_eq!(found.get("Fuel_Max"), None, "listed by two sets");
        assert_eq!(found.get("Odd_Max"), None, "names and ids differ");
        assert!(slots(&LAYOUT, &bytes[..bytes.len() - 1]).is_none());
        assert!(slots(&LAYOUT, &[]).is_none());
    }

    /// The installed client's archive, when `ENSHWICK_CLIENT_DIR` names its
    /// folder (build 1269579): every slot the client table names by its
    /// attribute reads as the table pins it.
    #[test]
    fn the_client_archive_holds_the_table_slots() {
        let Ok(dir) = std::env::var("ENSHWICK_CLIENT_DIR") else {
            return;
        };
        let archive =
            crate::archive::Archive::open(std::path::Path::new(&dir), "enshrouded").unwrap();
        let keys = archive.keys_of(enshwick_loader_reflect::fnv1a32(
            b"keen::AttributeContainerResource",
        ));
        assert_eq!(keys.len(), 1);
        let resource = archive.resource(&keys[0]).unwrap();
        let found = slots(&LAYOUT, &resource).unwrap();
        let table = crate::builds::embedded(enshwick_loader_api::abi::Side::CLIENT);
        let named = table.attributes();
        assert_eq!(named.len(), 9);
        for (id, name) in named {
            let slot = found
                .get(name.as_str())
                .and_then(|&s| u64::try_from(s).ok());
            assert_eq!(slot, table.constant(id), "{id}");
        }
    }
}
