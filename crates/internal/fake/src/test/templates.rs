//! The fake's reflection of entity templates and the components our content
//! edits in them, and the vanilla templates those edits start from.

use crate::content::items::METER_TEMPLATE_BASE;
use crate::content::workshops::stations;
use crate::content::{EngineFacts, Guid};
use crate::resources::{Constant, Field, Kind, Registry, Type, Value, Variant, fnv1a32};
use enshwick_core::__fake::ids;

use super::FakeHost;

mod enemies;

pub use enemies::{BITE, ENEMY, ENEMY_ARSENAL, HOWL, LUNGE, NEST, NEST_GROUP, OTHER_ARSENAL};

/// The template types and every type they hold, by their qualified names.
pub(crate) const TEMPLATE: &str = "demo::ecs::TemplateResource";
const COLLECTION: &str = "demo::ecs::TemplateCollectionResource";
const COMPONENT: &str = "demo::ecs::Component";
const CRAFTING: &str = "demo::ecs::Crafting";
const INTERACTION: &str = "demo::ecs::CraftingInteraction";
const CLIENT: &str = "demo::ecs::ClientCraftingInteraction";
const FUEL_SETUP: &str = "demo::ecs::FuelSetup";
const FACTION: &str = "demo::ecs::Faction";
const AGGRO: &str = "demo::ecs::Aggro";
const BOSS: &str = "demo::ecs::Boss";

/// The workshop the vanilla Workbench's template crafts at.
pub(crate) const WORKBENCH_WORKSHOP: Guid = Guid::parse("965d3192-0287-49a2-ad51-a716139571a6");

/// A vanilla creature template the fake holds: family `Npc`, a faction and
/// an aggro component.
pub const CREATURE: Guid = Guid::parse("c0ffee00-0000-4000-8000-00000000c0de");

/// Another `Npc` template, a boss, so `Boss` is a component the family has.
pub const BOSS_CREATURE: Guid = Guid::parse("c0ffee00-0000-4000-8000-00000000b055");

/// Each type the presets resolve by an enshwick id, with its qualified name.
const NAMED: &[(&str, &str)] = &[
    (ids::TEMPLATE, TEMPLATE),
    ("ecs::TemplateResource", TEMPLATE),
    (ids::TEMPLATE_COLLECTION, COLLECTION),
    ("ecs::TemplateCollectionResource", COLLECTION),
    (ids::CRAFTING, CRAFTING),
    ("ecs::Crafting", CRAFTING),
    (ids::CRAFTING_INTERACTION, INTERACTION),
    ("ecs::CraftingInteraction", INTERACTION),
    (ids::CLIENT_CRAFTING_INTERACTION, CLIENT),
    ("ecs::ClientCraftingInteraction", CLIENT),
    (ids::FUEL_SETUP, FUEL_SETUP),
    ("ecs::FuelSetup", FUEL_SETUP),
    ("ecs::Faction", FACTION),
    ("ecs::Aggro", AGGRO),
    ("ecs::Boss", BOSS),
];

/// The reflected types, laid out as the game lays them out where the
/// generated items say so.
fn types() -> Vec<Type<'static>> {
    let field = Field::new;
    let mut types =
        vec![
            Type::new("demo::bool", Kind::Bool, 1, 1),
            Type::new("demo::uint8", Kind::Uint8, 1, 1),
            Type::new("demo::uint16", Kind::Uint16, 2, 2),
            Type::new("demo::uint32", Kind::Uint32, 4, 4),
            Type::new("demo::uint64", Kind::Uint64, 8, 8),
            Type::new("demo::sint32", Kind::Sint32, 4, 4),
            Type::new("demo::sint64", Kind::Sint64, 8, 8),
            Type::new("demo::float32", Kind::Float32, 4, 4),
            Type::new("demo::Guid", Kind::Guid, 16, 4),
            Type::new("demo::BlobString", Kind::BlobString, 8, 4),
            Type::new(COMPONENT, Kind::Struct, 1, 1),
            Type::new("demo::Time", Kind::Struct, 8, 8).with_fields(vec![field(
                "value",
                0,
                "demo::sint64",
            )]),
            Type::new("demo::HashKey32", Kind::Struct, 4, 4).with_fields(vec![field(
                "value",
                0,
                "demo::uint32",
            )]),
            Type::new(
                "demo::ObjectReference<demo::Workshop>",
                Kind::ObjectReference,
                16,
                4,
            ),
            Type::new(
                "demo::ObjectReference<demo::ecs::TemplateResource>",
                Kind::ObjectReference,
                16,
                4,
            )
            .with_inner(TEMPLATE),
            Type::new("demo::ecs::TemplateFamily", Kind::Enum, 1, 1)
                .with_inner("demo::uint8")
                .with_constants(vec![
                    Constant::new("Factory", 4),
                    Constant::new("Npc", 5),
                    Constant::new("Prop", 8),
                ]),
            Type::new("demo::FactionType", Kind::Enum, 1, 1)
                .with_inner("demo::uint8")
                .with_constants(vec![Constant::new("Vukah", 3), Constant::new("Player", 7)]),
            Type::new("demo::ecs::EntityProxyModelId", Kind::Struct, 2, 2)
                .with_fields(vec![field("value", 0, "demo::uint16")]),
            Type::new(
                "demo::BlobVariant<demo::ecs::Component>",
                Kind::BlobVariant,
                12,
                4,
            )
            .with_inner(COMPONENT),
            Type::new(
                "demo::BlobArray<demo::BlobVariant<demo::ecs::Component>>",
                Kind::BlobArray,
                8,
                4,
            )
            .with_inner("demo::BlobVariant<demo::ecs::Component>"),
            Type::new(
                "demo::BlobArray<demo::ObjectReference<demo::ecs::TemplateResource>>",
                Kind::BlobArray,
                8,
                4,
            )
            .with_inner("demo::ObjectReference<demo::ecs::TemplateResource>"),
        ];
    types.extend(templates_and_components());
    types
}

/// The template, its collection and the components the fake's templates hold.
fn templates_and_components() -> Vec<Type<'static>> {
    let field = Field::new;
    let component = |name, size, fields| {
        Type::new(name, Kind::Struct, size, 4)
            .with_inner(COMPONENT)
            .with_fields(fields)
    };
    vec![
        Type::new(TEMPLATE, Kind::Struct, 0x18, 4).with_fields(vec![
            field("name", 0, "demo::BlobString"),
            field("family", 8, "demo::ecs::TemplateFamily"),
            field("predictEntity", 9, "demo::bool"),
            field("farCulling", 10, "demo::bool"),
            field("omitTransformReplication", 11, "demo::bool"),
            field("proxyModelId", 12, "demo::ecs::EntityProxyModelId"),
            field(
                "components",
                0x10,
                "demo::BlobArray<demo::BlobVariant<demo::ecs::Component>>",
            ),
        ]),
        Type::new(COLLECTION, Kind::Struct, 8, 4).with_fields(vec![field(
            "templates",
            0,
            "demo::BlobArray<demo::ObjectReference<demo::ecs::TemplateResource>>",
        )]),
        component(
            CRAFTING,
            0x18,
            vec![
                field("workshop", 0, "demo::ObjectReference<demo::Workshop>"),
                field("workshopId", 0x10, "demo::HashKey32"),
            ],
        ),
        component(
            INTERACTION,
            0x20,
            vec![
                field("craftingPropsInRange", 0, "demo::uint64"),
                field("nextRecipeComfortRequirement", 8, "demo::uint32"),
                field("workshop", 12, "demo::ObjectReference<demo::Workshop>"),
                field("workshopId", 28, "demo::HashKey32"),
            ],
        ),
        component(
            CLIENT,
            0x10,
            vec![
                field("craftingPropsInRange", 0, "demo::uint64"),
                field("nextRecipeComfortRequirement", 8, "demo::uint32"),
                field("workshopId", 12, "demo::HashKey32"),
            ],
        ),
        Type::new("demo::ecs::FuelRefillSetup", Kind::Struct, 16, 8).with_fields(vec![
            field("fillDuration", 0, "demo::Time"),
            field("enabled", 8, "demo::bool"),
        ]),
        component(
            FUEL_SETUP,
            40,
            vec![
                field("automaticRefill", 0, "demo::ecs::FuelRefillSetup"),
                field("rainRefill", 16, "demo::ecs::FuelRefillSetup"),
                field("maxFuelCharges", 32, "demo::sint32"),
                field("initialFuelPercentage", 36, "demo::float32"),
            ],
        ),
        component(FACTION, 1, vec![field("faction", 0, "demo::FactionType")]),
        Type::new("demo::AttachmentSlot", Kind::Enum, 1, 1)
            .with_inner("demo::uint8")
            .with_constants(vec![Constant::new("Root", 0), Constant::new("Head", 1)]),
        component(
            AGGRO,
            2,
            vec![
                field("applyEnemyValue", 0, "demo::bool"),
                field("sightCheckSlot", 1, "demo::AttachmentSlot"),
            ],
        ),
        component(BOSS, 1, Vec::new()),
    ]
}

/// A template value of `family` with `components`, by qualified name.
fn template(name: &str, family: &str, components: Vec<(&str, Value)>) -> Value {
    Value::Struct(vec![
        ("name".into(), Value::Text(name.into())),
        ("family".into(), Value::Name(family.into())),
        ("predictEntity".into(), Value::Bool(false)),
        ("farCulling".into(), Value::Bool(false)),
        ("omitTransformReplication".into(), Value::Bool(false)),
        (
            "proxyModelId".into(),
            Value::Struct(vec![("value".into(), Value::Uint(3))]),
        ),
        (
            "components".into(),
            Value::List(
                components
                    .into_iter()
                    .map(|(ty, value)| Value::Variant(Box::new(Variant::new(ty, value))))
                    .collect(),
            ),
        ),
    ])
}

/// A workshop reference and id, as the crafting components hold them.
fn at_workshop(workshop: Guid) -> Vec<(String, Value)> {
    vec![
        ("workshop".into(), Value::Guid(workshop)),
        (
            "workshopId".into(),
            Value::Struct(vec![(
                "value".into(),
                Value::Uint(u64::from(workshop.fnv1a())),
            )]),
        ),
    ]
}

/// The vanilla templates the fake's archive holds, as the codec writes them.
fn vanilla(registry: &Registry<'_>) -> Vec<(Guid, Vec<u8>)> {
    let refill = || {
        Value::Struct(vec![
            (
                "fillDuration".into(),
                Value::Struct(vec![("value".into(), Value::Int(0))]),
            ),
            ("enabled".into(), Value::Bool(false)),
        ])
    };
    let props = || {
        vec![
            ("craftingPropsInRange".to_owned(), Value::Uint(0)),
            ("nextRecipeComfortRequirement".to_owned(), Value::Uint(0)),
        ]
    };
    let mut interaction = props();
    interaction.extend(at_workshop(WORKBENCH_WORKSHOP));
    let mut client = props();
    client.extend(at_workshop(WORKBENCH_WORKSHOP).into_iter().skip(1));
    let faction =
        |faction: &str| Value::Struct(vec![("faction".into(), Value::Name(faction.into()))]);
    let aggro = Value::Struct(vec![
        ("applyEnemyValue".into(), Value::Bool(true)),
        ("sightCheckSlot".into(), Value::Name("Head".into())),
    ]);
    let templates = [
        (
            stations::WORKBENCH.template,
            template(
                "Craftingstation_Workbench",
                "Factory",
                vec![
                    (CRAFTING, Value::Struct(at_workshop(WORKBENCH_WORKSHOP))),
                    (INTERACTION, Value::Struct(interaction)),
                    (CLIENT, Value::Struct(client)),
                ],
            ),
        ),
        (
            METER_TEMPLATE_BASE,
            template(
                "Pide_Fuel_Water_Pouch",
                "Prop",
                vec![(
                    FUEL_SETUP,
                    Value::Struct(vec![
                        ("automaticRefill".into(), refill()),
                        ("rainRefill".into(), refill()),
                        ("maxFuelCharges".into(), Value::Int(10)),
                        ("initialFuelPercentage".into(), Value::Float(1.0)),
                    ]),
                )],
            ),
        ),
        (
            CREATURE,
            template(
                "Enemy_Wolf",
                "Npc",
                vec![(FACTION, faction("Player")), (AGGRO, aggro.clone())],
            ),
        ),
        (
            BOSS_CREATURE,
            template(
                "Enemy_Wolf_Boss",
                "Npc",
                vec![
                    (FACTION, faction("Vukah")),
                    (BOSS, Value::Struct(Vec::new())),
                ],
            ),
        ),
    ];
    // Each value is the fake's own, of the fake's types, so every one writes.
    templates
        .into_iter()
        .filter_map(|(guid, value)| Some((guid, registry.encode(TEMPLATE, &value, None).ok()?)))
        .collect()
}

impl FakeHost {
    /// Entity templates as the game reflects them, and in the archive the
    /// vanilla templates content edits start from: the Workbench's, the
    /// water pouch's, [`CREATURE`] and [`BOSS_CREATURE`]. What
    /// [`Load::add_template`](crate::Load::add_template) and
    /// [`Load::change_template`](crate::Load::change_template) resolve;
    /// [`FakeHost::workshops`] and [`FakeHost::meters`] add it.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_fake as enshwick;
    /// use enshwick::content::EngineFacts;
    /// use enshwick::test::FakeHost;
    ///
    /// let host = FakeHost::new().templates();
    /// assert_eq!(host.hash("ecs::TemplateResource")?, host.hash("template")?);
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    #[must_use]
    pub fn templates(mut self) -> Self {
        let types = types();
        let registry = Registry::new(types.clone());
        for (guid, bytes) in vanilla(&registry) {
            self = self.resource(guid.key_by_hash(fnv1a32(TEMPLATE.as_bytes())), bytes);
        }
        for &(id, name) in NAMED {
            self = self.type_hash(id, fnv1a32(name.as_bytes()));
        }
        self.types(types)
    }
}

/// The registries and the component a crafting prop is written in.
const WORKSHOP_REGISTRY: &str = "demo::WorkshopRegistryResource";
const RECIPE_REGISTRY: &str = "demo::RecipeRegistryResource";
const PROP_ENTRY: &str = "demo::CraftingPropResource";
const RECIPE: &str = "demo::RecipeInfo";
const REQUIRED_PROP: &str = "demo::ecs::RequiredCraftingProp";

/// A vanilla prop item the fake holds: a chest, placing a template of
/// family `Prop` without components.
pub const PROP_ITEM: Guid = Guid::parse("c0ffee00-0000-4000-8000-0000000c4e57");

/// A vanilla crafting prop's item, placing a `Prop` template that marks it
/// a crafting prop, so the family has that component.
pub const CRAFTING_PROP_ITEM: Guid = Guid::parse("c0ffee00-0000-4000-8000-00000000a0a0");

/// How many crafting props the fake's workshop registry lists: two short of
/// its 64-bit mask.
pub const VANILLA_CRAFTING_PROPS: usize = 62;

const PROP_TEMPLATE: Guid = Guid::parse("c0ffee00-0000-4000-8000-0000000c4e58");
const CRAFTING_PROP_TEMPLATE: Guid = Guid::parse("c0ffee00-0000-4000-8000-00000000a0a1");

/// The registries as the fake reflects them, at the offsets
/// [`FakeHost::workshops`] and [`FakeHost::recipes`] give.
fn registry_types() -> Vec<Type<'static>> {
    let field = Field::new;
    let array = |of: &'static str| {
        Type::new(
            Box::leak(format!("demo::BlobArray<{of}>").into_boxed_str()),
            Kind::BlobArray,
            8,
            4,
        )
        .with_inner(of)
    };
    vec![
        Type::new(WORKSHOP_REGISTRY, Kind::Struct, 16, 4).with_fields(vec![
            field("workshops", 0, "demo::BlobArray<demo::uint8>"),
            field(
                "craftingProps",
                8,
                "demo::BlobArray<demo::CraftingPropResource>",
            ),
        ]),
        array("demo::uint8"),
        array(PROP_ENTRY),
        Type::new(PROP_ENTRY, Kind::Struct, 48, 4).with_fields(vec![
            field("craftingPropGuid", 0, "demo::Guid"),
            field("craftingPropId", 16, "demo::HashKey32"),
            field("craftingPropIndex", 20, "demo::uint8"),
            field("name", 24, "demo::HashKey32"),
            field("item", 28, "demo::HashKey32"),
            field("itemRef", 32, "demo::Guid"),
        ]),
        Type::new(RECIPE_REGISTRY, Kind::Struct, 8, 4).with_fields(vec![field(
            "recipes",
            0,
            "demo::BlobArray<demo::RecipeInfo>",
        )]),
        array(RECIPE),
        array("demo::HashKey32"),
        Type::new(RECIPE, Kind::Struct, 0xa8, 8).with_fields(vec![
            field("recipeGuid", 0, "demo::Guid"),
            field("recipeId", 0x10, "demo::HashKey32"),
            field("workshopId", 0x24, "demo::HashKey32"),
            field("requiredProps", 0x38, "demo::BlobArray<demo::HashKey32>"),
            field("requiredPropsMask", 0x48, "demo::uint64"),
            field("level", 0x80, "demo::uint32"),
            field("isUpgrade", 0xa0, "demo::bool"),
        ]),
        Type::new(REQUIRED_PROP, Kind::Struct, 20, 4)
            .with_inner(COMPONENT)
            .with_fields(vec![
                field(
                    "type",
                    0,
                    "demo::ObjectReference<demo::CraftingPropDefinition>",
                ),
                field("craftingPropId", 16, "demo::HashKey32"),
            ]),
        Type::new(
            "demo::ObjectReference<demo::CraftingPropDefinition>",
            Kind::ObjectReference,
            16,
            4,
        ),
    ]
}

/// A game id as the codec writes one.
fn game_id(id: u32) -> Value {
    Value::Struct(vec![("value".into(), Value::Uint(id.into()))])
}

/// A vanilla item placing `template`, laid out as [`FakeHost::items`] says.
fn placing(item: Guid, template: Guid) -> Vec<u8> {
    let mut info = vec![0u8; 0xc0];
    info[..4].copy_from_slice(&item.fnv1a().to_le_bytes());
    info[4..20].copy_from_slice(&item.0);
    info[0xb0..0xc0].copy_from_slice(&template.0);
    info
}

impl FakeHost {
    /// Prop types and crafting props: [`FakeHost::items`],
    /// [`FakeHost::workshops`] and [`FakeHost::recipes`], the registries
    /// reflected, and in the archive the vanilla [`PROP_ITEM`] and
    /// [`CRAFTING_PROP_ITEM`] with their templates and a workshop registry
    /// of [`VANILLA_CRAFTING_PROPS`] crafting props. What
    /// [`Load::add_prop`](crate::Load::add_prop) resolves.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_fake as enshwick;
    /// use enshwick::content::EngineFacts;
    /// use enshwick::test::FakeHost;
    ///
    /// let host = FakeHost::new().props();
    /// assert!(host.hash("workshop_registry").is_ok());
    /// ```
    #[must_use]
    pub fn props(self) -> Self {
        let mut types = types();
        types.extend(registry_types());
        let registry = Registry::new(types.clone());
        let hash = |name: &str| fnv1a32(name.as_bytes());
        let props = (0..VANILLA_CRAFTING_PROPS)
            .map(|i| {
                let guid = Guid([u8::try_from(i).unwrap_or(0); 16]);
                Value::Struct(vec![
                    ("craftingPropGuid".into(), Value::Guid(guid)),
                    ("craftingPropId".into(), game_id(guid.fnv1a())),
                    ("craftingPropIndex".into(), Value::Uint(i as u64)),
                ])
            })
            .collect();
        let workshops = Value::Struct(vec![
            ("workshops".into(), Value::List(Vec::new())),
            ("craftingProps".into(), Value::List(props)),
        ]);
        let mark = Value::Struct(vec![
            ("type".into(), Value::Guid(Guid([0; 16]))),
            ("craftingPropId".into(), game_id(Guid([0; 16]).fnv1a())),
        ]);
        let templates = [
            (PROP_TEMPLATE, template("Prop_Chest", "Prop", Vec::new())),
            (
                CRAFTING_PROP_TEMPLATE,
                template("Prop_Mortar", "Prop", vec![(REQUIRED_PROP, mark)]),
            ),
        ];
        let mut host = self.items().recipes().workshops().types(types);
        for (guid, value) in templates {
            if let Ok(bytes) = registry.encode(TEMPLATE, &value, None) {
                host = host.resource(guid.key_by_hash(hash(TEMPLATE)), bytes);
            }
        }
        if let Ok(bytes) = registry.encode(WORKSHOP_REGISTRY, &workshops, None) {
            let key = crate::content::workshops::WORKSHOP_REGISTRY;
            host = host.resource(key.key_by_hash(hash(WORKSHOP_REGISTRY)), bytes);
        }
        let item = host.hash(ids::ITEM_INFO).unwrap_or(0);
        host.resource(
            PROP_ITEM.key_by_hash(item),
            placing(PROP_ITEM, PROP_TEMPLATE),
        )
        .resource(
            CRAFTING_PROP_ITEM.key_by_hash(item),
            placing(CRAFTING_PROP_ITEM, CRAFTING_PROP_TEMPLATE),
        )
        .type_hash(ids::WORKSHOP_REGISTRY, hash(WORKSHOP_REGISTRY))
        .type_hash(ids::RECIPE_REGISTRY, hash(RECIPE_REGISTRY))
        .type_hash("ecs::RequiredCraftingProp", hash(REQUIRED_PROP))
    }

    /// The recipe registry with each of `recipes` and the props it requires,
    /// as the fake reflects it, to build served registries from.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_fake as enshwick;
    /// use enshwick::content::Guid;
    ///
    /// let base = enshwick::test::FakeHost::recipe_registry(&[(Guid([1; 16]), &[])]);
    /// assert!(!base.is_empty());
    /// ```
    #[must_use]
    pub fn recipe_registry(recipes: &[(Guid, &[u32])]) -> Vec<u8> {
        let recipes = recipes
            .iter()
            .map(|(guid, required)| {
                Value::Struct(vec![
                    ("recipeGuid".into(), Value::Guid(*guid)),
                    ("recipeId".into(), game_id(guid.fnv1a())),
                    (
                        "requiredProps".into(),
                        Value::List(required.iter().copied().map(game_id).collect()),
                    ),
                    ("requiredPropsMask".into(), Value::Uint(0)),
                ])
            })
            .collect();
        encode_recipes(recipes)
    }

    /// The vanilla recipe registry in the archive holding one upgrade
    /// recipe per `(guid, workshop, level)`: a recipe of the workshop with
    /// game id `workshop` that upgrades its station from that level, beside
    /// [`FakeHost::props`]. The altar's own upgrades share the workshop of
    /// the one at level 0, the claim.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_fake as enshwick;
    /// use enshwick::content::Guid;
    ///
    /// let claim = (Guid([1; 16]), 7, 0);
    /// let _host = enshwick::test::FakeHost::new().props().upgrade_recipes(&[claim]);
    /// ```
    #[must_use]
    pub fn upgrade_recipes(self, recipes: &[(Guid, u32, u32)]) -> Self {
        let recipes = recipes
            .iter()
            .map(|&(guid, workshop, level)| {
                Value::Struct(vec![
                    ("recipeGuid".into(), Value::Guid(guid)),
                    ("recipeId".into(), game_id(guid.fnv1a())),
                    ("workshopId".into(), game_id(workshop)),
                    ("level".into(), Value::Uint(level.into())),
                    ("isUpgrade".into(), Value::Bool(true)),
                ])
            })
            .collect();
        let key = crate::content::recipes::RECIPE_REGISTRY;
        self.resource(
            key.key_by_hash(fnv1a32(RECIPE_REGISTRY.as_bytes())),
            encode_recipes(recipes),
        )
    }
}

/// The recipe registry holding `recipes`, as the fake reflects it; empty when
/// they do not fit its recipe type, so the mod that reads it fails there.
fn encode_recipes(recipes: Vec<Value>) -> Vec<u8> {
    let mut types = types();
    types.extend(registry_types());
    Registry::new(types)
        .encode(
            RECIPE_REGISTRY,
            &Value::Struct(vec![("recipes".into(), Value::List(recipes))]),
            None,
        )
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fake_s_registries_write_with_their_types() {
        assert!(!FakeHost::recipe_registry(&[(Guid([1; 16]), &[7])]).is_empty());
        let mut types = types();
        types.extend(registry_types());
        let registry = Registry::new(types);
        let entry = Value::Struct(vec![("craftingPropIndex".into(), Value::Uint(1))]);
        let workshops = Value::Struct(vec![("craftingProps".into(), Value::List(vec![entry]))]);
        assert!(registry.encode(WORKSHOP_REGISTRY, &workshops, None).is_ok());
    }

    #[test]
    fn every_vanilla_template_of_the_fake_writes_with_its_types() {
        let registry = Registry::new(types());
        assert_eq!(vanilla(&registry).len(), 4);
    }
}
