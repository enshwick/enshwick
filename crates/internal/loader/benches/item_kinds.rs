//! What the item kinds add to the game's resource open and resolve: a
//! resolve once the items are in, a resolve of another resource while items
//! wait, and an item's open and resolve.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick::abi::RecipeKind;
use enshwick_loader::item_kinds::{CAPACITY, ItemKinds};

/// Items the vanilla game and a few mods load, about.
const ITEMS: u32 = 4096;

static LOADED: ItemKinds<CAPACITY> = ItemKinds::new();
static LOADING: ItemKinds<CAPACITY> = ItemKinds::new();

fn kinds(c: &mut Criterion) {
    for items in [&LOADED, &LOADING] {
        items.seal(0, 0x104, Box::new([(10, RecipeKind::BUILDING_BLOCK)]));
        for handle in 1..=ITEMS {
            items.opened(handle);
            items.take_opened(handle);
        }
    }
    LOADING.opened(ITEMS + 1);

    let mut group = c.benchmark_group("item_kinds");
    group.bench_function("resolve_with_no_item_pending", |b| {
        b.iter(|| black_box(&LOADED).pending());
    });
    group.bench_function("resolve_of_another_resource_while_items_load", |b| {
        b.iter(|| black_box(&LOADING).pending() && LOADING.take_opened(black_box(0x7fff_0001)));
    });
    group.bench_function("open_and_resolve_of_an_item", |b| {
        b.iter(|| {
            LOADED.opened(black_box(7));
            LOADED.take_opened(black_box(7))
        });
    });
    group.finish();
}

criterion_group!(benches, kinds);
criterion_main!(benches);
