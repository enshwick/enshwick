//! The fake's survivors: their lookups, beds, summons and menu statuses.

use enshwick_core::Entity;
use enshwick_core::abi::{BedCheck, Header, Summon, SurvivorLookup};

use crate::test::{Installed, table};

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// Asks the mod's survivor lookup as the game would: survivor
    /// `survivor`, the game's answer `vanilla`, `purpose` one of the
    /// [`SurvivorLookup`] constants with its `entity` and `cell`; `beds`
    /// answers [`Lookup::bed_of`](enshwick_core::survivors::Lookup::bed_of). The
    /// mod's answer, [`Answer::Vanilla`](enshwick_core::survivors::Answer::Vanilla)
    /// without a lookup.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::SurvivorLookup;
    /// use enshwick::survivors::{Answer, NpcId};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let vanilla = Some(enshwick::Entity(4096));
    /// let answer = loader.survivor_lookup(NpcId(7), vanilla, SurvivorLookup::SPAWN, None, [0; 3], &[]);
    /// assert_eq!(answer, Answer::Vanilla);
    /// ```
    #[must_use]
    pub fn survivor_lookup(
        &self,
        survivor: enshwick_core::survivors::NpcId,
        vanilla: Option<Entity>,
        purpose: u32,
        entity: Option<Entity>,
        cell: [i64; 3],
        beds: &[(Entity, Entity)],
    ) -> enshwick_core::survivors::Answer {
        /// # Safety
        ///
        /// - `token` points at the `beds` slice of a running
        ///   `survivor_lookup`, alive and unchanged for the call.
        unsafe extern "system" fn bed_of(token: *mut core::ffi::c_void, entity: Entity) -> Entity {
            // SAFETY: the caller guarantees `token` points at the `beds` slice
            // of the running call, an aligned `&[(Entity, Entity)]` alive for it.
            let beds = unsafe { &*token.cast::<&[(Entity, Entity)]>() };
            beds.iter()
                .find(|(e, _)| *e == entity)
                .map_or(Entity::NONE, |(_, bed)| *bed)
        }
        use enshwick_core::survivors::Answer;
        let Some(registered) = table::with(None, |s| s.survivor_lookup) else {
            return Answer::Vanilla;
        };
        let mut beds = beds;
        let lookup = SurvivorLookup {
            header: Header::of::<SurvivorLookup>(),
            survivor,
            vanilla: vanilla.unwrap_or(Entity::NONE),
            purpose,
            entity: entity.unwrap_or(Entity::NONE),
            cell,
            bed_of: Some(bed_of),
            token: (&raw mut beds).cast(),
        };
        let mut out = Entity::NONE;
        // SAFETY: a valid call and out pointer; `user` is the mod's pointer.
        let answered =
            registered.run(|answer, user| unsafe { answer(&raw const lookup, user, &raw mut out) });
        match (answered != 0, Entity::from_raw(out.0)) {
            (false, _) => Answer::Vanilla,
            (true, Some(entity)) => Answer::Entity(entity),
            (true, None) => Answer::Nobody,
        }
    }

    /// Asks the mod's bed checks as the game would; whether every one keeps
    /// the bed.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{BedCheck, Header};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let check = BedCheck { header: Header::of::<BedCheck>(), npc_id: enshwick::survivors::NpcId::new(7), entity: enshwick::Entity(1), bed_id: enshwick::Entity(2), home: [0; 3], bed_at: [0; 3] };
    /// assert!(loader.survivor_bed_check(&check));
    /// ```
    #[must_use]
    pub fn survivor_bed_check(&self, check: &BedCheck) -> bool {
        let filters = table::with(Vec::new(), |s| s.bed_checks.clone());
        filters.iter().all(|filter| {
            // SAFETY: a valid check; `user` is the mod's pointer.
            filter.run(|callback, user| unsafe { callback(check, user) != 0 })
        })
    }

    /// Asks the mod's summon filters as the game would: [`Summon::REFUSE`]
    /// when one refuses the use, [`Summon::TAKE`] when one makes it take
    /// the item, else 0.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Header, Summon};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let summon = Summon {
    ///     header: Header::of::<Summon>(),
    ///     item: enshwick::ItemId::new(1),
    ///     ammo: enshwick::ItemId::new(0),
    ///     template: [0; 16],
    ///     target: [0; 3],
    ///     slot: 0,
    ///     pide: enshwick::Entity(0),
    ///     meter: [0; 2],
    ///     consumed: 0,
    ///     reserved: [0; 3],
    /// };
    /// assert_eq!(loader.summon(&summon), 0);
    /// ```
    #[must_use]
    pub fn summon(&self, summon: &Summon) -> u8 {
        let filters = table::with(Vec::new(), |s| s.summons.clone());
        let mut verdict = 0;
        for filter in &filters {
            // SAFETY: a valid summon; `user` is the mod's pointer.
            match filter.run(|callback, user| unsafe { callback(summon, user) }) {
                Summon::REFUSE => return Summon::REFUSE,
                0 => {}
                _ => verdict = Summon::TAKE,
            }
        }
        verdict
    }

    /// Tells the mod's summon handlers that a use worked, as the game does
    /// after the spawn or move; returns how many handlers ran.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Header, Summon};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let summon = Summon {
    ///     header: Header::of::<Summon>(),
    ///     item: enshwick::ItemId::new(1),
    ///     ammo: enshwick::ItemId::new(0),
    ///     template: [0; 16],
    ///     target: [0; 3],
    ///     slot: 0,
    ///     pide: enshwick::Entity(0),
    ///     meter: [0; 2],
    ///     consumed: 1,
    ///     reserved: [0; 3],
    /// };
    /// assert_eq!(loader.summoned(&summon), 0);
    /// ```
    #[must_use]
    pub fn summoned(&self, summon: &Summon) -> usize {
        let handlers = table::with(Vec::new(), |s| s.summoned.clone());
        for handler in &handlers {
            // SAFETY: a valid summon; `user` is the mod's pointer.
            handler.run(|callback, user| unsafe { callback(summon, user) });
        }
        handlers.len()
    }

    /// The items the mod filed as summoning staffs, as `(item, survivor)`.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.summon_items().is_empty());
    /// ```
    #[must_use]
    pub fn summon_items(&self) -> Vec<(enshwick_core::ItemId, enshwick_core::survivors::NpcId)> {
        table::with(Vec::new(), |s| s.summon_items.clone())
    }

    /// Asks the mod's survivor menu merge about `statuses`; the byte to show,
    /// or `None` for the game's.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.survivor_status(enshwick::survivors::NpcId::new(7), &[1, 3]), None);
    /// ```
    #[must_use]
    pub fn survivor_status(
        &self,
        survivor: enshwick_core::survivors::NpcId,
        statuses: &[u8],
    ) -> Option<u8> {
        let registered = table::with(None, |s| s.survivor_status)?;
        // SAFETY: valid statuses for the call; `user` is the mod's pointer.
        let answer = registered.run(|callback, user| unsafe {
            callback(survivor, statuses.as_ptr(), statuses.len(), user)
        });
        (answer & enshwick_core::abi::SURVIVOR_STATUS_SET != 0).then(|| answer.to_le_bytes()[0])
    }
}
