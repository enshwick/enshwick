//! What one `HostApi` entry call costs through the layer every entry passes
//! through, with no loader running: `log_level` answers a handle it does not
//! know, as every log call asks, and `type_hash` refuses with no loader.

use criterion::{Criterion, criterion_group, criterion_main};

#[cfg(windows)]
fn host_entry(c: &mut Criterion) {
    use std::hint::black_box;

    use enshwick_core::abi::{Level, ModHandle, Str};

    let api = enshwick_loader::bench::host_api();
    let mut group = c.benchmark_group("host_entry");
    group.bench_function("log_level", |b| {
        let mut level = Level::INFO;
        b.iter(|| (api.log_level)(black_box(ModHandle(7)), black_box(&raw mut level)));
    });
    group.bench_function("type_hash_refused", |b| {
        let mut out = 0u32;
        b.iter(|| {
            (api.type_hash)(
                black_box(ModHandle(7)),
                black_box(Str::new("Item")),
                black_box(&raw mut out),
            )
        });
    });
    group.finish();
}

#[cfg(not(windows))]
fn host_entry(_: &mut Criterion) {}

criterion_group!(benches, host_entry);
criterion_main!(benches);
