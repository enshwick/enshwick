//! Every character on a server keeps up with the one furthest ahead in
//! experience.
//!
//! Before the server applies gained experience, the mod moves the server's
//! high-water mark (the leader's gain once, or a character already further
//! ahead) and adds to the gains of each character below it, up to the end of
//! its current level. The game then applies it and sends it to the player.

mod share;

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use enshwick_loader_api::abi::Side;
use enshwick_loader_api::events::{ShutdownStarted, WorldLoaded, WorldSaved};
use enshwick_loader_api::progress::ExperienceRow;
use enshwick_loader_api::state::Store;
use enshwick_loader_api::{Load, Mod, Off};

pub use share::{Character, Levels, high_water};

/// Shared experience settings.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// Every character catches up with the one furthest ahead in experience.
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
    let previous = HIGH_WATER.load(Ordering::Relaxed);
    let mark = high_water(previous, most_gained, furthest);
    if mark != previous {
        HIGH_WATER.store(mark, Ordering::Relaxed);
    }
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
            updates: github("themaiby/enshwick", "enshwick-shared-experience"),
            signed_by: ["RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF"],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        ENABLED.store(settings.enabled, Ordering::Relaxed);
        load.on_experience(share_experience)?;
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
    fn the_saved_mark_round_trips() {
        let text = toml::to_string(&Saved {
            high_water: 123_456,
        })
        .unwrap();
        let saved: Saved = toml::from_str(&text).unwrap();
        assert_eq!(saved.high_water, 123_456);
    }
}
