//! The fake's knowledge rules: one world rule and one player rule, laid out
//! as the game lays out the fields the fake keeps (server 26430253).

use enshwick_core::content::Guid;
use enshwick_core::resources::{Field, Type, Value};

use super::queries::{VANILLA_PLAYER_QUERY, VANILLA_WORLD_QUERY};
use super::vanilla::{VANILLA_PLAYER_KNOWLEDGE, blobs, fields, hash_key, named};

/// The trigger list's type, by its qualified name.
pub(super) const TRIGGERS: &str = "demo::GameKnowledgeQueryTriggerResource";
const TRIGGER: &str = "demo::GameKnowledgeQueryTrigger";
const UNLOCK: &str = "demo::KnowledgeUnlockOperation";

/// The fake's trigger list, the one resource of its type.
pub const KNOWLEDGE_RULES: Guid = Guid::parse("ead77791-0a1d-4535-a581-f48ac18fed0c");

/// The trigger list and the types it holds that the knowledge types do not.
pub(super) fn rule_types() -> Vec<Type<'static>> {
    let field = Field::new;
    vec![
        named(
            UNLOCK,
            12,
            4,
            vec![
                field("knowledgeId", 0, "demo::KnowledgeId"),
                field("type", 4, "demo::KnowledgeType"),
                field("value", 8, "demo::uint32"),
            ],
        ),
        blobs(UNLOCK),
        named(
            TRIGGER,
            24,
            4,
            vec![
                field("requiresEvent", 0, "demo::bool"),
                field("query", 4, "demo::KnowledgeQueryData"),
                field(
                    "unlockKnowledge",
                    16,
                    "demo::BlobArray<demo::KnowledgeUnlockOperation>",
                ),
            ],
        ),
        blobs(TRIGGER),
        named(
            TRIGGERS,
            16,
            4,
            vec![
                field(
                    "worldQueries",
                    0,
                    "demo::BlobArray<demo::GameKnowledgeQueryTrigger>",
                ),
                field(
                    "playerQueries",
                    8,
                    "demo::BlobArray<demo::GameKnowledgeQueryTrigger>",
                ),
            ],
        ),
    ]
}

/// The vanilla trigger list, as the codec writes it: a world rule on
/// [`VANILLA_WORLD_QUERY`] and a player rule on [`VANILLA_PLAYER_QUERY`],
/// each granting [`VANILLA_PLAYER_KNOWLEDGE`].
pub(super) fn rules_value() -> Value {
    let rule = |query: u32| {
        fields(vec![
            ("requiresEvent", Value::Bool(false)),
            (
                "query",
                fields(vec![
                    ("knowledgeOrQueryId", hash_key(query)),
                    ("compareValue", Value::Uint(0)),
                    ("compareOperator", Value::Name("Equals".into())),
                    ("type", Value::Name("Extern".into())),
                    ("isExplicitPlayerKnowledgeQuery", Value::Bool(false)),
                ]),
            ),
            (
                "unlockKnowledge",
                Value::List(vec![fields(vec![
                    (
                        "knowledgeId",
                        fields(vec![("id", hash_key(VANILLA_PLAYER_KNOWLEDGE))]),
                    ),
                    ("type", Value::Name("Bool".into())),
                    ("value", Value::Uint(1)),
                ])]),
            ),
        ])
    };
    fields(vec![
        ("worldQueries", Value::List(vec![rule(VANILLA_WORLD_QUERY)])),
        (
            "playerQueries",
            Value::List(vec![rule(VANILLA_PLAYER_QUERY)]),
        ),
    ])
}
