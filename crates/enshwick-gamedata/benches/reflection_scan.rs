//! How long reflecting a game executable takes, the loader's first step at
//! every start: `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` name the
//! executables, and a bench without its executable is skipped.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick_gamedata::reflect::{Image, Layout, Reflection};

fn scan(c: &mut Criterion) {
    let mut group = c.benchmark_group("reflection_scan");
    group.sample_size(10);
    for (name, variable) in [("server", "ENSH_SERVER_EXE"), ("client", "ENSH_CLIENT_EXE")] {
        let Some(bytes) = std::env::var_os(variable).and_then(|exe| std::fs::read(exe).ok()) else {
            continue;
        };
        let Ok(image) = Image::parse(&bytes, Layout::File, None) else {
            continue;
        };
        group.bench_function(name, |b| b.iter(|| Reflection::scan(black_box(&image))));
    }
    group.finish();
}

criterion_group!(benches, scan);
criterion_main!(benches);
