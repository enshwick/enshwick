//! What every interaction detour pays before it hands a call to the game:
//! one atomic load while nothing is held and no mod filters, against the
//! lookup it does while props are held.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick_loader::prop_holds::PropHolds;

fn quiet(c: &mut Criterion) {
    let idle = PropHolds::new();
    idle.linked(0, true);
    let busy = PropHolds::new();
    busy.linked(0, true);
    for entity in 0..8 {
        let _held = busy.hold(entity, false);
    }
    let mut group = c.benchmark_group("prop_holds");
    group.bench_function("quiet_nothing_held", |b| {
        b.iter(|| black_box(&idle).quiet());
    });
    group.bench_function("is_held_eight_held", |b| {
        b.iter(|| black_box(&busy).is_held(black_box(5)));
    });
    group.finish();
    assert!(!busy.quiet(), "the busy set holds props");
}

criterion_group!(benches, quiet);
criterion_main!(benches);
