//! What the build filters' account copy costs: a cursor check's lookup of
//! the last character in a full copy, and a server step's write of one.

#[cfg(windows)]
use criterion::{Criterion, criterion_group, criterion_main};

#[cfg(windows)]
fn accounts(c: &mut Criterion) {
    let mut group = c.benchmark_group("accounts");
    let (mut lookup, mut write) = enshwick_loader::bench_accounts();
    group.bench_function("lookup_last_of_full", |b| b.iter(&mut lookup));
    group.bench_function("write_full", |b| b.iter(&mut write));
    group.finish();
}

#[cfg(windows)]
criterion_group!(benches, accounts);
#[cfg(windows)]
criterion_main!(benches);

#[cfg(not(windows))]
fn main() {}
