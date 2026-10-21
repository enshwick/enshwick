//! The status summary: what the loader knows of every mod, the lines the
//! mods' status sources give, what mods serve and patch, and the hook
//! counters. Written to `logs/enshwick/status.log` a minute after start,
//! then every `status_minutes`, and whenever an administrator asks with
//! `/enshwick status`; each write replaces the last.

use core::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use enshwick_loader_api::abi::{Side, Status, StatusFn, Str};

use crate::diag::{HookStat, ModStatus, Summary};
use crate::host::Loader;
use crate::settings::LOADER_ID;

/// How often the writer looks at the clock and for requests.
const POLL: Duration = Duration::from_secs(1);
/// The first summary after start, so a fresh server shows one soon.
const FIRST: Duration = Duration::from_secs(60);

/// A mod's status source.
#[derive(Clone, Copy, Debug)]
struct Source {
    owner: usize,
    callback: StatusFn,
    user: usize,
}

static SOURCES: Mutex<Vec<Source>> = Mutex::new(Vec::new());
/// Set by `/enshwick status`; the writer writes on its next look.
static REQUESTED: AtomicBool = AtomicBool::new(false);
static STARTED: OnceLock<Instant> = OnceLock::new();

/// Marks the loader's start, for the uptime.
pub(crate) fn mark_start() {
    let _first = STARTED.set(Instant::now());
}

/// Adds mod `owner`'s status source.
pub(crate) fn register(owner: usize, callback: StatusFn, user: *mut c_void) -> Status {
    SOURCES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(Source {
            owner,
            callback,
            user: user.expose_provenance(),
        });
    Status::OK
}

/// Asks the writer to write the summary now.
pub(crate) fn request() {
    REQUESTED.store(true, Ordering::Release);
}

/// The lines mod `owner`'s sources give now.
fn lines_of(sources: &[Source], owner: usize) -> Vec<String> {
    sources
        .iter()
        .filter(|s| s.owner == owner)
        .flat_map(|source| {
            let mut out = Str::EMPTY;
            // SAFETY: the source lives in the mod's DLL, which stays loaded;
            // the safe layer catches its panics.
            crate::profile::call(source.owner, crate::profile::Feature::Status, || unsafe {
                (source.callback)(
                    core::ptr::with_exposed_provenance_mut(source.user),
                    &raw mut out,
                );
            });
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
    crate::diag::name_of(target)
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
    let sources = SOURCES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let mods = loader
        .mods
        .iter()
        .enumerate()
        .map(|(index, record)| {
            let running = !record.off.load(Ordering::Acquire);
            ModStatus {
                id: record.id.clone(),
                name: record.meta.name.clone(),
                version: record.version.clone(),
                credits: record.meta.credits(),
                notes: record.notes.clone(),
                running,
                lines: if running {
                    lines_of(&sources, index)
                } else {
                    Vec::new()
                },
            }
        })
        .collect();
    let owner_name = |owner: usize| {
        if owner == crate::chains::LOADER {
            LOADER_ID.to_owned()
        } else {
            loader
                .mods
                .get(owner)
                .map_or_else(|| "?".to_owned(), |m| m.id.clone())
        }
    };
    let (targets, hooks) = match loader.chains.lock() {
        Ok(chains) => {
            let targets = chains.iter().filter(|c| c.head.is_some()).count();
            let hooks = crate::diag::counting().then(|| {
                crate::patch::counters()
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
        Err(_) => (0, None),
    };
    Summary {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        game: loader.build.name.clone(),
        hosting: loader.side == Side::SERVER || crate::world::hosting(),
        uptime: STARTED.get().map_or(Duration::ZERO, Instant::elapsed),
        last_crash: crate::crash::last().map(str::to_owned),
        at: crate::diag::now(),
        mods,
        disabled: crate::modlist::disabled().to_vec(),
        replaced: crate::modlist::replaced()
            .iter()
            .map(crate::modlist::Replaced::line)
            .collect(),
        turned_off: crate::diag::history(),
        links: crate::services::lines(loader),
        served: crate::content::summary(),
        patches: crate::diag::patches(),
        targets,
        hooks,
        timed: crate::diag::timing(),
        tsc_hz: crate::patch::tsc_hz(),
        costs: cost_lines(loader, 10, 4),
        conflicts: conflicts(loader),
        knowledge_guard: crate::knowledge::guard::status(),
    }
}

/// Writes the summary to `status.log`.
fn write(loader: &Loader) {
    let Some(game_dir) = loader.mods_dir.parent() else {
        return;
    };
    let path = game_dir.join("logs").join(LOADER_ID).join("status.log");
    if let Err(e) = std::fs::write(&path, summary(loader).render()) {
        crate::log!(WARN, "status summary not written: {e}");
    }
}

/// Starts the writer on a thread of its own.
pub(crate) fn start(loader: &'static Loader) {
    let started = std::thread::Builder::new()
        .name("enshwick-status".into())
        .spawn(move || {
            let mut next = Instant::now() + FIRST;
            loop {
                std::thread::sleep(POLL);
                crate::crash::reassert();
                let minutes = crate::diag::status_minutes();
                let due = minutes > 0 && Instant::now() >= next;
                if REQUESTED.swap(false, Ordering::AcqRel) || due {
                    write(loader);
                    next = Instant::now() + Duration::from_secs(u64::from(minutes.max(1)) * 60);
                }
            }
        });
    if let Err(e) = started {
        crate::log!(WARN, "no status summaries: {e}");
    }
}

/// The readings a profile window starts from: when, every timed thunk's
/// counter by address, and the callback buckets.
#[derive(Debug, Default)]
struct Baseline {
    at: Option<Instant>,
    hops: rustc_hash::FxHashMap<usize, crate::profile::Reading>,
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
    chains: &crate::chains::Chains,
    base: &Baseline,
) -> Vec<crate::profile::ChainReading> {
    use crate::profile::{ChainReading, LinkReading, Reading};
    let read = |hop: &crate::chains::Hop| {
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
                        live: !crate::host::is_off(link.owner),
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
    if let Ok(chains) = loader.chains.lock() {
        for chain in chains.iter() {
            let all = chain
                .links
                .iter()
                .filter_map(|l| l.hop)
                .chain(chain.original_hop);
            for hop in all {
                hops.insert(
                    core::ptr::from_ref(hop.counter).addr(),
                    crate::profile::Reading::of(hop.counter),
                );
            }
        }
    }
    *BASELINE.lock().unwrap_or_else(PoisonError::into_inner) = Baseline {
        at: Some(Instant::now()),
        hops,
        buckets: crate::profile::buckets(),
    };
}

/// The costs of every mod in the profile window, its length, and whether it
/// started at a reset; `None` without diagnostics.
pub(crate) fn costs(loader: &Loader) -> Option<(Vec<crate::profile::Cost>, Duration, bool)> {
    if !crate::diag::counting() {
        return None;
    }
    let base = BASELINE.lock().unwrap_or_else(PoisonError::into_inner);
    let chains = match loader.chains.lock() {
        Ok(chains) => chain_readings(loader, &chains, &base),
        Err(_) => Vec::new(),
    };
    let name = |owner: usize| {
        if owner == crate::chains::LOADER {
            LOADER_ID.to_owned()
        } else {
            loader
                .mods
                .get(owner)
                .map_or_else(|| "?".to_owned(), |m| m.id.clone())
        }
    };
    let costs = crate::profile::costs(&chains, &crate::profile::buckets(), &base.buckets, name);
    let since = base.at.or_else(|| STARTED.get().copied())?;
    Some((costs, since.elapsed(), base.at.is_some()))
}

/// The cost lines for the status summary and the chat, with a header.
pub(crate) fn cost_lines(loader: &Loader, mods: usize, items: usize) -> Vec<String> {
    let Some((costs, window, reset)) = costs(loader) else {
        return vec![
            "per-mod costs need diagnostics = true in mod-configs/enshwick.toml (hook links also diagnostics_timing = true); read at start"
                .to_owned(),
        ];
    };
    let Some(hz) = crate::patch::tsc_hz() else {
        return vec!["per-mod costs: measuring, ask again in a second".to_owned()];
    };
    let since = if reset {
        "since /enshwick profile reset"
    } else {
        "since start"
    };
    let mut lines = vec![format!(
        "per-mod costs over {} ({since}), work per second of time:",
        crate::diag::duration(window)
    )];
    let rendered = crate::profile::render(&costs, hz, window, mods, items);
    if rendered.is_empty() {
        lines.push("no mod has cost anything yet".to_owned());
    }
    lines.extend(rendered);
    lines
}

/// The conflict report: functions, features and resources several mods
/// share, or one holds against another.
pub(crate) fn conflicts(loader: &Loader) -> Vec<String> {
    use crate::conflicts::{HookLink, Hooked};
    let name = |owner: usize| {
        if owner == crate::chains::LOADER {
            LOADER_ID.to_owned()
        } else {
            loader
                .mods
                .get(owner)
                .map_or_else(|| "?".to_owned(), |m| m.id.clone())
        }
    };
    let mut hooks: Vec<Hooked> = match loader.chains.lock() {
        Ok(chains) => {
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
                            exclusive: l
                                .flags
                                .contains(enshwick_loader_api::abi::HookFlags::EXCLUSIVE),
                            caller_frame: l
                                .flags
                                .contains(enshwick_loader_api::abi::HookFlags::CALLER_FRAME),
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
        Err(_) => Vec::new(),
    };
    if let Ok(mids) = loader.mids.lock() {
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
    let resources: Vec<(String, Vec<String>)> = crate::content::shared()
        .into_iter()
        .map(|(key, owners)| (key, owners.into_iter().map(name).collect()))
        .collect();
    crate::conflicts::report(&hooks, &crate::conflicts::claims(), &resources, name)
}
