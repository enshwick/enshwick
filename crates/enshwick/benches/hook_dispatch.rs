//! What a game call costs through one linked hook against the call alone:
//! the detour's entry, its panic guard and the chain to the original.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick::abi::Status;
use enshwick::test::{FakeHost, address};
use enshwick::{Load, Mod, Next, Off};

type Reward = extern "system" fn(u32) -> u32;

extern "system" fn vanilla(base: u32) -> u32 {
    base.wrapping_add(1)
}

enshwick::detour! {
    static PASS: extern "system" fn(base: u32) -> u32 = pass;
}

fn pass(next: Next<Reward>, base: u32) -> u32 {
    next.get()(base)
}

struct Bench;

impl Mod for Bench {
    fn manifest() -> &'static enshwick::abi::Manifest {
        enshwick::manifest!(id: "enshwick.bench", side: SERVER)
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let target = load.symbol("bench.reward")?;
        // SAFETY: the target is `vanilla`, of the type `PASS` declares.
        unsafe { load.hook(&PASS, target) }?;
        Ok(Self)
    }
}

enshwick::declare_mod!(Bench);

#[expect(clippy::panic, reason = "without its hook the bench measures nothing")]
fn dispatch(c: &mut Criterion) {
    let loader = FakeHost::new()
        .symbol("bench.reward", address(vanilla as Reward))
        .install();
    // SAFETY: both functions are the exports of this file's `declare_mod!(Bench)`.
    let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
    assert_eq!(status, Status::OK, "{:?}", loader.logs());
    let Some(hooked) = loader.detour(&PASS) else {
        panic!("the hook is not linked: {:?}", loader.logs());
    };
    let mut group = c.benchmark_group("hook_dispatch");
    group.bench_function("vanilla", |b| b.iter(|| vanilla(black_box(7))));
    group.bench_function("one_link", |b| b.iter(|| hooked(black_box(7))));
    group.finish();
}

criterion_group!(benches, dispatch);
criterion_main!(benches);
