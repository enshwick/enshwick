//! What each knowledge change pays for the quest events: whether its id is
//! one a quest step reads, on an index of 2,000 ids, about what the game's
//! and a few mods' quests name; before the quest events no change paid it.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick_loader::bench::bench_quest_change;

fn quest_change(c: &mut Criterion) {
    let indexed = bench_quest_change(2_000);
    let mut group = c.benchmark_group("quest_change");
    group.bench_function("not_a_quest_id", |b| {
        b.iter(|| indexed(black_box(0x1d)));
    });
    group.bench_function("a_quest_id", |b| {
        b.iter(|| indexed(black_box(0x9e37_79b9)));
    });
    group.finish();
}

criterion_group!(benches, quest_change);
criterion_main!(benches);
