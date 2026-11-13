//! What a call costs between Luau and the API, in time and in heap
//! allocations, the VM's own included: one hit through a damage filter that
//! reads and scales it, and a call into the API with each kind of argument.

use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::Write as _;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick::abi::{DamageCheck, Header, Status};
use enshwick::test::{FakeHost, Installed};
use enshwick::{Load, Mod, Off};
use enshwick_luau::{Limits, Scripts};

/// Counts every allocation, so the bench reports them per call.
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

/// The calls into the API each hit of a kind makes, so the hit's own cost
/// spreads thin; the `loop` kind makes none and is subtracted.
const CALLS: u32 = 100;

/// Each kind of call: its bench name and the call [`script`] makes; the
/// `loop` kind makes none.
const KINDS: [(&str, &str); 7] = [
    ("loop", ""),
    ("scalar", "scale_whole(100, 1.5)"),
    ("string", r#"is_id("you.bench:a_content_id")"#),
    ("list", "combine(factors)"),
    ("record", "start_fill(meter)"),
    ("bytes", "looks_like_code(bytes)"),
    ("function", "store:with(read)"),
];

/// A damage filter that makes [`CALLS`] of the call of the kind a hit's
/// total names, by its place in [`KINDS`] from 1, and scales any other
/// fall by half.
fn script() -> String {
    let kinds = KINDS.iter().fold(String::new(), |mut kinds, (_, call)| {
        let _ = writeln!(kinds, "function() for _ = 1, {CALLS} do {call} end end,");
        kinds
    });
    MAIN.replace("--[[kinds]]", &kinds)
}

/// [`script`] without its kinds.
const MAIN: &str = r#"
    local scale_whole = enshwick.rates.scale_whole
    local is_id = enshwick.content.is_id
    local combine = enshwick.rates.combine
    local start_fill = enshwick.content.start_fill
    local looks_like_code = enshwick.net.looks_like_code
    local store = enshwick.world_store("bench")
    local factors = { 1.5, 2, 0.5, 1.25 }
    local meter = { template = "you.bench:pouch", max_fuel_charges = 4, start = 2 }
    local bytes = buffer.create(64)
    local read = function(value) return value end
    local kinds = {
        --[[kinds]]
    }
    enshwick.damage.on_damage(function(damage)
        local kind = kinds[damage:total()]
        if kind then
            kind()
        elseif damage:cause() == "FallDamage" then
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
            Scripts::new().with("main.luau", script()),
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
const HITS: u32 = 10_000;

/// A hit of `total`: a fall of 40 for the plain filter, else the kind it
/// names.
fn hit(total: f32) -> DamageCheck {
    DamageCheck {
        header: Header::of::<DamageCheck>(),
        cause: DamageCheck::FALL_DAMAGE,
        target: enshwick::Entity(2),
        source: enshwick::Entity(0),
        attacker: enshwick::Entity(0),
        amounts: [0.0; enshwick::abi::DAMAGE_TYPES],
        total,
    }
}

/// The allocations `hits` hits of `total` make, after one warm hit.
fn allocations(loader: &Installed, total: f32, hits: u32) -> u64 {
    black_box(loader.damage(hit(total)));
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    for _ in 0..hits {
        black_box(loader.damage(black_box(hit(total))));
    }
    ALLOCATIONS.load(Ordering::Relaxed) - before
}

#[expect(
    clippy::print_stdout,
    reason = "the bench reports its counts beside criterion's times"
)]
fn calls(c: &mut Criterion) {
    let loader = FakeHost::new().install();
    // SAFETY: both functions are the exports of this file's `declare_mod!(Bench)`.
    let status = unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
    assert_eq!(status, Status::OK, "{:?}", loader.logs());
    let warm = loader.damage(hit(40.0));
    assert!((warm.total - 20.0).abs() < 1e-6, "{:?}", loader.logs());

    let made = allocations(&loader, 40.0, HITS);
    println!("luau_damage_filter: {made} allocations in {HITS} hits");
    c.bench_function("luau_damage_filter", |b| {
        b.iter(|| loader.damage(black_box(hit(40.0))));
    });

    let mut group = c.benchmark_group(format!("luau_call_x{CALLS}"));
    for (at, (kind, _)) in (1u8..).zip(KINDS) {
        let total = f32::from(at);
        let made = allocations(&loader, total, HITS / CALLS);
        let after = loader.damage(hit(40.0));
        assert!(
            (after.total - 20.0).abs() < 1e-6,
            "{kind}: {:?}",
            loader.logs()
        );
        println!("luau_call {kind}: {made} allocations in {HITS} calls");
        group.bench_function(kind, |b| {
            b.iter(|| loader.damage(black_box(hit(total))));
        });
    }
    group.finish();
}

criterion_group!(benches, calls);
criterion_main!(benches);
