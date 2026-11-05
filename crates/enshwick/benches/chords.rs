//! What the client's look at the mod control keys costs once an interface
//! frame: which controls a few held keys bring down.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick::controls::{Key, chords};

fn look(c: &mut Criterion) {
    let names = ["G", "Shift+G", "Ctrl+G", "Shift+Ctrl+G", "H", "J", "K", "L"];
    let mut group = c.benchmark_group("chords");
    for count in [8usize, 32] {
        let bound: Vec<[u32; 2]> = (0..count)
            .map(|i| {
                let key = Key::parse(names[i % names.len()]).map_or(0, Key::bits);
                [key, if i % 2 == 0 { 0 } else { key }]
            })
            .collect();
        let g = Key::parse("G").map_or(0, Key::code);
        let shift = Key::parse("Shift+G").map_or(0, Key::modifiers);
        let mut out = Vec::new();
        group.bench_function(format!("{count} controls, Shift and G held"), |b| {
            b.iter(|| {
                out.clear();
                chords(
                    black_box(&bound),
                    |_, _, key| (key.code() == g).then_some(shift),
                    &mut out,
                );
            });
        });
    }
    group.finish();
}

criterion_group!(benches, look);
criterion_main!(benches);
