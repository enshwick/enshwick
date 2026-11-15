//! What the input builder's detour pays per pass: with nothing handed, its
//! one atomic load, and while a move holds the stick.

#[cfg(windows)]
use criterion::{Criterion, criterion_group, criterion_main};

#[cfg(windows)]
fn client_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("client_build");
    let mut idle = enshwick_loader::bench_client_build(false);
    group.bench_function("nothing_handed", |b| b.iter(&mut idle));
    let mut moving = enshwick_loader::bench_client_build(true);
    group.bench_function("move_held", |b| b.iter(&mut moving));
    group.finish();
}

#[cfg(windows)]
criterion_group!(benches, client_build);
#[cfg(windows)]
criterion_main!(benches);

#[cfg(not(windows))]
fn main() {}
