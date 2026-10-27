//! What the post-step pays for periodic tick handlers: nothing listed, one
//! handler on a step it skips, the same without the step time average, one
//! handler due, and a clock read for scale.

use core::ffi::c_void;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick::abi::Event;
use enshwick_loader::mod_index::ModIndex;
use enshwick_loader::ticks::{StepTime, Ticks};

/// A step's length on an empty dedicated server, about 20 steps a second.
const STEP: u64 = 50_000_000;
const HOUR: u64 = 3_600_000_000_000;

unsafe extern "system" fn handler(_: Event, data: *const c_void, _: *mut c_void) {
    black_box(data);
}

/// One handler on a period of `every`, past its first call.
fn one(every: u64) -> Ticks {
    let ticks = Ticks::new();
    let _added = ticks.add(ModIndex::new(0), 0, every, handler, core::ptr::null_mut());
    let time = ticks.advance(STEP);
    let _first = ticks.run(1, STEP, time, |_| true, |_, _| {});
    ticks
}

/// One step after step `step`: the work the post-step does for ticks.
fn step(ticks: &Ticks, step: &mut u64) -> usize {
    *step += 1;
    let now = *step * STEP;
    let time = ticks.advance(now);
    ticks.run(
        *step,
        now,
        time,
        |_| true,
        |tick, data| {
            // SAFETY: `handler` reads nothing through its pointers.
            unsafe { (tick.callback())(Event::TICKED, (&raw const data).cast(), tick.user()) };
        },
    )
}

fn ticks(c: &mut Criterion) {
    let mut group = c.benchmark_group("ticks");
    let empty = Ticks::new();
    group.bench_function("step_nothing_listed", |b| {
        b.iter(|| black_box(&empty).listed());
    });
    let skipped = one(HOUR);
    let mut n = 1;
    group.bench_function("step_one_skipped", |b| {
        b.iter(|| step(black_box(&skipped), &mut n));
    });
    let previous = AtomicU64::new(0);
    let mut n = 1;
    group.bench_function("step_one_skipped_without_average", |b| {
        b.iter(|| {
            n += 1;
            let now = n * STEP;
            let elapsed = now.saturating_sub(previous.swap(now, Ordering::Relaxed));
            let time = StepTime {
                elapsed,
                average: 0,
            };
            black_box(&skipped).run(n, now, time, |_| true, |_, _| {})
        });
    });
    let due = one(0);
    let mut n = 1;
    group.bench_function("step_one_due", |b| {
        b.iter(|| step(black_box(&due), &mut n));
    });
    group.bench_function("instant_now", |b| b.iter(Instant::now));
    group.finish();
}

criterion_group!(benches, ticks);
criterion_main!(benches);
