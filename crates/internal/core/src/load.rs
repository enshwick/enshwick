//! What a mod does while the loader initialises it: [`Load`], whose base
//! methods forward to the base handle it holds.

pub(crate) use enshwick_base::__off_from as off_from;
use enshwick_base::load::BaseLoad;
#[doc(inline)]
pub use enshwick_values::Off;

use crate::host::Host;

/// A mod: what the loader initialises when it loads the DLL.
///
/// Implement it on the mod's state type and export it with
/// [`declare_mod!`](crate::declare_mod), which reads what the mod is from its
/// `Cargo.toml`.
///
/// # Examples
///
/// ```
/// struct Quiet;
///
/// impl enshwick::Mod for Quiet {
///     fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
///         Ok(Self)
///     }
/// }
/// ```
pub trait Mod: Sized + Send + Sync + 'static {
    /// Registers hooks, services and settings. Nothing takes effect until every
    /// mod is initialised and the loader arms the hooks.
    ///
    /// # Errors
    ///
    /// [`Off`] when the mod cannot run on this game or with these settings.
    fn init(load: &mut Load<'_>) -> Result<Self, Off>;

    /// Runs once the hooks are armed; start background work here.
    fn ready(&'static self, _host: Host) {}
}

/// The loader during a mod's initialisation: lookups, hooks, settings and
/// services.
///
/// # Examples
///
/// See [`Mod`].
#[derive(Debug)]
pub struct Load<'a> {
    base: BaseLoad,
    _init: core::marker::PhantomData<&'a ()>,
}

#[enshwick_macros::forward(base)]
#[crate::script(dirs)]
impl Load<'_> {
    /// The loader's services, also available later through
    /// [`host()`](crate::host()).
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) {
    ///     let side = load.host().build().side;
    ///     let _ = side;
    /// }
    /// ```
    #[must_use]
    pub fn host(&self) -> Host;

    /// The mod's [`Scheduler`](crate::schedule::Scheduler), which runs work on
    /// the server's game thread on a tick of the default period: the dedicated
    /// server's, or the built-in server's while a client hosts a game.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader raises no ticks.
    ///
    /// # Examples
    ///
    /// See [`Scheduler`](crate::schedule::Scheduler).
    pub fn scheduler(&mut self) -> Result<crate::schedule::Scheduler, Off>;

    /// The mod's data folder in the world that runs, `<stem>-mod-data/<id>`
    /// beside the world's save; `None` while no world runs, as during init
    /// before a hosted game or a dedicated server's world loads. A world's
    /// state belongs there; [`WorldStore`](crate::state::WorldStore) keeps
    /// it for you, and `World::data_dir`
    /// answers the same once the world runs.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     if let Some(dir) = load.data_dir() {
    ///         enshwick::info!("this world's data: {}", dir.display());
    ///     }
    ///     Ok(())
    /// }
    /// ```
    #[must_use]
    #[script]
    pub fn data_dir(&self) -> Option<std::path::PathBuf>;

    /// The mod's folder in `mods/`, which holds its installation, such as an
    /// installation [`Hold`](crate::state::Hold).
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader names no folder or a path that is not UTF-8.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let hold = enshwick::state::Hold::new(&load.mod_dir()?);
    ///     let _ = hold.reason();
    ///     Ok(())
    /// }
    /// ```
    #[script]
    pub fn mod_dir(&self) -> Result<std::path::PathBuf, Off>;

    /// The folder `export/<id>/` in the game folder, named by the mod id
    /// whatever the mod's folder in `mods/` is called, where a mod writes
    /// files for tools outside the game, such as a wiki or a map; a mod may
    /// write to any other path too.
    ///
    /// # Errors
    ///
    /// As [`Load::mod_dir`], for a mod folder outside a `mods/` folder, and
    /// when the loader lists no mod in that folder.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let items = load.export_dir()?.join("items.json");
    ///     enshwick::files::write_atomically(&items, "[]").map_err(|e| enshwick::Off::new(e.to_string()))
    /// }
    /// ```
    #[script]
    pub fn export_dir(&self) -> Result<std::path::PathBuf, Off>;

    /// A value of type `T` kept per world in the mod's data folder of that
    /// world, opened when the world loads and written when it saves; see
    /// [`WorldStore`](crate::state::WorldStore). A world file that does not
    /// read keeps the store shut, with the reason in the status summary and
    /// on the Mods screen, as the feature `world store <name>` off.
    ///
    /// # Errors
    ///
    /// [`Off`] when `name` is not 1 to 64 of `[a-z0-9_-]` or the world
    /// events are refused.
    ///
    /// # Examples
    ///
    /// See [`WorldStore`](crate::state::WorldStore).
    pub fn world_store<T>(&mut self, name: &str) -> Result<crate::state::WorldStore<T>, Off>
    where
        T: serde::de::DeserializeOwned + serde::Serialize + Default + Send + 'static;
}

impl Load<'_> {
    pub(crate) fn new(host: Host) -> Self {
        Self::of(host, false, None)
    }

    /// A `Load` for a mod that takes part in the join check when `joins`,
    /// with the texts `declare_mod!` embedded.
    pub(crate) fn of(host: Host, joins: bool, embedded_texts: Option<&'static str>) -> Self {
        Self {
            base: BaseLoad::of(host, joins, embedded_texts),
            _init: core::marker::PhantomData,
        }
    }
}

// `Load`'s methods of the crates below, by the module they forward to.
#[cfg(feature = "unstable")]
mod actions;
mod affixes;
mod altars;
mod buffs;
mod build;
mod chat;
mod code;
mod config;
mod content;
#[cfg(feature = "unstable")]
mod controls;
#[cfg(feature = "unstable")]
mod damage;
mod daytime;
mod effects;
#[cfg(feature = "unstable")]
mod entity_data;
mod events;
mod game_events;
mod game_settings;
mod i18n;
#[cfg(feature = "unstable")]
mod input;
mod interactions;
mod inventory;
#[cfg(feature = "unstable")]
mod item_data;
#[cfg(feature = "unstable")]
mod items;
mod knowledge;
mod loot;
#[cfg(feature = "unstable")]
mod map;
mod net;
mod pacing;
mod points;
mod progress;
#[cfg(feature = "typed")]
mod quests;
#[cfg(feature = "unstable")]
mod rates;
mod resources;
#[cfg(feature = "unstable")]
mod rewards;
mod services;
mod settings;
mod sound;
mod state;
mod stats;
mod status;
mod survivors;
mod terrain;
mod time;
#[cfg(feature = "typed")]
mod typed;
mod ui;
mod vetoes;
mod voxels;
mod weather;
#[cfg(feature = "unstable")]
mod world;

impl enshwick_base::load::sealed::Sealed for Load<'_> {}

impl crate::Loading for Load<'_> {
    fn base(&self) -> &BaseLoad {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseLoad {
        &mut self.base
    }
}

impl crate::facts::EngineFacts for Load<'_> {
    fn offset(&self, id: &str) -> Result<usize, Off> {
        crate::facts::EngineFacts::offset(&self.base, id)
    }

    fn size(&self, id: &str) -> Result<usize, Off> {
        crate::facts::EngineFacts::size(&self.base, id)
    }

    fn hash(&self, id: &str) -> Result<u32, Off> {
        crate::facts::EngineFacts::hash(&self.base, id)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn load_stays_send_sync_and_unwind_safe() {
        fn held<T: Send + Sync + core::panic::UnwindSafe + core::panic::RefUnwindSafe>() {}
        held::<super::Load<'static>>();
    }
}
