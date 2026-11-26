//! What a call costs between Luau and the API, in time and in heap
//! allocations, the VM's own included: one hit through a damage filter that
//! reads and scales it, a call into the API with each kind of argument, and
//! one hit through the same filter written in Rust and in Luau.

use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::Write as _;
use std::hint::black_box;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick::abi::{DamageCheck, Status};
use enshwick::damage::DamageCause;
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

/// The Luau filters of the `handler` group, each a bench name and the
/// function: the rule the Rust filter also runs, where a fall hurts half as
/// much, then the parts of its cost one at a time: the call with an empty
/// body, a method looked up, and a method called.
const LUAU: [(&str, &str); 4] = [
    (
        "luau",
        r#"function(damage)
            if damage:cause() == "FallDamage" then
                damage:scale(0.5)
            end
        end"#,
    ),
    ("luau_empty", "function(damage) end"),
    ("luau_lookup", "function(damage) local _ = damage.cause end"),
    ("luau_method", "function(damage) damage:cause() end"),
];

/// What the bench mod's init subscribes: [`script`]'s filter at
/// [`CALLS_MODE`], the Rust filter at [`RUST_MODE`], else the filter of
/// [`LUAU`] at this place past [`RUST_MODE`]. A `static`, since
/// [`Mod::init`] takes nothing a bench could pass it in.
static MODE: AtomicU8 = AtomicU8::new(CALLS_MODE);
const CALLS_MODE: u8 = 0;
const RUST_MODE: u8 = 1;

struct Bench;

impl Mod for Bench {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let main = match MODE.load(Ordering::Relaxed) {
            CALLS_MODE => script(),
            RUST_MODE => {
                load.on_damage(|damage| {
                    if damage.cause() == DamageCause::FallDamage {
                        damage.scale(0.5);
                    }
                })?;
                return Ok(Self);
            }
            luau => {
                let (_, filter) = LUAU[usize::from(luau - RUST_MODE - 1)];
                format!("enshwick.damage.on_damage({filter})")
            }
        };
        // The bench runs far more hits a second than a game's budget allows.
        let limits = Limits::default()
            .with_call(Duration::from_secs(1))
            .with_per_second(Duration::from_secs(3_600));
        let _runtime = enshwick_luau::start(
            load,
            "enshwick.bench",
            Scripts::new().with("main.luau", main),
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
    enshwick::abi::headed!(DamageCheck {
        cause: DamageCheck::FALL_DAMAGE,
        target: enshwick::Entity::from_engine(2),
        source: enshwick::Entity::from_engine(0),
        attacker: enshwick::Entity::from_engine(0),
        total,
        amount_count: 0,
        amounts: core::ptr::null_mut(),
    })
}

/// The allocations `hits` hits of `total` make, after one warm hit.
fn allocations(loader: &Installed, total: f32, hits: u32) -> u64 {
    black_box(loader.damage(hit(total), &mut []));
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    for _ in 0..hits {
        black_box(loader.damage(black_box(hit(total)), &mut []));
    }
    ALLOCATIONS.load(Ordering::Relaxed) - before
}

/// A fake loader running the bench mod in `mode`.
fn started(mode: u8) -> Installed {
    MODE.store(mode, Ordering::Relaxed);
    let loader = FakeHost::new().install();
    // SAFETY: all three functions are the exports of this file's `declare_mod!(Bench)`.
    let status = unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
    assert_eq!(status, Status::OK, "{:?}", loader.logs());
    loader
}

#[expect(
    clippy::print_stdout,
    reason = "the bench reports its counts beside criterion's times"
)]
fn handler(c: &mut Criterion) {
    let mut group = c.benchmark_group("handler");
    let luau = (RUST_MODE + 1..)
        .zip(LUAU)
        .map(|(mode, (name, _))| (name, mode));
    for (name, mode) in [("rust", RUST_MODE)].into_iter().chain(luau) {
        let loader = started(mode);
        let fall = loader.damage(hit(40.0), &mut []);
        let scales = matches!(name, "rust" | "luau");
        let expected = if scales { 20.0 } else { 40.0 };
        assert!(
            (fall.total - expected).abs() < 1e-6,
            "{name}: {:?}",
            loader.logs()
        );
        let made = allocations(&loader, 40.0, HITS);
        println!("handler {name}: {made} allocations in {HITS} hits");
        group.bench_function(name, |b| {
            b.iter(|| loader.damage(black_box(hit(40.0)), &mut []));
        });
    }
    group.finish();
}

#[expect(
    clippy::print_stdout,
    reason = "the bench reports its counts beside criterion's times"
)]
fn calls(c: &mut Criterion) {
    let loader = started(CALLS_MODE);
    let warm = loader.damage(hit(40.0), &mut []);
    assert!((warm.total - 20.0).abs() < 1e-6, "{:?}", loader.logs());

    let made = allocations(&loader, 40.0, HITS);
    println!("luau_damage_filter: {made} allocations in {HITS} hits");
    c.bench_function("luau_damage_filter", |b| {
        b.iter(|| loader.damage(black_box(hit(40.0)), &mut []));
    });

    let mut group = c.benchmark_group(format!("luau_call_x{CALLS}"));
    for (at, (kind, _)) in (1u8..).zip(KINDS) {
        let total = f32::from(at);
        let made = allocations(&loader, total, HITS / CALLS);
        let after = loader.damage(hit(40.0), &mut []);
        assert!(
            (after.total - 20.0).abs() < 1e-6,
            "{kind}: {:?}",
            loader.logs()
        );
        println!("luau_call {kind}: {made} allocations in {HITS} calls");
        group.bench_function(kind, |b| {
            b.iter(|| loader.damage(black_box(hit(total)), &mut []));
        });
    }
    group.finish();
}

criterion_group!(benches, calls, handler);
criterion_main!(benches);
