//! The Mods screen's catalog: updates of installed mods and mods the player
//! may add, which a mod such as the updater found at the sources the
//! installed mods declare.
//!
//! A mod publishes its catalog with [`publish`] whenever it learns more, and
//! hears with [`Load::on_catalog_pick`] which entry the player picked on the
//! screen. The loader only shows the entries: what an entry means, and
//! whether picking it downloads anything, stays with the mod that published
//! it. Client only.
//!
//! # Examples
//!
//! ```
//! use enshwick_loader_api::catalog::{self, Entry};
//!
//! fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
//!     load.on_catalog_pick(|id| enshwick_loader_api::info!("the player picked {id}"))?;
//!     let update = Entry::new("safes", "0.7.0").installed("0.6.1").ready(true);
//!     let _shown = catalog::publish(&[update]);
//!     Ok(())
//! }
//! ```

use core::ffi::c_void;

use crate::abi::{HostApi, Str};
use crate::{Load, Off};

/// One entry of a catalog.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::catalog::Entry;
///
/// let new = Entry::new("lanterns", "1.2.0")
///     .name("Lanterns")
///     .source("github.com/someone/lanterns")
///     .note("signed by the author of your mods from there")
///     .ready(true);
/// assert!(new.installed.is_none() && new.ready);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct Entry {
    /// The mod id, or the release's component for a mod not installed.
    pub id: String,
    /// The name players read; empty for none.
    #[serde(default)]
    pub name: String,
    /// The version on offer.
    pub version: String,
    /// The version installed; `None` for a mod the player may add.
    #[serde(default)]
    pub installed: Option<String>,
    /// Where it comes from, as the player reads it.
    #[serde(default)]
    pub source: String,
    /// A line for the player: how it is checked, or why it is not ready.
    #[serde(default)]
    pub note: String,
    /// Whether picking it does something: the screen shows a button.
    #[serde(default)]
    pub ready: bool,
}

impl Entry {
    /// An entry for `id` offering `version`, not installed and not ready.
    ///
    /// # Examples
    ///
    /// See [`Entry`].
    #[must_use]
    pub fn new(id: &str, version: &str) -> Self {
        Self {
            id: id.to_owned(),
            version: version.to_owned(),
            ..Self::default()
        }
    }

    /// With the name players read.
    ///
    /// # Examples
    ///
    /// See [`Entry`].
    #[must_use]
    pub fn name(mut self, name: &str) -> Self {
        name.clone_into(&mut self.name);
        self
    }

    /// As an update of the installed `version`.
    ///
    /// # Examples
    ///
    /// See [`Entry`].
    #[must_use]
    pub fn installed(mut self, version: &str) -> Self {
        self.installed = Some(version.to_owned());
        self
    }

    /// With where it comes from.
    ///
    /// # Examples
    ///
    /// See [`Entry`].
    #[must_use]
    pub fn source(mut self, source: &str) -> Self {
        source.clone_into(&mut self.source);
        self
    }

    /// With a line for the player.
    ///
    /// # Examples
    ///
    /// See [`Entry`].
    #[must_use]
    pub fn note(mut self, note: &str) -> Self {
        note.clone_into(&mut self.note);
        self
    }

    /// Whether picking it does something.
    ///
    /// # Examples
    ///
    /// See [`Entry`].
    #[must_use]
    pub const fn ready(mut self, ready: bool) -> Self {
        self.ready = ready;
        self
    }

    /// `Name (id)`, or the id when the entry names none.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::catalog::Entry;
    ///
    /// assert_eq!(Entry::new("safes", "1.0.0").name("Safes").label(), "Safes (safes)");
    /// ```
    #[must_use]
    pub fn label(&self) -> String {
        crate::mods::label(&self.name, &self.id)
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
struct Catalog {
    #[serde(default)]
    entry: Vec<Entry>,
}

/// `entries` as the text the loader reads, an array of `[[entry]]` tables.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::catalog::{Entry, from_toml, to_toml};
///
/// let entries = vec![Entry::new("safes", "0.7.0").installed("0.6.1")];
/// assert_eq!(from_toml(&to_toml(&entries))?, entries);
/// # Ok::<(), enshwick_loader_api::Off>(())
/// ```
#[must_use]
pub fn to_toml(entries: &[Entry]) -> String {
    toml::to_string(&Catalog {
        entry: entries.to_vec(),
    })
    .unwrap_or_default()
}

/// The entries of a catalog's text.
///
/// # Errors
///
/// [`Off`] for text that is not a catalog.
///
/// # Examples
///
/// See [`to_toml`].
pub fn from_toml(text: &str) -> Result<Vec<Entry>, Off> {
    toml::from_str::<Catalog>(text)
        .map(|catalog| catalog.entry)
        .map_err(|e| Off::new(format!("catalog: {}", e.message())))
}

/// Shows `entries` as the mod's catalog on the Mods screen, in place of the
/// ones it published before; none clears it. Any thread.
///
/// # Errors
///
/// [`Off`] on a server, outside a mod, or under a loader older than ABI
/// minor 58.
///
/// # Examples
///
/// See the [module](self).
pub fn publish(entries: &[Entry]) -> Result<(), Off> {
    let host = crate::host().ok_or_else(|| Off::new("catalog: outside a mod"))?;
    let api = host.api();
    crate::load::offered(api, core::mem::offset_of!(HostApi, catalog_set), "catalog")?;
    let text = to_toml(entries);
    let status = (api.catalog_set)(host.handle(), Str::new(&text));
    crate::load::ok(status, (), || "catalog".to_owned())
}

type Picker = Box<dyn Fn(&str) + Send + Sync>;

unsafe extern "system" fn picked(id: Str, user: *mut c_void) {
    // SAFETY: `user` is the picker this function was registered with, kept
    // by the loader for the process.
    let picker = unsafe { &*user.cast::<Picker>() };
    // SAFETY: the loader passes the entry's id for the call.
    let Some(id) = (unsafe { id.as_str() }) else {
        return;
    };
    crate::host::guard("its catalog pick panicked", || (), || picker(id));
}

impl Load<'_> {
    /// Runs `pick` with the entry's id when the player picks an entry of the
    /// mod's catalog on the Mods screen, on the thread that draws the
    /// interface: it must return at once, so it hands any work to a thread
    /// of the mod's own. Client only; a panic turns the mod off.
    ///
    /// # Errors
    ///
    /// [`Off`] on a server, after `init`, for a second callback, or under a
    /// loader older than ABI minor 58.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn on_catalog_pick(
        &mut self,
        pick: impl Fn(&str) + Send + Sync + 'static,
    ) -> Result<(), Off> {
        let host = self.host();
        if host.build().side == crate::abi::Side::SERVER {
            return Err(Off::new("catalog: on clients only"));
        }
        crate::load::offered(
            host.api(),
            core::mem::offset_of!(HostApi, catalog_pick),
            "catalog",
        )?;
        let picker: Picker = Box::new(pick);
        crate::load::hand_over(
            picker,
            |user| (host.api().catalog_pick)(host.handle(), picked, user),
            "catalog pick",
        )
    }
}
