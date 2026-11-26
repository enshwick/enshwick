//! What one served read costs: the loader builds the resource through the
//! mod's builder, which here copies a large base and changes one byte,
//! decodes a typed table, doubles a field and encodes it again, or lists an
//! item in a large item registry with a content helper, in the lent output
//! and, for comparison, in a `Vec` copied into it.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick::abi::{ResourceKey, Status};
use enshwick::content::Guid;
use enshwick::content::items::{ItemLayout, list_item};
use enshwick::resources::{Field, Kind, Registry, Type, Value, fnv1a32};
use enshwick::{Load, Mod, Off};

const TABLE_TYPE: &str = "keen::balance::Table";
const TABLE: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
const LARGE: Guid = Guid::parse("5afe5afe-0000-4000-8000-000000000001");
/// A large resource's size, as the game's item registry is on server 26430253.
const LARGE_LEN: usize = 256 * 1024;

fn table_key() -> ResourceKey {
    TABLE.key_by_hash(fnv1a32(TABLE_TYPE.as_bytes()))
}

fn large_key() -> ResourceKey {
    LARGE.key_by_hash(7)
}

/// An item registry listed in place, and the same listed in a `Vec` first.
const LISTED: Guid = Guid::parse("5afe5afe-0000-4000-8000-000000000002");
const COPIED: Guid = Guid::parse("5afe5afe-0000-4000-8000-000000000003");
/// The item a build lists.
const ITEM: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
/// A registry's reference array at 0 and its names, none, at 8.
fn layout() -> ItemLayout {
    let mut value = ItemLayout::default();
    value.id = 0;
    value.object = 4;
    value.name = 0x48;
    value.description = 0x68;
    value.flags = 0x70;
    value.pid_entity = 0x78;
    value.ui_values = 0x88;
    value.icon_image = 0x58;
    value.ui_value = [0x28, 0, 4, 8, 0x24];
    value.refs = 0;
    value.names = 8;
    value
}

/// An item registry of `LARGE_LEN` bytes of references after its headers.
fn registry() -> Vec<u8> {
    let refs = u32::try_from(LARGE_LEN / 16).unwrap_or(u32::MAX);
    let mut bytes = [16u32, refs, 8, 0].map(u32::to_le_bytes).concat();
    bytes.resize(16 + LARGE_LEN, 0);
    bytes
}

fn types() -> Vec<Type<'static>> {
    vec![
        Type::new("keen::float32", Kind::Float32, 4, 4),
        Type::new(TABLE_TYPE, Kind::Struct, 8, 4).with_fields(vec![
            Field::new("damage", 0, "keen::float32"),
            Field::new("speed", 4, "keen::float32"),
        ]),
    ]
}

struct Bench;

impl Mod for Bench {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let registry = Registry::game()?;
        load.serve(large_key(), large_key(), |base, out| {
            out.reserve(base.len());
            out.extend_from_slice(base);
            *out.first_mut()? ^= 1;
            Some(())
        })?;
        load.serve(table_key(), table_key(), move |base, out| {
            let mut value = registry.decode(TABLE_TYPE, base).ok()?;
            if let Some(Value::Float(damage)) = value.field_mut("damage") {
                *damage *= 2.0;
            }
            registry
                .encode_into(TABLE_TYPE, &value, Some(base), out)
                .ok()
        })?;
        let item_layout = layout();
        let listed = LISTED.key_by_hash(7);
        load.serve(listed, listed, move |base, out| {
            list_item(&item_layout, base, ITEM, "Item", out)
        })?;
        let copied = COPIED.key_by_hash(7);
        load.serve(copied, copied, move |base, out| {
            let mut bytes = Vec::new();
            list_item(&item_layout, base, ITEM, "Item", &mut bytes)?;
            out.copy_from(&bytes)
        })?;
        Ok(Self)
    }
}

enshwick::test::declare_mod!(
    Bench,
    r#"
    id = "enshwick.bench"
    side = "server"
    "#
);

#[expect(clippy::panic, reason = "without a host the bench measures nothing")]
fn reads(c: &mut Criterion) {
    let table = [2.5f32.to_le_bytes(), 1.0f32.to_le_bytes()].concat();
    let loader = enshwick::test::FakeHost::new()
        .types(types())
        .resource(table_key(), table)
        .resource(large_key(), vec![0x5a; LARGE_LEN])
        .resource(LISTED.key_by_hash(7), registry())
        .resource(COPIED.key_by_hash(7), registry())
        .install();
    // SAFETY: the three functions are the exports of this file's `declare_mod!(Bench)`.
    let status = unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
    assert_eq!(status, Status::OK, "{:?}", loader.logs());
    let Some(host) = enshwick::host() else {
        panic!("no host: {:?}", loader.logs());
    };
    c.bench_function("served_read_large", |b| {
        b.iter(|| black_box(host.served_resource(black_box(large_key()))));
    });
    c.bench_function("served_read_content", |b| {
        b.iter(|| black_box(host.served_resource(black_box(LISTED.key_by_hash(7)))));
    });
    c.bench_function("served_read_content_copied", |b| {
        b.iter(|| black_box(host.served_resource(black_box(COPIED.key_by_hash(7)))));
    });
    c.bench_function("served_read_value", |b| {
        b.iter(|| black_box(host.served_resource(black_box(table_key()))));
    });
}

criterion_group!(benches, reads);
criterion_main!(benches);
