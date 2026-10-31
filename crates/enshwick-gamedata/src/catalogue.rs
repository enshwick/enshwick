//! Engine facts by an id of enshwick's own, answered from the game's
//! reflection.
//!
//! The API asks for a type, a hash or a field offset by id, and the loader
//! and the tools answer it, so a game update that renames or moves something
//! changes this table, not the mods.

mod facts;

pub use enshwick::__private::ids;
pub use facts::FACTS;

use crate::reflect::{Reflection, TypeInfo};
use enshwick::Off;
use enshwick::content::recipes::RecipeKind;
use enshwick::content::{EngineFacts, Guid};

/// How one engine fact is found.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::catalogue::{Fact, fact};
///
/// assert!(matches!(fact("item_info"), Some(Fact::Type(_))));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Fact {
    /// A type, by its reflected name: answers type, size and hash lookups
    /// by the id.
    Type(&'static str),
    /// The offset of `field` in `ty`, plus `plus` bytes into the field.
    Offset {
        /// The reflected type.
        ty: &'static str,
        /// The field in it.
        field: &'static str,
        /// Bytes past the field's start.
        plus: u64,
    },
    /// The sum of the offsets of fields nested one in the next: each pair
    /// is a reflected type and its field, the field holding the next type.
    Path(&'static [(&'static str, &'static str)]),
    /// The value of the constant `name` of the reflected enum `ty`; for a
    /// mask, the index of its bit.
    Constant {
        /// The reflected enum.
        ty: &'static str,
        /// The constant in it.
        name: &'static str,
    },
}

/// The ids of a placed piece's fields, as the place event holds them:
/// position, orientation, lowest and highest corner, material feedback,
/// tracking item and owner.
pub const BUILDING_PLACE_EVENT_FIELDS: [&str; 7] = [
    ids::BUILDING_PLACE_EVENT_POSITION,
    ids::BUILDING_PLACE_EVENT_ORIENTATION,
    ids::BUILDING_PLACE_EVENT_VOLUME_MIN,
    ids::BUILDING_PLACE_EVENT_VOLUME_MAX,
    ids::BUILDING_PLACE_EVENT_MATERIAL,
    ids::BUILDING_PLACE_EVENT_TRACKING_ITEM_ID,
    ids::BUILDING_PLACE_EVENT_OWNER_ID,
];
/// The ids of a torn-down piece's fields, as the tear-down event holds
/// them: position, orientation, lowest and highest corner, material
/// feedback and owner.
pub const BUILDING_TEAR_DOWN_EVENT_FIELDS: [&str; 6] = [
    ids::BUILDING_TEAR_DOWN_EVENT_POSITION,
    ids::BUILDING_TEAR_DOWN_EVENT_ORIENTATION,
    ids::BUILDING_TEAR_DOWN_EVENT_VOLUME_MIN,
    ids::BUILDING_TEAR_DOWN_EVENT_VOLUME_MAX,
    ids::BUILDING_TEAR_DOWN_EVENT_MATERIAL,
    ids::BUILDING_TEAR_DOWN_EVENT_OWNER_ID,
];

/// The ids of that event's fields: who placed it, then the prop.
pub const LOGIC_ENTITY_PLACED_FIELDS: [&str; 2] = [
    ids::LOGIC_ENTITY_PLACED_EVENT_PLACED_BY_PLAYER_ID,
    ids::LOGIC_ENTITY_PLACED_EVENT_PLACED_LOGIC_ENTITY_ID,
];
/// The ids of an item's placement box, before it turns: the lowest and
/// the highest corner, `f32` on each axis.
pub const ITEM_PLACEMENT_BOX: [&str; 2] = [
    ids::ITEM_INFO_PLACEMENT_BOX_MIN,
    ids::ITEM_INFO_PLACEMENT_BOX_MAX,
];
/// The ids of the bools that make a prop's `Fuel` refill on its own: over
/// time, as a well does, and from rain, as a water barrel does.
pub const FUEL_REFILLS: [&str; 2] = [
    ids::FUEL_SETUP_AUTOMATIC_REFILL_ENABLED,
    ids::FUEL_SETUP_RAIN_REFILL_ENABLED,
];
/// The ids of a packed transform's position halves, low then high word of
/// x, y and z, each a `u16`.
pub const SCENE_TRANSFORM_POSITION: [&str; 6] = [
    ids::SCENE_TRANSFORM_POSITION_X0,
    ids::SCENE_TRANSFORM_POSITION_X1,
    ids::SCENE_TRANSFORM_POSITION_Y0,
    ids::SCENE_TRANSFORM_POSITION_Y1,
    ids::SCENE_TRANSFORM_POSITION_Z0,
    ids::SCENE_TRANSFORM_POSITION_Z1,
];

/// The kind of the recipes that make an item, by the name of the constant
/// of [`ids::ITEM_SLOT_TYPE`] the item's equipment slot holds; every other slot is
/// [`RecipeKind::Other`]. A new kind is a row here.
///
/// # Examples
///
/// ```
/// use enshwick::content::recipes::RecipeKind;
/// use enshwick_gamedata::catalogue::RECIPE_KINDS;
///
/// assert!(RECIPE_KINDS.iter().any(|&(_, kind)| kind == RecipeKind::Roof));
/// ```
pub const RECIPE_KINDS: &[(&str, RecipeKind)] = &[
    ("BlueprintMaterial_Default", RecipeKind::BuildingBlock),
    ("BlueprintMaterial_Roof", RecipeKind::Roof),
    ("OvergrowthMaterial", RecipeKind::Overgrowth),
    ("TerrainMaterial", RecipeKind::Terrain),
];

/// The item tag behind the crafting menu's Decorative group.
///
/// The game files a placed prop under that group by this tag on its item.
/// Game data, read from build 410's crafting menu definitions; the dedicated
/// server has no menu, so the item's tag is what both sides share. On server
/// build 399, 685 of the 2536 vanilla recipes make an item with it.
pub const DECORATIVE_ITEM_TAG: Guid = Guid::parse("ee3b266d-83e7-4239-8bdd-8e58fe12e9d1");

/// The item tag behind the crafting menu's Comfort Furniture group.
///
/// Game data, read from build 410's crafting menu definitions like
/// [`DECORATIVE_ITEM_TAG`]; besides furniture, lights, braziers, trophies
/// and a few logic switches and instruments carry it. On server build 399,
/// 347 of the 2536 vanilla recipes make an item with it and not the
/// Decorative tag.
pub const COMFORT_FURNITURE_ITEM_TAG: Guid = Guid::parse("e6e36d0a-3eaf-4cbf-a767-d26aa76ccef1");

/// The kinds an item in a slot of no kind of its own takes from its tags.
///
/// Each row is the name of the constant of [`ids::ITEM_SLOT_TYPE`] the item's
/// equipment slot holds, the item tag and the kind. A row of
/// [`RECIPE_KINDS`] wins over these, and of these an earlier row wins over
/// a later one: an item with both tags is Decorative. A new kind by tag is
/// a row here.
///
/// # Examples
///
/// ```
/// use enshwick::content::recipes::RecipeKind;
/// use enshwick_gamedata::catalogue::RECIPE_KIND_TAGS;
///
/// assert!(RECIPE_KIND_TAGS.iter().any(|&(_, _, kind)| kind == RecipeKind::Decorative));
/// ```
pub const RECIPE_KIND_TAGS: &[(&str, Guid, RecipeKind)] = &[
    ("BuildTool", DECORATIVE_ITEM_TAG, RecipeKind::Decorative),
    (
        "BuildTool",
        COMFORT_FURNITURE_ITEM_TAG,
        RecipeKind::ComfortFurniture,
    ),
];

/// The slot value each row of [`RECIPE_KIND_TAGS`] has in `reflection`,
/// with its tag and kind; a constant the build does not reflect is left out.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::catalogue::recipe_kind_tags;
/// use enshwick_gamedata::reflect::Reflection;
///
/// fn tags(reflection: &Reflection<'_>) -> usize {
///     recipe_kind_tags(reflection).len()
/// }
/// ```
#[must_use]
pub fn recipe_kind_tags(reflection: &Reflection<'_>) -> Vec<(u64, Guid, RecipeKind)> {
    let slot = type_name(ids::ITEM_SLOT_TYPE);
    RECIPE_KIND_TAGS
        .iter()
        .filter_map(|&(name, tag, kind)| Some((reflection.enum_value(slot, name).ok()?, tag, kind)))
        .collect()
}

/// The slot value each row of [`RECIPE_KINDS`] has in `reflection`, with
/// its kind; a constant the build does not reflect is left out.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::catalogue::recipe_kind_slots;
/// use enshwick_gamedata::reflect::Reflection;
///
/// fn slots(reflection: &Reflection<'_>) -> usize {
///     recipe_kind_slots(reflection).len()
/// }
/// ```
#[must_use]
pub fn recipe_kind_slots(reflection: &Reflection<'_>) -> Vec<(u64, RecipeKind)> {
    let slot = type_name(ids::ITEM_SLOT_TYPE);
    RECIPE_KINDS
        .iter()
        .filter_map(|&(name, kind)| Some((reflection.enum_value(slot, name).ok()?, kind)))
        .collect()
}

const fn offset(ty: &'static str, field: &'static str) -> Fact {
    Fact::Offset { ty, field, plus: 0 }
}

const fn enum_constant(ty: &'static str, name: &'static str) -> Fact {
    Fact::Constant { ty, name }
}

/// The fact named `id`, if the catalogue has one.
///
/// # Examples
///
/// ```
/// assert!(enshwick_gamedata::catalogue::fact("no.such.id").is_none());
/// ```
#[must_use]
pub const fn fact(id: &str) -> Option<Fact> {
    let mut i = 0;
    while i < FACTS.len() {
        let (name, fact) = FACTS[i];
        if same(name, id) {
            return Some(fact);
        }
        i += 1;
    }
    None
}

/// Fails the build unless [`FACTS`] holds one row for each id of [`ids`].
///
/// Both sides sum a mixed 64-bit hash of each id, so a missing, repeated or
/// stray row moves one sum, in linear time where a pairwise check overruns
/// const evaluation; two different sets agree only by a 64-bit collision.
/// The test `the_catalogue_holds_one_fact_for_each_id_the_api_asks_for`
/// names the row.
const _: () = {
    let mut ours = 0u64;
    let mut i = 0;
    while i < FACTS.len() {
        ours = ours.wrapping_add(id_hash(FACTS[i].0));
        i += 1;
    }
    let mut asked = 0u64;
    let mut i = 0;
    while i < ids::ALL.len() {
        asked = asked.wrapping_add(id_hash(ids::ALL[i]));
        i += 1;
    }
    assert!(
        FACTS.len() == ids::ALL.len() && ours == asked,
        "FACTS lacks, repeats or adds a row against the API's ids"
    );
};

/// FNV-1a over `id`, spread by the splitmix64 finaliser so that sums of
/// hashes of similar ids do not cancel.
const fn id_hash(id: &str) -> u64 {
    let bytes = id.as_bytes();
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut i = 0;
    while i < bytes.len() {
        hash = (hash ^ bytes[i] as u64).wrapping_mul(0x0100_0000_01b3);
        i += 1;
    }
    hash = (hash ^ (hash >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    hash = (hash ^ (hash >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    hash ^ (hash >> 31)
}

const fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// The engine's name for the type `name` stands for: the reflected name of
/// a type id, or `name` itself, which a mod may pass at the low level.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::catalogue::type_name;
///
/// assert_ne!(type_name("item_info"), "item_info");
/// assert_eq!(type_name("not_an_id"), "not_an_id");
/// ```
#[must_use]
pub fn type_name(name: &str) -> &str {
    match fact(name) {
        Some(Fact::Type(engine)) => engine,
        _ => name,
    }
}

/// The hash the game's reflection gives the type `id` names: FNV-1a over
/// its reflected name, the value resource keys and component lookups carry;
/// `None` when `id` names no type.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::catalogue::type_hash;
///
/// assert!(type_hash("item_info").is_some());
/// assert_eq!(type_hash("item_info.name"), None);
/// ```
#[must_use]
pub const fn type_hash(id: &str) -> Option<u32> {
    match fact(id) {
        Some(Fact::Type(engine)) => Some(crate::reflect::fnv1a32(engine.as_bytes())),
        _ => None,
    }
}

/// The offset `fact` names, with `field` reading a reflected type's field
/// offset; `None` for a type or a field `field` does not find.
fn offset_of(fact: Fact, field: impl Fn(&str, &str) -> Option<u64>) -> Option<u64> {
    match fact {
        Fact::Offset {
            ty,
            field: name,
            plus,
        } => field(ty, name)?.checked_add(plus),
        Fact::Path(steps) => steps
            .iter()
            .try_fold(0u64, |sum, &(ty, name)| sum.checked_add(field(ty, name)?)),
        Fact::Type(_) | Fact::Constant { .. } => None,
    }
}

/// The type every game event type derives from.
const EVENT_BASE: &str = "keen::ecs::Event";

/// Every game event type `reflection` holds, in no set order: each type
/// that derives from the engine's event base, the bases between included.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::catalogue::event_types;
/// use enshwick_gamedata::reflect::Reflection;
///
/// fn count(reflection: &Reflection<'_>) -> usize {
///     event_types(reflection).count()
/// }
/// ```
pub fn event_types<'r, 'a>(
    reflection: &'r Reflection<'a>,
) -> impl Iterator<Item = &'r TypeInfo<'a>> + 'r {
    reflection.derived_from(EVENT_BASE)
}

/// The value of the offset or enum constant named `id` in `reflection`;
/// `None` when the id names neither or reflection lacks what it names.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::catalogue::constant;
/// use enshwick_gamedata::reflect::Reflection;
///
/// fn name_at(reflection: &Reflection<'_>) -> Option<u64> {
///     constant("item_info.name", reflection)
/// }
/// ```
#[must_use]
pub fn constant(id: &str, reflection: &Reflection<'_>) -> Option<u64> {
    match fact(id)? {
        Fact::Constant { ty, name } => reflection.enum_value(ty, name).ok(),
        fact => offset_of(fact, |ty, field| reflection.field_offset(ty, field).ok()),
    }
}

/// The game's reflection answering engine facts by enshwick id, for the
/// loader and for tools that read a game image rather than run in the game.
///
/// # Examples
///
/// ```
/// use enshwick::content::items::ItemLayout;
/// use enshwick_gamedata::catalogue::Reflected;
/// use enshwick_gamedata::reflect::Reflection;
///
/// fn items(reflection: &Reflection<'_>) -> Result<ItemLayout, enshwick::Off> {
///     ItemLayout::resolve(&Reflected(reflection))
/// }
/// ```
#[derive(Debug)]
pub struct Reflected<'r, 'a>(pub &'r Reflection<'a>);

impl EngineFacts for Reflected<'_, '_> {
    fn offset(&self, id: &str) -> Result<usize, Off> {
        constant(id, self.0)
            .and_then(|offset| usize::try_from(offset).ok())
            .ok_or_else(|| Off::new(format!("offset {id} is not reflected")))
    }

    fn size(&self, id: &str) -> Result<usize, Off> {
        self.0
            .type_info(type_name(id))
            .ok()
            .and_then(|info| usize::try_from(info.size()).ok())
            .ok_or_else(|| Off::new(format!("type {id} is not reflected")))
    }

    fn hash(&self, id: &str) -> Result<u32, Off> {
        type_hash(id).ok_or_else(|| Off::new(format!("{id} names no type")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_holds_one_fact_for_each_id_the_api_asks_for() {
        let mut ours: Vec<&str> = FACTS.iter().map(|&(id, _)| id).collect();
        let mut asked = ids::ALL.to_vec();
        ours.sort_unstable();
        asked.sort_unstable();
        assert_eq!(ours, asked);
        asked.dedup();
        assert_eq!(asked.len(), ids::ALL.len());
    }

    #[test]
    fn a_row_repeated_in_place_of_another_moves_the_id_sum() {
        let sum = |ids: &[&str]| {
            ids.iter()
                .fold(0u64, |sum, id| sum.wrapping_add(id_hash(id)))
        };
        let mut rows = ids::ALL.to_vec();
        let whole = sum(&rows);
        for i in 1..rows.len() {
            let dropped = rows[i];
            rows[i] = rows[i - 1];
            assert_ne!(sum(&rows), whole, "{dropped}");
            rows[i] = dropped;
        }
    }

    /// `name` in lower case without underscores, to compare an id with the
    /// engine name it stands for.
    fn plain(name: &str) -> String {
        name.chars()
            .filter(|&c| c != '_')
            .map(|c| c.to_ascii_lowercase())
            .collect()
    }

    #[test]
    fn every_enum_constant_id_is_named_after_its_engine_constant() {
        for &(id, fact) in FACTS {
            if let Fact::Constant { name, .. } = fact {
                let (_, ours) = id.rsplit_once('.').unwrap();
                assert_eq!(plain(ours), plain(name), "{id}");
            }
        }
    }

    #[test]
    fn every_event_id_is_named_after_its_engine_type_and_field() {
        let events: Vec<(&str, &str)> = FACTS
            .iter()
            .filter_map(|&(id, fact)| match fact {
                Fact::Type(ty) if ty.ends_with("Event") => Some((id, ty)),
                _ => None,
            })
            .collect();
        for &(id, ty) in &events {
            let (_, short) = ty.rsplit_once("::").unwrap();
            assert_eq!(plain(id), plain(short), "{id}");
        }
        for &(id, fact) in FACTS {
            let Some((event, suffix)) = id.split_once('.') else {
                continue;
            };
            if let (true, Fact::Offset { field, .. }) =
                (events.iter().any(|&(e, _)| e == event), fact)
            {
                assert_eq!(plain(suffix), plain(field), "{id}");
            }
        }
    }

    #[test]
    fn every_game_settings_id_is_named_after_its_engine_field() {
        for &(id, fact) in FACTS {
            if let (Some(suffix), Fact::Offset { field, .. }) =
                (id.strip_prefix("game_settings."), fact)
            {
                assert_eq!(plain(suffix), plain(field), "{id}");
            }
        }
    }

    #[test]
    fn an_enum_constant_is_neither_a_type_nor_an_offset() {
        let fact = fact("damage_type.magical_fire");
        assert!(matches!(fact, Some(Fact::Constant { .. })));
        assert_eq!(type_hash("damage_type.magical_fire"), None);
        assert_eq!(offset_of(fact.unwrap(), |_, _| Some(8)), None);
    }

    #[test]
    fn a_type_id_answers_its_engine_name_and_other_names_pass_through() {
        assert_eq!(type_name("ui_ping_event"), "keen::ecs::UiPingEvent");
        assert_eq!(
            type_name("keen::ecs::UiPingEvent"),
            "keen::ecs::UiPingEvent"
        );
        assert_eq!(
            type_name("world_transform.position"),
            "world_transform.position"
        );
    }

    #[test]
    fn a_path_adds_the_offsets_of_its_fields() {
        let path = Fact::Path(&[("A", "b"), ("B", "c"), ("C", "d")]);
        let field = |ty: &str, name: &str| match (ty, name) {
            ("A", "b") => Some(0x10),
            ("B", "c") => Some(0x08),
            ("C", "d") => Some(0x04),
            _ => None,
        };
        assert_eq!(offset_of(path, field), Some(0x1c));
        let broken = Fact::Path(&[("A", "b"), ("B", "missing")]);
        assert_eq!(offset_of(broken, field), None);
    }

    #[test]
    fn an_offset_adds_its_bytes_into_the_field() {
        let fact = Fact::Offset {
            ty: "A",
            field: "b",
            plus: 4,
        };
        assert_eq!(offset_of(fact, |_, _| Some(8)), Some(12));
        assert_eq!(offset_of(Fact::Type("A"), |_, _| Some(8)), None);
    }

    #[test]
    fn a_type_hash_is_fnv1a_of_the_reflected_name() {
        assert_eq!(type_hash("item_info"), Some(0xb5ce_8765));
        assert_eq!(type_hash("item_registry"), Some(0x72ab_39b6));
        assert_eq!(type_hash("item_info.name"), None);
        assert_eq!(type_hash("no.such.id"), None);
    }

    #[test]
    fn every_offset_and_path_names_types_the_engine_spells_out() {
        for &(id, fact) in FACTS {
            match fact {
                Fact::Type(ty) | Fact::Offset { ty, .. } | Fact::Constant { ty, .. } => {
                    assert!(ty.starts_with("keen::"), "{id}");
                }
                Fact::Path(steps) => {
                    assert!(steps.len() > 1, "{id}");
                    assert!(steps.iter().all(|(ty, _)| ty.starts_with("keen::")), "{id}");
                }
            }
        }
    }

    #[test]
    fn each_slot_constant_names_one_kind() {
        let mut names: Vec<&str> = RECIPE_KINDS.iter().map(|&(name, _)| name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before);
        assert!(matches!(fact(ids::ITEM_SLOT_TYPE), Some(Fact::Type(_))));
        assert!(matches!(fact(ids::ITEM_INFO_ID), Some(Fact::Offset { .. })));
        assert!(matches!(
            fact(ids::ITEM_INFO_EQUIPMENT_SLOT),
            Some(Fact::Path(_))
        ));
    }

    #[test]
    fn a_tag_kind_is_a_kind_no_slot_gives_and_its_layout_is_catalogued() {
        for &(slot, tag, kind) in RECIPE_KIND_TAGS {
            assert!(
                RECIPE_KINDS
                    .iter()
                    .all(|&(name, k)| name != slot && k != kind)
            );
            assert_ne!(kind, RecipeKind::Other);
            assert_ne!(tag, Guid([0; 16]));
        }
        assert!(matches!(
            fact(ids::ITEM_INFO_TAGS),
            Some(Fact::Offset { .. })
        ));
        assert!(matches!(fact(ids::ITEM_TAG_WEIGHT), Some(Fact::Type(_))));
        assert!(matches!(
            fact(ids::ITEM_TAG_WEIGHT_TAG),
            Some(Fact::Offset { .. })
        ));
    }

    #[test]
    fn the_comfort_furniture_tag_reads_as_the_game_data_writes_it() {
        assert_eq!(
            COMFORT_FURNITURE_ITEM_TAG.to_string(),
            "e6e36d0a-3eaf-4cbf-a767-d26aa76ccef1"
        );
    }

    #[test]
    fn decorative_comes_before_comfort_furniture_so_it_wins_an_item_with_both() {
        let row = |kind| RECIPE_KIND_TAGS.iter().position(|&(_, _, k)| k == kind);
        assert!(row(RecipeKind::Decorative) < row(RecipeKind::ComfortFurniture));
        assert!(row(RecipeKind::ComfortFurniture).is_some());
    }

    #[test]
    fn the_decorative_tag_reads_as_the_game_data_writes_it() {
        assert_eq!(
            DECORATIVE_ITEM_TAG.to_string(),
            "ee3b266d-83e7-4239-8bdd-8e58fe12e9d1"
        );
    }

    /// With either executable `ENSH_SERVER_EXE` or `ENSH_CLIENT_EXE` names
    /// (skipped without them), an item's tags are a blob array of tag
    /// entries, each holding its tag as a 16-byte GUID, and every slot of
    /// the tag kinds resolves.
    #[test]
    fn the_item_tags_are_reflected_as_guids() {
        for var in ["ENSH_SERVER_EXE", "ENSH_CLIENT_EXE"] {
            let Ok(path) = std::env::var(var) else {
                continue;
            };
            let bytes = std::fs::read(path).unwrap();
            let image =
                crate::reflect::Image::parse(&bytes, crate::reflect::Layout::File, None).unwrap();
            let reflection = Reflection::scan(&image);
            let Some(Fact::Offset { ty, field, .. }) = fact(ids::ITEM_INFO_TAGS) else {
                panic!("{} is not an offset", ids::ITEM_INFO_TAGS);
            };
            let tags = reflection.type_info(ty).unwrap().fields();
            let tags = tags.iter().find(|f| f.name() == field).unwrap();
            let array = reflection.type_info(tags.type_name().unwrap()).unwrap();
            assert_eq!(array.kind(), crate::reflect::Kind::BlobArray, "{var}");
            assert_eq!(
                array.inner(),
                Some(type_name(ids::ITEM_TAG_WEIGHT)),
                "{var}"
            );
            let Some(Fact::Offset { ty, field, .. }) = fact(ids::ITEM_TAG_WEIGHT_TAG) else {
                panic!("{} is not an offset", ids::ITEM_TAG_WEIGHT_TAG);
            };
            let entry = reflection.type_info(ty).unwrap().fields();
            let tag = entry.iter().find(|f| f.name() == field).unwrap();
            let tag = reflection.type_info(tag.type_name().unwrap()).unwrap();
            assert_eq!(tag.size(), 16, "{var}: {}", tag.name());
            assert_eq!(recipe_kind_tags(&reflection).len(), RECIPE_KIND_TAGS.len());
        }
    }

    /// With either executable `ENSH_SERVER_EXE` or `ENSH_CLIENT_EXE` names
    /// (skipped without them), the marker type registry keeps its types as
    /// a blob array of marker types, and each marker type sorting resolves
    /// to a value of its own.
    #[test]
    fn the_marker_type_registry_is_reflected_as_a_blob_array_of_marker_types() {
        for var in ["ENSH_SERVER_EXE", "ENSH_CLIENT_EXE"] {
            let Ok(path) = std::env::var(var) else {
                continue;
            };
            let bytes = std::fs::read(path).unwrap();
            let image =
                crate::reflect::Image::parse(&bytes, crate::reflect::Layout::File, None).unwrap();
            let reflection = Reflection::scan(&image);
            let Some(Fact::Path(path)) = fact(ids::MAP_MARKER_REGISTRY_TYPES) else {
                panic!("{} is not a path", ids::MAP_MARKER_REGISTRY_TYPES);
            };
            assert_eq!(path[0].0, type_name(ids::MAP_MARKER_REGISTRY), "{var}");
            let &(ty, field) = path.last().unwrap();
            let fields = reflection.type_info(ty).unwrap().fields();
            let types = fields.iter().find(|f| f.name() == field).unwrap();
            let array = reflection.type_info(types.type_name().unwrap()).unwrap();
            assert_eq!(array.kind(), crate::reflect::Kind::BlobArray, "{var}");
            assert_eq!(
                array.inner(),
                Some(type_name(ids::MAP_MARKER_TYPE)),
                "{var}"
            );
            let groups: Vec<&str> = FACTS
                .iter()
                .filter_map(|&(id, fact)| match fact {
                    Fact::Constant { ty, .. } if ty == type_name(ids::MARKER_TYPE_SORTING) => {
                        Some(id)
                    }
                    _ => None,
                })
                .collect();
            let mut values: Vec<u64> = groups
                .iter()
                .map(|&id| constant(id, &reflection).unwrap())
                .collect();
            values.sort_unstable();
            values.dedup();
            assert_eq!(values.len(), groups.len(), "{var}");
            let declared = reflection
                .type_info(type_name(ids::MARKER_TYPE_SORTING))
                .unwrap()
                .enum_values()
                .len();
            assert_eq!(
                groups.len(),
                declared,
                "{var}: a sorting the catalogue lacks"
            );
        }
    }

    /// With either executable `ENSH_SERVER_EXE` or `ENSH_CLIENT_EXE` names
    /// (skipped without them), every slot constant of the kinds resolves,
    /// each to a value of its own.
    #[test]
    fn every_recipe_kind_s_slot_is_reflected() {
        for var in ["ENSH_SERVER_EXE", "ENSH_CLIENT_EXE"] {
            let Ok(path) = std::env::var(var) else {
                continue;
            };
            let bytes = std::fs::read(path).unwrap();
            let image =
                crate::reflect::Image::parse(&bytes, crate::reflect::Layout::File, None).unwrap();
            let reflection = Reflection::scan(&image);
            let slots = recipe_kind_slots(&reflection);
            assert_eq!(slots.len(), RECIPE_KINDS.len(), "{var}");
            let mut values: Vec<u64> = slots.iter().map(|&(value, _)| value).collect();
            values.sort_unstable();
            values.dedup();
            assert_eq!(values.len(), RECIPE_KINDS.len(), "{var}");
        }
    }

    /// With both executables `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` named
    /// (skipped without them), every API name the game names itself stands
    /// for a type, field or enum constant one of them reflects.
    #[test]
    fn every_api_game_name_is_reflected() {
        let (Ok(server), Ok(client)) = (
            std::env::var("ENSH_SERVER_EXE"),
            std::env::var("ENSH_CLIENT_EXE"),
        ) else {
            return;
        };
        let bytes = [
            std::fs::read(server).unwrap(),
            std::fs::read(client).unwrap(),
        ];
        let images = bytes.each_ref().map(|bytes| {
            crate::reflect::Image::parse(bytes, crate::reflect::Layout::File, None).unwrap()
        });
        let reflections = images.each_ref().map(Reflection::scan);
        for row in crate::game_names::GAME_NAMES {
            let found = reflections.iter().any(|reflection| {
                reflection.type_info(row.game_type).is_ok_and(|info| {
                    row.game_member.is_empty()
                        || info.fields().iter().any(|f| f.name() == row.game_member)
                        || info
                            .enum_values()
                            .iter()
                            .any(|v| v.name() == row.game_member)
                })
            });
            assert!(found, "{row:?}");
        }
    }

    /// With both executables `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` name
    /// (skipped without them), every type and offset of the catalogue
    /// resolves in the reflection of at least one: the client alone has
    /// its interface.
    #[test]
    fn every_fact_resolves_in_a_game_executable() {
        let (Ok(server), Ok(client)) = (
            std::env::var("ENSH_SERVER_EXE"),
            std::env::var("ENSH_CLIENT_EXE"),
        ) else {
            return;
        };
        let bytes = [
            std::fs::read(server).unwrap(),
            std::fs::read(client).unwrap(),
        ];
        let images = bytes.each_ref().map(|bytes| {
            crate::reflect::Image::parse(bytes, crate::reflect::Layout::File, None).unwrap()
        });
        let reflections = images.each_ref().map(Reflection::scan);
        for &(id, fact) in FACTS {
            let found = reflections.iter().any(|reflection| match fact {
                Fact::Type(engine) => reflection.type_info(engine).is_ok(),
                Fact::Offset { .. } | Fact::Path(_) | Fact::Constant { .. } => {
                    constant(id, reflection).is_some()
                }
            });
            assert!(found, "{id}");
        }
        for &(id, fact) in FACTS {
            let Fact::Type(engine) = fact else { continue };
            if id.ends_with("_event") {
                let listed = reflections
                    .iter()
                    .any(|reflection| event_types(reflection).any(|ty| ty.name() == engine));
                assert!(listed, "{id} is not a listed event type");
            }
        }
    }
}
