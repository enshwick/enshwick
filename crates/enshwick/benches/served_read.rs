//! What one served read costs: the loader builds the resource through the
//! mod's builder, which here copies a large base and changes one byte, or
//! decodes a typed table, doubles a field and encodes it again.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick::abi::{ResourceKey, Status};
use enshwick::content::Guid;
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
    c.bench_function("served_read_value", |b| {
        b.iter(|| black_box(host.served_resource(black_box(table_key()))));
    });
}

criterion_group!(benches, reads);
criterion_main!(benches);
