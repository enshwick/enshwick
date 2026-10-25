//! Every character in a world keeps up with the one furthest ahead in
//! experience.
//!
//! Before the server applies gained experience, the mod moves the world's
//! high-water mark (the leader's gain once, or a character already further
//! ahead) and adds to the gains of each character below it, up to the end of
//! its current level. The game then applies it and sends it to the player.
//!
//! The mark is kept per world, in
//! `mod-data/shared-experience/worlds/<world>/experience.toml`.

mod share;

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use enshwick_loader_api::events::{ShutdownStarted, Ticked, WorldLoaded};
use enshwick_loader_api::progress::Experience;
use enshwick_loader_api::state::{Store, WorldStore};
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

/// What a world keeps between starts.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
struct Saved {
    /// The most cumulative experience any character has reached here.
    high_water: u64,
}

static LEVELS: OnceLock<Levels> = OnceLock::new();
static ENABLED: AtomicBool = AtomicBool::new(true);
static HIGH_WATER: AtomicU64 = AtomicU64::new(0);
/// The mark last handed to the world's store.
static STORED: AtomicU64 = AtomicU64::new(0);
/// The largest gain counted in this server step: the engine hands the
/// characters over in several batches a step, and one kill shared by
/// characters in two batches is still one moment.
static MOMENT: AtomicU32 = AtomicU32::new(0);

const fn character(row: &Experience) -> Character {
    Character {
        level: row.level,
        progress: row.progress,
        need: row.need,
        gained: row.gained,
    }
}

/// Moves the high-water mark and raises every character below it.
fn share_experience(rows: &mut [Experience], table: &enshwick_loader_api::progress::Levels) {
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
    let moved = HIGH_WATER.try_update(Ordering::Relaxed, Ordering::Relaxed, |previous| {
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

/// Hands a moved mark to the world's store after a step, off the
/// experience path; the store writes it as the world saves and at shutdown.
fn keep(store: &WorldStore<Saved>) {
    let mark = HIGH_WATER.load(Ordering::Relaxed);
    if STORED.load(Ordering::Relaxed) == mark {
        return;
    }
    if store.update(|saved| saved.high_water = mark).is_some() {
        STORED.store(mark, Ordering::Relaxed);
    }
}

/// Puts the loaded world's mark in force. A world without a mark of its own
/// takes the one the mod kept for the whole server before marks were kept
/// per world, once; that file is then renamed out of the way.
fn open_world(store: &WorldStore<Saved>, dir: &Path) {
    let mut mark = store.with(|saved| saved.high_water).unwrap_or(0);
    let shared = dir.join("experience.toml");
    if mark == 0 && store.key().is_some() && shared.exists() {
        match Store::<Saved>::new(dir, "experience").load() {
            Ok(old) => {
                mark = old.high_water;
                store.update(|saved| saved.high_water = mark);
                let moved = store.flush().map_err(|e| e.to_string()).and_then(|()| {
                    std::fs::rename(&shared, dir.join("experience.toml.moved"))
                        .map_err(|e| e.to_string())
                });
                match moved {
                    Ok(()) => enshwick_loader_api::info!(
                        "experience mark {mark} moved from experience.toml into this world"
                    ),
                    Err(why) => enshwick_loader_api::warn!("experience.toml stays: {why}"),
                }
            }
            Err(error) => enshwick_loader_api::warn!("experience.toml not read: {error}"),
        }
    }
    STORED.store(mark, Ordering::Relaxed);
    let mark = HIGH_WATER.fetch_max(mark, Ordering::Relaxed).max(mark);
    enshwick_loader_api::info!(
        "experience mark {mark} in world {}",
        store.key().unwrap_or_default()
    );
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
        let dir = load.data_dir()?;
        // The store closes at shutdown in a handler of its own: the mark the
        // last step moved goes to it first, as no tick follows.
        let last: Arc<OnceLock<WorldStore<Saved>>> = Arc::default();
        let closing = Arc::clone(&last);
        load.on::<ShutdownStarted>(move |()| {
            if let Some(store) = closing.get() {
                keep(store);
            }
        })?;
        // The store opens on the world's load, before the handler below runs.
        let store = load.world_store::<Saved>("experience")?;
        let _first = last.set(store.clone());
        let kept = store.clone();
        load.on::<Ticked>(move |_| {
            MOMENT.store(0, Ordering::Relaxed);
            keep(&kept);
        })?;
        load.on::<WorldLoaded>(move |()| open_world(&store, &dir))?;
        // The next game may be another world.
        load.on::<ShutdownStarted>(|()| {
            HIGH_WATER.store(0, Ordering::Relaxed);
            STORED.store(0, Ordering::Relaxed);
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

    fn tick(loader: &enshwick_loader_api::test::Installed, step: u64) {
        use enshwick_loader_api::abi::{Header, TickData};

        let _ran = loader.raise::<Ticked>(&TickData {
            header: Header::of::<TickData>(),
            step,
        });
    }

    /// A game of the world `key` with the mod's data in `dir`.
    fn game(dir: &Path, key: &str) -> enshwick_loader_api::test::Installed {
        let loader = enshwick_loader_api::test::FakeHost::new()
            .settings(
                "enabled = true
",
            )
            .data_dir(dir.to_str().unwrap())
            .world_key(key)
            .install();
        // SAFETY: the exports of `declare_mod!`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, enshwick_loader_api::abi::Status::OK);
        HIGH_WATER.store(0, Ordering::Relaxed);
        STORED.store(0, Ordering::Relaxed);
        assert_eq!(loader.raise::<WorldLoaded>(&()), 2, "{:?}", loader.logs());
        loader
    }

    fn mark_of(dir: &Path, key: &str) -> u64 {
        let text =
            std::fs::read_to_string(dir.join("worlds").join(key).join("experience.toml")).unwrap();
        toml::from_str::<Saved>(&text).unwrap().high_water
    }

    #[test]
    fn a_world_raises_the_trailing_keeps_its_mark_and_a_fresh_world_starts_clean() {
        let dir = std::env::temp_dir().join(format!("shared-experience-{}", std::process::id()));
        let _fresh = std::fs::remove_dir_all(&dir);
        // A mark kept for the whole server before marks were per world.
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("experience.toml"),
            "high_water = 30
",
        )
        .unwrap();

        let loader = game(&dir, "first");
        assert_eq!(
            HIGH_WATER.load(Ordering::Relaxed),
            30,
            "moved into the first world"
        );
        assert!(!dir.join("experience.toml").exists());
        assert!(dir.join("experience.toml.moved").exists());
        let mut rows = [
            Experience {
                entity: enshwick_loader_api::ecs::Entity(1),
                level: 1,
                progress: 0,
                need: 100,
                gained: 40,
            },
            Experience {
                entity: enshwick_loader_api::ecs::Entity(2),
                level: 1,
                progress: 0,
                need: 100,
                gained: 0,
            },
        ];
        tick(&loader, 1);
        loader.experience(&mut rows, 10, |_| 100);
        assert_eq!(
            (rows[0].gained, rows[1].gained),
            (70, 70),
            "both catch up with the moved mark"
        );
        // The game stops right after that step: no tick hands the mark over.
        assert_eq!(loader.raise::<ShutdownStarted>(&()), 3);
        assert_eq!(mark_of(&dir, "first"), 70, "kept before the store closed");
        assert_eq!(
            HIGH_WATER.load(Ordering::Relaxed),
            0,
            "the next game starts clean"
        );
        let _cleaned = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_fresh_world_inherits_no_other_world_s_mark() {
        let dir =
            std::env::temp_dir().join(format!("shared-experience-fresh-{}", std::process::id()));
        let _fresh = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("worlds").join("first")).unwrap();
        std::fs::write(
            dir.join("worlds").join("first").join("experience.toml"),
            "high_water = 70
",
        )
        .unwrap();
        std::fs::write(
            dir.join("experience.toml.moved"),
            "high_water = 30
",
        )
        .unwrap();
        let _loader = game(&dir, "second");
        assert_eq!(HIGH_WATER.load(Ordering::Relaxed), 0);
        let _cleaned = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_world_takes_back_its_own_mark() {
        let dir =
            std::env::temp_dir().join(format!("shared-experience-again-{}", std::process::id()));
        let _fresh = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("worlds").join("first")).unwrap();
        std::fs::write(
            dir.join("worlds").join("first").join("experience.toml"),
            "high_water = 70
",
        )
        .unwrap();
        // A server-wide mark left beside it is not this world's.
        std::fs::write(
            dir.join("experience.toml"),
            "high_water = 30
",
        )
        .unwrap();
        let _loader = game(&dir, "first");
        assert_eq!(HIGH_WATER.load(Ordering::Relaxed), 70);
        assert!(dir.join("experience.toml").exists());
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
        STORED.store(0, Ordering::Relaxed);
        let row = |entity, progress, gained| Experience {
            entity: enshwick_loader_api::ecs::Entity(entity),
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
