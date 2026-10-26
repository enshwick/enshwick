//! Gameplay answers: damage, knowledge, buffs, altars, survivors, stats and checks.

use crate::abi::AltarInfo;
use crate::abi::AltarZone;
use crate::abi::BedCheck;
use crate::abi::Bounds;
use crate::abi::Comfort;
use crate::abi::ExperiencePass;
use crate::abi::ExperienceRow;
use crate::abi::Header;
use crate::abi::Interaction;
use crate::abi::SkillCheck;
use crate::abi::Summon;
use crate::abi::SurvivorLookup;
use crate::abi::ZoneCheck;

use super::Installed;
use super::stats;
use super::table;

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// Makes `altars` the altars of the world, as the loader lists them to a
    /// mod that opened altars, and raises
    /// [`AltarsChanged`](crate::events::AltarsChanged) as the loader does.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::AltarInfo;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let mut altar = AltarInfo::EMPTY;
    /// altar.entity = 4096;
    /// loader.set_altars(&[altar]);
    /// ```
    pub fn set_altars(&self, altars: &[AltarInfo]) {
        table::with((), |s| s.altars = altars.to_vec());
        let _handled = self.raise::<crate::events::AltarsChanged>(&());
    }

    /// Runs the mod's damage filters on `check` as the loader does before
    /// the game applies damage; returns what they left.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{DamageCheck, Header};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let fall = DamageCheck {
    ///     header: Header::of::<DamageCheck>(),
    ///     cause: DamageCheck::FALL,
    ///     target: 2,
    ///     source: 0,
    ///     attacker: 0,
    ///     amounts: [0.0; 13],
    ///     total: 40.0,
    /// };
    /// assert_eq!(loader.damage(fall), fall, "no filters, no change");
    /// ```
    #[must_use]
    pub fn damage(&self, mut check: crate::abi::DamageCheck) -> crate::abi::DamageCheck {
        let filters = table::with(Vec::new(), |s| s.damage_filters.clone());
        for (callback, user) in filters {
            // SAFETY: the filter the mod registered, with its pointer.
            unsafe { callback(&raw mut check, core::ptr::with_exposed_provenance_mut(user)) };
        }
        check
    }

    /// Runs the mod's knowledge input filters on `request`, as the loader
    /// runs them on what a client asks for, and says whether it goes
    /// through: each filter's answer replaces the verdict so far, which
    /// starts at `request.allowed`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Header, KnowledgeInfo, KnowledgeInput};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let quest = KnowledgeInput {
    ///     header: Header::of::<KnowledgeInput>(),
    ///     player: 2,
    ///     id: 0x51,
    ///     kind: KnowledgeInfo::BOOL,
    ///     scope: KnowledgeInfo::PLAYER,
    ///     reserved: 0,
    ///     value: 0,
    ///     refusal: KnowledgeInput::QUEST,
    ///     allowed: 0,
    /// };
    /// assert!(!loader.knowledge_input(quest), "no filters, the guard's verdict");
    /// ```
    #[must_use]
    pub fn knowledge_input(&self, mut request: crate::abi::KnowledgeInput) -> bool {
        let filters = table::with(Vec::new(), |s| s.knowledge_input_filters.clone());
        for (callback, user) in filters {
            // SAFETY: the filter the mod registered, with its pointer.
            let verdict = unsafe {
                callback(
                    &raw const request,
                    core::ptr::with_exposed_provenance_mut(user),
                )
            };
            request.allowed = u32::from(verdict != 0);
        }
        request.allowed != 0
    }

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
    /// let ty = BuffTypeInfo { header: Header::of::<BuffTypeInfo>(), id: 7, slot: 1, flags: 0, name: 0, duration: 60_000_000_000 };
    /// loader.set_buffs(&[ty], &[2]);
    /// assert_eq!(loader.buffs_of(2), Some(Vec::new()));
    /// ```
    pub fn set_buffs(&self, types: &[crate::abi::BuffTypeInfo], holders: &[u32]) {
        table::with((), |s| {
            s.buff_types = types.to_vec();
            s.buffs = holders.iter().map(|&e| (e, Vec::new())).collect();
        });
    }

    /// The buffs `entity` holds as the mod left them; `None` for one that
    /// holds no buffs.
    ///
    /// # Examples
    ///
    /// See [`Installed::set_buffs`].
    #[must_use]
    pub fn buffs_of(&self, entity: u32) -> Option<Vec<crate::abi::ActiveBuff>> {
        table::with(None, |s| s.buffs.get(&entity).cloned())
    }

    /// Sets the knowledge the fake knows, `(character, value)`; a world
    /// value's character is ignored. A grant is recorded, not applied.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Header, KnowledgeInfo};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let flag = KnowledgeInfo { header: Header::of::<KnowledgeInfo>(), id: 9, kind: KnowledgeInfo::BOOL, scope: KnowledgeInfo::WORLD, reserved: 0, value: 1, flags: 0 };
    /// loader.set_knowledge(&[(0, flag)], &[]);
    /// assert!(loader.grants().is_empty());
    /// ```
    pub fn set_knowledge(&self, values: &[(u32, crate::abi::KnowledgeInfo)], quests: &[u32]) {
        table::with((), |s| {
            s.knowledge = values.to_vec();
            s.quests = quests.to_vec();
        });
    }

    /// The knowledge grants the mod asked for: character, id, kind, value.
    ///
    /// # Examples
    ///
    /// See [`Installed::set_knowledge`].
    #[must_use]
    pub fn grants(&self) -> Vec<(u32, u32, u32, u32)> {
        table::with(Vec::new(), |s| s.grants.clone())
    }

    /// The knowledge revokes the mod asked for: character, id, kind, value.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.revokes().is_empty());
    /// ```
    #[must_use]
    pub fn revokes(&self) -> Vec<(u32, u32, u32, u32)> {
        table::with(Vec::new(), |s| s.revokes.clone())
    }

    /// The knowledge changes the mod asked for accounts: the account's key,
    /// id, kind, value, and whether it takes the knowledge back.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.account_changes().is_empty());
    /// ```
    #[must_use]
    pub fn account_changes(&self) -> Vec<(u64, u32, u32, u32, bool)> {
        table::with(Vec::new(), |s| s.account_changes.clone())
    }

    /// Whether the mod opened altars, and whether it controls their zones.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.altars_opened(), (false, false));
    /// ```
    #[must_use]
    pub fn altars_opened(&self) -> (bool, bool) {
        table::with((false, false), |s| (s.altars_opened, s.zone_control))
    }

    /// Runs the mod's altar zone filters on one altar's vanilla zone of
    /// `height` cells, as the loader would, a `grow` below 0 counting as 0 and
    /// a `height` below one cell keeping the one before; the `grow` and
    /// `height` they leave.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.altar_zone(4096, 2, 60), (0, 60));
    /// ```
    #[must_use]
    pub fn altar_zone(&self, altar: u32, level: u8, height: i64) -> (i64, i64) {
        let filters = table::with(Vec::new(), |s| s.zone_sizing.filters.clone());
        let mut zone = AltarZone {
            header: Header::of::<AltarZone>(),
            altar,
            level,
            reserved: [0; 3],
            grow: 0,
            height,
        };
        for (callback, user) in filters {
            let before = zone;
            // SAFETY: a valid registration; `user` is the mod's pointer.
            unsafe { callback(&raw mut zone, core::ptr::with_exposed_provenance_mut(user)) };
            zone.grow = zone.grow.max(0);
            if zone.height <= 0 {
                zone.height = before.height;
            }
        }
        (zone.grow, zone.height)
    }

    /// How often the mod asked for the altar zones to be sized again.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.zone_resizes(), 0);
    /// ```
    #[must_use]
    pub fn zone_resizes(&self) -> usize {
        table::with(0, |s| s.zone_sizing.resizes)
    }

    /// Asks the mod's zone check filters about a placement the game refused,
    /// as the loader would; whether any accepts it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Bounds;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(!loader.zone_check(&[], Bounds::default(), 0));
    /// ```
    #[must_use]
    pub fn zone_check(&self, zones: &[Bounds], object: Bounds, margin: i64) -> bool {
        let filters = table::with(Vec::new(), |s| s.zone_filters.clone());
        let call = ZoneCheck {
            header: Header::of::<ZoneCheck>(),
            zones: zones.as_ptr(),
            zone_count: zones.len(),
            object,
            margin,
        };
        filters.iter().any(|&(callback, user)| {
            let user = core::ptr::with_exposed_provenance_mut(user);
            // SAFETY: a valid call; `user` is the pointer the mod registered with.
            unsafe { callback(&raw const call, user) != 0 }
        })
    }

    /// Asks the mod's survivor lookup as the game would: survivor
    /// `survivor`, the game's answer `vanilla`, `purpose` one of the
    /// [`SurvivorLookup`] constants with its `entity` and `cell`; `beds`
    /// answers [`Lookup::bed_of`](crate::survivors::Lookup::bed_of). `None`
    /// for the game's own answer.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::SurvivorLookup;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.survivor_lookup(7, 4096, SurvivorLookup::SPAWN, 0, [0; 3], &[]), None);
    /// ```
    #[must_use]
    pub fn survivor_lookup(
        &self,
        survivor: u32,
        vanilla: u32,
        purpose: u32,
        entity: u32,
        cell: [i64; 3],
        beds: &[(u32, u32)],
    ) -> Option<u32> {
        /// # Safety
        ///
        /// - `token` points at the `beds` slice of a running
        ///   `survivor_lookup`, alive and unchanged for the call.
        unsafe extern "system" fn bed_of(token: *mut core::ffi::c_void, entity: u32) -> u32 {
            // SAFETY: the caller guarantees `token` points at the `beds` slice
            // of the running call, an aligned `&[(u32, u32)]` alive for it.
            let beds = unsafe { &*token.cast::<&[(u32, u32)]>() };
            beds.iter()
                .find(|(e, _)| *e == entity)
                .map_or(0, |(_, bed)| *bed)
        }
        let (answer, user) = table::with(None, |s| s.survivor_lookup)?;
        let mut beds = beds;
        let lookup = SurvivorLookup {
            header: Header::of::<SurvivorLookup>(),
            survivor,
            vanilla,
            purpose,
            entity,
            cell,
            bed_of: Some(bed_of),
            token: (&raw mut beds).cast(),
        };
        let mut out = 0;
        let user = core::ptr::with_exposed_provenance_mut(user);
        // SAFETY: a valid call and out pointer; `user` is the mod's pointer.
        let answered = unsafe { answer(&raw const lookup, user, &raw mut out) };
        (answered != 0).then_some(out)
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
    /// let check = BedCheck { header: Header::of::<BedCheck>(), survivor: 7, entity: 1, bed: 2, home: [0; 3], bed_at: [0; 3] };
    /// assert!(loader.survivor_bed_check(&check));
    /// ```
    #[must_use]
    pub fn survivor_bed_check(&self, check: &BedCheck) -> bool {
        let filters = table::with(Vec::new(), |s| s.bed_checks.clone());
        filters.iter().all(|&(callback, user)| {
            let user = core::ptr::with_exposed_provenance_mut(user);
            // SAFETY: a valid check; `user` is the mod's pointer.
            unsafe { callback(check, user) != 0 }
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
    ///     item: 1,
    ///     ammo: 0,
    ///     template: [0; 16],
    ///     target: [0; 3],
    ///     slot: 0,
    ///     pide: 0,
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
        for &(callback, user) in &filters {
            let user = core::ptr::with_exposed_provenance_mut(user);
            // SAFETY: a valid summon; `user` is the mod's pointer.
            match unsafe { callback(summon, user) } {
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
    ///     item: 1,
    ///     ammo: 0,
    ///     template: [0; 16],
    ///     target: [0; 3],
    ///     slot: 0,
    ///     pide: 0,
    ///     meter: [0; 2],
    ///     consumed: 1,
    ///     reserved: [0; 3],
    /// };
    /// assert_eq!(loader.summoned(&summon), 0);
    /// ```
    #[must_use]
    pub fn summoned(&self, summon: &Summon) -> usize {
        let handlers = table::with(Vec::new(), |s| s.summoned.clone());
        for &(callback, user) in &handlers {
            let user = core::ptr::with_exposed_provenance_mut(user);
            // SAFETY: a valid summon; `user` is the mod's pointer.
            unsafe { callback(summon, user) };
        }
        handlers.len()
    }

    /// Puts `item` on `entity`, as worn equipment the stats count.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ecs::Entity;
    /// use enshwick::item_data::ItemInstance;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.wear(2, ItemInstance { pide: Entity(900), item: 0x5e01 });
    /// ```
    pub fn wear(&self, entity: u32, item: crate::item_data::ItemInstance) {
        table::with((), |s| s.stats.worn.entry(entity).or_default().push(item));
    }

    /// Works out the stat values of every watched entity and runs the mod's
    /// [`on_stats_changed`](crate::Load::on_stats_changed) handlers for each
    /// that changed since the last call, as the loader does after a step;
    /// how many changes it raised.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.settle_stats(), 0);
    /// ```
    #[must_use]
    pub fn settle_stats(&self) -> usize {
        stats::settle()
    }

    /// The modifiers the mod keeps on entities: `(entity, key hash, stat
    /// index, operation, value, flags, duration in ms)`.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.stat_modifiers().is_empty());
    /// ```
    #[must_use]
    pub fn stat_modifiers(&self) -> Vec<(u32, u32, u32, u8, f32, u32, u64)> {
        stats::modifiers()
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
    pub fn summon_items(&self) -> Vec<(u32, u32)> {
        table::with(Vec::new(), |s| s.summon_items.clone())
    }

    /// Asks the mod's survivor menu merge about `statuses`; the byte to show,
    /// or `None` for the game's.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.survivor_status(7, &[1, 3]), None);
    /// ```
    #[must_use]
    pub fn survivor_status(&self, survivor: u32, statuses: &[u8]) -> Option<u8> {
        let (callback, user) = table::with(None, |s| s.survivor_status)?;
        let user = core::ptr::with_exposed_provenance_mut(user);
        // SAFETY: valid statuses for the call; `user` is the mod's pointer.
        let answer = unsafe { callback(survivor, statuses.as_ptr(), statuses.len(), user) };
        (answer & crate::abi::SURVIVOR_STATUS_SET != 0).then(|| answer.to_le_bytes()[0])
    }

    /// Asks the mod's interaction filters about `call` as the loader would;
    /// whether every one allows it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Header, Interaction};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let call = Interaction {
    ///     header: Header::of::<Interaction>(),
    ///     kind: Interaction::OPEN,
    ///     player: 1,
    ///     target: 2,
    ///     item: 0,
    ///     known: 0,
    ///     reserved: [0; 3],
    ///     position: [0.0; 3],
    /// };
    /// assert!(loader.interaction(&call));
    /// ```
    #[must_use]
    pub fn interaction(&self, call: &Interaction) -> bool {
        let filters = table::with(Vec::new(), |s| s.interaction_filters.clone());
        filters.iter().all(|&(callback, user)| {
            let user = core::ptr::with_exposed_provenance_mut(user);
            // SAFETY: a valid call; `user` is the mod's pointer.
            unsafe { callback(call, user) != 0 }
        })
    }

    /// Asks the mod's skill overrides about `check` as the loader would: the
    /// first answer for the node, [`SkillCheck::VANILLA`] for none, else
    /// [`SkillCheck::UNLOCKED`] or [`SkillCheck::LOCKED`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Header, SkillCheck};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let check = SkillCheck { header: Header::of::<SkillCheck>(), node: 7, gate: SkillCheck::OTHER, unlocked: 0, reserved: [0; 3] };
    /// assert_eq!(loader.skill(&check), SkillCheck::VANILLA);
    /// ```
    #[must_use]
    pub fn skill(&self, check: &SkillCheck) -> u8 {
        let skills = table::with(Vec::new(), |s| s.skills.clone());
        skills
            .iter()
            .filter(|(node, _, _)| *node == check.node)
            .map(|&(_, callback, user)| {
                let user = core::ptr::with_exposed_provenance_mut(user);
                // SAFETY: a valid check; `user` is the mod's pointer.
                unsafe { callback(check, user) }
            })
            .find(|&answer| answer != SkillCheck::VANILLA)
            .unwrap_or(SkillCheck::VANILLA)
    }

    /// Runs the mod's comfort filters on `comfort` as the loader would, then
    /// its comfort rate ([`Rate::Comfort`](crate::rates::Rate::Comfort));
    /// the comfort registered.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Comfort, Header};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let comfort = Comfort { header: Header::of::<Comfort>(), category: 1, value: 3, position: [0.0; 3] };
    /// assert_eq!(loader.comfort(&comfort), 3);
    /// ```
    #[must_use]
    pub fn comfort(&self, comfort: &Comfort) -> i32 {
        let filters = table::with(Vec::new(), |s| s.comfort_filters.clone());
        let filtered = filters
            .iter()
            .fold(comfort.value, |value, &(callback, user)| {
                let call = Comfort { value, ..*comfort };
                let user = core::ptr::with_exposed_provenance_mut(user);
                // SAFETY: a valid registration; `user` is the mod's pointer.
                unsafe { callback(&raw const call, user) }
            });
        let factor = self.rate(crate::rates::Rate::Comfort).unwrap_or(1.0);
        let scaled = crate::rates::scale_whole(i64::from(filtered), factor);
        i32::try_from(scaled).unwrap_or(if scaled < 0 { i32::MIN } else { i32::MAX })
    }

    /// Runs the mod's experience filters on `rows` as the loader would, with
    /// a level table of `cap` levels, level `l` needing `need(l)`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ecs::Entity;
    /// use enshwick::progress::Experience;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let mut rows = [Experience { entity: Entity(1), level: 1, progress: 0, need: 100, gained: 5 }];
    /// loader.experience(&mut rows, 25, |level| level * 100);
    /// assert_eq!(rows[0].gained, 5);
    /// ```
    pub fn experience(
        &self,
        rows: &mut [crate::progress::Experience],
        cap: u32,
        need: fn(u32) -> u32,
    ) {
        /// # Safety
        ///
        /// - `token` points at the `need` function of a running
        ///   `experience`, alive and unchanged for the call.
        unsafe extern "system" fn need_for(token: *mut core::ffi::c_void, level: u32) -> u32 {
            // SAFETY: the caller guarantees `token` points at the `need`
            // function of the running call, an aligned `fn(u32) -> u32`.
            let need = unsafe { *token.cast::<fn(u32) -> u32>() };
            need(level)
        }
        let filters = table::with(Vec::new(), |s| s.experience_filters.clone());
        let mut need = need;
        for &(callback, user) in &filters {
            let mut pass = ExperiencePass {
                header: Header::of::<ExperiencePass>(),
                rows: rows.as_mut_ptr().cast::<ExperienceRow>(),
                row_count: rows.len(),
                level_cap: cap,
                need_for: Some(need_for),
                token: (&raw mut need).cast(),
            };
            let user = core::ptr::with_exposed_provenance_mut(user);
            // SAFETY: a valid pass; `user` is the mod's pointer.
            unsafe { callback(&raw mut pass, user) };
        }
    }

    /// Asks the mod's altar limit about `flame_level`; `None` without one.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.altar_limit(3), None);
    /// ```
    #[must_use]
    pub fn altar_limit(&self, flame_level: u32) -> Option<u8> {
        let (callback, user) = table::with(None, |s| s.altar_limit)?;
        let user = core::ptr::with_exposed_provenance_mut(user);
        // SAFETY: `user` is the mod's pointer.
        Some(unsafe { callback(flame_level, user) })
    }

    /// Whether the mod raised the base limit.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(!loader.base_limit_raised());
    /// ```
    #[must_use]
    pub fn base_limit_raised(&self) -> bool {
        table::with(false, |s| s.base_limit_raised)
    }

    /// Makes the last grouping find `zones` altar zones in `bases` bases.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.set_grouping(17, 16);
    /// ```
    pub fn set_grouping(&self, zones: u32, bases: u32) {
        table::with((), |s| s.grouping = Some((zones, bases)));
    }

    /// Asks the mod's craft filters about `call`, as the server does
    /// before it crafts; whether the craft goes ahead.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{CraftCheck, Header};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let craft = CraftCheck { header: Header::of::<CraftCheck>(), player: 2, station: 0, recipe: 1, amount: 1 };
    /// assert!(loader.craft(&craft));
    /// ```
    #[must_use]
    pub fn craft(&self, call: &crate::abi::CraftCheck) -> bool {
        let filters = table::with(Vec::new(), |s| s.craft_filters.clone());
        filters.iter().all(|&(callback, user)| {
            // SAFETY: a filter the mod registered with its own pointer.
            unsafe { callback(call, core::ptr::with_exposed_provenance_mut(user)) != 0 }
        })
    }

    /// Asks the mod's build filters about `call`, as the cursor check does
    /// after it allowed a placement; whether the placement goes ahead.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{BuildCheck, Header};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let wall = BuildCheck { header: Header::of::<BuildCheck>(), player: 2, item: 7, position: [0; 3] };
    /// assert!(loader.build(&wall));
    /// ```
    #[must_use]
    pub fn build(&self, call: &crate::abi::BuildCheck) -> bool {
        let filters = table::with(Vec::new(), |s| s.build_filters.clone());
        filters.iter().all(|&(callback, user)| {
            // SAFETY: a filter the mod registered with its own pointer.
            unsafe { callback(call, core::ptr::with_exposed_provenance_mut(user)) != 0 }
        })
    }
}
