//! What the content detours pay per call: the text lookup of a game tag and
//! of a mod's tag, the content request with nothing served, and the recipe
//! filter with nothing disabled and with a cached list.

#[cfg(windows)]
use std::hint::black_box;

#[cfg(windows)]
use criterion::{Criterion, criterion_group, criterion_main};

#[cfg(windows)]
fn content_detours(c: &mut Criterion) {
    let mut group = c.benchmark_group("content_detours");
    let mut lookup = enshwick_loader::bench_text_lookup();
    group.bench_function("text_lookup_game_tag", |b| {
        b.iter(|| lookup(black_box(1)));
    });
    group.bench_function("text_lookup_mod_tag", |b| {
        b.iter(|| lookup(black_box(7)));
    });
    let mut request = enshwick_loader::bench_texture_request();
    group.bench_function("texture_request_nothing_served", |b| {
        b.iter(&mut request);
    });
    let mut idle = enshwick_loader::content::recipes::bench_recipe_filter(&[]);
    group.bench_function("recipe_filter_nothing_disabled", |b| {
        b.iter(&mut idle);
    });
    let mut cached = enshwick_loader::content::recipes::bench_recipe_filter(&[5]);
    group.bench_function("recipe_filter_one_disabled_cached", |b| {
        b.iter(&mut cached);
    });
    group.finish();
}

#[cfg(windows)]
criterion_group!(benches, content_detours);
#[cfg(windows)]
criterion_main!(benches);

#[cfg(not(windows))]
fn main() {}
