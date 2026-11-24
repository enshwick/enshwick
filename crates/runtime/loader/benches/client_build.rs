//! What the input builder's detour pays per pass: with nothing handed, its
//! atomic loads; while a move holds the stick; and while a move and two
//! presses hold.

#[cfg(windows)]
use criterion::{Criterion, criterion_group, criterion_main};

#[cfg(windows)]
fn client_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("client_build");
    let mut idle = enshwick_loader::bench::bench_client_build(false, false);
    group.bench_function("nothing_handed", |b| b.iter(&mut idle));
    let mut moving = enshwick_loader::bench::bench_client_build(true, false);
    group.bench_function("move_held", |b| b.iter(&mut moving));
    let mut pressing = enshwick_loader::bench::bench_client_build(true, true);
    group.bench_function("move_and_two_presses_held", |b| b.iter(&mut pressing));
    group.finish();
}

#[cfg(windows)]
criterion_group!(benches, client_build);
#[cfg(windows)]
criterion_main!(benches);

#[cfg(not(windows))]
fn main() {}
