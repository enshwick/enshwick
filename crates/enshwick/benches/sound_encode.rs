//! What encoding a mod's recording as `FramelessQOA` costs a background
//! thread at load: one minute of stereo at the game's rate.

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use enshwick::sound::SAMPLE_RATE;

/// One minute of a stereo chord, as a mod's music file holds it.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "a frame index well within f64, and samples within half of the i16 range"
)]
fn minute() -> Vec<i16> {
    let frames = SAMPLE_RATE as usize * 60;
    let rate = f64::from(SAMPLE_RATE);
    (0..frames * 2)
        .map(|i| {
            let t = (i / 2) as f64 / rate;
            let side = if i % 2 == 0 { 1.0 } else { 0.8 };
            let chord = [220.0, 277.18, 329.63]
                .iter()
                .map(|hz| (core::f64::consts::TAU * hz * t).sin())
                .sum::<f64>();
            (chord * side * 5_000.0) as i16
        })
        .collect()
}

fn encode(c: &mut Criterion) {
    let samples = minute();
    let mut group = c.benchmark_group("sound encode");
    group.sample_size(10);
    group.throughput(Throughput::Elements(samples.len() as u64 / 2));
    group.bench_function("one minute of stereo", |b| {
        b.iter(|| enshwick_ui::__internal::encode_qoa(black_box(&samples), 2));
    });
    group.finish();
}

criterion_group!(benches, encode);
criterion_main!(benches);
