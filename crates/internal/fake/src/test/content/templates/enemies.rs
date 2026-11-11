//! The fake's enemies, arsenals and random spawners: their components and
//! the arsenal registry reflected, and the vanilla ones content edits start
//! from.

use super::{
    COMPONENT, Constant, FACTION, FakeHost, Field, Guid, Kind, Registry, TEMPLATE, Type, Value,
    fnv1a32, game_id, template, types,
};

/// A vanilla enemy template the fake holds: family `Npc` at level 1, with
/// the components `Load::add_enemy` sets.
pub const ENEMY: Guid = Guid::parse("c0ffee00-0000-4000-8000-00000000e4e1");

/// The vanilla arsenals the fake's arsenal registry lists: the one
/// [`ENEMY`] uses, with the attacks [`BITE`] and [`LUNGE`], and another
/// with [`HOWL`].
pub const ENEMY_ARSENAL: Guid = Guid::parse("c0ffee00-0000-4000-8000-0000000a45e1");
/// The second vanilla arsenal of the fake.
pub const OTHER_ARSENAL: Guid = Guid::parse("c0ffee00-0000-4000-8000-0000000a45e2");
/// An attack of [`ENEMY_ARSENAL`].
pub const BITE: Guid = Guid::parse("c0ffee00-0000-4000-8000-0000000b17e1");
/// Another attack of [`ENEMY_ARSENAL`].
pub const LUNGE: Guid = Guid::parse("c0ffee00-0000-4000-8000-0000000b17e2");
/// The attack of [`OTHER_ARSENAL`].
pub const HOWL: Guid = Guid::parse("c0ffee00-0000-4000-8000-0000000b17e3");

/// A vanilla random spawner template the fake holds, listing [`ENEMY`].
pub const NEST: Guid = Guid::parse("c0ffee00-0000-4000-8000-00000000e57e");

/// The spawn group of [`NEST`]'s entry: a GUID its bytes hold that no entry
/// lists as a template.
pub const NEST_GROUP: Guid = Guid::parse("c0ffee00-0000-4000-8000-0000000e5791");

/// The enemy and spawner components and the arsenal registry, by qualified
/// name.
const LEVEL: &str = "demo::ecs::Level";
const DAMAGE_MOD: &str = "demo::ecs::DamageMod";
const SCALING: &str = "demo::ecs::EnemyScaling";
const ARSENAL_REF: &str = "demo::ecs::EnemyArsenalReference";
const ARSENAL_REGISTRY: &str = "demo::enemy::EnemyArsenalRegistryResource";
const RANDOM_SPAWNER: &str = "demo::ecs::RandomSpawner";

/// A struct type of `fields`.
fn named(name: &'static str, size: u32, align: u16, fields: Vec<Field<'static>>) -> Type<'static> {
    Type::new(name, Kind::Struct, size, align).with_fields(fields)
}

/// A blob array of `of`.
fn blobs(of: &'static str) -> Type<'static> {
    Type::new(
        Box::leak(format!("demo::BlobArray<{of}>").into_boxed_str()),
        Kind::BlobArray,
        8,
        4,
    )
    .with_inner(of)
}

/// The attribute components and the enemy's own.
#[expect(
    clippy::too_many_lines,
    reason = "a table of reflected types, one entry each"
)]
fn enemy_types() -> Vec<Type<'static>> {
    let field = Field::new;
    let array = |name: &'static str, of: &'static str, count, size, align| {
        Type::new(name, Kind::StaticArray, size, align)
            .with_inner(of)
            .with_count(count)
    };
    vec![
        Type::new("demo::ecs::AttributeFlags", Kind::Enum, 1, 1)
            .with_inner("demo::uint8")
            .with_constants(vec![Constant::new("Initialized", 0)]),
        Type::new(
            "demo::Bitmask8<demo::ecs::AttributeFlags>",
            Kind::Bitmask8,
            1,
            1,
        )
        .with_inner("demo::ecs::AttributeFlags"),
        named(
            "demo::ecs::AttributeComponent",
            8,
            4,
            vec![
                field("rootId", 0, "demo::HashKey32"),
                field("storageOffset", 4, "demo::uint16"),
                field("flags", 6, "demo::Bitmask8<demo::ecs::AttributeFlags>"),
                field("storageCount", 7, "demo::uint8"),
            ],
        )
        .with_inner(COMPONENT),
        Type::new("demo::ecs::UInt32Attribute", Kind::Struct, 8, 4)
            .with_inner("demo::ecs::AttributeComponent"),
        Type::new("demo::ecs::FloatAttribute", Kind::Struct, 8, 4)
            .with_inner("demo::ecs::AttributeComponent"),
        array("demo::StaticArray<demo::uint32,2>", "demo::uint32", 2, 8, 4),
        array(
            "demo::StaticArray<demo::float32,4>",
            "demo::float32",
            4,
            16,
            4,
        ),
        named(
            LEVEL,
            16,
            4,
            vec![field("dataStorage", 8, "demo::StaticArray<demo::uint32,2>")],
        )
        .with_inner("demo::ecs::UInt32Attribute"),
        named(
            DAMAGE_MOD,
            24,
            4,
            vec![field(
                "dataStorage",
                8,
                "demo::StaticArray<demo::float32,4>",
            )],
        )
        .with_inner("demo::ecs::FloatAttribute"),
        named(
            "demo::ecs::PlayerCountScaling",
            20,
            4,
            vec![
                field("scaleHealth", 0, "demo::bool"),
                field("scaleDamage", 1, "demo::bool"),
                field("scaleStamina", 2, "demo::bool"),
                field("healthScaling", 4, "demo::float32"),
                field("damageScaling", 8, "demo::float32"),
                field("staminaScaling", 12, "demo::float32"),
                field("allowDownScaling", 16, "demo::bool"),
                field("minPlayerCount", 17, "demo::uint8"),
            ],
        ),
        named(
            "demo::ecs::EnemyLevelScaling",
            8,
            4,
            vec![
                field("healthScaleFactor", 0, "demo::float32"),
                field("staminaScaleFactor", 4, "demo::float32"),
            ],
        ),
        named(
            SCALING,
            40,
            8,
            vec![
                field("updateInterval", 0, "demo::Time"),
                field("playerCountScaling", 8, "demo::ecs::PlayerCountScaling"),
                field("enemyLevelScaling", 28, "demo::ecs::EnemyLevelScaling"),
            ],
        )
        .with_inner(COMPONENT),
        Type::new(
            "demo::ObjectReference<demo::ecs::EnemyArsenal>",
            Kind::ObjectReference,
            16,
            4,
        ),
        named(
            ARSENAL_REF,
            16,
            4,
            vec![field(
                "arsenal",
                0,
                "demo::ObjectReference<demo::ecs::EnemyArsenal>",
            )],
        )
        .with_inner(COMPONENT),
    ]
}

/// The arsenal registry and the random spawner.
#[expect(
    clippy::too_many_lines,
    reason = "a table of reflected types, one entry each"
)]
fn arsenal_and_spawner_types() -> Vec<Type<'static>> {
    let field = Field::new;
    vec![
        named(
            "demo::enemy01::AttackDesscription",
            16,
            4,
            vec![
                field("selectionChance", 0, "demo::float32"),
                field("threat", 4, "demo::float32"),
                field("minRange", 8, "demo::float32"),
                field("maxRange", 12, "demo::float32"),
            ],
        ),
        named(
            "demo::enemy01::AttackDesscriptionResource",
            32,
            4,
            vec![
                field("guid", 0, "demo::Guid"),
                field("description", 16, "demo::enemy01::AttackDesscription"),
            ],
        ),
        named(
            "demo::enemy01::BehaviorDesscriptionResource",
            16,
            4,
            vec![field("guid", 0, "demo::Guid")],
        ),
        blobs("demo::enemy01::AttackDesscriptionResource"),
        blobs("demo::enemy01::BehaviorDesscriptionResource"),
        named(
            "demo::enemy::EnemyArsenalResource",
            32,
            4,
            vec![
                field("guid", 0, "demo::Guid"),
                field(
                    "attacks",
                    16,
                    "demo::BlobArray<demo::enemy01::AttackDesscriptionResource>",
                ),
                field(
                    "behaviors",
                    24,
                    "demo::BlobArray<demo::enemy01::BehaviorDesscriptionResource>",
                ),
            ],
        ),
        blobs("demo::enemy::EnemyArsenalResource"),
        named(
            ARSENAL_REGISTRY,
            8,
            4,
            vec![field(
                "arsenals",
                0,
                "demo::BlobArray<demo::enemy::EnemyArsenalResource>",
            )],
        ),
        Type::new("demo::ecs::SpawnCondition", Kind::Enum, 1, 1)
            .with_inner("demo::uint8")
            .with_constants(vec![Constant::new("Night", 0)]),
        Type::new(
            "demo::Bitmask8<demo::ecs::SpawnCondition>",
            Kind::Bitmask8,
            1,
            1,
        )
        .with_inner("demo::ecs::SpawnCondition"),
        Type::new(
            "demo::ObjectReference<demo::ecs::SpawnGroupId>",
            Kind::ObjectReference,
            16,
            4,
        ),
        named(
            "demo::ecs::RuntimeSpawnerEntry",
            32,
            4,
            vec![
                field(
                    "entityTemplates",
                    0,
                    "demo::BlobArray<demo::ObjectReference<demo::ecs::TemplateResource>>",
                ),
                field(
                    "spawnGroupId",
                    8,
                    "demo::ObjectReference<demo::ecs::SpawnGroupId>",
                ),
                field(
                    "spawnConditions",
                    24,
                    "demo::Bitmask8<demo::ecs::SpawnCondition>",
                ),
                field("weight", 28, "demo::float32"),
                field("isHighPriorityEntry", 25, "demo::bool"),
            ],
        ),
        blobs("demo::ecs::RuntimeSpawnerEntry"),
        named(
            RANDOM_SPAWNER,
            32,
            8,
            vec![
                field(
                    "spawnEntries",
                    0,
                    "demo::BlobArray<demo::ecs::RuntimeSpawnerEntry>",
                ),
                field("minSpawnTimeout", 8, "demo::Time"),
                field("maxSpawnTimeout", 16, "demo::Time"),
                field("minSpawnAmount", 24, "demo::uint8"),
                field("maxSpawnAmount", 25, "demo::uint8"),
                field("maxEntitiesAlive", 26, "demo::uint8"),
                field("maxTotalSpawnCount", 27, "demo::uint8"),
            ],
        )
        .with_inner(COMPONENT),
    ]
}

/// A struct value of `fields`.
fn fields(fields: Vec<(&str, Value)>) -> Value {
    Value::Struct(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

/// An attribute's value: its base fields and `storage`.
fn attribute(storage: Vec<Value>) -> Value {
    fields(vec![
        ("rootId", game_id(0)),
        ("storageOffset", Value::Uint(0)),
        ("flags", Value::List(Vec::new())),
        ("storageCount", Value::Uint(0)),
        ("dataStorage", Value::List(storage)),
    ])
}

/// The vanilla nest template, as the codec writes it.
fn nest_value() -> Value {
    let time = |seconds: i64| fields(vec![("value", Value::Int(seconds * 1_000_000_000))]);
    let entry = fields(vec![
        ("entityTemplates", Value::List(vec![Value::Guid(ENEMY)])),
        ("spawnGroupId", Value::Guid(NEST_GROUP)),
        ("spawnConditions", Value::List(Vec::new())),
        ("weight", Value::Float(1.0)),
        ("isHighPriorityEntry", Value::Bool(false)),
    ]);
    template(
        "Spawnpoints_Enemy_Wolf_Pack",
        "Factory",
        vec![(
            RANDOM_SPAWNER,
            fields(vec![
                ("spawnEntries", Value::List(vec![entry])),
                ("minSpawnTimeout", time(4)),
                ("maxSpawnTimeout", time(6)),
                ("minSpawnAmount", Value::Uint(1)),
                ("maxSpawnAmount", Value::Uint(3)),
                ("maxEntitiesAlive", Value::Uint(5)),
                ("maxTotalSpawnCount", Value::Uint(32)),
            ]),
        )],
    )
}

/// The vanilla enemy template and arsenal registry, as the codec writes
/// them.
fn enemy_values() -> (Value, Value) {
    let float = |value: f64| Value::Float(value);
    let scaling = fields(vec![
        ("updateInterval", fields(vec![("value", Value::Int(0))])),
        (
            "playerCountScaling",
            fields(vec![
                ("scaleHealth", Value::Bool(true)),
                ("scaleDamage", Value::Bool(true)),
                ("scaleStamina", Value::Bool(false)),
                ("healthScaling", float(0.5)),
                ("damageScaling", float(0.0)),
                ("staminaScaling", float(0.0)),
                ("allowDownScaling", Value::Bool(false)),
                ("minPlayerCount", Value::Uint(0)),
            ]),
        ),
        (
            "enemyLevelScaling",
            fields(vec![
                ("healthScaleFactor", float(0.5)),
                ("staminaScaleFactor", float(0.25)),
            ]),
        ),
    ]);
    let enemy = template(
        "Enemy_Wolf_Pack",
        "Npc",
        vec![
            (
                FACTION,
                fields(vec![("faction", Value::Name("Player".into()))]),
            ),
            (LEVEL, attribute(vec![Value::Uint(1), Value::Uint(105)])),
            (
                DAMAGE_MOD,
                attribute(vec![float(0.0), float(0.5), float(1.0), float(0.0)]),
            ),
            (SCALING, scaling),
            (
                ARSENAL_REF,
                fields(vec![("arsenal", Value::Guid(ENEMY_ARSENAL))]),
            ),
        ],
    );
    let attack = |guid: Guid| {
        fields(vec![
            ("guid", Value::Guid(guid)),
            (
                "description",
                fields(vec![
                    ("selectionChance", float(1.0)),
                    ("threat", float(4.5)),
                    ("minRange", float(0.0)),
                    ("maxRange", float(2.0)),
                ]),
            ),
        ])
    };
    let arsenal = |guid: Guid, attacks: Vec<Value>| {
        fields(vec![
            ("guid", Value::Guid(guid)),
            ("attacks", Value::List(attacks)),
            ("behaviors", Value::List(Vec::new())),
        ])
    };
    let registry = fields(vec![(
        "arsenals",
        Value::List(vec![
            arsenal(ENEMY_ARSENAL, vec![attack(BITE), attack(LUNGE)]),
            arsenal(OTHER_ARSENAL, vec![attack(HOWL)]),
        ]),
    )]);
    (enemy, registry)
}

/// Every type the enemies preset reflects.
fn all_types() -> Vec<Type<'static>> {
    let mut all = types();
    all.extend(enemy_types());
    all.extend(arsenal_and_spawner_types());
    all
}

impl FakeHost {
    /// Enemies, arsenals and spawners: [`FakeHost::items`] and
    /// [`FakeHost::workshops`], the enemy components, the random spawner and
    /// the arsenal registry reflected, and in the archive the vanilla
    /// [`ENEMY`], the spawner [`NEST`] that lists it, and an arsenal registry
    /// of [`ENEMY_ARSENAL`] and [`OTHER_ARSENAL`]. What `Load::add_enemy`,
    /// `Load::add_arsenal` and `Load::add_spawn_entry` resolve.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::EngineFacts;
    ///
    /// let host = enshwick::test::FakeHost::new().enemies();
    /// assert!(host.hash("ecs::Level").is_ok());
    /// ```
    #[must_use]
    pub fn enemies(self) -> Self {
        let types = all_types();
        let registry = Registry::new(types.clone());
        let hash = |name: &str| fnv1a32(name.as_bytes());
        let (enemy, arsenals) = enemy_values();
        let mut host = self.items().workshops().types(types);
        for (guid, value) in [(ENEMY, enemy), (NEST, nest_value())] {
            if let Ok(bytes) = registry.encode(TEMPLATE, &value, None) {
                host = host.resource(guid.key_by_hash(hash(TEMPLATE)), bytes);
            }
        }
        if let Ok(bytes) = registry.encode(ARSENAL_REGISTRY, &arsenals, None) {
            // The game's arsenal registry, `content::enemies::ARSENAL_REGISTRY`
            // where the typed API is built.
            let key = Guid::parse("cd861d95-a79c-4c11-8b48-8fae34a4156c");
            host = host.resource(key.key_by_hash(hash(ARSENAL_REGISTRY)), bytes);
        }
        [
            ("ecs::Level", LEVEL),
            ("ecs::DamageMod", DAMAGE_MOD),
            ("ecs::EnemyScaling", SCALING),
            ("ecs::EnemyArsenalReference", ARSENAL_REF),
            ("enemy::EnemyArsenalRegistryResource", ARSENAL_REGISTRY),
            ("ecs::RandomSpawner", RANDOM_SPAWNER),
        ]
        .into_iter()
        .fold(host, |host, (id, name)| host.type_hash(id, hash(name)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fake_s_enemy_spawner_and_arsenals_write_with_their_types() {
        let registry = Registry::new(all_types());
        let (enemy, arsenals) = enemy_values();
        registry.encode(TEMPLATE, &enemy, None).unwrap();
        registry.encode(TEMPLATE, &nest_value(), None).unwrap();
        registry.encode(ARSENAL_REGISTRY, &arsenals, None).unwrap();
    }
}
