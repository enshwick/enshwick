//! What the post-step pays to reach its parts: 26 parts with no work, each
//! called directly under its own fuse, against the same parts called
//! through a list of fused function pointers, as the layers register them.

use std::hint::black_box;
use std::time::Instant;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick_loader::engine::fuse::Fuse;

/// The parts the step calls, as many as the loader registers.
const PARTS: usize = 26;

/// What each part is given.
struct Stepping {
    now: Instant,
    step: u64,
}

/// A part's work: nothing but reading what it is given.
fn work(stepping: &Stepping) {
    black_box((stepping.now, stepping.step));
}

/// A part's step as a layer lists it.
struct Step {
    fuse: &'static Fuse,
    run: fn(&Stepping),
}

static FUSES: [Fuse; PARTS] = [const { Fuse::new("bench part") }; PARTS];

static LISTED: [Step; PARTS] = {
    let mut steps = [const {
        Step {
            fuse: &FUSES[0],
            run: work,
        }
    }; PARTS];
    let mut i = 0;
    while i < PARTS {
        steps[i].fuse = &FUSES[i];
        i += 1;
    }
    steps
};

/// The parts called one by one, each under its own fuse.
fn direct(stepping: &Stepping) {
    macro_rules! parts {
        ($($i:literal)*) => {
            $(let _ran = FUSES[$i].run("the post-step link", || work(stepping));)*
        };
    }
    parts!(0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25);
}

/// The parts called through the list, as the step walks a stage.
fn listed(steps: &[Step], stepping: &Stepping) {
    for step in steps {
        let _ran = step.fuse.run("the post-step link", || (step.run)(stepping));
    }
}

fn step_parts(c: &mut Criterion) {
    let mut group = c.benchmark_group("step_parts");
    let stepping = Stepping {
        now: Instant::now(),
        step: 1,
    };
    group.bench_function("direct", |b| b.iter(|| direct(black_box(&stepping))));
    group.bench_function("listed", |b| {
        b.iter(|| listed(black_box(&LISTED), black_box(&stepping)));
    });
    group.finish();
}

criterion_group!(benches, step_parts);
criterion_main!(benches);
