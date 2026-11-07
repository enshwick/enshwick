//! What a typed read of a live component costs: one read as its generated
//! struct from the component's own bytes, and one that also follows a
//! `BlobArray` into the set-up data after the component.

use std::hint::black_box;
use std::sync::OnceLock;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick::abi::Status;
use enshwick::resources::{Constant, Field, Kind, Type, fnv1a32};
use enshwick::typed::ComponentType;
use enshwick::typed::ecs::{EntityTagContainer, Faction};
use enshwick::world::World;
use enshwick::{Entity, Load, Mod, Off};

const FACTION: &str = "keen::ecs::Faction";
const TAGS: &str = "keen::ecs::EntityTagContainer";
const ENTRY: &str = "keen::ecs::EntityTagReferenceEntry";
const FACTIONS: Entity = Entity(4096);
const TAGGED: Entity = Entity(4097);

type Resolved = (
    World,
    ComponentType<Faction>,
    ComponentType<EntityTagContainer>,
);

static RESOLVED: OnceLock<Resolved> = OnceLock::new();

/// The build's types of `Faction` and `EntityTagContainer`, as server build
/// 479 reflects them.
fn types() -> Vec<Type<'static>> {
    let field = |name, offset, ty| Field::new(name, offset, ty);
    vec![
        Type::new("keen::uint8", Kind::Uint8, 1, 1),
        Type::new("keen::ecs::Component", Kind::Struct, 1, 1),
        Type::new("keen::FactionType", Kind::Enum, 1, 1)
            .with_inner("keen::uint8")
            .with_constants(vec![Constant::new("Vukah", 3), Constant::new("Player", 7)]),
        Type::new(FACTION, Kind::Struct, 1, 1)
            .with_inner("keen::ecs::Component")
            .with_fields(vec![field("faction", 0, "keen::FactionType")]),
        Type::new(
            "keen::ObjectReference<keen::ecs::EntityTag>",
            Kind::ObjectReference,
            16,
            4,
        ),
        Type::new("keen::ecs::EntityTagReference", Kind::Typedef, 16, 4)
            .with_inner("keen::ObjectReference<keen::ecs::EntityTag>"),
        Type::new(ENTRY, Kind::Struct, 16, 4).with_fields(vec![field(
            "tag",
            0,
            "keen::ecs::EntityTagReference",
        )]),
        Type::new(
            "keen::BlobArray<keen::ecs::EntityTagReferenceEntry>",
            Kind::BlobArray,
            8,
            4,
        )
        .with_inner(ENTRY),
        Type::new(TAGS, Kind::Struct, 8, 4)
            .with_inner("keen::ecs::Component")
            .with_fields(vec![field(
                "tags",
                0,
                "keen::BlobArray<keen::ecs::EntityTagReferenceEntry>",
            )]),
    ]
}

struct Bench;

impl Mod for Bench {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let faction = ComponentType::<Faction>::resolve(load)?;
        let tags = ComponentType::<EntityTagContainer>::resolve(load)?;
        let _ = RESOLVED.set((load.world()?, faction, tags));
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

#[expect(clippy::panic, reason = "without its types the bench measures nothing")]
fn reads(c: &mut Criterion) {
    let hash = |name: &str| fnv1a32(name.as_bytes());
    // Three tags, as the most a live `EntityTagContainer` held on server
    // 26430253.
    let mut tagged = [8i32.to_le_bytes(), 3u32.to_le_bytes()].concat();
    tagged.extend_from_slice(&[0x5a; 48]);
    let loader = enshwick::test::FakeHost::new()
        .world()
        .types(types())
        .type_hash("ecs::Faction", hash(FACTION))
        .type_size("ecs::Faction", 1)
        .type_hash("ecs::EntityTagContainer", hash(TAGS))
        .type_size("ecs::EntityTagContainer", 8)
        .component(FACTIONS, hash(FACTION), &[7])
        .component(TAGGED, hash(TAGS), &tagged)
        .install();
    // SAFETY: the three functions are the exports of this file's `declare_mod!(Bench)`.
    let status = unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
    assert_eq!(status, Status::OK, "{:?}", loader.logs());
    let Some(&(world, faction, tags)) = RESOLVED.get() else {
        panic!("not resolved: {:?}", loader.logs());
    };
    let mut group = c.benchmark_group("typed");
    group.bench_function("component_read", |b| {
        b.iter(|| faction.read(black_box(world), black_box(FACTIONS)));
    });
    group.bench_function("component_read_blob", |b| {
        b.iter(|| tags.read(black_box(world), black_box(TAGGED)));
    });
    group.finish();
}

criterion_group!(benches, reads);
criterion_main!(benches);
