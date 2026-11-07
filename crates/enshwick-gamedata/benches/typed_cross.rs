//! How long a sample of every covered typed type takes to cross into a
//! script and back, through its shape and through its compiled Rust
//! conversions: `ENSH_GAMEDATA` names the game data folder with the
//! committed snapshot, and a bench without it is skipped.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick::resources::Value;
use enshwick_gamedata::typed::{Catalogue, engine_name, snapshot_path};
use enshwick_gamedata::{Side, Table, kept, newest};

fn typed_cross(c: &mut Criterion) {
    let tables: Vec<Table> = kept()
        .unwrap_or_default()
        .iter()
        .filter_map(|(_, text)| Table::parse(text).ok())
        .collect();
    let Some(text) = newest(&tables, Side::Server)
        .and_then(|table| snapshot_path(&table.file_name()))
        .and_then(|path| std::fs::read_to_string(path).ok())
    else {
        return;
    };
    let Ok(catalogue) = Catalogue::from_snapshot(&text) else {
        return;
    };
    let registry = catalogue.registry();
    let samples: Vec<Option<Value>> = enshwick::typed::NAMED
        .iter()
        .map(|row| enshwick::typed::sample(&registry, engine_name(row.id)?))
        .collect();
    let rows = || {
        enshwick::typed::NAMED
            .iter()
            .zip(enshwick::typed::COMPILED)
            .zip(&samples)
            .filter_map(|(row, sample)| Some((row, sample.as_ref()?)))
    };
    let mut group = c.benchmark_group("typed_cross");
    group.sample_size(20);
    group.bench_function("shape", |b| {
        b.iter(|| {
            for ((row, _), sample) in rows() {
                if let Ok(shown) = row.to_script(black_box(sample)) {
                    black_box(row.to_codec(shown).ok());
                }
            }
        });
    });
    group.bench_function("compiled", |b| {
        b.iter(|| {
            for ((_, rust), sample) in rows() {
                if let Ok(shown) = (rust.to_script)(black_box(sample)) {
                    black_box((rust.from_script)(shown).ok());
                }
            }
        });
    });
    group.finish();
}

criterion_group!(benches, typed_cross);
criterion_main!(benches);
