//! What a hot path pays to read the hooks mods registered: a sealed slice
//! behind a `OnceLock`, as the loader kept them by hand, and the same through
//! `Registry`.

use std::hint::black_box;
use std::sync::OnceLock;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick_loader::registry::Registry;

/// How many hooks mods registered, about a full mod list's.
const ITEMS: u32 = 16;

static BY_HAND: OnceLock<Box<[u32]>> = OnceLock::new();
static REGISTRY: Registry<u32> = Registry::new();

fn read(c: &mut Criterion) {
    let _first = BY_HAND.set((0..ITEMS).collect());
    for item in 0..ITEMS {
        let _added = REGISTRY.add(item);
    }
    REGISTRY.seal();
    let mut group = c.benchmark_group("registry_read");
    group.bench_function("once_lock_by_hand", |b| {
        b.iter(|| {
            BY_HAND
                .get()
                .map_or(0, |items| items.iter().map(|&i| black_box(i)).sum::<u32>())
        });
    });
    group.bench_function("registry", |b| {
        b.iter(|| {
            REGISTRY
                .sealed()
                .map_or(0, |items| items.iter().map(|&i| black_box(i)).sum::<u32>())
        });
    });
    group.finish();
}

criterion_group!(benches, read);
criterion_main!(benches);
