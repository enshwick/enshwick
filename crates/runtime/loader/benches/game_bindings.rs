//! What one look at the client's key bindings pays beyond its memory reads:
//! decoding 140 actions' two keys, as client 26430625 has, with three
//! modifiers listed, and the rebinds to write with one mod's rebind asked.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick_loader::actions::game_bindings::ActionNumber;
use enshwick_loader::actions::game_bindings::keys::{Operation, key_bits};
use enshwick_loader::actions::game_bindings::rebinds::Rebinds;
use enshwick_loader::engine::mod_index::ModIndex;

const ACTIONS: u8 = 140;

fn key(usage: u8) -> Operation {
    [1, 0, usage, 0, 0, 0, 0, 0, 0, 0, 0, 0]
}

/// Each action's two combinations: keys spread over the HID usages, every
/// fifth with a modifier, every seventh second one empty.
fn table() -> Vec<[(Operation, u32); 2]> {
    (0..ACTIONS)
        .map(|n| {
            let usage = 4 + n % 96;
            let mask = u32::from(n % 5 == 0);
            let second = if n % 7 == 0 { [0; 12] } else { key(usage ^ 1) };
            [(key(usage), mask), (second, 0)]
        })
        .collect()
}

fn game_bindings(c: &mut Criterion) {
    let table = table();
    let modifiers = [key(0xe1), key(0xe0), key(0xe2)];
    let mut group = c.benchmark_group("game_bindings");
    group.bench_function("decode_140_actions", |b| {
        b.iter(|| {
            let mut sum = 0u32;
            for combos in &table {
                for (op, mask) in combos {
                    sum = sum.wrapping_add(key_bits(black_box(op), *mask, &modifiers));
                }
            }
            sum
        });
    });
    let mut rebinds = Rebinds::new();
    rebinds.set(ModIndex::new(0), ActionNumber::new(73), 0x24);
    let mut wanted = Vec::with_capacity(4);
    group.bench_function("wanted_one_rebind", |b| {
        b.iter(|| {
            black_box(&rebinds).wanted(|owner| black_box(owner) != ModIndex::LOADER, &mut wanted);
            wanted.len()
        });
    });
    group.finish();
}

criterion_group!(benches, game_bindings);
criterion_main!(benches);
