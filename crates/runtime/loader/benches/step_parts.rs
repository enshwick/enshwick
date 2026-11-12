//! What the post-step pays to reach its parts: 26 parts whose work does
//! nothing but may panic, each turned off alone by a panic. `direct` calls
//! them one by one, each under its own guard; `guarded` calls them through
//! references into each layer's list, each under its own guard; `walked`
//! is the loader's walk, through a stage's steps held side by side under
//! one guard ([`Fuse::run_each`]).

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

/// A part's work: out of line and able to panic, as the loader's parts
/// are, so each guard stays around its call.
#[inline(never)]
fn work(stepping: &Stepping) {
    assert!(black_box(stepping.step) != 0, "never");
    black_box((stepping.now, stepping.step));
}

/// A part's step as a layer lists it.
struct Step {
    fuse: &'static Fuse,
    run: fn(&Stepping),
}

static FUSES: [Fuse; PARTS] = [const { Fuse::new("bench part") }; PARTS];

static STEPS: [Step; PARTS] = {
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

fn direct(stepping: &Stepping) {
    macro_rules! parts {
        ($($i:literal)*) => {
            $(let _ran = FUSES[$i].run("the post-step link", || work(stepping));)*
        };
    }
    parts!(0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25);
}

fn guarded(steps: &[&Step], stepping: &Stepping) {
    for step in steps {
        let _ran = step.fuse.run("the post-step link", || (step.run)(stepping));
    }
}

fn walked(steps: &[Step], stepping: &Stepping) {
    Fuse::run_each(
        steps,
        "the post-step link",
        |step| step.fuse,
        |step| (step.run)(stepping),
    );
}

fn step_parts(c: &mut Criterion) {
    let mut group = c.benchmark_group("step_parts");
    let stepping = Stepping {
        now: Instant::now(),
        step: 1,
    };
    group.bench_function("direct", |b| b.iter(|| direct(black_box(&stepping))));
    let references: Vec<&Step> = STEPS.iter().collect();
    group.bench_function("guarded", |b| {
        b.iter(|| guarded(black_box(&references), black_box(&stepping)));
    });
    group.bench_function("walked", |b| {
        b.iter(|| walked(black_box(&STEPS), black_box(&stepping)));
    });
    group.finish();
}

criterion_group!(benches, step_parts);
criterion_main!(benches);
