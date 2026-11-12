//! What a step with page turns waiting pays on the game thread to find
//! their pages: one pass over a finest grid of 80 by 80 slots, as on server
//! 26430253, for a few turns and for a busy step's worth.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick_loader::world::terrain_pages::{Pairing, Turn, Waiting};

/// The finest grid's pages on server 26430253.
const SLOTS: u32 = 80 * 80;

fn pair(c: &mut Criterion) {
    // Distinct handles, as nearly every page holds its own entry.
    let slots: Vec<u32> = (1..=SLOTS)
        .map(|i| i.wrapping_mul(2_654_435_761) | 1)
        .collect();
    let mut pairing = Box::new(Pairing::new());
    let mut waiting = Box::new(Waiting::new());
    let mut group = c.benchmark_group("terrain_pages");
    for turns in [4u32, 80] {
        group.bench_function(format!("pair_{turns}_turns_6400_slots"), |b| {
            b.iter(|| {
                for i in 0..turns {
                    // Half match a page of this editor's finest grid, half
                    // another level's or editor's entry.
                    let handle = if i % 2 == 0 {
                        slots[(i * 97 % SLOTS) as usize]
                    } else {
                        i | 0x8000_0000
                    };
                    let _kept = waiting.push(Turn {
                        loaded: i % 3 != 0,
                        handle,
                    });
                }
                pairing.take(&mut waiting);
                let (pages, dropped) = pairing.pair(SLOTS, |i| slots[i as usize]);
                black_box(pages.count()) + usize::try_from(dropped).unwrap_or(0)
            });
        });
    }
    group.finish();
}

criterion_group!(benches, pair);
criterion_main!(benches);
