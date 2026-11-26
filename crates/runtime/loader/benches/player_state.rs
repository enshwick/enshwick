//! What one step's read of the local player's slots costs in `player_ui`'s
//! entry while a mod asks for them: the walk and copy of 48 slots alone, and
//! with the player's inventory watched for an item command's outcome.

#[cfg(windows)]
use criterion::{Criterion, criterion_group, criterion_main};

#[cfg(windows)]
fn player_state(c: &mut Criterion) {
    let mut group = c.benchmark_group("player_state");
    let mut slots = enshwick_loader::bench::bench_slot_read(false);
    group.bench_function("slots_48", |b| b.iter(&mut slots));
    let mut watched = enshwick_loader::bench::bench_slot_read(true);
    group.bench_function("slots_48_watched", |b| b.iter(&mut watched));
    group.finish();
}

#[cfg(windows)]
criterion_group!(benches, player_state);
#[cfg(windows)]
criterion_main!(benches);

#[cfg(not(windows))]
fn main() {}
