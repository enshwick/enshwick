//! A world past 16 altar zones cannot open safely with the vanilla grouping,
//! so the dedicated server holds the game on it while it has one.
//!
//! The hold is set as soon as a grouping counts more than 16 zones, and goes
//! only once the loaded world has stayed at 16 or fewer for a while: a
//! grouping that runs before the world loads, or before every altar of it is
//! listed, counts too few and must not release a world that needs the hold.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use enshwick_loader_api::altars::VANILLA_ALTAR_ZONES;
use enshwick_loader_api::events::{AltarsChanged, ShutdownStarted, WorldLoaded};
use enshwick_loader_api::state::Hold;
use enshwick_loader_api::{Load, Off};

const EVERY: Duration = Duration::from_secs(5);
/// How long the loaded world stays at 16 zones or fewer before the hold goes.
const RELEASE_AFTER: Duration = Duration::from_secs(60);

/// Keeps the hold on a thread of its own, woken when the altars change.
pub(crate) fn keep(load: &mut Load<'_>) -> Result<(), Off> {
    let hold = Hold::new(&load.data_dir()?);
    // Following the altars is what raises `AltarsChanged`.
    let _altars = load.altars()?;
    let loaded = Arc::new(AtomicBool::new(false));
    let running = Arc::clone(&loaded);
    let handle = std::thread::Builder::new()
        .name("flame-altar-count-hold".into())
        .spawn(move || run(&hold, &running))
        .map_err(|e| Off::new(format!("hold thread: {e}")))?;
    let thread = handle.thread().clone();
    let opened = Arc::clone(&loaded);
    load.on::<WorldLoaded>(move |()| opened.store(true, Ordering::Release))?;
    load.on::<ShutdownStarted>(move |()| loaded.store(false, Ordering::Release))?;
    load.on::<AltarsChanged>(move |()| thread.unpark())
}

/// The reason a held world must not open with the vanilla grouping.
pub(crate) fn reason(load: &Load<'_>) -> Result<Option<String>, Off> {
    Ok(Hold::new(&load.data_dir()?).reason())
}

/// What the hold thread does after a look at the grouping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Set,
    Release,
    Wait,
}

/// Whether the hold is set, and since when the loaded world has had 16
/// zones or fewer.
#[derive(Debug)]
struct Watch {
    set: bool,
    below_since: Option<Instant>,
}

impl Watch {
    const fn new(set: bool) -> Self {
        Self {
            set,
            below_since: None,
        }
    }

    /// The step for a grouping past 16 zones or not, `past`, at `now`, with
    /// the world `loaded` or not yet.
    fn step(&mut self, past: bool, loaded: bool, now: Instant) -> Step {
        if past {
            self.below_since = None;
            return if self.set { Step::Wait } else { Step::Set };
        }
        if !self.set {
            return Step::Wait;
        }
        if !loaded {
            self.below_since = None;
            return Step::Wait;
        }
        let since = *self.below_since.get_or_insert(now);
        if now.duration_since(since) >= RELEASE_AFTER {
            Step::Release
        } else {
            Step::Wait
        }
    }

    /// Notes that the hold was set or released.
    fn done(&mut self, set: bool) {
        self.set = set;
        self.below_since = None;
    }
}

fn run(hold: &Hold, loaded: &AtomicBool) {
    let mut watch = Watch::new(hold.reason().is_some());
    loop {
        std::thread::park_timeout(EVERY);
        let Some(grouping) = enshwick_loader_api::altars::grouping() else {
            continue;
        };
        let zones = grouping.zones;
        let past = zones > u32::from(VANILLA_ALTAR_ZONES);
        let done = match watch.step(past, loaded.load(Ordering::Acquire), Instant::now()) {
            Step::Wait => continue,
            Step::Set => hold.set(&format!(
                "the world has {zones} altar zones; the vanilla game groups only {VANILLA_ALTAR_ZONES}"
            )),
            Step::Release => hold.clear(),
        };
        match done {
            Ok(()) => {
                watch.done(past);
                enshwick_loader_api::info!(
                    "hold {}: {zones} altar zones",
                    if past { "set" } else { "released" }
                );
            }
            Err(error) => {
                enshwick_loader_api::warn_every!(Duration::from_secs(600), "hold: {error}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hold_is_set_at_once_and_released_only_after_a_quiet_minute_in_the_world() {
        let t0 = Instant::now();
        let mut watch = Watch::new(false);
        assert_eq!(watch.step(false, false, t0), Step::Wait);
        assert_eq!(
            watch.step(true, false, t0),
            Step::Set,
            "set before the load"
        );
        watch.done(true);
        // A grouping before the world loaded counts too few.
        assert_eq!(watch.step(false, false, t0 + RELEASE_AFTER * 2), Step::Wait);
        // Loaded, but the altars arrive one batch at a time.
        let loaded = t0 + RELEASE_AFTER * 3;
        assert_eq!(watch.step(false, true, loaded), Step::Wait);
        assert_eq!(
            watch.step(true, true, loaded + EVERY),
            Step::Wait,
            "still set"
        );
        assert_eq!(
            watch.step(false, true, loaded + EVERY * 2),
            Step::Wait,
            "the count starts over"
        );
        assert_eq!(
            watch.step(false, true, loaded + EVERY * 2 + RELEASE_AFTER),
            Step::Release,
            "altars removed down to 16"
        );
        watch.done(false);
        assert_eq!(
            watch.step(false, true, loaded + RELEASE_AFTER * 5),
            Step::Wait
        );
    }

    #[test]
    fn a_hold_from_the_last_run_stays_until_the_world_shows_it_is_not_needed() {
        let t0 = Instant::now();
        let mut watch = Watch::new(true);
        assert_eq!(watch.step(false, false, t0), Step::Wait);
        assert_eq!(watch.step(false, true, t0), Step::Wait);
        assert_eq!(watch.step(false, true, t0 + RELEASE_AFTER), Step::Release);
    }
}
