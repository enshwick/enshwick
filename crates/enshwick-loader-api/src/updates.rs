//! Updates on the Mods screen: a mod that looks after other mods, such as
//! the updater, marks each installed mod that has an update on offer, and
//! hears which one the player asked to update.
//!
//! A mod publishes its updates with [`publish`] whenever it learns more, and
//! hears with [`Load::on_update_pick`] which one the player picked. The
//! loader only shows them under the installed mods they name: whether a pick
//! downloads anything stays with the mod that published it. Client only.
//!
//! # Examples
//!
//! ```
//! use enshwick_loader_api::updates::{self, Update};
//!
//! fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
//!     load.on_update_pick(|id| enshwick_loader_api::info!("the player picked {id}"))?;
//!     let _shown = updates::publish(&[Update::new("safes", "0.7.0").ready(true)]);
//!     Ok(())
//! }
//! ```

use core::ffi::c_void;

use crate::abi::Str;
use crate::{Load, Off};

/// An update of an installed mod.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::updates::Update;
///
/// let update = Update::new("safes", "0.7.0").note("unsigned").ready(true);
/// assert!(update.ready && update.note == "unsigned");
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct Update {
    /// The installed mod's id.
    pub id: String,
    /// The version on offer.
    pub version: String,
    /// A line for the player: how it is checked, or why it is not ready.
    #[serde(default)]
    pub note: String,
    /// Whether picking it does something: the screen shows a button.
    #[serde(default)]
    pub ready: bool,
}

impl Update {
    /// An update of mod `id` to `version`, not ready.
    ///
    /// # Examples
    ///
    /// See [`Update`].
    #[must_use]
    pub fn new(id: &str, version: &str) -> Self {
        Self {
            id: id.to_owned(),
            version: version.to_owned(),
            ..Self::default()
        }
    }

    /// With a line for the player.
    ///
    /// # Examples
    ///
    /// See [`Update`].
    #[must_use]
    pub fn note(mut self, note: &str) -> Self {
        note.clone_into(&mut self.note);
        self
    }

    /// Whether picking it does something.
    ///
    /// # Examples
    ///
    /// See [`Update`].
    #[must_use]
    pub const fn ready(mut self, ready: bool) -> Self {
        self.ready = ready;
        self
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
struct List {
    #[serde(default)]
    update: Vec<Update>,
}

/// `updates` as the text the loader reads, an array of `[[update]]` tables.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::updates::{Update, from_toml, to_toml};
///
/// let updates = vec![Update::new("safes", "0.7.0")];
/// assert_eq!(from_toml(&to_toml(&updates))?, updates);
/// # Ok::<(), enshwick_loader_api::Off>(())
/// ```
#[must_use]
pub fn to_toml(updates: &[Update]) -> String {
    toml::to_string(&List {
        update: updates.to_vec(),
    })
    .unwrap_or_default()
}

/// The updates in a text [`to_toml`] wrote.
///
/// # Errors
///
/// [`Off`] for text that is not such a list.
///
/// # Examples
///
/// See [`to_toml`].
pub fn from_toml(text: &str) -> Result<Vec<Update>, Off> {
    toml::from_str::<List>(text)
        .map(|list| list.update)
        .map_err(|e| Off::new(format!("updates: {}", e.message())))
}

/// Shows `updates` on the Mods screen, in place of the ones the mod
/// published before; none clears them. Any thread.
///
/// # Errors
///
/// [`Off`] on a server or outside a mod.
///
/// # Examples
///
/// See the [module](self).
pub fn publish(updates: &[Update]) -> Result<(), Off> {
    let host = crate::host().ok_or_else(|| Off::new("updates: outside a mod"))?;
    let api = host.api();
    let text = to_toml(updates);
    let status = (api.updates_set)(host.handle(), Str::new(&text));
    crate::load::ok(status, (), || "updates".to_owned())
}

type Picker = Box<dyn Fn(&str) + Send + Sync>;

unsafe extern "system" fn picked(id: Str, user: *mut c_void) {
    // SAFETY: `user` is the picker this function was registered with, kept
    // by the loader for the process.
    let picker = unsafe { &*user.cast::<Picker>() };
    // SAFETY: the loader passes the mod's id for the call.
    let Some(id) = (unsafe { id.as_str() }) else {
        return;
    };
    crate::host::guard("its update pick panicked", || (), || picker(id));
}

impl Load<'_> {
    /// Runs `pick` with the mod's id when the player presses Update under a
    /// mod this mod published an update of, on the thread that draws the
    /// interface: it must return at once, so it hands any work to a thread
    /// of the mod's own. Client only; a panic turns the mod off.
    ///
    /// # Errors
    ///
    /// [`Off`] on a server, after `init`, or for a second callback.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn on_update_pick(
        &mut self,
        pick: impl Fn(&str) + Send + Sync + 'static,
    ) -> Result<(), Off> {
        let host = self.host();
        if host.build().side == crate::abi::Side::SERVER {
            return Err(Off::new("updates: on clients only"));
        }
        let picker: Picker = Box::new(pick);
        crate::load::hand_over(
            picker,
            |user| (host.api().update_pick)(host.handle(), picked, user),
            "update pick",
        )
    }
}
