//! Engine facts by an id of enshwick's own, answered from the game's
//! reflection.
//!
//! The API asks for a type, a hash or a field offset by id, and the loader
//! and the tools answer it, so a game update that renames or moves something
//! changes this table, not the mods.

mod facts;

pub use facts::FACTS;

use crate::reflect::{Reflection, TypeInfo};
use enshwick::Off;
use enshwick::content::EngineFacts;
use enshwick::content::recipes::RecipeKind;

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

/// The id of the enum an item's equipment slot holds, which
/// [`RECIPE_KINDS`] names constants of.
pub const SLOT_TYPE: &str = "item_slot_type";

/// The id of the offset of the item id in an item's static data.
pub const ITEM_ID: &str = "item_info.id";

/// The id of the offset of the equipment slot in an item's static data.
pub const ITEM_SLOT: &str = "item_info.equipment_slot";

/// The id of the offset of how many of an item one slot holds, a `u16`,
/// in an item's static data.
pub const ITEM_MAX_STACK: &str = "item_info.max_stack";

/// The id of the offset of how long a recipe takes, a `keen::Time` in
/// nanoseconds, in a recipe.
pub const RECIPE_DURATION: &str = "recipe.duration";

/// The id of the offset of an item's flags, a `u32`, in an item's static
/// data.
pub const ITEM_FLAGS: &str = "item_info.flags";

/// The id of the offset of an item's `ItemCategory`, a `u8`, in an item's
/// static data.
pub const ITEM_CATEGORY: &str = "item_info.category";

/// The id of the offset of the template an item places, a GUID, in an
/// item's static data.
pub const ITEM_PLACED_ENTITY: &str = "item_info.placed_entity";

/// The id of a weapon's damage factor, an `f32`, in an item's static data.
pub const ITEM_DAMAGE_MOD: &str = "item_info.damage_mod";
/// The id of a weapon's attack speed factor, an `f32`.
pub const ITEM_SPEED_MOD: &str = "item_info.speed_mod";
/// The id of the flag that says an item deals damage.
pub const ITEM_DAMAGE_SET: &str = "item_info.damage_set";
/// The id of an item's armour values, `f32` each, as many as
/// [`ARMOR_DISTRIBUTION`] holds.
pub const ITEM_ARMOR: &str = "item_info.armor";
/// The id of the flag that says an item gives armour.
pub const ITEM_ARMOR_SET: &str = "item_info.armor_set";
/// The id of how long an item burns as fuel, a `keen::Time` in nanoseconds.
pub const ITEM_BURN_TIME: &str = "item_info.burn_time";
/// The id of the energy an item gives as fuel, a `u32`.
pub const ITEM_ENERGY_OUTPUT: &str = "item_info.energy_output";
/// The id of the flag that says an item is fuel.
pub const ITEM_FUEL_SET: &str = "item_info.fuel_set";
/// The id of the comfort a placed item gives, a `u32`.
pub const ITEM_COMFORT_AMOUNT: &str = "item_info.comfort_amount";
/// The id of the flag that says an item gives comfort.
pub const ITEM_COMFORT_SET: &str = "item_info.comfort_set";

/// The id of the random loot table's type, of which the game has one.
pub const LOOT_RANDOM_TABLE: &str = "lootable_items";

/// The id of a container fill table's type.
pub const LOOT_FILL_TABLE: &str = "loot_fill_table";

/// The id of the armour values' type, whose size counts them.
pub const ARMOR_DISTRIBUTION: &str = "armor_distribution";

/// The kind of the recipes that make an item, by the name of the constant
/// of [`SLOT_TYPE`] the item's equipment slot holds; every other slot is
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
    let slot = type_name(SLOT_TYPE);
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
    fn every_id_is_listed_once() {
        let mut ids: Vec<&str> = FACTS.iter().map(|&(id, _)| id).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before);
    }

    #[test]
    fn the_catalogue_lists_exactly_the_ids_the_api_asks_for_in_its_order() {
        let ours: Vec<&str> = FACTS.iter().map(|&(id, _)| id).collect();
        assert_eq!(enshwick::__private::ENGINE_IDS, ours.as_slice());
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
        assert!(matches!(fact(SLOT_TYPE), Some(Fact::Type(_))));
        assert!(matches!(fact(ITEM_ID), Some(Fact::Offset { .. })));
        assert!(matches!(fact(ITEM_SLOT), Some(Fact::Path(_))));
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
