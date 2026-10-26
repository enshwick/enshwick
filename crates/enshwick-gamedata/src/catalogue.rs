//! Engine facts by an id of enshwick's own: the API asks for a type, a hash
//! or a field offset by id, and the loader and the tools answer it from the
//! game's reflection, so a game update that renames or moves something
//! changes this table, not the mods.

use crate::reflect::Reflection;
use enshwick::Off;
use enshwick::content::EngineFacts;

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
}

/// Every fact the API asks for, by id, in the order the API lists its ids.
pub const FACTS: &[(&str, Fact)] = &[
    (
        "world_transform.position",
        offset("keen::WorldTransform", "position"),
    ),
    (
        "world_transform.orientation",
        offset("keen::WorldTransform", "orientation"),
    ),
    (
        "world_transform.scale",
        offset("keen::WorldTransform", "scale"),
    ),
    (
        "npc_command_event",
        Fact::Type("keen::ecs::NpcCommandEvent"),
    ),
    (
        "npc_command_event.kind",
        offset("keen::ecs::NpcCommandEvent", "type"),
    ),
    (
        "npc_command_event.npc",
        offset("keen::ecs::NpcCommandEvent", "npcId"),
    ),
    (
        "npc_command_event.target",
        offset("keen::ecs::NpcCommandEvent", "targetTransform"),
    ),
    (
        "set_toggle_state_event",
        Fact::Type("keen::ecs::SetToggleStateEvent"),
    ),
    (
        "set_toggle_state_event.sender",
        offset("keen::ecs::SetToggleStateEvent", "senderId"),
    ),
    (
        "set_toggle_state_event.signal",
        offset("keen::ecs::SetToggleStateEvent", "signalId"),
    ),
    (
        "set_toggle_state_event.state",
        offset("keen::ecs::SetToggleStateEvent", "newState"),
    ),
    (
        "add_threat_target_event",
        Fact::Type("keen::ecs::AddThreatTargetEvent"),
    ),
    (
        "add_threat_target_event.attacker",
        offset("keen::ecs::AddThreatTargetEvent", "attackerId"),
    ),
    (
        "add_threat_target_event.target",
        offset("keen::ecs::AddThreatTargetEvent", "targetId"),
    ),
    (
        "add_threat_target_event.threat",
        offset("keen::ecs::AddThreatTargetEvent", "threatValue"),
    ),
    ("ui_ping_event", Fact::Type("keen::ecs::UiPingEvent")),
    (
        "ui_ping_event.sender",
        offset("keen::ecs::UiPingEvent", "senderEntityId"),
    ),
    (
        "ui_ping_event.position",
        offset("keen::ecs::UiPingEvent", "position"),
    ),
    ("item_info", Fact::Type("keen::ItemInfo")),
    ("item_info.id", offset("keen::ItemInfo", "itemId")),
    ("item_info.object", offset("keen::ItemInfo", "objectId")),
    ("item_info.name", offset("keen::ItemInfo", "name")),
    (
        "item_info.description",
        offset("keen::ItemInfo", "description"),
    ),
    ("item_info.flags", offset("keen::ItemInfo", "flags")),
    (
        "item_info.pid_entity",
        offset("keen::ItemInfo", "pidEntity"),
    ),
    ("item_info.ui_values", offset("keen::ItemInfo", "uiValues")),
    (
        "item_info.icon_image",
        offset("keen::ItemInfo", "iconImage"),
    ),
    (
        "item_info.placed_entity",
        Fact::Path(&[
            ("keen::ItemInfo", "equipment"),
            ("keen::EquipmentSetup", "placedEntity"),
        ]),
    ),
    ("item_stat", Fact::Type("keen::ImpactUiValue")),
    ("item_stat.label", offset("keen::ImpactUiValue", "locaId")),
    (
        "item_stat.format",
        offset("keen::ImpactUiValue", "valueFormat"),
    ),
    ("item_stat.value", offset("keen::ImpactUiValue", "value")),
    (
        "item_stat.signed",
        offset("keen::ImpactUiValue", "isSigned"),
    ),
    ("item_registry", Fact::Type("keen::ItemRegistryResource")),
    (
        "item_registry.refs",
        offset("keen::ItemRegistryResource", "itemRefs"),
    ),
    (
        "item_registry.names",
        offset("keen::ItemRegistryResource", "dbgNames"),
    ),
    ("template", Fact::Type("keen::ecs::TemplateResource")),
    (
        "template.components",
        offset("keen::ecs::TemplateResource", "components"),
    ),
    (
        "template_collection",
        Fact::Type("keen::ecs::TemplateCollectionResource"),
    ),
    (
        "template_collection.templates",
        offset("keen::ecs::TemplateCollectionResource", "templates"),
    ),
    ("fuel_setup", Fact::Type("keen::ecs::FuelSetup")),
    (
        "fuel_setup.max",
        offset("keen::ecs::FuelSetup", "maxFuelCharges"),
    ),
    (
        "fuel_setup.start",
        offset("keen::ecs::FuelSetup", "initialFuelPercentage"),
    ),
    ("crafting", Fact::Type("keen::ecs::Crafting")),
    (
        "crafting.workshop",
        offset("keen::ecs::Crafting", "workshop"),
    ),
    (
        "crafting.workshop_id",
        offset("keen::ecs::Crafting", "workshopId"),
    ),
    (
        "crafting_interaction",
        Fact::Type("keen::ecs::CraftingInteraction"),
    ),
    (
        "crafting_interaction.workshop",
        offset("keen::ecs::CraftingInteraction", "workshop"),
    ),
    (
        "crafting_interaction.workshop_id",
        offset("keen::ecs::CraftingInteraction", "workshopId"),
    ),
    (
        "client_crafting_interaction",
        Fact::Type("keen::ecs::ClientCraftingInteraction"),
    ),
    (
        "client_crafting_interaction.workshop_id",
        offset("keen::ecs::ClientCraftingInteraction", "workshopId"),
    ),
    (
        "workshop_registry",
        Fact::Type("keen::WorkshopRegistryResource"),
    ),
    (
        "workshop_registry.workshops",
        offset("keen::WorkshopRegistryResource", "workshops"),
    ),
    ("workshop", Fact::Type("keen::WorkshopResource")),
    (
        "workshop.guid",
        offset("keen::WorkshopResource", "workshopGuid"),
    ),
    (
        "workshop.id",
        offset("keen::WorkshopResource", "workshopId"),
    ),
    ("workshop.name", offset("keen::WorkshopResource", "name")),
    ("workshop.item", offset("keen::WorkshopResource", "item")),
    (
        "workshop.item_ref",
        offset("keen::WorkshopResource", "itemRef"),
    ),
    ("workshop.npc", offset("keen::WorkshopResource", "isNpc")),
    (
        "recipe_registry",
        Fact::Type("keen::RecipeRegistryResource"),
    ),
    (
        "recipe_registry.recipes",
        offset("keen::RecipeRegistryResource", "recipes"),
    ),
    ("recipe", Fact::Type("keen::RecipeInfo")),
    ("recipe.guid", offset("keen::RecipeInfo", "recipeGuid")),
    ("recipe.id", offset("keen::RecipeInfo", "recipeId")),
    (
        "recipe.workshop_guid",
        offset("keen::RecipeInfo", "workshopGuid"),
    ),
    ("recipe.workshop", offset("keen::RecipeInfo", "workshopId")),
    ("recipe.input", offset("keen::RecipeInfo", "input")),
    ("recipe.output", offset("keen::RecipeInfo", "output")),
    (
        "recipe.completion",
        offset("keen::RecipeInfo", "completionRequirementQuery"),
    ),
    (
        "recipe.required_props",
        offset("keen::RecipeInfo", "requiredProps"),
    ),
    ("recipe.debug_name", offset("keen::RecipeInfo", "debugName")),
    ("recipe_ingredient", Fact::Type("keen::RecipeInputResource")),
    ("recipe_stack", Fact::Type("keen::RecipeItemStackResource")),
    (
        "recipe_stack.item",
        offset("keen::RecipeItemStackResource", "item"),
    ),
    (
        "recipe_stack.count",
        offset("keen::RecipeItemStackResource", "count"),
    ),
    ("ui_image", Fact::Type("keen::UiTextureResource")),
    ("ui_bundle", Fact::Type("keen::FbUiBundle")),
    (
        "ui_bundle.crafting_menu",
        Fact::Path(&[
            ("keen::FbUiBundle", "menu"),
            ("keen::FbUiMenu", "crafting"),
            ("keen::FbUiCrafting", "recipes"),
        ]),
    ),
    (
        "crafting_menu.workshops",
        offset("keen::FbUiCraftingRecipeCollection", "workshops"),
    ),
    (
        "crafting_menu.trees",
        offset("keen::FbUiCraftingRecipeCollection", "trees"),
    ),
    (
        "crafting_menu.max_groups",
        offset("keen::FbUiCraftingRecipeCollection", "maxRecipeGroupCount"),
    ),
    (
        "crafting_menu.max_sets",
        offset("keen::FbUiCraftingRecipeCollection", "maxRecipeSetCount"),
    ),
    (
        "crafting_menu_tree",
        Fact::Type("keen::FbUiCraftingRecipeTree"),
    ),
    (
        "crafting_menu_tree.groups",
        offset("keen::FbUiCraftingRecipeTree", "groups"),
    ),
    (
        "crafting_menu_group",
        Fact::Type("keen::FbUiCraftingRecipeGroup"),
    ),
    (
        "crafting_menu_group.id",
        offset("keen::FbUiCraftingRecipeGroup", "groupId"),
    ),
    (
        "crafting_menu_group.sets",
        offset("keen::FbUiCraftingRecipeGroup", "sets"),
    ),
    (
        "crafting_menu_set",
        Fact::Type("keen::FbUiCraftingRecipeSet"),
    ),
    (
        "crafting_menu_set.entries",
        offset("keen::FbUiCraftingRecipeSet", "entries"),
    ),
];

const fn offset(ty: &'static str, field: &'static str) -> Fact {
    Fact::Offset { ty, field, plus: 0 }
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
        Fact::Type(_) => None,
    }
}

/// The value of the offset named `id` in `reflection`; `None` when the id
/// names no offset or reflection lacks its type or field.
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
    offset_of(fact(id)?, |ty, field| {
        reflection.field_offset(ty, field).ok()
    })
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
                Fact::Type(ty) | Fact::Offset { ty, .. } => {
                    assert!(ty.starts_with("keen::"), "{id}");
                }
                Fact::Path(steps) => {
                    assert!(steps.len() > 1, "{id}");
                    assert!(steps.iter().all(|(ty, _)| ty.starts_with("keen::")), "{id}");
                }
            }
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
                Fact::Offset { .. } | Fact::Path(_) => constant(id, reflection).is_some(),
            });
            assert!(found, "{id}");
        }
    }
}
