//! The status summary: what the loader knows of every mod, the lines the
//! mods' status sources give, what mods serve and patch, and the hook
//! counters. Written to the loader's log when an administrator asks with
//! `enshwick status` on the console.

use core::ffi::c_void;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use enshwick_core::abi::{Side, Status, StatusFn, Str};

use crate::engine::diag::{HookStat, ModStatus, Summary};
use crate::engine::loader::Loader;
use crate::engine::mod_index::ModIndex;
use crate::engine::settings::LOADER_ID;
use enshwick_base::Relock;

/// How often the housekeeping thread looks.
const POLL: Duration = Duration::from_secs(1);

/// A mod's status source.
#[derive(Clone, Copy, Debug)]
struct Source {
    owner: ModIndex,
    callback: StatusFn,
    user: usize,
}

static SOURCES: Mutex<Vec<Source>> = Mutex::new(Vec::new());
static STARTED: OnceLock<Instant> = OnceLock::new();

/// Marks the loader's start, for the uptime.
pub(crate) fn mark_start() {
    let _first = STARTED.set(Instant::now());
}

/// Adds mod `owner`'s status source.
pub(crate) fn register(owner: ModIndex, callback: StatusFn, user: *mut c_void) -> Status {
    SOURCES.relock().push(Source {
        owner,
        callback,
        user: user.expose_provenance(),
    });
    Status::OK
}

/// The lines mod `owner`'s sources give now.
fn lines_of(sources: &[Source], owner: ModIndex) -> Vec<String> {
    sources
        .iter()
        .filter(|s| s.owner == owner)
        .flat_map(|source| {
            let mut out = Str::EMPTY;
            // SAFETY: the source lives in the mod's DLL, which stays loaded;
            // the safe layer catches its panics.
            crate::engine::profile::call(
                source.owner,
                crate::engine::profile::Feature::Status,
                || unsafe {
                    (source.callback)(
                        core::ptr::with_exposed_provenance_mut(source.user),
                        &raw mut out,
                    );
                },
            );
            // SAFETY: the text stays valid until the source runs again.
            let text = unsafe { out.as_str() }.unwrap_or("");
            text.lines()
                .filter(|l| !l.trim().is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The name of a hooked function: as a mod or the loader found it, else its
/// place in the game.
fn hook_name(loader: &Loader, target: u64) -> String {
    crate::engine::diag::name_of(target)
        .or_else(|| {
            loader
                .build
                .symbols()
                .iter()
                .find(|&&(_, rva)| loader.base + rva == target)
                .map(|(name, _)| name.clone())
        })
        .unwrap_or_else(|| format!("game+{:#x}", target.wrapping_sub(loader.base)))
}

/// Takes the summary now.
pub(crate) fn summary(loader: &Loader) -> Summary {
    let sources = SOURCES.relock().clone();
    let mods = loader
        .mods
        .iter()
        .enumerate()
        .map(|(index, record)| {
            let running = !record.off.load(Ordering::Acquire);
            ModStatus {
                id: record.id.clone(),
                name: record.name(),
                version: record.version.clone(),
                credits: record.meta.credits(),
                features: {
                    let features = record.features.relock().clone();
                    crate::engine::features::shown(&features, record.texts.relock().as_ref())
                },
                running,
                waiting: running && record.waiting.load(Ordering::Acquire),
                lines: if running {
                    lines_of(&sources, ModIndex::new(index))
                } else {
                    Vec::new()
                },
            }
        })
        .collect();
    let owner_name = |owner: ModIndex| {
        if owner == ModIndex::LOADER {
            LOADER_ID.to_owned()
        } else {
            loader
                .mods
                .get(owner.get())
                .map_or_else(|| "?".to_owned(), |m| m.id.clone())
        }
    };
    let (targets, hooks) = {
        let chains = loader.chains.relock();
        {
            let targets = chains.iter().filter(|c| c.head.is_some()).count();
            let hooks = crate::engine::diag::counting().then(|| {
                crate::engine::patch::counters()
                    .iter()
                    .map(|&(target, counter)| HookStat {
                        name: hook_name(loader, target),
                        owners: chains.owners(target).into_iter().map(owner_name).collect(),
                        calls: counter.calls.load(Ordering::Relaxed),
                        samples: counter.samples.load(Ordering::Relaxed),
                        ticks: counter.ticks.load(Ordering::Relaxed),
                    })
                    .collect()
            });
            (targets, hooks)
        }
    };
    Summary {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        game: loader.build.name.clone(),
        hosting: loader.side == Side::SERVER || crate::engine::world::hosting(),
        uptime: STARTED.get().map_or(Duration::ZERO, Instant::elapsed),
        last_crash: crate::engine::crash::last().map(str::to_owned),
        at: crate::engine::diag::now(),
        mods,
        others: crate::engine::others::lines(&loader.others),
        turned_off: crate::engine::diag::history(),
        parts_off: crate::engine::diag::parts_off(),
        links: {
            let mut links = crate::base::services::lines(loader);
            links.extend(crate::base::points::lines(loader));
            links.extend(crate::base::calls::lines(loader));
            links
        },
        served: crate::content::served::summary(),
        patches: crate::engine::diag::patches(),
        targets,
        hooks,
        timed: crate::engine::diag::timing(),
        tsc_hz: crate::engine::patch::tsc_hz(),
        costs: cost_lines(loader, 10, 4),
        conflicts: conflicts(loader),
        knowledge_guard: crate::knowledge::store::guard::status(),
        messages: crate::base::net::status(),
        settings: loader.settings_problems.clone(),
        parts: loader.build.parts(loader.parts.table_ids()),
    }
}

/// Starts the housekeeping thread: it puts the crash filter back on top and
/// keeps the measured clock rate for the profile.
pub(crate) fn start() {
    let started = std::thread::Builder::new()
        .name("enshwick-status".into())
        .spawn(|| {
            loop {
                std::thread::sleep(POLL);
                crate::engine::crash::reassert();
                if crate::engine::diag::counting()
                    && !crate::engine::profile::tsc_hz_kept()
                    && let Some(hz) = crate::engine::patch::tsc_hz()
                {
                    crate::engine::profile::keep_tsc_hz(hz);
                }
            }
        });
    if let Err(e) = started {
        crate::log!(WARN, "no housekeeping thread: {e}");
    }
}

/// The readings a profile window starts from: when, every timed thunk's
/// counter by address, and the callback buckets.
#[derive(Debug, Default)]
struct Baseline {
    at: Option<Instant>,
    hops: rustc_hash::FxHashMap<usize, crate::engine::profile::Reading>,
    buckets: Vec<(u64, u64)>,
}

static BASELINE: Mutex<Baseline> = Mutex::new(Baseline {
    at: None,
    hops: rustc_hash::FxHashMap::with_hasher(rustc_hash::FxBuildHasher),
    buckets: Vec::new(),
});

/// Every chain with timed thunks, as the profile reads it, since `base`.
fn chain_readings(
    loader: &Loader,
    chains: &crate::engine::chains::Chains,
    base: &Baseline,
) -> Vec<crate::engine::profile::ChainReading> {
    use crate::engine::profile::{ChainReading, LinkReading, Reading};
    let read = |hop: &crate::engine::chains::Hop| {
        let key = core::ptr::from_ref(hop.counter).addr();
        let now = Reading::of(hop.counter);
        base.hops.get(&key).map_or(now, |&b| now.since(b))
    };
    chains
        .iter()
        .filter(|c| c.head.is_some())
        .filter_map(|chain| {
            let original = read(chain.original_hop.as_ref()?);
            let links = chain
                .running_order()
                .into_iter()
                .filter_map(|link| {
                    Some(LinkReading {
                        owner: link.owner,
                        live: !crate::engine::loader::is_off(link.owner),
                        reading: read(link.hop.as_ref()?),
                    })
                })
                .collect();
            Some(ChainReading {
                name: hook_name(loader, chain.target),
                links,
                original,
            })
        })
        .collect()
}

/// Starts a new profile window now.
pub(crate) fn reset_profile(loader: &Loader) {
    let mut hops = rustc_hash::FxHashMap::default();
    {
        let chains = loader.chains.relock();
        for chain in chains.iter() {
            let all = chain
                .links
                .iter()
                .filter_map(|l| l.hop)
                .chain(chain.original_hop);
            for hop in all {
                hops.insert(
                    core::ptr::from_ref(hop.counter).addr(),
                    crate::engine::profile::Reading::of(hop.counter),
                );
            }
        }
    }
    *BASELINE.relock() = Baseline {
        at: Some(Instant::now()),
        hops,
        buckets: crate::engine::profile::buckets(),
    };
}

/// The costs of every mod in the profile window, its length, and whether it
/// started at a reset; `None` without diagnostics.
pub(crate) fn costs(
    loader: &Loader,
) -> Option<(Vec<crate::engine::profile::Cost>, Duration, bool)> {
    if !crate::engine::diag::counting() {
        return None;
    }
    let base = BASELINE.relock();
    let chains = {
        let chains = loader.chains.relock();
        chain_readings(loader, &chains, &base)
    };
    let name = |owner: ModIndex| {
        if owner == ModIndex::LOADER {
            LOADER_ID.to_owned()
        } else {
            loader
                .mods
                .get(owner.get())
                .map_or_else(|| "?".to_owned(), |m| m.id.clone())
        }
    };
    let costs = crate::engine::profile::costs(
        &chains,
        &crate::engine::profile::buckets(),
        &base.buckets,
        name,
    );
    let since = base.at.or_else(|| STARTED.get().copied())?;
    Some((costs, since.elapsed(), base.at.is_some()))
}

/// The cost lines for the status summary and the console, with a header.
pub(crate) fn cost_lines(loader: &Loader, mods: usize, items: usize) -> Vec<String> {
    let Some((costs, window, reset)) = costs(loader) else {
        return vec![crate::engine::texts::fill("costs.off", &[])];
    };
    let Some(hz) = crate::engine::patch::tsc_hz() else {
        return vec![crate::engine::texts::fill("costs.measuring", &[])];
    };
    let head = if reset {
        "costs.since-reset"
    } else {
        "costs.since-start"
    };
    let mut lines = vec![crate::engine::texts::fill(
        head,
        &[("window", &crate::engine::diag::duration(window))],
    )];
    let rendered = crate::engine::profile::render(&costs, hz, window, mods, items);
    if rendered.is_empty() {
        lines.push(crate::engine::texts::fill("costs.none", &[]));
    }
    lines.extend(rendered);
    lines
}

/// The conflict report: functions, features and resources several mods
/// share, or one holds against another.
pub(crate) fn conflicts(loader: &Loader) -> Vec<String> {
    use crate::engine::conflicts::{HookLink, Hooked};
    let name = |owner: ModIndex| {
        if owner == ModIndex::LOADER {
            LOADER_ID.to_owned()
        } else {
            loader
                .mods
                .get(owner.get())
                .map_or_else(|| "?".to_owned(), |m| m.id.clone())
        }
    };
    let mut hooks: Vec<Hooked> = {
        let chains = loader.chains.relock();
        {
            let mut hooks: Vec<Hooked> = chains
                .iter()
                .map(|chain| Hooked {
                    name: hook_name(loader, chain.target),
                    links: chain
                        .running_order()
                        .into_iter()
                        .map(|l| HookLink {
                            owner: name(l.owner),
                            priority: l.priority,
                            exclusive: l.flags.contains(enshwick_core::abi::HookFlags::EXCLUSIVE),
                            caller_frame: l
                                .flags
                                .contains(enshwick_core::abi::HookFlags::CALLER_FRAME),
                        })
                        .collect(),
                    refused: chains
                        .refused()
                        .iter()
                        .filter(|r| r.target == chain.target)
                        .map(|r| (name(r.owner), r.holder.map(name)))
                        .collect(),
                })
                .collect();
            hooks.sort_by(|a, b| a.name.cmp(&b.name));
            hooks
        }
    };
    {
        let mids = loader.mids.relock();
        for address in mids.addresses() {
            let owners = mids.owners(address);
            if owners.len() >= 2 {
                hooks.push(Hooked {
                    name: format!("inside {}", hook_name(loader, address)),
                    links: owners
                        .into_iter()
                        .map(|o| HookLink {
                            owner: name(o),
                            priority: 0,
                            exclusive: false,
                            caller_frame: false,
                        })
                        .collect(),
                    refused: Vec::new(),
                });
            }
        }
    }
    let resources: Vec<(String, Vec<String>)> = crate::content::served::shared()
        .into_iter()
        .map(|(key, owners)| (key, owners.into_iter().map(name).collect()))
        .collect();
    let mut lines = crate::engine::conflicts::report(
        &hooks,
        &crate::engine::conflicts::claims(),
        &resources,
        name,
    );
    lines.extend(crate::engine::conflicts::fields());
    lines.extend(crate::engine::conflicts::write_report(
        &crate::engine::conflicts::shared_writes(),
        |hash| component_name(loader, hash),
        name,
    ));
    let (text_rva, changes) = crate::engine::conflicts::code_changes();
    lines.extend(crate::engine::code_watch::report(&changes, |offset| {
        hook_name(loader, loader.base + (text_rva + offset) as u64)
    }));
    lines.extend(crate::base::points::conflicts(loader));
    lines
}

/// A component type's reflected name, or its hash where reflection has no
/// type of that hash.
fn component_name(loader: &Loader, hash: u32) -> String {
    loader
        .reflection
        .type_names()
        .find(|name| {
            loader
                .reflection
                .type_info(name)
                .ok()
                .and_then(|info| crate::engine::world::type_hash(info.va()))
                == Some(hash)
        })
        .map_or_else(|| format!("{hash:#010x}"), str::to_owned)
}
