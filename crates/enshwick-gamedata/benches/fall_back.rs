//! How long carrying the newest table of a side over to a build takes, the
//! loader's start on a build without a table, searched and read back from
//! its save: `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` name the executables,
//! and a bench without its executable is skipped.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick_gamedata::reflect::{Image, Layout};
use enshwick_gamedata::{Side, Table, embedded, newest};

fn fall_back(c: &mut Criterion) {
    let tables: Vec<Table> = embedded()
        .iter()
        .filter_map(|(_, text)| Table::parse(text).ok())
        .collect();
    let mut group = c.benchmark_group("fall_back");
    group.sample_size(10);
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
        let (Some(table), Some(text)) = (newest(&tables, side), image.section(".text")) else {
            continue;
        };
        let range = image.range(text);
        let Some(code) = image.read(range.start, range.end - range.start) else {
            continue;
        };
        let code_rva = range.start - image.base();
        group.bench_function(format!("{name}_search"), |b| {
            b.iter(|| table.fall_back(black_box(code), code_rva, |_| None));
        });
        let key = table.carry_key(code, code_rva, 0..0);
        let Some(saved) = table.fall_back(code, code_rva, |_| None).to_saved(key) else {
            continue;
        };
        group.bench_function(format!("{name}_saved"), |b| {
            b.iter(|| {
                let key = table.carry_key(black_box(code), code_rva, 0..0);
                table.restore(black_box(&saved), key)
            });
        });
    }
    group.finish();
}

criterion_group!(benches, fall_back);
criterion_main!(benches);
