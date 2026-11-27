//! What one step's read of the local player's slots costs in `player_ui`'s
//! entry while a mod asks for them: the walk and copy of 48 slots alone, and
//! with the player's inventory watched for an item command's outcome; and
//! the read of the 20 equipment slots every step makes, with none or one
//! moved.

#[cfg(windows)]
use criterion::{Criterion, criterion_group, criterion_main};

#[cfg(windows)]
fn player_state(c: &mut Criterion) {
    let mut group = c.benchmark_group("player_state");
    let mut slots = enshwick_loader::bench::bench_slot_read(false);
    group.bench_function("slots_48", |b| b.iter(&mut slots));
    let mut watched = enshwick_loader::bench::bench_slot_read(true);
    group.bench_function("slots_48_watched", |b| b.iter(&mut watched));
    let mut still = enshwick_loader::bench::bench_equipment_step(false);
    group.bench_function("equipment_20", |b| b.iter(&mut still));
    let mut moving = enshwick_loader::bench::bench_equipment_step(true);
    group.bench_function("equipment_20_moved", |b| b.iter(&mut moving));
    group.finish();
}

#[cfg(windows)]
criterion_group!(benches, player_state);
#[cfg(windows)]
criterion_main!(benches);

#[cfg(not(windows))]
fn main() {}
