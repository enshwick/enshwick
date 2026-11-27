//! The fake's buffs: the types it knows and what each holder holds.

#[cfg(feature = "unstable")]
use enshwick_core::__fake::ids;
use enshwick_core::Entity;

use crate::test::{FakeHost, Installed, table};

impl FakeHost {
    /// The buff collection: what
    /// [`BuffLayout`](enshwick_core::buffs::BuffLayout) resolves, with the
    /// collection's type hash `add_buff` serves under, and the vanilla
    /// collection in the archive holding a type for each of `vanilla`, every
    /// field zero but its id, all in one bucket.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::buffs::{BuffLayout, BuffTypeId};
    ///
    /// let host = enshwick::test::FakeHost::new().buffs(&[BuffTypeId(0x10)]);
    /// let layout = BuffLayout::resolve(&host)?;
    /// assert!(layout.icon < layout.size);
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    #[cfg(feature = "unstable")]
    #[must_use]
    pub fn buffs(self, vanilla: &[enshwick_core::buffs::BuffTypeId]) -> Self {
        use enshwick_core::buffs::{BUFF_COLLECTION, BuffLayout};
        use enshwick_core::content::{Blob, EngineFacts as _};

        let host = self.buff_facts();
        let (Ok(layout), Ok(hash), Ok(count)) = (
            BuffLayout::resolve(&host),
            host.hash(ids::BUFF_COLLECTION),
            u32::try_from(vanilla.len()),
        ) else {
            return host;
        };
        let root = [layout.buffs, layout.entries, layout.keys, layout.values]
            .into_iter()
            .max()
            .unwrap_or(0)
            + 8;
        let mut blob = Blob::new(&vec![0; root]);
        let mut buffs = vec![0; layout.size * vanilla.len()];
        let mut entry = vec![0; layout.bucket];
        let mut keys = Vec::new();
        let mut values = Vec::new();
        for (i, (id, buff)) in vanilla
            .iter()
            .zip(buffs.chunks_exact_mut(layout.size))
            .enumerate()
        {
            buff[layout.id..layout.id + 4].copy_from_slice(&id.0.to_le_bytes());
            keys.extend(id.0.to_le_bytes());
            values.extend(u32::try_from(i).unwrap_or(u32::MAX).to_le_bytes());
        }
        entry[layout.bucket_count..layout.bucket_count + 4].copy_from_slice(&count.to_le_bytes());
        let arrays = [
            (layout.buffs, buffs, 8, count),
            (layout.entries, entry, 4, 1),
            (layout.keys, keys, 4, count),
            (layout.values, values, 4, count),
        ];
        for (field, bytes, align, count) in arrays {
            let at = blob.append(&bytes, align);
            blob.set_array(field, at, count);
        }
        host.resource(BUFF_COLLECTION.key_by_hash(hash), blob.into_bytes())
    }

    /// The buff collection's facts, for [`FakeHost::buffs`].
    #[cfg(feature = "unstable")]
    fn buff_facts(self) -> Self {
        self.preset(
            &[ids::BUFF_COLLECTION],
            &[
                (ids::BUFF_TYPE, 184),
                (ids::BUFF_MAP_ENTRY, 8),
                (ids::IMPACT_PROGRAM_REFERENCE, 16),
                (ids::BUFF_ON_TIMEOUT, 16),
            ],
            &[
                (ids::BUFF_COLLECTION_ENTRIES, 0),
                (ids::BUFF_COLLECTION_KEYS, 8),
                (ids::BUFF_COLLECTION_VALUES, 16),
                (ids::BUFF_COLLECTION_BUFFS, 24),
                (ids::BUFF_TYPE_ID, 0),
                (ids::BUFF_TYPE_SLOT, 4),
                (ids::BUFF_TYPE_LIFE_TIME, 8),
                (ids::BUFF_TYPE_NAME, 16),
                (ids::BUFF_TYPE_DESCRIPTION, 32),
                (ids::BUFF_TYPE_HIDE_IN_UI, 49),
                (ids::BUFF_TYPE_ICON, 52),
                (ids::BUFF_TYPE_HIDE_DURATION_IN_UI, 72),
                (ids::BUFF_TYPE_IS_SAVED, 74),
                (ids::BUFF_TYPE_IMPACT, 76),
                (ids::BUFF_TYPE_IMPACT_SIMPLE, 96),
                (ids::BUFF_TYPE_IMPACT_SCALED, 104),
                (ids::BUFF_TYPE_UI_VALUES, 112),
                (ids::BUFF_TYPE_ON_CREATE, 120),
                (ids::BUFF_TYPE_WHILE_APPLIED, 128),
                (ids::BUFF_TYPE_ON_TIMEOUT, 136),
                (ids::BUFF_TYPE_ON_TIMEOUT_IS_SET, 152),
                (ids::BUFF_TYPE_VFX_MESHES, 156),
                (ids::BUFF_TYPE_VFX_MODELS, 164),
                (ids::BUFF_TYPE_VFX_JOINTS, 172),
                (ids::BUFF_MAP_ENTRY_START, 0),
                (ids::BUFF_MAP_ENTRY_COUNT, 4),
            ],
        )
    }
}

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
    /// let ty = enshwick::abi::headed!(BuffTypeInfo { id: enshwick::buffs::BuffTypeId::new(7), slot: 1, flags: 0, name: 0, duration_ns: 60_000_000_000 });
    /// loader.set_buffs(&[ty], &[enshwick::Entity::from_raw(2).ok_or("no entity")?]);
    /// assert_eq!(loader.buffs_of(enshwick::Entity::from_raw(2).ok_or("no entity")?), Some(Vec::new()));
    /// # Ok::<(), &str>(())
    /// ```
    pub fn set_buffs(&self, types: &[enshwick_core::abi::BuffTypeInfo], holders: &[Entity]) {
        table::with((), |s| {
            s.world.buff_types = types.to_vec();
            s.world.buffs = holders.iter().map(|&e| (e.get(), Vec::new())).collect();
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
        table::with(None, |s| s.world.buffs.get(&entity.get()).cloned())
    }
}
