//! The per-event check of quest objectives: the one lookup a counted event
//! runs before anything else, with 0 and 100 objectives, for an event no
//! objective counts and one that one objective counts.

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick::ItemId;
use enshwick_knowledge::__internal::objective_lookup;

fn objectives(c: &mut Criterion) {
    let mut group = c.benchmark_group("objective lookup");
    for count in [0, 100] {
        let lookup = objective_lookup(count);
        group.bench_function(format!("{count} objectives, not counted"), |b| {
            b.iter(|| lookup(std::hint::black_box(ItemId::new(0x0bad))));
        });
        group.bench_function(format!("{count} objectives, counted"), |b| {
            b.iter(|| lookup(std::hint::black_box(ItemId::new(50))));
        });
    }
    group.finish();
}

criterion_group!(benches, objectives);
criterion_main!(benches);
