//! What each mod costs: the time of its own hook links, and of every callback
//! the loader makes into it, by feature.
//!
//! With `diagnostics` on, the loader times every callback into a mod, the mods
//! it calls in turn left out; with `diagnostics_timing` also, the timed thunk
//! in front of each link gives the link's own time as its time less the rest of
//! the chain's. `enshwick profile` on the console and the status summary show
//! the busiest mods since start or since `enshwick profile reset`.

use std::cell::Cell;
use std::fmt::Write as _;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::chains::Counter;
use crate::mod_index::ModIndex;

/// What the loader calls a mod for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Feature {
    /// Loader events: tick, world loaded and saved, players, settings.
    Events,
    /// The game's own events.
    GameEvents,
    /// Other mods' events.
    ModEvents,
    /// Player actions.
    Actions,
    /// Messages between server and client.
    Messages,
    /// Interface frames, the HUD and dialog answers.
    Interface,
    /// Interaction filters.
    Interactions,
    /// Crafting and building vetoes.
    Vetoes,
    /// Damage filters.
    Damage,
    /// Skill, comfort and experience filters.
    Progress,
    /// Rename filters.
    Renames,
    /// Survivor lookups, bed checks, summons and status.
    Survivors,
    /// Altar zone checks and altar limits.
    Altars,
    /// Stat changes.
    Stats,
    /// Inventory results.
    Inventory,
    /// Status lines.
    Status,
    /// Building served resources.
    Resources,
    /// Migrations.
    Migrations,
    /// Links on other mods' extension points.
    Points,
}

/// How many features there are.
pub const FEATURES: usize = 19;

impl Feature {
    /// The feature in the report.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Events => "events",
            Self::GameEvents => "game events",
            Self::ModEvents => "mod events",
            Self::Actions => "actions",
            Self::Messages => "messages",
            Self::Interface => "interface",
            Self::Interactions => "interaction filter",
            Self::Vetoes => "vetoes",
            Self::Damage => "damage filter",
            Self::Progress => "progress filters",
            Self::Renames => "rename filter",
            Self::Survivors => "you.market-stalls",
            Self::Altars => "altars",
            Self::Stats => "stats",
            Self::Inventory => "inventory",
            Self::Status => "status lines",
            Self::Resources => "resources",
            Self::Migrations => "migrations",
            Self::Points => "extension points",
        }
    }

    const ALL: [Self; FEATURES] = [
        Self::Events,
        Self::GameEvents,
        Self::ModEvents,
        Self::Actions,
        Self::Messages,
        Self::Interface,
        Self::Interactions,
        Self::Vetoes,
        Self::Damage,
        Self::Progress,
        Self::Renames,
        Self::Survivors,
        Self::Altars,
        Self::Stats,
        Self::Inventory,
        Self::Status,
        Self::Resources,
        Self::Migrations,
        Self::Points,
    ];
}

/// One mod's calls of one feature.
#[derive(Debug, Default)]
struct Bucket {
    calls: AtomicU64,
    ticks: AtomicU64,
}

/// Every mod's buckets, `FEATURES` per mod, once timing is on.
static BUCKETS: OnceLock<Box<[Bucket]>> = OnceLock::new();

thread_local! {
    /// Ticks the callbacks inside the running one took, so each callback
    /// counts its own time alone.
    static INSIDE: Cell<u64> = const { Cell::new(0) };
    /// The callback this thread runs, for crash blame: the mod's index plus
    /// one, shifted past the feature; 0 for none.
    static RUNNING: Cell<u32> = const { Cell::new(0) };
}

/// Turns callback timing on for `mods` mods; at start, with diagnostics on.
pub fn start(mods: usize) {
    let _first = BUCKETS.set((0..mods * FEATURES).map(|_| Bucket::default()).collect());
}

/// Runs mod `owner`'s callback `f` for `feature`, marking it for crash
/// blame, and timing it when diagnostics are on.
#[inline]
pub fn call<R>(owner: ModIndex, feature: Feature, f: impl FnOnce() -> R) -> R {
    let marker = u32::try_from(owner.get())
        .ok()
        .and_then(|o| o.checked_add(1))
        .and_then(|o| o.checked_mul(256))
        .map_or(0, |o| o | feature as u32);
    let outer = RUNNING.replace(marker);
    let result = match BUCKETS
        .get()
        .and_then(|b| b.get(owner.get() * FEATURES + feature as usize))
    {
        None => f(),
        Some(bucket) => timed(bucket, f),
    };
    RUNNING.set(outer);
    result
}

/// The mod callback this thread runs, if any: the mod's index and the
/// feature. Reads a thread-local only, so a crash filter may call it.
#[must_use]
pub fn current_callback() -> Option<(usize, Feature)> {
    decode(marker())
}

/// The mod callback this thread runs, encoded, 0 for none: for a handler
/// that keeps it for later, as [`decode`] reads it.
#[must_use]
pub fn marker() -> u32 {
    RUNNING.get()
}

/// The mod's index and the feature of a [`marker`].
#[must_use]
pub fn decode(marker: u32) -> Option<(usize, Feature)> {
    let owner = usize::try_from((marker >> 8).checked_sub(1)?).ok()?;
    let feature = *Feature::ALL.get((marker & 0xff) as usize)?;
    Some((owner, feature))
}

#[inline(never)]
fn timed<R>(bucket: &Bucket, f: impl FnOnce() -> R) -> R {
    let outer = INSIDE.replace(0);
    let start = rdtsc();
    let result = f();
    let elapsed = rdtsc().wrapping_sub(start);
    let inside = INSIDE.replace(outer.wrapping_add(elapsed));
    bucket.calls.fetch_add(1, Ordering::Relaxed);
    bucket
        .ticks
        .fetch_add(elapsed.saturating_sub(inside), Ordering::Relaxed);
    result
}

fn rdtsc() -> u64 {
    // SAFETY: `rdtsc` has no preconditions on x86-64.
    unsafe { core::arch::x86_64::_rdtsc() }
}

/// A counter's three numbers at one moment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reading {
    /// Calls.
    pub calls: u64,
    /// Timed calls.
    pub samples: u64,
    /// Ticks of the timed calls.
    pub ticks: u64,
}

impl Reading {
    /// The counter now.
    #[must_use]
    pub fn of(counter: &Counter) -> Self {
        Self {
            calls: counter.calls.load(Ordering::Relaxed),
            samples: counter.samples.load(Ordering::Relaxed),
            ticks: counter.ticks.load(Ordering::Relaxed),
        }
    }

    /// What happened since `base`.
    #[must_use]
    pub fn since(self, base: Self) -> Self {
        Self {
            calls: self.calls.saturating_sub(base.calls),
            samples: self.samples.saturating_sub(base.samples),
            ticks: self.ticks.saturating_sub(base.ticks),
        }
    }

    /// The ticks every call took, estimated from the timed ones.
    #[must_use]
    pub fn estimate(self) -> f64 {
        Counter::estimate(self.calls, self.samples, self.ticks).unwrap_or(0.0)
    }
}

/// One link of a chain, in running order, as the profile reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct LinkReading {
    /// Its owner's index, [`ModIndex::LOADER`] for the loader.
    pub owner: ModIndex,
    /// Whether its owner still runs.
    pub live: bool,
    /// Its timed thunk's counter.
    pub reading: Reading,
}

/// One hooked function's links and original.
#[derive(Clone, Debug, PartialEq)]
pub struct ChainReading {
    /// The function's name.
    pub name: String,
    /// Its links, in running order.
    pub links: Vec<LinkReading>,
    /// The original's timed thunk.
    pub original: Reading,
}

/// Each live link's own calls and ticks: its time less the time of what it
/// calls on.
#[must_use]
pub fn own_times(chain: &ChainReading) -> Vec<(ModIndex, u64, f64)> {
    let live: Vec<&LinkReading> = chain.links.iter().filter(|l| l.live).collect();
    live.iter()
        .enumerate()
        .map(|(i, link)| {
            let rest = live
                .get(i + 1)
                .map_or_else(|| chain.original.estimate(), |next| next.reading.estimate());
            let own = (link.reading.estimate() - rest).max(0.0);
            (link.owner, link.reading.calls, own)
        })
        .collect()
}

/// Every mod's callback buckets now: calls and ticks, `FEATURES` per mod.
#[must_use]
pub fn buckets() -> Vec<(u64, u64)> {
    BUCKETS.get().map_or_else(Vec::new, |buckets| {
        buckets
            .iter()
            .map(|b| {
                (
                    b.calls.load(Ordering::Relaxed),
                    b.ticks.load(Ordering::Relaxed),
                )
            })
            .collect()
    })
}

/// One thing a mod spent time on.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// A feature, or `hook <function>`.
    pub what: String,
    /// Calls in the window.
    pub calls: u64,
    /// Ticks in the window.
    pub ticks: f64,
}

/// What one mod cost in the window.
#[derive(Clone, Debug, PartialEq)]
pub struct Cost {
    /// The mod id, or `enshwick` for the loader's own links.
    pub id: String,
    /// What it spent time on, the costliest first.
    pub items: Vec<Item>,
}

impl Cost {
    /// Every item's ticks together.
    #[must_use]
    pub fn ticks(&self) -> f64 {
        self.items.iter().map(|i| i.ticks).sum()
    }
}

/// Every mod's costs in a window, from the readings: `chains` with the readings
/// at its start subtracted already, and the callback buckets now and at its
/// start.
///
/// The costliest mod first; mods that cost nothing are left out.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "ticks become times for people to read"
)]
pub fn costs(
    chains: &[ChainReading],
    now: &[(u64, u64)],
    base: &[(u64, u64)],
    name: impl Fn(ModIndex) -> String,
) -> Vec<Cost> {
    let mut by_owner: Vec<(usize, Vec<Item>)> = Vec::new();
    let mut push = |owner: ModIndex, item: Item| {
        if item.calls == 0 && item.ticks <= 0.0 {
            return;
        }
        match by_owner.iter_mut().find(|(o, _)| *o == owner.get()) {
            Some((_, items)) => items.push(item),
            None => by_owner.push((owner.get(), vec![item])),
        }
    };
    for chain in chains {
        for (owner, calls, ticks) in own_times(chain) {
            push(
                owner,
                Item {
                    what: format!("hook {}", chain.name),
                    calls,
                    ticks,
                },
            );
        }
    }
    for (index, &(calls, ticks)) in now.iter().enumerate() {
        let (base_calls, base_ticks) = base.get(index).copied().unwrap_or_default();
        let feature = Feature::ALL[index % FEATURES];
        push(
            ModIndex::new(index / FEATURES),
            Item {
                what: feature.name().to_owned(),
                calls: calls.saturating_sub(base_calls),
                ticks: ticks.saturating_sub(base_ticks) as f64,
            },
        );
    }
    let mut costs: Vec<Cost> = by_owner
        .into_iter()
        .map(|(owner, mut items)| {
            items.sort_by(|a, b| b.ticks.total_cmp(&a.ticks));
            Cost {
                id: name(ModIndex::new(owner)),
                items,
            }
        })
        .collect();
    costs.sort_by(|a, b| b.ticks().total_cmp(&a.ticks()));
    costs
}

/// Ticks as a time, given the counter's rate.
fn time(ticks: f64, tsc_hz: f64) -> Duration {
    Duration::from_secs_f64((ticks / tsc_hz).max(0.0))
}

/// A share of the window: milliseconds of work per second.
fn per_second(ticks: f64, tsc_hz: f64, window: Duration) -> String {
    let seconds = window.as_secs_f64().max(1e-9);
    let ms = time(ticks, tsc_hz).as_secs_f64() * 1e3 / seconds;
    if ms >= 10.0 {
        format!("{ms:.0} ms/s")
    } else if ms >= 0.1 {
        format!("{ms:.2} ms/s")
    } else {
        format!("{:.1} µs/s", ms * 1e3)
    }
}

/// The costs as lines: each mod with its share and its `items` costliest
/// items.
#[must_use]
pub fn render(
    costs: &[Cost],
    tsc_hz: f64,
    window: Duration,
    mods: usize,
    items: usize,
) -> Vec<String> {
    costs
        .iter()
        .take(mods)
        .map(|cost| {
            let mut line = format!("{} {}", cost.id, per_second(cost.ticks(), tsc_hz, window));
            let parts: Vec<String> = cost
                .items
                .iter()
                .take(items)
                .map(|i| {
                    format!(
                        "{} {} in {} calls",
                        i.what,
                        per_second(i.ticks, tsc_hz, window),
                        i.calls
                    )
                })
                .collect();
            if !parts.is_empty() {
                let _w = write!(line, ": {}", parts.join("; "));
            }
            line
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(calls: u64, samples: u64, ticks: u64) -> Reading {
        Reading {
            calls,
            samples,
            ticks,
        }
    }

    #[test]
    fn a_link_costs_its_time_less_the_rest_of_the_chain() {
        let chain = ChainReading {
            name: "building.place".into(),
            links: vec![
                LinkReading {
                    owner: ModIndex::new(0),
                    live: true,
                    reading: reading(100, 10, 10_000),
                },
                LinkReading {
                    owner: ModIndex::new(1),
                    live: true,
                    reading: reading(100, 10, 6_000),
                },
            ],
            original: reading(100, 10, 5_000),
        };
        let own = own_times(&chain);
        assert_eq!(
            own,
            [
                (ModIndex::new(0), 100, 40_000.0),
                (ModIndex::new(1), 100, 10_000.0)
            ]
        );
    }

    #[test]
    fn an_off_link_is_left_out_and_a_short_cut_costs_nothing_less() {
        let chain = ChainReading {
            name: "f".into(),
            links: vec![
                LinkReading {
                    owner: ModIndex::new(0),
                    live: false,
                    reading: reading(10, 1, 100),
                },
                LinkReading {
                    owner: ModIndex::new(1),
                    live: true,
                    reading: reading(10, 1, 50),
                },
            ],
            original: reading(4, 1, 100),
        };
        assert_eq!(own_times(&chain), [(ModIndex::new(1), 10, 100.0)]);
    }

    #[test]
    fn costs_add_hooks_and_callbacks_per_mod() {
        let chain = ChainReading {
            name: "f".into(),
            links: vec![LinkReading {
                owner: ModIndex::new(1),
                live: true,
                reading: reading(10, 10, 300),
            }],
            original: reading(10, 10, 100),
        };
        let mut now = vec![(0, 0); 2 * FEATURES];
        now[Feature::Events as usize] = (5, 50);
        now[FEATURES + Feature::Interactions as usize] = (3, 30);
        let base = vec![(1, 10); 2 * FEATURES];
        let names = |i: ModIndex| ["you.trading-post", "you.market-stalls"][i.get()].to_owned();
        let costs = costs(&[chain], &now, &base, names);
        assert_eq!(costs.len(), 2);
        assert_eq!(costs[0].id, "you.market-stalls");
        assert_eq!(costs[0].items[0].what, "hook f");
        assert!((costs[0].ticks() - 220.0).abs() < 1e-9);
        assert_eq!(
            costs[1].items,
            [Item {
                what: "events".into(),
                calls: 4,
                ticks: 40.0
            }]
        );
        let lines = render(&costs, 1e9, Duration::from_secs(1), 5, 3);
        assert_eq!(
            lines[0],
            "you.market-stalls 0.2 µs/s: hook f 0.2 µs/s in 10 calls; interaction filter 0.0 µs/s in 2 calls"
        );
    }

    #[test]
    fn a_running_callback_is_marked_for_crash_blame() {
        assert_eq!(current_callback(), None);
        call(ModIndex::new(3), Feature::Actions, || {
            assert_eq!(current_callback(), Some((3, Feature::Actions)));
            call(ModIndex::new(5), Feature::ModEvents, || {
                assert_eq!(current_callback(), Some((5, Feature::ModEvents)));
            });
            assert_eq!(current_callback(), Some((3, Feature::Actions)));
        });
        assert_eq!(current_callback(), None);
        call(ModIndex::new(usize::MAX), Feature::Events, || {
            assert_eq!(current_callback(), None);
        });
    }

    #[test]
    fn callbacks_count_their_own_time_alone() {
        let bucket = Bucket::default();
        let inner = Bucket::default();
        timed(&bucket, || {
            timed(&inner, || std::hint::black_box((0..1000).sum::<u64>()));
        });
        assert_eq!(bucket.calls.load(Ordering::Relaxed), 1);
        assert_eq!(inner.calls.load(Ordering::Relaxed), 1);
    }
}
