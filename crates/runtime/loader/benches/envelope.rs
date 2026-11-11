//! What one message on the own socket costs to seal on its way out and to
//! open on its way in, for a small payload and one near the largest a
//! control message takes.

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use enshwick_loader::wire::{MAX_SEALED, open, seal};

fn envelope(c: &mut Criterion) {
    let mut group = c.benchmark_group("envelope");
    for size in [64usize, 6000] {
        let payload: Vec<u8> = (0..size)
            .map(|i| u8::try_from(i % 251).unwrap_or(0) + 1)
            .collect();
        let mut out = [0; MAX_SEALED];
        let sealed = seal(&mut out, 0x1234_5678, &payload).unwrap_or_default();
        group.throughput(Throughput::Bytes(u64::try_from(size).unwrap_or(0)));
        group.bench_function(format!("seal_{size}"), |b| {
            let mut out = [0; MAX_SEALED];
            b.iter(|| seal(&mut out, black_box(0x1234_5678), black_box(&payload)));
        });
        group.bench_function(format!("open_{size}"), |b| {
            b.iter(|| open(black_box(&out[..sealed])).map(|(channel, p)| (channel, p.len())));
        });
    }
    group.finish();
}

criterion_group!(benches, envelope);
criterion_main!(benches);
