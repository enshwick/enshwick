//! What the item widgets pay per frame: the slot widget's detour per slot,
//! for a mod's slot and for a hovered slot of a screen, and a tooltip site's
//! lookup of the lines kept for its item.

#[cfg(windows)]
use criterion::{Criterion, criterion_group, criterion_main};

#[cfg(windows)]
fn item_ui(c: &mut Criterion) {
    let mut group = c.benchmark_group("item_ui");
    let mut mods = enshwick_loader::bench::bench_item_slot(false);
    group.bench_function("slot_of_a_mod", |b| b.iter(&mut mods));
    let mut screen = enshwick_loader::bench::bench_item_slot(true);
    group.bench_function("hovered_slot_of_a_screen", |b| b.iter(&mut screen));
    let mut lines = enshwick_loader::bench::bench_tooltip_lines();
    group.bench_function("kept_tooltip_lines", |b| b.iter(&mut lines));
    group.finish();
}

#[cfg(windows)]
criterion_group!(benches, item_ui);
#[cfg(windows)]
criterion_main!(benches);

#[cfg(not(windows))]
fn main() {}
