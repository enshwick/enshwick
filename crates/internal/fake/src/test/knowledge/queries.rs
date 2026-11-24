//! The fake's query database: one world query and one player query that
//! names it, laid out as the game lays out the fields the fake keeps
//! (server 26430253).

use enshwick_core::resources::{Field, Kind, Type, Value};

use super::vanilla::{blobs, enumeration, fields, hash_key, named};

/// The query database's type, by its qualified name.
pub(super) const DATABASE: &str = "demo::GameKnowledgeQueryResourceDb";
const QUERY: &str = "demo::GameKnowledgeQueryResource";
const ACTION: &str = "demo::KnowledgeQueryActionInfo";

/// The fake's query database, the one resource of its type.
pub const QUERY_DATABASE: enshwick_core::content::Guid =
    enshwick_core::content::Guid::parse("aacd27f0-25ad-42b6-9c8d-91dca8fcafd8");

/// The vanilla world query of the fake's database, at index 0: it tests the
/// world's [`VANILLA_WORLD_KNOWLEDGE`](super::vanilla::VANILLA_WORLD_KNOWLEDGE).
pub const VANILLA_WORLD_QUERY: u32 = 0x0000_0700;
/// The vanilla player query of the fake's database, at index 1, the start
/// of its player range: [`VANILLA_WORLD_QUERY`] holds.
pub const VANILLA_PLAYER_QUERY: u32 = 0x0000_0701;
/// The name of [`VANILLA_PLAYER_QUERY`] in the fake's database.
pub const VANILLA_QUERY_NAME: &str = "Vanilla_World_Counted";

/// The query database and the types it holds that the knowledge types do
/// not.
pub(super) fn query_types() -> Vec<Type<'static>> {
    let field = Field::new;
    vec![
        Type::new("demo::BlobString", Kind::BlobString, 8, 4),
        enumeration(
            "demo::KnowledgeQueryType",
            &["Simple", "SimpleCount", "SimpleFlag", "AND", "OR", "XOR"],
        ),
        enumeration("demo::KnowledgeQueryActionType", &["Knowledge", "Query"]),
        named(
            "demo::KnowledgeQueryId",
            4,
            4,
            vec![field("id", 0, "demo::HashKey32")],
        ),
        named(
            ACTION,
            36,
            4,
            vec![
                field("type", 0, "demo::KnowledgeQueryActionType"),
                field("name", 4, "demo::BlobString"),
                field("query", 12, "demo::KnowledgeQueryData"),
                field("queryIndex", 24, "demo::uint32"),
                field("playerProgressionQueryIndex", 28, "demo::uint32"),
                field("invertResult", 32, "demo::bool"),
                field("isPlayerAction", 33, "demo::bool"),
            ],
        ),
        blobs(ACTION),
        named(
            QUERY,
            32,
            4,
            vec![
                field("name", 0, "demo::BlobString"),
                field("queryId", 8, "demo::KnowledgeQueryId"),
                field("queryIndex", 12, "demo::uint32"),
                field("type", 16, "demo::KnowledgeQueryType"),
                field("isPlayerQuery", 17, "demo::bool"),
                field("invertResult", 18, "demo::bool"),
                field("hasOptionalPlayerProgression", 19, "demo::bool"),
                field(
                    "actions",
                    20,
                    "demo::BlobArray<demo::KnowledgeQueryActionInfo>",
                ),
            ],
        ),
        blobs(QUERY),
        named(
            DATABASE,
            12,
            4,
            vec![
                field("playerQueryStartIndex", 0, "demo::uint32"),
                field(
                    "queries",
                    4,
                    "demo::BlobArray<demo::GameKnowledgeQueryResource>",
                ),
            ],
        ),
    ]
}

/// The vanilla query database, as the codec writes it.
pub(super) fn database_value() -> Value {
    let data = |id: u32, kind: &str, compare: u64, player: bool| {
        fields(vec![
            ("knowledgeOrQueryId", hash_key(id)),
            ("compareValue", Value::Uint(compare)),
            ("compareOperator", Value::Name("Equals".into())),
            ("type", Value::Name(kind.into())),
            ("isExplicitPlayerKnowledgeQuery", Value::Bool(player)),
        ])
    };
    let action = |kind: &str, query: Value, index: u64| {
        fields(vec![
            ("type", Value::Name(kind.into())),
            ("name", Value::Text("Part".into())),
            ("query", query),
            ("queryIndex", Value::Uint(index)),
            ("playerProgressionQueryIndex", Value::Uint(index)),
            ("invertResult", Value::Bool(false)),
            ("isPlayerAction", Value::Bool(false)),
        ])
    };
    let query = |name: &str, id: u32, index: u64, player: bool, actions: Vec<Value>| {
        fields(vec![
            ("name", Value::Text(name.into())),
            ("queryId", fields(vec![("id", hash_key(id))])),
            ("queryIndex", Value::Uint(index)),
            ("type", Value::Name("AND".into())),
            ("isPlayerQuery", Value::Bool(player)),
            ("invertResult", Value::Bool(false)),
            ("hasOptionalPlayerProgression", Value::Bool(false)),
            ("actions", Value::List(actions)),
        ])
    };
    let world = query(
        "Vanilla_World_Set",
        VANILLA_WORLD_QUERY,
        0,
        false,
        vec![action(
            "Knowledge",
            data(
                super::vanilla::VANILLA_WORLD_KNOWLEDGE,
                "SimpleCount",
                0,
                false,
            ),
            0,
        )],
    );
    let player = query(
        VANILLA_QUERY_NAME,
        VANILLA_PLAYER_QUERY,
        1,
        true,
        vec![action(
            "Query",
            data(VANILLA_WORLD_QUERY, "Extern", 0, false),
            0,
        )],
    );
    fields(vec![
        ("playerQueryStartIndex", Value::Uint(1)),
        ("queries", Value::List(vec![world, player])),
    ])
}
