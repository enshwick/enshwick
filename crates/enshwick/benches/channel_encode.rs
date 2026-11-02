//! What a channel message costs to encode before it crosses to the loader:
//! into a fresh allocation, as `Channel::send` did, and into a stack buffer,
//! as it does now, for a small message and one of 6,000 bytes.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick::net::MAX_MESSAGE;

#[derive(serde::Serialize)]
enum Sync {
    Marker { at: [f64; 3] },
    Blob(Vec<u8>),
}

fn encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("channel_encode");
    let messages = [
        (
            "marker",
            Sync::Marker {
                at: [1.0, -2.0, 3.5],
            },
        ),
        ("blob_6000", Sync::Blob(vec![7; 6000])),
    ];
    for (name, message) in &messages {
        group.bench_function(format!("alloc_{name}"), |b| {
            b.iter(|| postcard::to_allocvec(black_box(message)).map(|bytes| bytes.len()));
        });
        group.bench_function(format!("stack_{name}"), |b| {
            b.iter(|| {
                let mut buffer = [0u8; MAX_MESSAGE];
                postcard::to_slice(black_box(message), &mut buffer).map(|bytes| bytes.len())
            });
        });
    }
    group.finish();
}

criterion_group!(benches, encode);
criterion_main!(benches);
