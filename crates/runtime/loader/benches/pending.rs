//! What a step pays to look at a queue: an idle step's take on a locked list
//! as the step parts took them before, the same take behind the queue's
//! gate, and a take that finds one item.

use std::hint::black_box;
use std::sync::Mutex;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick_base::Relock;
use enshwick_loader::engine::pending::Pending;

fn pending(c: &mut Criterion) {
    let mut group = c.benchmark_group("pending");
    let locked: Mutex<Vec<u64>> = Mutex::new(Vec::new());
    group.bench_function("idle_take_locked", |b| {
        b.iter(|| core::mem::take(&mut *black_box(&locked).relock()));
    });
    let gated: Pending<u64> = Pending::new();
    group.bench_function("idle_take_gated", |b| {
        b.iter(|| black_box(&gated).take());
    });
    group.bench_function("take_one", |b| {
        b.iter(|| {
            gated.push(7);
            black_box(&gated).take()
        });
    });
    group.finish();
}

criterion_group!(benches, pending);
criterion_main!(benches);
