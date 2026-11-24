//! The fake journal's item sets: one category of one vanilla set, paid by
//! one row, laid out as the game lays out the fields the fake keeps
//! (server 26430253).

use enshwick_core::content::Guid;
use enshwick_core::resources::{Field, Kind, Type, Value};

use super::queries::{VANILLA_PLAYER_QUERY, VANILLA_WORLD_QUERY};
use super::vanilla::{QUEST_ITEM, blobs, fields, hash_key, named};

/// The fake journal's one item set category, the game's "Collections": the
/// one a mod's set may take here.
pub const ITEM_SET_CATEGORY: enshwick_core::content::journal::ItemSetCategory =
    enshwick_core::content::journal::item_set_category::COLLECTIONS;
/// The vanilla item set of the fake journal: one entry of [`QUEST_ITEM`],
/// shown on [`VANILLA_WORLD_QUERY`] and complete on
/// [`VANILLA_PLAYER_QUERY`].
pub const VANILLA_ITEM_SET: u32 = 0x0000_0800;
/// The player knowledge the pay row of [`VANILLA_ITEM_SET`] names.
pub const VANILLA_ITEM_SET_REWARD: u32 = 0x0000_0804;

const HINT: &str = "demo::JournalItemSetEntryHintResource";
const ENTRY: &str = "demo::JournalItemSetEntryResource";
const SET: &str = "demo::JournalItemSetResource";
const CATEGORY: &str = "demo::JournalItemSetCategoryResource";
const REWARD: &str = "demo::JournalItemSetRewardResource";

/// The item set types the journal holds.
pub(super) fn item_set_types() -> Vec<Type<'static>> {
    let field = Field::new;
    let key = "demo::HashKey32";
    let query = "demo::KnowledgeQueryData";
    vec![
        Type::new(
            "demo::ObjectReference<demo::UiImage>",
            Kind::ObjectReference,
            16,
            4,
        ),
        named(
            "demo::UiTexture",
            16,
            4,
            vec![field("image", 0, "demo::ObjectReference<demo::UiImage>")],
        ),
        named(
            HINT,
            40,
            4,
            vec![
                field("hintStateKnowledgeQuery", 0, query),
                field("completionStateKnowledgeQuery", 12, query),
                field("objective", 24, key),
                field("hintDescription", 28, key),
                field("mapMarkerReference", 32, key),
                field("recommendedLevel", 36, "demo::uint8"),
            ],
        ),
        blobs(HINT),
        named(
            ENTRY,
            60,
            4,
            vec![
                field("entryId", 0, key),
                field("itemId", 4, key),
                field("lockedImageOverride", 8, "demo::UiTexture"),
                field("completionKnowledgeQuery", 24, query),
                field("questReference", 36, key),
                field("recipeId", 40, key),
                field("collectibleDisplayProp", 44, key),
                field("hasMultipleDisplayProps", 48, "demo::bool"),
                field(
                    "hints",
                    52,
                    "demo::BlobArray<demo::JournalItemSetEntryHintResource>",
                ),
            ],
        ),
        blobs(ENTRY),
        blobs(key),
        named(
            SET,
            52,
            4,
            vec![
                field("setId", 0, key),
                field("setName", 4, key),
                field("experienceValueId", 8, key),
                field(
                    "entries",
                    12,
                    "demo::BlobArray<demo::JournalItemSetEntryResource>",
                ),
                field("recipeRewards", 20, "demo::BlobArray<demo::HashKey32>"),
                field("completionKnowledgeQuery", 28, query),
                field("unlockKnowledgeQuery", 40, query),
            ],
        ),
        blobs(SET),
        named(
            CATEGORY,
            12,
            4,
            vec![
                field("categoryName", 0, key),
                field(
                    "entries",
                    4,
                    "demo::BlobArray<demo::JournalItemSetResource>",
                ),
            ],
        ),
        blobs(CATEGORY),
        named(
            REWARD,
            20,
            4,
            vec![
                field("rewardKnowledge", 0, "demo::KnowledgeId"),
                field("experienceValueId", 4, key),
                field("knowledgeQueryData", 8, query),
            ],
        ),
        blobs(REWARD),
    ]
}

/// The journal's item set fields of the fields the fake journal holds.
pub(super) fn item_set_fields() -> [Field<'static>; 2] {
    [
        Field::new(
            "itemSetCategories",
            16,
            "demo::BlobArray<demo::JournalItemSetCategoryResource>",
        ),
        Field::new(
            "itemSetRewards",
            24,
            "demo::BlobArray<demo::JournalItemSetRewardResource>",
        ),
    ]
}

/// The fake journal's item set lists, as the codec writes them.
pub(super) fn item_set_values() -> [(&'static str, Value); 2] {
    let query = |id: u32, kind: &str, compare: u64, player: bool| {
        fields(vec![
            ("knowledgeOrQueryId", hash_key(id)),
            ("compareValue", Value::Uint(compare)),
            ("compareOperator", Value::Name("Equals".into())),
            ("type", Value::Name(kind.into())),
            ("isExplicitPlayerKnowledgeQuery", Value::Bool(player)),
        ])
    };
    let item = QUEST_ITEM.fnv1a();
    let entry = fields(vec![
        ("entryId", hash_key(0x0000_0802)),
        ("itemId", hash_key(item)),
        (
            "lockedImageOverride",
            fields(vec![("image", Value::Guid(Guid([0; 16])))]),
        ),
        (
            "completionKnowledgeQuery",
            query(item, "SimpleFlag", 1, true),
        ),
        ("questReference", hash_key(0)),
        ("recipeId", hash_key(0)),
        ("collectibleDisplayProp", hash_key(0)),
        ("hasMultipleDisplayProps", Value::Bool(false)),
        ("hints", Value::List(Vec::new())),
    ]);
    let set = fields(vec![
        ("setId", hash_key(VANILLA_ITEM_SET)),
        ("setName", hash_key(0x0000_0805)),
        ("experienceValueId", hash_key(0)),
        ("entries", Value::List(vec![entry])),
        ("recipeRewards", Value::List(Vec::new())),
        (
            "completionKnowledgeQuery",
            query(VANILLA_PLAYER_QUERY, "Extern", 0, false),
        ),
        (
            "unlockKnowledgeQuery",
            query(VANILLA_WORLD_QUERY, "Extern", 0, false),
        ),
    ]);
    let category = fields(vec![
        ("categoryName", hash_key(ITEM_SET_CATEGORY.get())),
        ("entries", Value::List(vec![set])),
    ]);
    let reward = fields(vec![
        (
            "rewardKnowledge",
            fields(vec![("id", hash_key(VANILLA_ITEM_SET_REWARD))]),
        ),
        ("experienceValueId", hash_key(0)),
        (
            "knowledgeQueryData",
            query(VANILLA_PLAYER_QUERY, "Extern", 0, false),
        ),
    ]);
    [
        ("itemSetCategories", Value::List(vec![category])),
        ("itemSetRewards", Value::List(vec![reward])),
    ]
}
