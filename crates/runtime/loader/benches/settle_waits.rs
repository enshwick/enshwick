//! What the post-step pays for mods' waits on their transactions' ends: no
//! wait, the only cost while no mod waits, and one and eight waits on
//! transactions still pending, a step that ends none of them.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick_loader::bench_settle_step;

fn settle_waits(c: &mut Criterion) {
    let mut group = c.benchmark_group("settle_waits");
    for pending in [0, 1, 8] {
        let step = bench_settle_step(pending);
        group.bench_function(format!("step_{pending}_pending"), |b| {
            b.iter(|| black_box(&step)());
        });
    }
    group.finish();
}

criterion_group!(benches, settle_waits);
criterion_main!(benches);
