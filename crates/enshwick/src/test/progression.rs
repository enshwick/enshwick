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
    types
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
    /// [`VANILLA_PLAYER_KNOWLEDGE`], with its skill point. What
    /// `Load::add_knowledge` resolves.
    ///
    /// # Examples
    ///
    /// ```
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
        host.type_hash("GameKnowledgeResource", hash(KNOWLEDGE))
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
    }
}
