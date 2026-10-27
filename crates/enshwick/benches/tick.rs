//! What a mod pays on every server step: the tick event delivered to its
//! handlers, and its scheduler looking for due tasks while none is due. The
//! fake host's own lookup of the handlers is in every number.

use std::hint::black_box;
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick::abi::{Header, Status, TickData};
use enshwick::events::Ticked;
use enshwick::test::FakeHost;
use enshwick::{Load, Mod, Off};

/// Handlers a mod with a few per-step rules subscribes.
const HANDLERS: usize = 4;

static TICKS: AtomicU64 = AtomicU64::new(0);

struct Bench;

impl Mod for Bench {
    fn manifest() -> &'static enshwick::abi::Manifest {
        enshwick::manifest!(id: "enshwick.bench", side: SERVER)
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        for _ in 0..HANDLERS {
            load.on::<Ticked>(|tick| {
                TICKS.fetch_add(tick.step, Ordering::Relaxed);
            })?;
        }
        load.scheduler()?
            .every(Duration::from_secs(3600), || ControlFlow::Continue(()));
        Ok(Self)
    }
}

enshwick::declare_mod!(Bench);

fn tick(c: &mut Criterion) {
    let loader = FakeHost::new().install();
    // SAFETY: both functions are the exports of this file's `declare_mod!(Bench)`.
    let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
    assert_eq!(status, Status::OK, "{:?}", loader.logs());
    let data = TickData {
        header: Header::of::<TickData>(),
        step: 1,
    };
    let mut group = c.benchmark_group("tick");
    group.bench_function("four_handlers_and_an_idle_scheduler", |b| {
        b.iter(|| loader.raise::<Ticked>(black_box(&data)));
    });
    group.finish();
}

criterion_group!(benches, tick);
criterion_main!(benches);
