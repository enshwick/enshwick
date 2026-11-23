//! What the mods' sounds cost the interface frame: a frame with sounds
//! playing and nothing queued, and a frame that runs one mod's full starts.

#[cfg(windows)]
use criterion::{Criterion, criterion_group, criterion_main};

#[cfg(windows)]
fn sound_frame(c: &mut Criterion) {
    let mut group = c.benchmark_group("sound_frame");
    for live in [0, 32] {
        let mut idle = enshwick_loader::bench::bench_sound_frame(live, 0);
        group.bench_function(format!("{live}_playing_nothing_queued"), |b| {
            b.iter(&mut idle);
        });
    }
    let mut starts = enshwick_loader::bench::bench_sound_frame(0, 64);
    group.bench_function("64_starts_and_stops", |b| b.iter(&mut starts));
    group.finish();
}

#[cfg(windows)]
criterion_group!(benches, sound_frame);
#[cfg(windows)]
criterion_main!(benches);

#[cfg(not(windows))]
fn main() {}
