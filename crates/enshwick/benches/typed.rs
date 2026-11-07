//! What a typed read costs: one game event handled as its generated struct,
//! and one component read as its generated struct.

use std::hint::black_box;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick::abi::Status;
use enshwick::resources::{Constant, Field, Kind, Type, fnv1a32};
use enshwick::typed::ComponentType;
use enshwick::typed::ecs::{EntityDiedEvent, Faction};
use enshwick::world::World;
use enshwick::{Entity, Load, Mod, Off};

const FACTION: &str = "keen::ecs::Faction";
const DIED: &str = "keen::ecs::EntityDiedEvent";

static RESOLVED: OnceLock<(World, ComponentType<Faction>)> = OnceLock::new();
static DEAD: AtomicU32 = AtomicU32::new(0);

/// The build's types of `Faction` and `EntityDiedEvent`, as server build
/// 479 reflects them.
fn types() -> Vec<Type<'static>> {
    let field = |name, offset, ty| Field::new(name, offset, ty);
    vec![
        Type::new("keen::uint8", Kind::Uint8, 1, 1),
        Type::new("keen::uint32", Kind::Uint32, 4, 4),
        Type::new("keen::sint64", Kind::Sint64, 8, 8),
        Type::new("keen::ecs::Component", Kind::Struct, 1, 1),
        Type::new("keen::ecs::Event", Kind::Struct, 1, 1),
        Type::new("keen::FactionType", Kind::Enum, 1, 1)
            .with_inner("keen::uint8")
            .with_constants(vec![Constant::new("Vukah", 3), Constant::new("Player", 7)]),
        Type::new(FACTION, Kind::Struct, 1, 1)
            .with_inner("keen::ecs::Component")
            .with_fields(vec![field("faction", 0, "keen::FactionType")]),
        Type::new("keen::Time", Kind::Struct, 8, 8).with_fields(vec![field(
            "value",
            0,
            "keen::sint64",
        )]),
        Type::new("keen::ecs::GameEvent", Kind::Struct, 8, 8)
            .with_inner("keen::ecs::Event")
            .with_fields(vec![field("timeStamp", 0, "keen::Time")]),
        Type::new("keen::ecs::EntityId", Kind::Struct, 4, 4).with_fields(vec![field(
            "id",
            0,
            "keen::uint32",
        )]),
        Type::new(DIED, Kind::Struct, 16, 8)
            .with_inner("keen::ecs::GameEvent")
            .with_fields(vec![field("deadEntityId", 8, "keen::ecs::EntityId")]),
    ]
}

struct Bench;

impl Mod for Bench {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        enshwick::typed::EventType::<EntityDiedEvent>::resolve(load)?.on(load, |died| {
            DEAD.store(died.dead_entity_id.id, Ordering::Relaxed);
        })?;
        let faction = ComponentType::<Faction>::resolve(load)?;
        let _ = RESOLVED.set((load.world()?, faction));
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
    let entity = Entity(4096);
    let loader = enshwick::test::FakeHost::new()
        .world()
        .types(types())
        .type_hash("ecs::Faction", hash(FACTION))
        .type_size("ecs::Faction", 1)
        .game_event_type("ecs::EntityDiedEvent", hash(DIED), 16)
        .component(entity, hash(FACTION), &[7])
        .install();
    // SAFETY: the three functions are the exports of this file's `declare_mod!(Bench)`.
    let status = unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
    assert_eq!(status, Status::OK, "{:?}", loader.logs());
    let Some(&(world, faction)) = RESOLVED.get() else {
        panic!("not resolved: {:?}", loader.logs());
    };
    let mut died = [0u8; 16];
    died[8..12].copy_from_slice(&77u32.to_le_bytes());
    let mut group = c.benchmark_group("typed");
    group.bench_function("event_on", |b| {
        b.iter(|| loader.raise_game_event(black_box(hash(DIED)), black_box(&died)));
    });
    group.bench_function("component_read", |b| {
        b.iter(|| faction.read(black_box(world), black_box(entity)));
    });
    group.finish();
    assert_eq!(DEAD.load(Ordering::Relaxed), 77);
}

criterion_group!(benches, reads);
criterion_main!(benches);
