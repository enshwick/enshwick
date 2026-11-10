//! The fake's progression: its knowledge reflected, with a small vanilla
//! knowledge of the world and the players, which mod knowledge joins.

use crate::content::Guid;
use crate::resources::{Constant, Field, Kind, Registry, Type, Value, fnv1a32};

use super::FakeHost;

/// The knowledge types, by their qualified names.
const KNOWLEDGE: &str = "demo::GameKnowledgeResource";
const OBJECT: &str = "demo::KnowledgeObjectResource";
const WORLD_OBJECT: &str = "demo::WorldKnowledgeObjectResource";
const PLAYER_OBJECT: &str = "demo::PlayerKnowledgeObjectResource";
const JOURNAL: &str = "demo::JournalRegistryResource";
const COLLECTION: &str = "demo::JournalCollectionResource";
const QUEST: &str = "demo::JournalQuestResource";
const TREE: &str = "demo::SkillTreeResource";
const PERK: &str = "demo::Perk";
const PERKS: &str = "demo::PerkCollectionResource";
const UNLOCKS: &str = "demo::ecs::UnlockedSkillNodes";
const BALANCING: &str = "demo::BalancingTable";
const BALANCE_VALUE: &str = "demo::ExperienceBalanceValue";
const EXPERIENCE_SOURCE: &str = "demo::ExperienceSourceType";

/// A vanilla experience value of the fake's balancing table: 5 of mining.
pub const VANILLA_EXPERIENCE: u32 = 0x0000_0e05;
/// A vanilla experience value of the fake's balancing table: 600 of a quest,
/// whose id is the content id `vanilla:quest-xp`'s, so a test can add a
/// value the game holds.
pub const VANILLA_QUEST_EXPERIENCE: u32 = Guid::named("vanilla:quest-xp").fnv1a();
/// A vanilla experience value of the fake's balancing table: 2 of the
/// source 9, which the fake's source enum does not name, as a later build's
/// new source reads.
pub const VANILLA_UNNAMED_EXPERIENCE: u32 = 0x0000_0909;
/// The fake's balancing table, the one resource of its type, under the
/// game's own GUID (server 387).
pub const BALANCING_TABLE: Guid = Guid::parse("82706b40-61b1-4b8f-8b23-dcec6971bda1");

/// The unlocked nodes the fake's saves keep: room for two nodes beside its
/// tree's two.
pub const SKILL_ROOM: u32 = 4;
/// The vanilla root of the fake's skill tree.
pub const ROOT_NODE: u32 = 0x0000_0601;
/// The vanilla attribute node linked from [`ROOT_NODE`].
pub const ATTRIBUTE_NODE: u32 = 0x0000_0611;
/// The vanilla perk the fake's perk collection lists.
pub const VANILLA_PERK: Guid = Guid::parse("c0ffee00-0000-4000-8000-00000000be4c");

/// Each player's quest progression component.
pub const PROGRESSION: &str = "demo::ecs::PlayerQuestProgressionMode";

/// The vanilla quest of the fake's journal, a world's main quest of one
/// step.
pub const VANILLA_QUEST: u32 = 0x0000_0500;
/// The text tag of the lore category the vanilla journal's one page is
/// listed under, the game's "General": the one category a mod's page may
/// take here.
pub const LORE_CATEGORY: Guid = crate::content::journal::lore_category::GENERAL;
/// The item [`VANILLA_QUEST`] pays.
pub const QUEST_ITEM: Guid = Guid::parse("c0ffee00-0000-4000-8000-0000000a0e57");
/// The document the fake's journal entries name.
const VANILLA_DOCUMENT: u32 = 0x0000_0506;

/// A vanilla bool of the fake's players' knowledge, which gives a skill
/// point.
pub const VANILLA_PLAYER_KNOWLEDGE: u32 = 0x0000_0102;
/// A vanilla counter of the fake's world knowledge.
pub const VANILLA_WORLD_KNOWLEDGE: u32 = 0x0000_0203;

/// The buckets of each of the fake's knowledge maps.
const BUCKETS: u32 = 8;

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

/// A map of the game's knowledge: buckets, keys and values of `value`.
fn map(name: &'static str, value: &'static str) -> Vec<Type<'static>> {
    let field = Field::new;
    vec![
        blobs(value),
        named(
            name,
            24,
            4,
            vec![
                field("entries", 0, "demo::BlobArray<demo::StaticHashMapEntry>"),
                field("keys", 8, "demo::BlobArray<demo::KnowledgeId>"),
                field(
                    "values",
                    16,
                    Box::leak(format!("demo::BlobArray<{value}>").into_boxed_str()),
                ),
            ],
        ),
    ]
}

/// The knowledge resource and every type it holds, laid out as the game
/// lays out the fields the fake keeps.
fn knowledge_types() -> Vec<Type<'static>> {
    let field = Field::new;
    let mut types = vec![
        Type::new("demo::bool", Kind::Bool, 1, 1),
        Type::new("demo::uint8", Kind::Uint8, 1, 1),
        Type::new("demo::uint32", Kind::Uint32, 4, 4),
        named(
            "demo::HashKey32",
            4,
            4,
            vec![field("value", 0, "demo::uint32")],
        ),
        named(
            "demo::KnowledgeId",
            4,
            4,
            vec![field("id", 0, "demo::HashKey32")],
        ),
        Type::new("demo::KnowledgeType", Kind::Enum, 1, 1)
            .with_inner("demo::uint8")
            .with_constants(vec![
                Constant::new("Bool", 0),
                Constant::new("Counter", 1),
                Constant::new("Flags", 2),
            ]),
        named(
            OBJECT,
            12,
            4,
            vec![
                field("id", 0, "demo::KnowledgeId"),
                field("sendToAnalytics", 4, "demo::bool"),
                field("hasOptionalPlayerProgression", 5, "demo::bool"),
                field("isPlayerKnowledge", 6, "demo::bool"),
                field("type", 7, "demo::KnowledgeType"),
                field("expectedFlags", 8, "demo::uint32"),
            ],
        ),
        Type::new(WORLD_OBJECT, Kind::Struct, 12, 4).with_inner(OBJECT),
        named(
            PLAYER_OBJECT,
            24,
            4,
            vec![
                field("isAdditionalPlayerKnowledgePerFlag", 12, "demo::uint32"),
                field("experienceGainId", 16, "demo::HashKey32"),
                field("unlockedSkillPoints", 20, "demo::uint8"),
            ],
        )
        .with_inner(OBJECT),
        named(
            "demo::StaticHashMapEntry",
            8,
            4,
            vec![
                field("start", 0, "demo::uint32"),
                field("count", 4, "demo::uint32"),
            ],
        ),
        blobs("demo::StaticHashMapEntry"),
        blobs("demo::KnowledgeId"),
        named(
            "demo::SkillPointUnlock",
            8,
            4,
            vec![
                field("knowledgeId", 0, "demo::KnowledgeId"),
                field("amount", 4, "demo::uint8"),
            ],
        ),
        blobs("demo::SkillPointUnlock"),
        named(
            KNOWLEDGE,
            56,
            4,
            vec![
                field("worldKnowledge", 0, "demo::WorldKnowledgeMap"),
                field("playerKnowledge", 24, "demo::PlayerKnowledgeMap"),
                field("skillPoints", 48, "demo::BlobArray<demo::SkillPointUnlock>"),
            ],
        ),
    ];
    types.extend(map("demo::WorldKnowledgeMap", WORLD_OBJECT));
    types.extend(map("demo::PlayerKnowledgeMap", PLAYER_OBJECT));
    types.extend(journal_types());
    types.extend(skill_types());
    types.extend(balancing_types());
    types
}

/// The balancing table with its experience values, laid out as the game
/// lays them out (server 224), the sources numbered as the game numbers
/// them.
fn balancing_types() -> Vec<Type<'static>> {
    let field = Field::new;
    vec![
        enumeration(
            EXPERIENCE_SOURCE,
            &[
                "None",
                "Exploration",
                "Lore",
                "Quest",
                "Combat",
                "Mining",
                "ItemSetCompleted",
                // A source the crate does not name, as a later build's new one.
                "Gathering",
            ],
        ),
        named(
            BALANCE_VALUE,
            12,
            4,
            vec![
                field("source", 0, EXPERIENCE_SOURCE),
                field("experienceValueId", 4, "demo::HashKey32"),
                field("experienceGain", 8, "demo::uint32"),
            ],
        ),
        blobs(BALANCE_VALUE),
        named(
            BALANCING,
            8,
            4,
            vec![field(
                "experienceValues",
                0,
                "demo::BlobArray<demo::ExperienceBalanceValue>",
            )],
        ),
    ]
}

/// The fake's vanilla balancing table: [`VANILLA_EXPERIENCE`],
/// [`VANILLA_QUEST_EXPERIENCE`] and [`VANILLA_UNNAMED_EXPERIENCE`].
fn balancing_value() -> Value {
    let value = |source: Value, id: u32, gain: u64| {
        fields(vec![
            ("source", source),
            (
                "experienceValueId",
                fields(vec![("value", Value::Uint(id.into()))]),
            ),
            ("experienceGain", Value::Uint(gain)),
        ])
    };
    fields(vec![(
        "experienceValues",
        Value::List(vec![
            value(Value::Name("Mining".into()), VANILLA_EXPERIENCE, 5),
            value(Value::Name("Quest".into()), VANILLA_QUEST_EXPERIENCE, 600),
            value(Value::Uint(9), VANILLA_UNNAMED_EXPERIENCE, 2),
        ]),
    )])
}

/// The skill tree, the perks and the saved unlocks, with the fields the
/// fake keeps; a save keeps [`SKILL_ROOM`] unlocked nodes here.
fn skill_types() -> Vec<Type<'static>> {
    let field = Field::new;
    let key = "demo::HashKey32";
    vec![
        Type::new("demo::uint16", Kind::Uint16, 2, 2),
        Type::new("demo::float32", Kind::Float32, 4, 4),
        named(
            "demo::float2",
            8,
            4,
            vec![
                field("x", 0, "demo::float32"),
                field("y", 4, "demo::float32"),
            ],
        ),
        enumeration(
            "demo::SkillNodeType",
            &["Root", "Attribute", "ActiveSkill", "PassiveSkill"],
        ),
        blobs(key),
        named(
            "demo::SkillTreeNode",
            40,
            4,
            vec![
                field("id", 0, key),
                field("uiPosition", 4, "demo::float2"),
                field("name", 12, key),
                field("description", 16, key),
                field("incomingLinks", 20, "demo::BlobArray<demo::HashKey32>"),
                field("outgoingLinks", 28, "demo::BlobArray<demo::HashKey32>"),
                field("costs", 36, "demo::uint16"),
                field("type", 38, "demo::SkillNodeType"),
                field("maxLevel", 39, "demo::uint8"),
            ],
        ),
        blobs("demo::SkillTreeNode"),
        named(
            "demo::SkillTreeNodeLink",
            16,
            4,
            vec![
                field("id", 0, key),
                field("sourceNode", 4, key),
                field("targetNode", 8, key),
                field("isBidirectional", 12, "demo::bool"),
            ],
        ),
        blobs("demo::SkillTreeNodeLink"),
        named(
            TREE,
            16,
            4,
            vec![
                field("nodes", 0, "demo::BlobArray<demo::SkillTreeNode>"),
                field("links", 8, "demo::BlobArray<demo::SkillTreeNodeLink>"),
            ],
        ),
    ]
    .into_iter()
    .chain(perk_types())
    .collect()
}

/// The perks and the saved unlocks of [`skill_types`].
fn perk_types() -> Vec<Type<'static>> {
    let field = Field::new;
    let key = "demo::HashKey32";
    vec![
        Type::new(
            "demo::ObjectReference<demo::Text>",
            Kind::ObjectReference,
            16,
            4,
        ),
        named(
            PERK,
            32,
            4,
            vec![
                field("name", 0, "demo::ObjectReference<demo::Text>"),
                field("description", 16, "demo::ObjectReference<demo::Text>"),
            ],
        ),
        Type::new(
            "demo::ObjectReference<demo::Perk>",
            Kind::ObjectReference,
            16,
            4,
        ),
        blobs("demo::ObjectReference<demo::Perk>"),
        named(
            PERKS,
            8,
            4,
            vec![field(
                "perks",
                0,
                "demo::BlobArray<demo::ObjectReference<demo::Perk>>",
            )],
        ),
        Type::new(
            "demo::StaticArray<demo::HashKey32,4>",
            Kind::StaticArray,
            16,
            4,
        )
        .with_inner(key)
        .with_count(SKILL_ROOM),
        named(
            UNLOCKS,
            20,
            4,
            vec![
                field("initialized", 0, "demo::bool"),
                field("knownSkillNodes", 4, "demo::StaticArray<demo::HashKey32,4>"),
            ],
        )
        .with_inner("demo::ecs::Component"),
    ]
}

/// The vanilla skill tree: [`ROOT_NODE`] at (0, 0) linked to
/// [`ATTRIBUTE_NODE`] at (200, 0).
fn tree_value() -> Value {
    let node = |id: u32, x: f64, kind: &str, incoming: Vec<Value>, outgoing: Vec<Value>| {
        fields(vec![
            ("id", hash_key(id)),
            (
                "uiPosition",
                fields(vec![("x", Value::Float(x)), ("y", Value::Float(0.0))]),
            ),
            ("name", hash_key(id + 1)),
            ("description", hash_key(id + 2)),
            ("incomingLinks", Value::List(incoming)),
            ("outgoingLinks", Value::List(outgoing)),
            ("costs", Value::Uint(1)),
            ("type", Value::Name(kind.into())),
            ("maxLevel", Value::Uint(0)),
        ])
    };
    let link = 0x0000_0700;
    fields(vec![
        (
            "nodes",
            Value::List(vec![
                node(ROOT_NODE, 0.0, "Root", Vec::new(), vec![hash_key(link)]),
                node(
                    ATTRIBUTE_NODE,
                    200.0,
                    "Attribute",
                    vec![hash_key(link)],
                    Vec::new(),
                ),
            ]),
        ),
        (
            "links",
            Value::List(vec![fields(vec![
                ("id", hash_key(link)),
                ("sourceNode", hash_key(ROOT_NODE)),
                ("targetNode", hash_key(ATTRIBUTE_NODE)),
                ("isBidirectional", Value::Bool(false)),
            ])]),
        ),
    ])
}

/// An enum type of `constants`, one byte.
fn enumeration(name: &'static str, constants: &[&'static str]) -> Type<'static> {
    let constants = constants
        .iter()
        .zip(0..)
        .map(|(&name, value)| Constant::new(name, value))
        .collect();
    Type::new(name, Kind::Enum, 1, 1)
        .with_inner("demo::uint8")
        .with_constants(constants)
}

/// The journal and every type it holds, laid out as the game lays them out
/// (server 387); its ids and text tags are `HashKey32`s here.
#[expect(
    clippy::too_many_lines,
    reason = "a table of reflected types, one entry each"
)]
fn journal_types() -> Vec<Type<'static>> {
    let field = Field::new;
    let key = "demo::HashKey32";
    vec![
        Type::new("demo::ecs::Component", Kind::Struct, 1, 1),
        named(
            PROGRESSION,
            8,
            4,
            vec![
                field("isInPersonalProgressionMode", 0, "demo::bool"),
                field("changeCounter", 4, "demo::uint32"),
            ],
        )
        .with_inner("demo::ecs::Component"),
        enumeration(
            "demo::CompareOperator",
            &["GreaterThan", "LessThan", "Equals"],
        ),
        enumeration(
            "demo::KnowledgeQueryDataType",
            &["Extern", "SimpleBool", "SimpleCount", "SimpleFlag"],
        ),
        enumeration("demo::QuestSourceType", &["None", "Flame", "Blacksmith"]),
        enumeration("demo::QuestType", &["Auto", "WorldQuest", "PlayerQuest"]),
        named(
            "demo::KnowledgeQueryData",
            12,
            4,
            vec![
                field("knowledgeOrQueryId", 0, key),
                field("compareValue", 4, "demo::uint32"),
                field("compareOperator", 8, "demo::CompareOperator"),
                field("type", 9, "demo::KnowledgeQueryDataType"),
                field("isExplicitPlayerKnowledgeQuery", 10, "demo::bool"),
            ],
        ),
        blobs("demo::KnowledgeQueryData"),
        Type::new(
            "demo::ObjectReference<demo::Item>",
            Kind::ObjectReference,
            16,
            4,
        ),
        Type::new(
            "demo::ObjectReference<demo::Voiceover>",
            Kind::ObjectReference,
            16,
            4,
        ),
        named(
            "demo::JournalEntryResource",
            72,
            4,
            vec![
                field("entryId", 0, key),
                field("name", 4, key),
                field("text", 8, key),
                field("mapMarkerReference", 12, key),
                field("knowledgeRequirement", 16, "demo::KnowledgeQueryData"),
                field("completionRequirement", 28, "demo::KnowledgeQueryData"),
                field(
                    "progressStepsRequirement",
                    40,
                    "demo::BlobArray<demo::KnowledgeQueryData>",
                ),
                field("voiceover", 48, "demo::ObjectReference<demo::Voiceover>"),
                field("itemIconId", 64, key),
                field("recommendedLevel", 68, "demo::uint8"),
            ],
        ),
        blobs("demo::JournalEntryResource"),
        named(
            COLLECTION,
            32,
            4,
            vec![
                field("entryId", 0, key),
                field("loreCategory", 4, key),
                field("name", 8, key),
                field("referencedDocumentName", 12, key),
                field("priority", 16, "demo::uint32"),
                field("isTutorial", 20, "demo::bool"),
                field("entries", 24, "demo::BlobArray<demo::JournalEntryResource>"),
            ],
        ),
        blobs(COLLECTION),
        named(
            "demo::QuestRewardResource",
            24,
            4,
            vec![
                field("itemRef", 0, "demo::ObjectReference<demo::Item>"),
                field("itemId", 16, key),
                field("experienceValueId", 20, key),
            ],
        ),
        blobs("demo::QuestRewardResource"),
        named(
            QUEST,
            44,
            4,
            vec![
                field("source", 32, "demo::QuestSourceType"),
                field("type", 33, "demo::QuestType"),
                field("isMainQuest", 34, "demo::bool"),
                field("unlockForAllPlayers", 35, "demo::bool"),
                field("rewards", 36, "demo::BlobArray<demo::QuestRewardResource>"),
            ],
        )
        .with_inner(COLLECTION),
        blobs(QUEST),
        named(
            JOURNAL,
            16,
            4,
            vec![
                field("quests", 0, "demo::BlobArray<demo::JournalQuestResource>"),
                field(
                    "collections",
                    8,
                    "demo::BlobArray<demo::JournalCollectionResource>",
                ),
            ],
        ),
    ]
}

/// A `HashKey32` of `key`.
fn hash_key(key: u32) -> Value {
    fields(vec![("value", Value::Uint(key.into()))])
}

/// The vanilla journal, as the codec writes it: [`VANILLA_QUEST`], one step
/// shown on [`VANILLA_PLAYER_KNOWLEDGE`], paying the item [`QUEST_ITEM`],
/// and one lore page without entries under [`LORE_CATEGORY`].
fn journal_value() -> Value {
    let query = |id: u32, kind: &str| {
        fields(vec![
            ("knowledgeOrQueryId", hash_key(id)),
            ("compareValue", Value::Uint(1)),
            ("compareOperator", Value::Name("Equals".into())),
            ("type", Value::Name(kind.into())),
            ("isExplicitPlayerKnowledgeQuery", Value::Bool(true)),
        ])
    };
    let step = fields(vec![
        ("entryId", hash_key(0x0000_0501)),
        ("name", hash_key(0x0000_0502)),
        ("text", hash_key(0x0000_0503)),
        ("mapMarkerReference", hash_key(0)),
        (
            "knowledgeRequirement",
            query(VANILLA_PLAYER_KNOWLEDGE, "SimpleBool"),
        ),
        ("completionRequirement", query(0x0000_0504, "Extern")),
        ("progressStepsRequirement", Value::List(Vec::new())),
        ("voiceover", Value::Guid(Guid([0; 16]))),
        ("itemIconId", hash_key(0)),
        ("recommendedLevel", Value::Uint(1)),
    ]);
    let quest = fields(vec![
        ("entryId", hash_key(VANILLA_QUEST)),
        ("loreCategory", hash_key(0)),
        ("name", hash_key(0x0000_0505)),
        ("referencedDocumentName", hash_key(VANILLA_DOCUMENT)),
        ("priority", Value::Uint(100)),
        ("isTutorial", Value::Bool(false)),
        ("entries", Value::List(vec![step])),
        ("source", Value::Name("Flame".into())),
        ("type", Value::Name("WorldQuest".into())),
        ("isMainQuest", Value::Bool(true)),
        ("unlockForAllPlayers", Value::Bool(false)),
        (
            "rewards",
            Value::List(vec![fields(vec![
                ("itemRef", Value::Guid(QUEST_ITEM)),
                ("itemId", hash_key(QUEST_ITEM.fnv1a())),
                ("experienceValueId", hash_key(0)),
            ])]),
        ),
    ]);
    let page = fields(vec![
        ("entryId", hash_key(0x0000_0507)),
        ("loreCategory", hash_key(LORE_CATEGORY.fnv1a())),
        ("name", hash_key(0x0000_0508)),
        ("referencedDocumentName", hash_key(VANILLA_DOCUMENT)),
        ("priority", Value::Uint(0)),
        ("isTutorial", Value::Bool(false)),
        ("entries", Value::List(Vec::new())),
    ]);
    fields(vec![
        ("quests", Value::List(vec![quest])),
        ("collections", Value::List(vec![page])),
    ])
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

/// A `KnowledgeId` of `key`.
fn knowledge_id(key: u32) -> Value {
    fields(vec![(
        "id",
        fields(vec![("value", Value::Uint(key.into()))]),
    )])
}

/// A map holding the one entry `value` under `key`, its bucket's run
/// counted.
fn one_entry(key: u32, value: Value) -> Value {
    let entries = (0..BUCKETS)
        .map(|bucket| {
            let count = u64::from(key % BUCKETS == bucket);
            fields(vec![
                ("start", Value::Uint(0)),
                ("count", Value::Uint(count)),
            ])
        })
        .collect();
    fields(vec![
        ("entries", Value::List(entries)),
        ("keys", Value::List(vec![knowledge_id(key)])),
        ("values", Value::List(vec![value])),
    ])
}

/// The vanilla knowledge, as the codec writes it.
fn knowledge_value() -> Value {
    let base = |key: u32, player: bool, kind: &str| {
        vec![
            ("id", knowledge_id(key)),
            ("sendToAnalytics", Value::Bool(false)),
            ("hasOptionalPlayerProgression", Value::Bool(false)),
            ("isPlayerKnowledge", Value::Bool(player)),
            ("type", Value::Name(kind.into())),
            ("expectedFlags", Value::Uint(0)),
        ]
    };
    let world = fields(base(VANILLA_WORLD_KNOWLEDGE, false, "Counter"));
    let mut player = base(VANILLA_PLAYER_KNOWLEDGE, true, "Bool");
    player.extend([
        ("isAdditionalPlayerKnowledgePerFlag", Value::Uint(0)),
        ("experienceGainId", fields(vec![("value", Value::Uint(0))])),
        ("unlockedSkillPoints", Value::Uint(1)),
    ]);
    fields(vec![
        ("worldKnowledge", one_entry(VANILLA_WORLD_KNOWLEDGE, world)),
        (
            "playerKnowledge",
            one_entry(VANILLA_PLAYER_KNOWLEDGE, fields(player)),
        ),
        (
            "skillPoints",
            Value::List(vec![fields(vec![
                ("knowledgeId", knowledge_id(VANILLA_PLAYER_KNOWLEDGE)),
                ("amount", Value::Uint(1)),
            ])]),
        ),
    ])
}

impl FakeHost {
    /// Progression: the game's knowledge reflected, and in the archive a
    /// vanilla knowledge of one world counter,
    /// [`VANILLA_WORLD_KNOWLEDGE`], and one player bool,
    /// [`VANILLA_PLAYER_KNOWLEDGE`], with its skill point, and a balancing
    /// table of [`VANILLA_EXPERIENCE`], [`VANILLA_QUEST_EXPERIENCE`] and [`VANILLA_UNNAMED_EXPERIENCE`].
    /// What `Load::add_knowledge` and `Load::add_experience` resolve.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::content::EngineFacts;
    ///
    /// let host = enshwick::test::FakeHost::new().progression();
    /// assert!(host.hash("GameKnowledgeResource").is_ok());
    /// ```
    #[must_use]
    pub fn progression(self) -> Self {
        let types = knowledge_types();
        let registry = Registry::new(types.clone());
        let hash = |name: &str| fnv1a32(name.as_bytes());
        let mut host = self.types(types);
        if let Ok(bytes) = registry.encode(KNOWLEDGE, &knowledge_value(), None) {
            // The game's knowledge, `content::knowledge::GAME_KNOWLEDGE` where
            // the typed API is built.
            let key = Guid::parse("cb1eeb8b-f58a-4785-bf0f-6dd5437e9a5b");
            host = host.resource(key.key_by_hash(hash(KNOWLEDGE)), bytes);
        }
        if let Ok(bytes) = registry.encode(JOURNAL, &journal_value(), None) {
            // `content::journal::JOURNAL_REGISTRY`.
            let key = Guid::parse("33701b26-ec1d-423f-8e06-49f023b91b7f");
            host = host.resource(key.key_by_hash(hash(JOURNAL)), bytes);
        }
        let perk = fields(vec![
            ("name", Value::Guid(Guid([1; 16]))),
            ("description", Value::Guid(Guid([2; 16]))),
        ]);
        let perks = fields(vec![(
            "perks",
            Value::List(vec![Value::Guid(VANILLA_PERK)]),
        )]);
        let served = [
            // `content::skills::SKILL_TREE`, `VANILLA_PERK` and `PERK_COLLECTION`.
            ("108ac23f-a77d-4f18-bfac-acc397edaaf0", TREE, tree_value()),
            ("c0ffee00-0000-4000-8000-00000000be4c", PERK, perk),
            ("bcaac92e-70bf-4fdb-8119-001678e7a616", PERKS, perks),
        ];
        for (guid, ty, value) in served {
            if let Ok(bytes) = registry.encode(ty, &value, None) {
                host = host.resource(Guid::parse(guid).key_by_hash(hash(ty)), bytes);
            }
        }
        if let Ok(bytes) = registry.encode(BALANCING, &balancing_value(), None) {
            host = host.resource(BALANCING_TABLE.key_by_hash(hash(BALANCING)), bytes);
        }
        for (raw, (_, id)) in (0..).zip(crate::game_events::ExperienceSourceType::IDS) {
            host = host.constant(id, raw);
        }
        host.type_hash(crate::engine_ids::BALANCING_TABLE, hash(BALANCING))
            .type_hash(
                crate::engine_ids::EXPERIENCE_SOURCE,
                hash(EXPERIENCE_SOURCE),
            )
            .type_hash("SkillTreeResource", hash(TREE))
            .type_hash("Perk", hash(PERK))
            .type_hash("PerkCollectionResource", hash(PERKS))
            .type_hash("ecs::UnlockedSkillNodes", hash(UNLOCKS))
            .type_hash("GameKnowledgeResource", hash(KNOWLEDGE))
            .type_hash("JournalRegistryResource", hash(JOURNAL))
            .type_hash("ecs::PlayerQuestProgressionMode", hash(PROGRESSION))
            .type_size("ecs::PlayerQuestProgressionMode", 8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fake_s_knowledge_writes_with_its_types() {
        let registry = Registry::new(knowledge_types());
        let bytes = registry
            .encode(KNOWLEDGE, &knowledge_value(), None)
            .unwrap();
        let read = registry.decode(KNOWLEDGE, &bytes).unwrap();
        assert_eq!(
            read.field("skillPoints"),
            knowledge_value().field("skillPoints")
        );
        let bytes = registry.encode(JOURNAL, &journal_value(), None).unwrap();
        assert_eq!(registry.decode(JOURNAL, &bytes).unwrap(), journal_value());
        let bytes = registry.encode(TREE, &tree_value(), None).unwrap();
        assert_eq!(registry.decode(TREE, &bytes).unwrap(), tree_value());
    }
}
