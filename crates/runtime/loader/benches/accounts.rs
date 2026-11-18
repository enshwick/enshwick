//! What the build filters' accounts cost: a cursor check's lookup of the
//! last character in a full copy, a server step's write of one, and the
//! own prediction of a client that joined a server finding it has no
//! account.

#[cfg(windows)]
use criterion::{Criterion, criterion_group, criterion_main};

#[cfg(windows)]
fn accounts(c: &mut Criterion) {
    let mut group = c.benchmark_group("accounts");
    let (mut lookup, mut write) = enshwick_loader::bench::bench_accounts();
    group.bench_function("lookup_last_of_full", |b| b.iter(&mut lookup));
    group.bench_function("write_full", |b| b.iter(&mut write));
    let mut own = enshwick_loader::bench::bench_own_account();
    group.bench_function("own_in_prediction", |b| b.iter(&mut own));
    group.finish();
}

#[cfg(windows)]
criterion_group!(benches, accounts);
#[cfg(windows)]
criterion_main!(benches);

#[cfg(not(windows))]
fn main() {}
