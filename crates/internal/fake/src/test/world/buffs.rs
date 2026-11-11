//! The fake's buffs: the types it knows and what each holder holds.

use enshwick_core::Entity;

use crate::test::{Installed, table};

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// Sets the buff types the fake knows and the entities that hold buffs,
    /// with none yet; a buff the mod gives starts with the duration asked,
    /// or the type's, and one it removes goes at once.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{BuffTypeInfo, Header};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let ty = BuffTypeInfo { header: Header::of::<BuffTypeInfo>(), id: enshwick::buffs::BuffTypeId::new(7), slot: 1, flags: 0, name: 0, duration: 60_000_000_000 };
    /// loader.set_buffs(&[ty], &[enshwick::Entity(2)]);
    /// assert_eq!(loader.buffs_of(enshwick::Entity(2)), Some(Vec::new()));
    /// ```
    pub fn set_buffs(&self, types: &[enshwick_core::abi::BuffTypeInfo], holders: &[Entity]) {
        table::with((), |s| {
            s.buff_types = types.to_vec();
            s.buffs = holders.iter().map(|&e| (e.0, Vec::new())).collect();
        });
    }

    /// The buffs `entity` holds as the mod left them; `None` for one that
    /// holds no buffs.
    ///
    /// # Examples
    ///
    /// See [`Installed::set_buffs`].
    #[must_use]
    pub fn buffs_of(&self, entity: Entity) -> Option<Vec<enshwick_core::abi::ActiveBuff>> {
        table::with(None, |s| s.buffs.get(&entity.0).cloned())
    }
}
