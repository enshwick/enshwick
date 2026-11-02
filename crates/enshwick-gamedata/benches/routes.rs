//! How long the engine routes take at start, after the reflection scan:
//! `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` name the executables, and a bench
//! without its executable is skipped.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick_gamedata::reflect::{Image, Layout, Reflection};
use enshwick_gamedata::{Routes, Side, Table};

fn find(c: &mut Criterion) {
    let mut group = c.benchmark_group("routes");
    group.sample_size(10);
    let Ok(routes) = Routes::embedded() else {
        return;
    };
    let Ok(empty) = Table::parse(
        "name = \"empty\"\nside = \"server\"\nguid = \"00000000000000000000000000000000\"\nage = 0",
    ) else {
        return;
    };
    let known = empty.resolve();
    for (name, variable, side) in [
        ("server", "ENSH_SERVER_EXE", Side::Server),
        ("client", "ENSH_CLIENT_EXE", Side::Client),
    ] {
        let Some(bytes) = std::env::var_os(variable).and_then(|exe| std::fs::read(exe).ok()) else {
            continue;
        };
        let Ok(image) = Image::parse(&bytes, Layout::File, None) else {
            continue;
        };
        let reflection = Reflection::scan(&image);
        group.bench_function(name, |b| {
            b.iter(|| routes.find(black_box(&image), &reflection, side, &known));
        });
    }
    group.finish();
}

criterion_group!(benches, find);
criterion_main!(benches);
