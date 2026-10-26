//! What a game rate costs on the game's hot paths: combining the factors of
//! a few mods, and scaling a whole number by the result.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick::rates::{combine, scale_whole};

fn rates(c: &mut Criterion) {
    let mut group = c.benchmark_group("rates");
    for mods in [0usize, 1, 4] {
        let factors = vec![1.5f32; mods];
        group.bench_function(format!("combine {mods} factors"), |b| {
            b.iter(|| combine(black_box(&factors).iter().copied()));
        });
    }
    group.bench_function("scale a whole number", |b| {
        b.iter(|| scale_whole(black_box(37), black_box(1.5)));
    });
    group.finish();
}

criterion_group!(benches, rates);
criterion_main!(benches);
