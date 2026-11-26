//! What the content detours pay per call: the text lookup of a game tag and
//! of a mod's tag, the content request with nothing served, its reads with
//! content served beside the same lookups in a map swapped whole, the game's
//! build of a served key, and the recipe filter with nothing disabled and
//! with a cached list.

#[cfg(windows)]
use std::hint::black_box;

#[cfg(windows)]
use criterion::{Criterion, criterion_group, criterion_main};

#[cfg(windows)]
fn content_detours(c: &mut Criterion) {
    let mut group = c.benchmark_group("content_detours");
    let mut lookup = enshwick_loader::bench::bench_text_lookup();
    group.bench_function("text_lookup_game_tag", |b| {
        b.iter(|| lookup(black_box(1)));
    });
    group.bench_function("text_lookup_mod_tag", |b| {
        b.iter(|| lookup(black_box(7)));
    });
    let mut request = enshwick_loader::bench::bench_texture_request();
    group.bench_function("texture_request_nothing_served", |b| {
        b.iter(&mut request);
    });
    let mut reads = enshwick_loader::bench::bench_request_reads(SERVED, SERVED);
    assert_eq!(reads(), 0, "no request is served");
    group.bench_function("texture_request_reads_64_of_64_served", |b| {
        b.iter(&mut reads);
    });
    let mut by_map = lookups_by_swapped_map();
    group.bench_function("texture_request_lookups_64_of_64_served_by_map", |b| {
        b.iter(&mut by_map);
    });
    for (of_a_mod, name) in [
        (false, "served_game_build_three_patches"),
        (true, "served_game_build_three_mod_patches"),
    ] {
        let mut build = enshwick_loader::bench::bench_served_build(of_a_mod);
        assert_eq!(build(), 64 + 3, "{name}: every patch runs");
        group.bench_function(name, |b| {
            b.iter(&mut build);
        });
    }
    let mut idle = enshwick_loader::bench::bench_recipe_filter(&[]);
    group.bench_function("recipe_filter_nothing_disabled", |b| {
        b.iter(&mut idle);
    });
    let mut cached = enshwick_loader::bench::bench_recipe_filter(&[5]);
    group.bench_function("recipe_filter_one_disabled_cached", |b| {
        b.iter(&mut cached);
    });
    group.finish();
}

/// Contents served and requests read in the request benchmarks.
#[cfg(windows)]
const SERVED: u32 = 64;

/// The same lookups as [`enshwick_loader::bench::bench_request_reads`] in a
/// map swapped whole and loaded once per request, the baseline its table is
/// measured against; how many it found.
#[cfg(windows)]
fn lookups_by_swapped_map() -> impl FnMut() -> usize {
    let hash = |n: u32, of: u32| [4, of, n, 0x4b57_4845];
    let map: rustc_hash::FxHashMap<[u32; 4], &'static [u8]> =
        (0..SERVED).map(|n| (hash(n, n), &b"x"[..])).collect();
    let content = arc_swap::ArcSwapOption::from_pointee(map);
    let requests: Vec<[u32; 4]> = (0..SERVED).map(|n| hash(0, u32::MAX - n)).collect();
    move || {
        let content = content.load();
        content.as_ref().map_or(0, |map| {
            requests
                .iter()
                .filter(|hash| map.contains_key(black_box(*hash)))
                .count()
        })
    }
}

#[cfg(windows)]
criterion_group!(benches, content_detours);
#[cfg(windows)]
criterion_main!(benches);

#[cfg(not(windows))]
fn main() {}
