//! The fake's items and loot tables reflected, for removing and re-keying
//! vanilla content: four items, a random loot table and a fill table.

use enshwick_core::content::Guid;
use enshwick_core::resources::{Constant, Field, Kind, Registry, Type, Value, fnv1a32};

use super::FakeHost;

const ITEM: &str = "demo::ItemInfo";
const RANDOM: &str = "demo::LootableItemsResource";
const FILL: &str = "demo::ecs::DefaultInventoryResource";

/// A vanilla item of the fake that stacks to 20.
pub const LOOT_ITEM: Guid = Guid::parse("c0ffee00-0000-4000-8000-0000000017e1");
/// A vanilla item of the fake that stacks to 50.
pub const OTHER_ITEM: Guid = Guid::parse("c0ffee00-0000-4000-8000-0000000017e2");
/// A vanilla item of the fake that stacks to 30.
pub const SPARE_ITEM: Guid = Guid::parse("c0ffee00-0000-4000-8000-0000000017e4");
/// A vanilla item of the fake with an entity of its own (`HasPIDE`).
pub const ENTITY_ITEM: Guid = Guid::parse("c0ffee00-0000-4000-8000-0000000017e3");
/// The fake's random loot table, listing [`LOOT_ITEM`] and [`OTHER_ITEM`].
pub const RANDOM_LOOT: Guid = Guid::parse("c0ffee00-0000-4000-8000-00000000100a");
/// The fake's fill table, giving [`LOOT_ITEM`].
pub const FILL_TABLE: Guid = Guid::parse("c0ffee00-0000-4000-8000-00000000100b");

fn named(name: &'static str, size: u32, fields: Vec<Field<'static>>) -> Type<'static> {
    Type::new(name, Kind::Struct, size, 4).with_fields(fields)
}

fn blobs(of: &'static str) -> Type<'static> {
    Type::new(
        Box::leak(format!("demo::BlobArray<{of}>").into_boxed_str()),
        Kind::BlobArray,
        8,
        4,
    )
    .with_inner(of)
}

/// The item and loot types, with the fields a removal reads and writes.
fn removal_types() -> Vec<Type<'static>> {
    let field = Field::new;
    vec![
        Type::new("demo::uint8", Kind::Uint8, 1, 1),
        Type::new("demo::uint16", Kind::Uint16, 2, 2),
        Type::new("demo::uint32", Kind::Uint32, 4, 4),
        named("demo::ItemId", 4, vec![field("value", 0, "demo::uint32")]),
        Type::new("demo::ItemFlag", Kind::Enum, 1, 1)
            .with_inner("demo::uint8")
            .with_constants(vec![
                Constant::new("HasKnowledgeUnlocks", 0),
                Constant::new("HasPIDE", 1),
            ]),
        Type::new("demo::ItemFlags", Kind::Bitmask32, 4, 4).with_inner("demo::ItemFlag"),
        Type::new("demo::Guid", Kind::Guid, 16, 4),
        named(
            ITEM,
            32,
            vec![
                field("itemId", 0, "demo::ItemId"),
                field("objectId", 4, "demo::Guid"),
                field("maxStackSize", 20, "demo::uint16"),
                field("category", 22, "demo::uint8"),
                field("flags", 24, "demo::ItemFlags"),
            ],
        ),
        named(
            "demo::LootableItemInfo",
            8,
            vec![
                field("itemId", 0, "demo::ItemId"),
                field("stackSizeMax", 4, "demo::uint16"),
            ],
        ),
        blobs("demo::LootableItemInfo"),
        named(
            RANDOM,
            8,
            vec![field("items", 0, "demo::BlobArray<demo::LootableItemInfo>")],
        ),
        named(
            "demo::ecs::ItemStackInfoResource",
            8,
            vec![
                field("item", 0, "demo::ItemId"),
                field("countMin", 4, "demo::uint32"),
            ],
        ),
        blobs("demo::ecs::ItemStackInfoResource"),
        blobs("demo::uint32"),
        named(
            "demo::ItemLabelGroupResource",
            8,
            vec![field("labels", 0, "demo::BlobArray<demo::uint32>")],
        ),
        named(
            "demo::ecs::DefaultInventoryGroupResource",
            16,
            vec![
                field(
                    "stacks",
                    0,
                    "demo::BlobArray<demo::ecs::ItemStackInfoResource>",
                ),
                field("lootLabels", 8, "demo::ItemLabelGroupResource"),
            ],
        ),
        named(
            FILL,
            16,
            vec![field(
                "rootGroup",
                0,
                "demo::ecs::DefaultInventoryGroupResource",
            )],
        ),
    ]
}

fn fields(fields: Vec<(&str, Value)>) -> Value {
    Value::Struct(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

fn id(item: Guid) -> Value {
    fields(vec![("value", Value::Uint(u64::from(item.fnv1a())))])
}

impl FakeHost {
    /// Items and loot tables reflected: the vanilla items [`LOOT_ITEM`],
    /// [`OTHER_ITEM`], [`SPARE_ITEM`] and [`ENTITY_ITEM`], the random loot table
    /// [`RANDOM_LOOT`] and the fill table [`FILL_TABLE`]. What
    /// `Load::remove_item`, `remove_loot_table` and `rekey_item` resolve.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::EngineFacts;
    ///
    /// let host = enshwick::test::FakeHost::new().removal();
    /// assert!(host.hash("lootable_items").is_ok());
    /// ```
    #[must_use]
    pub fn removal(self) -> Self {
        let types = removal_types();
        let registry = Registry::new(types.clone());
        let hash = |name: &str| fnv1a32(name.as_bytes());
        let mut host = self.types(types);
        let item = |guid: Guid, stack: u64, flags: Vec<String>| {
            fields(vec![
                ("itemId", id(guid)),
                ("objectId", Value::Guid(guid)),
                ("maxStackSize", Value::Uint(stack)),
                ("category", Value::Uint(u64::from(guid.0[15]))),
                ("flags", Value::Flags(flags)),
            ])
        };
        let loot =
            |guid: Guid| fields(vec![("itemId", id(guid)), ("stackSizeMax", Value::Uint(5))]);
        let stack = fields(vec![("item", id(LOOT_ITEM)), ("countMin", Value::Uint(1))]);
        let served = [
            (LOOT_ITEM, ITEM, item(LOOT_ITEM, 20, vec![])),
            (OTHER_ITEM, ITEM, item(OTHER_ITEM, 50, vec![])),
            (SPARE_ITEM, ITEM, item(SPARE_ITEM, 30, vec![])),
            (
                ENTITY_ITEM,
                ITEM,
                item(ENTITY_ITEM, 1, vec!["HasPIDE".into()]),
            ),
            (
                RANDOM_LOOT,
                RANDOM,
                fields(vec![(
                    "items",
                    Value::List(vec![loot(LOOT_ITEM), loot(OTHER_ITEM)]),
                )]),
            ),
            (
                FILL_TABLE,
                FILL,
                fields(vec![(
                    "rootGroup",
                    fields(vec![
                        ("stacks", Value::List(vec![stack])),
                        (
                            "lootLabels",
                            fields(vec![("labels", Value::List(vec![Value::Uint(9)]))]),
                        ),
                    ]),
                )]),
            ),
        ];
        for (guid, ty, value) in served {
            if let Ok(bytes) = registry.encode(ty, &value, None) {
                host = host.resource(guid.key_by_hash(hash(ty)), bytes);
            }
        }
        host.type_hash("item_info", hash(ITEM))
            .type_hash("lootable_items", hash(RANDOM))
            .type_hash("loot_fill_table", hash(FILL))
    }
}
