//! What the loader adds to a hooked game call and to every callback into a
//! mod: the head slot and each link's `next` slot, the call counters the
//! thunks bump from many threads, the feature fuse, callback timing and a
//! mod's log line.

use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use enshwick::abi::Level;
use enshwick_loader::chains::{Counter, Next};
use enshwick_loader::fuse::Fuse;

type Reward = extern "system" fn(u32) -> u32;

extern "system" fn original(base: u32) -> u32 {
    base.wrapping_add(1)
}

/// The slot a hooked target jumps through: the first link, or the original.
static HEAD: AtomicUsize = AtomicUsize::new(0);

macro_rules! links {
    ($($next:ident $link:ident),+) => {$(
        // SAFETY: `Reward` is the signature of `original`, the hooked
        // function, and only `$link` reads the slot.
        static $next: Next<Reward> = unsafe { Next::new() };

        extern "system" fn $link(base: u32) -> u32 {
            $next.get()(black_box(base))
        }
    )+};
}

links!(NEXT_A link_a, NEXT_B link_b, NEXT_C link_c, NEXT_D link_d);

const LINKS: [(Reward, &Next<Reward>); 4] = [
    (link_a, &NEXT_A),
    (link_b, &NEXT_B),
    (link_c, &NEXT_C),
    (link_d, &NEXT_D),
];

/// Points the head through the first `count` links to the original, as
/// `Chain::wire` does.
fn wire(count: usize) {
    let mut rest = Next::<Reward>::address_of(original);
    for &(link, next) in LINKS[..count].iter().rev() {
        next.slot().store(rest, Ordering::Release);
        rest = Next::<Reward>::address_of(link);
    }
    HEAD.store(rest, Ordering::Release);
}

fn game_call(base: u32) -> u32 {
    let head = HEAD.load(Ordering::Acquire);
    // SAFETY: `wire` stores only addresses of `Reward` functions in `HEAD`.
    let function = unsafe { core::mem::transmute::<usize, Reward>(head) };
    function(base)
}

fn chain(c: &mut Criterion) {
    let mut group = c.benchmark_group("chain");
    group.bench_function("original", |b| b.iter(|| original(black_box(7))));
    for links in [0, 1, 4] {
        wire(links);
        group.bench_with_input(BenchmarkId::new("links", links), &links, |b, _| {
            b.iter(|| game_call(black_box(7)));
        });
    }
    group.finish();
}

/// Threads bumping counters side by side, as hooked calls on as many
/// threads bump the counters of neighbouring thunks.
const THREADS: usize = 4;

/// Runs `work` on `THREADS` threads at once, `iters` times each, and gives
/// the time per round.
fn on_threads(iters: u64, work: impl Fn(usize) + Sync) -> Duration {
    let start = Instant::now();
    thread::scope(|scope| {
        for t in 0..THREADS {
            let work = &work;
            scope.spawn(move || {
                for _ in 0..iters {
                    work(t);
                }
            });
        }
    });
    start.elapsed()
}

fn counters(c: &mut Criterion) {
    let counters: Vec<Counter> = (0..THREADS).map(|_| Counter::default()).collect();
    let mut group = c.benchmark_group("counters");
    group.bench_function("one_thread", |b| {
        b.iter(|| counters[0].calls.fetch_add(1, Ordering::Relaxed));
    });
    group.bench_function("neighbours_on_four_threads", |b| {
        b.iter_custom(|iters| {
            on_threads(iters, |t| {
                counters[t].calls.fetch_add(1, Ordering::Relaxed);
            })
        });
    });
    group.finish();
}

static FUSE: Fuse = Fuse::new("bench feature");

fn fuse(c: &mut Criterion) {
    let mut group = c.benchmark_group("fuse");
    group.bench_function("run", |b| {
        b.iter(|| FUSE.run("bench", || original(black_box(7))));
    });
    group.finish();
}

#[cfg(target_arch = "x86_64")]
fn profile(c: &mut Criterion) {
    use enshwick_loader::mod_index::ModIndex;
    use enshwick_loader::profile::{self, Feature};

    let mut group = c.benchmark_group("profile_call");
    group.bench_function("untimed", |b| {
        b.iter(|| profile::call(ModIndex::new(0), Feature::Events, || original(black_box(7))));
    });
    profile::start(THREADS);
    group.bench_function("timed", |b| {
        b.iter(|| profile::call(ModIndex::new(0), Feature::Events, || original(black_box(7))));
    });
    group.bench_function("timed_neighbours_on_four_threads", |b| {
        b.iter_custom(|iters| {
            on_threads(iters, |t| {
                let feature = [
                    Feature::Events,
                    Feature::GameEvents,
                    Feature::ModEvents,
                    Feature::Actions,
                ][t];
                profile::call(ModIndex::new(0), feature, || original(black_box(7)));
            })
        });
    });
    group.finish();
}

#[cfg(not(target_arch = "x86_64"))]
fn profile(_: &mut Criterion) {}

fn log_line(c: &mut Criterion) {
    let dir = std::env::temp_dir().join(format!("enshwick-bench-log-{}", std::process::id()));
    enshwick_loader::log::open(&dir);
    let mut group = c.benchmark_group("log_line");
    group.bench_function("mod_line", |b| {
        b.iter(|| {
            enshwick_loader::log::write(
                Level::INFO,
                black_box("enshwick.bench"),
                black_box("the altar at 1024, 2048 grew to level 3"),
            );
        });
    });
    group.finish();
    let _removed = std::fs::remove_dir_all(dir);
}

criterion_group!(benches, chain, counters, fuse, profile, log_line);
criterion_main!(benches);
