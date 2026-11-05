//! What one hit costs through a Luau damage filter that reads and scales
//! it, in time and in heap allocations, the VM's own included.

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick::abi::{DamageCheck, Header, Status};
use enshwick::test::FakeHost;
use enshwick::{Load, Mod, Off};
use enshwick_luau::{Limits, Scripts};

/// Counts every allocation, so the bench reports them per hit.
struct Counting;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);

// SAFETY: every call forwards to `System` with the caller's own arguments.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: the caller's guarantees for `alloc` are `System`'s.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from `System` through this allocator.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: `ptr` came from `System` through this allocator, and the
        // caller's guarantees for `realloc` are `System`'s.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

const MAIN: &str = r#"
    enshwick.damage.on_damage(function(damage)
        if damage:cause() == "FallDamage" then
            damage:scale(0.5)
        end
    end)
"#;

struct Bench;

impl Mod for Bench {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        // The bench runs far more hits a second than a game's budget allows.
        let limits = Limits::default()
            .with_call(Duration::from_secs(1))
            .with_per_second(Duration::from_secs(3_600));
        let _runtime = enshwick_luau::start(
            load,
            "enshwick.bench",
            Scripts::new().with("main.luau", MAIN),
            None,
            limits,
        )?;
        Ok(Self)
    }
}

enshwick::test::declare_mod!(
    Bench,
    r#"
    id = "enshwick.bench"
    side = "both"
    "#
);

/// The hits the allocation count averages over.
const HITS: u64 = 10_000;

fn fall() -> DamageCheck {
    DamageCheck {
        header: Header::of::<DamageCheck>(),
        cause: DamageCheck::FALL_DAMAGE,
        target: enshwick::Entity(2),
        source: enshwick::Entity(0),
        attacker: enshwick::Entity(0),
        amounts: [0.0; enshwick::abi::DAMAGE_TYPES],
        total: 40.0,
    }
}

fn filter(c: &mut Criterion) {
    let loader = FakeHost::new().install();
    // SAFETY: both functions are the exports of this file's `declare_mod!(Bench)`.
    let status = unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
    assert_eq!(status, Status::OK, "{:?}", loader.logs());
    let warm = loader.damage(fall());
    assert!((warm.total - 20.0).abs() < 1e-6, "{:?}", loader.logs());

    let before = ALLOCATIONS.load(Ordering::Relaxed);
    for _ in 0..HITS {
        black_box(loader.damage(black_box(fall())));
    }
    let made = ALLOCATIONS.load(Ordering::Relaxed) - before;
    #[expect(
        clippy::print_stdout,
        reason = "the bench reports its count beside criterion's times"
    )]
    {
        println!("luau_damage_filter: {made} allocations in {HITS} hits");
    }

    c.bench_function("luau_damage_filter", |b| {
        b.iter(|| loader.damage(black_box(fall())));
    });
}

criterion_group!(benches, filter);
criterion_main!(benches);
