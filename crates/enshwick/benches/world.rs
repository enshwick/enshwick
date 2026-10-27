//! What the world's hot accessors cost, and what a copy of the world costs
//! where a mod passes it by value or moves it into a closure.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick::Entity;
use enshwick::abi::Status;
use enshwick::state::ModLocal;
use enshwick::test::FakeHost;
use enshwick::world::World;
use enshwick::{Load, Mod, Off};

static WORLD: ModLocal<World> = ModLocal::new();

struct Bench;

impl Mod for Bench {
    fn manifest() -> &'static enshwick::abi::Manifest {
        enshwick::manifest!(id: "enshwick.bench", side: SERVER)
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let _ = WORLD.set(load.world()?);
        Ok(Self)
    }
}

enshwick::declare_mod!(Bench);

/// Props in the world for the scan bench, 4 metres apart on a 64-wide grid.
const PROPS: u32 = 4096;

/// Takes the world by value, as a mod's helper function would.
#[inline(never)]
fn item_of(world: World, entity: Entity) -> Option<u32> {
    world.used_item(entity)
}

#[expect(clippy::panic, reason = "without its world the bench measures nothing")]
fn accessors(c: &mut Criterion) {
    let mut fake = FakeHost::new()
        .type_hash("current_transform", 1)
        .type_hash("used_item", 2)
        .constant("current_transform.transform", 0)
        .constant("world_transform.orientation", 24)
        .constant("item_info.placed_entity", 0x3c0)
        .item(enshwick::ItemId::new(7), 1, 0)
        .item_places(enshwick::ItemId::new(7), [7; 16])
        .component(Entity(4096), 2, &7u32.to_le_bytes())
        .component(Entity(4096), 1, &[0; 64]);
    for i in 0..PROPS {
        let entity = 10_000 + i;
        let at = [f64::from(i % 64), 0.0, f64::from(i / 64)].map(|axis| axis * 4.0);
        fake = fake
            .placed(2, Entity(entity), at)
            .template(Entity(entity), [7; 16])
            .component(Entity(entity), 2, &7u32.to_le_bytes())
            .component(Entity(entity), 1, &[0; 40]);
    }
    let loader = fake.install();
    // SAFETY: both functions are the exports of this file's `declare_mod!(Bench)`.
    let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
    assert_eq!(status, Status::OK, "{:?}", loader.logs());
    let Some(&world) = WORLD.get() else {
        panic!("no world: {:?}", loader.logs());
    };
    let entity = Entity(4096);
    let mut group = c.benchmark_group("world");
    group.bench_function("used_item", |b| {
        b.iter(|| black_box(world).used_item(black_box(entity)));
    });
    group.bench_function("position", |b| {
        b.iter(|| black_box(world).position(black_box(entity)));
    });
    group.bench_function("props_in_64_of_4096", |b| {
        b.iter(|| black_box(world).props_in(black_box([0.0; 3]), black_box([31.0, 1.0, 31.0])));
    });
    group.bench_function("by_value", |b| {
        b.iter(|| item_of(black_box(world), black_box(entity)));
    });
    group.finish();
}

criterion_group!(benches, accessors);
criterion_main!(benches);
