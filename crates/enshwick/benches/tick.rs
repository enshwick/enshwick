//! What a mod pays on a server step: the tick event delivered to its
//! every-step handlers, its scheduler looking for due tasks while none is
//! due, and a handler on the default period on a step it skips and on one
//! it runs. The fake host's own lookup of the handlers is in every number.

use std::hint::black_box;
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick::abi::{Status, TickData};
use enshwick::events::Ticked;
use enshwick::test::FakeHost;
use enshwick::{Load, Mod, Off};

/// Handlers a mod with a few per-step rules subscribes.
const HANDLERS: usize = 4;

static TICKS: AtomicU64 = AtomicU64::new(0);
/// Whether the mod subscribes one handler on the default period instead of
/// its every-step handlers and scheduler.
static PERIODIC: AtomicBool = AtomicBool::new(false);

struct Bench;

impl Mod for Bench {
    fn manifest() -> &'static enshwick::abi::Manifest {
        enshwick::manifest!(id: "enshwick.bench", side: SERVER)
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let count = |tick: &TickData| {
            TICKS.fetch_add(tick.step, Ordering::Relaxed);
        };
        if PERIODIC.load(Ordering::Relaxed) {
            load.on::<Ticked>(count)?;
            return Ok(Self);
        }
        for _ in 0..HANDLERS {
            load.on_every_step(count)?;
        }
        load.scheduler()?
            .every(Duration::from_secs(3600), || ControlFlow::Continue(()));
        Ok(Self)
    }
}

enshwick::declare_mod!(Bench);

fn tick(c: &mut Criterion) {
    let mut group = c.benchmark_group("tick");
    {
        let loader = FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Bench)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let data = TickData::new(1);
        group.bench_function("four_handlers_and_an_idle_scheduler", |b| {
            b.iter(|| loader.raise::<Ticked>(black_box(&data)));
        });
    }
    PERIODIC.store(true, Ordering::Relaxed);
    let loader = FakeHost::new().install();
    // SAFETY: both functions are the exports of this file's `declare_mod!(Bench)`.
    let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
    assert_eq!(status, Status::OK, "{:?}", loader.logs());
    assert_eq!(loader.tick(Duration::ZERO), 1, "the first step runs it");
    group.bench_function("default_period_skipped_step", |b| {
        b.iter(|| loader.tick(black_box(Duration::ZERO)));
    });
    group.bench_function("default_period_due_step", |b| {
        b.iter(|| loader.tick(black_box(Ticked::DEFAULT_EVERY)));
    });
    group.finish();
}

criterion_group!(benches, tick);
criterion_main!(benches);
