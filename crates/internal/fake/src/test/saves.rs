//! The world's save, as the loader reports it and mods hold it.

use crate::abi::SaveInfo;

use super::Installed;
use super::table;

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// Sets the save the fake reports to mods; `None`, the start, is a
    /// world not saved yet.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_fake as enshwick;
    /// use enshwick::abi::{Header, SaveInfo};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.set_save_info(Some(SaveInfo {
    ///     header: Header::of::<SaveInfo>(),
    ///     world: [1; 16],
    ///     saved_at: 60,
    ///     size: 10,
    ///     backups: 0,
    ///     reserved: 0,
    /// }));
    /// ```
    pub fn set_save_info(&self, info: Option<SaveInfo>) {
        table::with((), |s| s.save.info = info);
    }

    /// Shuts the hosted game down as the loader does: the mod's hold on saves
    /// lets go, [`ShutdownStarted`](crate::events::ShutdownStarted) reaches
    /// the mod, and the final save runs unless the mod held saves again while
    /// it heard that event; whether it ran. A hold asked for after the event
    /// is not taken.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_fake as enshwick;
    /// let loader = enshwick::test::FakeHost::new().world().install();
    /// assert!(loader.shut_down(), "nothing holds the final save");
    /// ```
    #[must_use = "the answer says whether the final save ran"]
    pub fn shut_down(&self) -> bool {
        table::with((), |s| {
            s.save.held = false;
            s.save.shut = false;
        });
        // SAFETY: the event carries no data.
        let _heard =
            unsafe { self.raise_raw(crate::abi::Event::SHUTDOWN_STARTED, core::ptr::null()) };
        table::with(false, |s| {
            s.save.shut = true;
            !s.save.held
        })
    }

    /// Whether the mod holds the world's saves.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_fake as enshwick;
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(!loader.saves_held());
    /// ```
    #[must_use]
    pub fn saves_held(&self) -> bool {
        table::with(false, |s| s.save.held)
    }

    /// The entities the mod keeps out of the world's saves, in the order it
    /// last asked.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_fake as enshwick;
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.unsaved().is_empty());
    /// ```
    #[must_use]
    pub fn unsaved(&self) -> Vec<crate::Entity> {
        table::with(Vec::new(), |s| {
            s.save.unsaved.iter().map(|&e| crate::Entity(e)).collect()
        })
    }
}
