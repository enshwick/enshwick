//! What a log line's check costs before anything is formatted: the level
//! check and a once-per-life site, in a mod's own copy of the API and for a
//! data pack in the loader's copy.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick::__private::{Once, as_pack, enabled, install_host};
use enshwick::abi::{Level, ModHandle};

static SITE: Once = Once::new();

fn checks(c: &mut Criterion) {
    let loader = enshwick::test::FakeHost::new()
        .pack_log_level(ModHandle(2), Level::INFO)
        .install();
    // SAFETY: the fake host's table lives for the process; the loader makes
    // its own handle the host of the copy that runs the packs.
    unsafe { install_host(loader.api(), ModHandle(9)) };
    let mut group = c.benchmark_group("log_check");
    group.bench_function("enabled_mod", |b| {
        b.iter(|| enabled(black_box(Level::DEBUG)));
    });
    group.bench_function("once_site_mod", |b| b.iter(|| SITE.first()));
    // SAFETY: the fake host's table lives for the process.
    unsafe {
        as_pack(loader.api(), ModHandle(2), || {
            group.bench_function("enabled_pack", |b| {
                b.iter(|| enabled(black_box(Level::DEBUG)));
            });
            group.bench_function("once_site_pack", |b| b.iter(|| SITE.first()));
        });
    }
    group.finish();
}

criterion_group!(benches, checks);
criterion_main!(benches);
