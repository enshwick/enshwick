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

    /// Whether the mod holds the world's saves.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(!loader.saves_held());
    /// ```
    #[must_use]
    pub fn saves_held(&self) -> bool {
        table::with(false, |s| s.save.held)
    }
}
