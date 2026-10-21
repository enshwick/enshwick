//! Every character on a server keeps up with the one furthest ahead in
//! experience.
//!
//! Before the server applies gained experience, the mod moves the server's
//! high-water mark (the leader's gain once, or a character already further
//! ahead) and adds to the gains of each character below it, up to the end of
//! its current level. The game then applies it and sends it to the player.

mod share;

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use enshwick_loader_api::abi::Side;
use enshwick_loader_api::events::{ShutdownStarted, Ticked, WorldLoaded, WorldSaved};
use enshwick_loader_api::progress::ExperienceRow;
use enshwick_loader_api::state::Store;
use enshwick_loader_api::{Load, Mod, Off};

pub use share::{Character, Levels, high_water};

/// Shared experience settings.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// Every character catches up with the one furthest ahead in experience.
    #[config(runtime)]
    enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// What the dedicated server keeps between starts.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
struct Saved {
    /// The most cumulative experience any character has reached here.
    high_water: u64,
}

static LEVELS: OnceLock<Levels> = OnceLock::new();
static ENABLED: AtomicBool = AtomicBool::new(true);
static HIGH_WATER: AtomicU64 = AtomicU64::new(0);
/// The largest gain counted in this server step: the engine hands the
/// characters over in several batches a step, and one kill shared by
/// characters in two batches is still one moment.
static MOMENT: AtomicU32 = AtomicU32::new(0);

const fn character(row: &ExperienceRow) -> Character {
    Character {
        level: row.level,
        progress: row.progress,
        need: row.need,
        gained: row.gained,
    }
}

/// Moves the high-water mark and raises every character below it.
fn share_experience(rows: &mut [ExperienceRow], table: &enshwick_loader_api::progress::Levels) {
    if !ENABLED.load(Ordering::Relaxed) || rows.is_empty() {
        return;
    }
    let levels = LEVELS.get_or_init(|| {
        let levels = Levels::new(table.cap(), |level| table.need(level));
        enshwick_loader_api::info!(
            "level cap {}, {} experience to reach it",
            table.cap(),
            levels.total(Character {
                level: table.cap(),
                ..Character::default()
            })
        );
        levels
    });
    let most_gained = rows.iter().map(|r| r.gained).max().unwrap_or(0);
    let furthest = rows
        .iter()
        .map(|r| levels.total(character(r)))
        .max()
        .unwrap_or(0);
    let counted = MOMENT.fetch_max(most_gained, Ordering::Relaxed);
    let new_gain = most_gained.saturating_sub(counted);
    let moved = HIGH_WATER.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |previous| {
        let mark = high_water(previous, new_gain, furthest);
        (mark != previous).then_some(mark)
    });
    let mark = match moved {
        Ok(previous) => high_water(previous, new_gain, furthest),
        Err(unchanged) => unchanged,
    };
    for row in rows {
        row.gained += levels.catch_up(character(row), mark);
    }
}

/// Held while the mark is written: every write goes through one temporary
/// file, which two at once would leave half written.
static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Writes the current mark, one write at a time.
fn write(store: &Store<Saved>) {
    let _one = WRITING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let saved = Saved {
        high_water: HIGH_WATER.load(Ordering::Relaxed),
    };
    if let Err(error) = store.save(&saved) {
        enshwick_loader_api::warn!("experience mark not saved: {error}");
    }
}

/// Writes the mark on a thread of its own.
fn save(store: &'static Store<Saved>) {
    let spawned = std::thread::Builder::new()
        .name("shared-experience-save".into())
        .spawn(move || write(store));
    if spawned.is_err() {
        enshwick_loader_api::warn!("experience mark not saved: no thread");
    }
}

/// The mod's state.
#[derive(Debug)]
pub struct SharedExperience;

impl Mod for SharedExperience {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(
            id: "shared-experience",
            side: SERVER,
            name: "Shared experience",
            updates: github("themaiby/enshwick", "enshwick-shared-experience"),
            signed_by: ["RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF"],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        ENABLED.store(settings.enabled, Ordering::Relaxed);
        load.on_experience(share_experience)?;
        load.on::<Ticked>(|_| MOMENT.store(0, Ordering::Relaxed))?;
        if load.host().build().side == Side::SERVER {
            let store: &'static Store<Saved> =
                Box::leak(Box::new(Store::new(&load.data_dir()?, "experience")));
            HIGH_WATER.store(store.load()?.high_water, Ordering::Relaxed);
            load.on::<WorldSaved>(move |()| save(store))?;
            // The process may end soon after: write before returning.
            load.on::<ShutdownStarted>(move |()| write(store))?;
        } else {
            // A hosted game starts from the characters in it, its host among
            // them; the next game may be another world.
            load.on::<ShutdownStarted>(|()| HIGH_WATER.store(0, Ordering::Relaxed))?;
        }
        load.on::<WorldLoaded>(|()| {
            enshwick_loader_api::info!("experience mark {}", HIGH_WATER.load(Ordering::Relaxed));
        })?;
        load.on_config_change(|settings: Settings| {
            ENABLED.store(settings.enabled, Ordering::Relaxed);
            enshwick_loader_api::info!("shared experience {}", on_off(settings.enabled));
        })?;
        enshwick_loader_api::info!("shared experience {}", on_off(settings.enabled));
        Ok(Self)
    }
}

fn on_off(enabled: bool) -> &'static str {
    if enabled { "on" } else { "off" }
}

enshwick_loader_api::declare_mod!(SharedExperience);

#[cfg(test)]
mod tests {
    use enshwick_loader_api::config::Config as _;

    use super::*;

    #[test]
    fn the_default_settings_parse() {
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert!(settings.enabled);
    }

    #[test]
    fn a_server_life_raises_the_trailing_and_keeps_the_mark() {
        let dir = std::env::temp_dir().join(format!("shared-experience-{}", std::process::id()));
        let loader = enshwick_loader_api::test::FakeHost::new()
            .settings(
                "enabled = true
",
            )
            .data_dir(dir.to_str().unwrap())
            .install();
        // SAFETY: the exports of `declare_mod!`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, enshwick_loader_api::abi::Status::OK);
        let mut rows = [
            ExperienceRow {
                entity: 1,
                level: 1,
                progress: 0,
                need: 100,
                gained: 40,
            },
            ExperienceRow {
                entity: 2,
                level: 1,
                progress: 0,
                need: 100,
                gained: 0,
            },
        ];
        loader.experience(&mut rows, 10, |_| 100);
        assert_eq!(rows[0].gained, 40);
        assert_eq!(rows[1].gained, 40, "the trailing character catches up");

        assert_eq!(loader.raise::<ShutdownStarted>(&()), 1);
        let text = std::fs::read_to_string(dir.join("experience.toml")).unwrap();
        assert_eq!(toml::from_str::<Saved>(&text).unwrap().high_water, 40);
        let _cleaned = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn one_kill_shared_across_batches_counts_once() {
        use enshwick_loader_api::abi::{Header, TickData};

        let dir =
            std::env::temp_dir().join(format!("shared-experience-moment-{}", std::process::id()));
        let loader = enshwick_loader_api::test::FakeHost::new()
            .settings(
                "enabled = true
",
            )
            .data_dir(dir.to_str().unwrap())
            .install();
        // SAFETY: the exports of `declare_mod!`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, enshwick_loader_api::abi::Status::OK);
        HIGH_WATER.store(0, Ordering::Relaxed);
        let row = |entity, progress, gained| ExperienceRow {
            entity,
            level: 1,
            progress,
            need: 1000,
            gained,
        };
        let tick = |step| {
            let _ran = loader.raise::<Ticked>(&TickData {
                header: Header::of::<TickData>(),
                step,
            });
        };
        tick(1);
        // A and B share a kill worth 120, in two batches of one step.
        let mut a = [row(1, 0, 120)];
        let mut b = [row(2, 0, 120)];
        loader.experience(&mut a, 10, |_| 1000);
        loader.experience(&mut b, 10, |_| 1000);
        assert_eq!(HIGH_WATER.load(Ordering::Relaxed), 120);
        assert_eq!((a[0].gained, b[0].gained), (120, 120));
        tick(2);
        let mut both = [row(1, 120, 0), row(2, 120, 0)];
        loader.experience(&mut both, 10, |_| 1000);
        assert_eq!((both[0].gained, both[1].gained), (0, 0), "nobody is behind");
        // The next step's kill counts again.
        let mut a = [row(1, 120, 50)];
        loader.experience(&mut a, 10, |_| 1000);
        assert_eq!(HIGH_WATER.load(Ordering::Relaxed), 170);
        let _cleaned = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_saved_mark_round_trips() {
        let text = toml::to_string(&Saved {
            high_water: 123_456,
        })
        .unwrap();
        let saved: Saved = toml::from_str(&text).unwrap();
        assert_eq!(saved.high_water, 123_456);
    }
}
