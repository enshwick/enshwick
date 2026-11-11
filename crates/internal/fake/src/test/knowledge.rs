//! Knowledge in the fake: what the players and the world know, the quests,
//! skills, comfort and experience, with the fake's vanilla knowledge.

use enshwick_core::Entity;
use enshwick_core::abi::{Comfort, ExperiencePass, ExperienceRow, Header, KnowledgeId, SkillCheck};

use crate::test::{Installed, table};

#[cfg(feature = "typed")]
pub(crate) mod vanilla;

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
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
    ///     player: enshwick::Entity(2),
    ///     id: enshwick::knowledge::KnowledgeId::new(0x51),
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
    pub fn knowledge_input(&self, mut request: enshwick_core::abi::KnowledgeInput) -> bool {
        let filters = table::with(Vec::new(), |s| s.knowledge_input_filters.clone());
        for filter in filters {
            // SAFETY: the filter the mod registered, with its pointer.
            let verdict =
                filter.run(|callback, user| unsafe { callback(&raw const request, user) });
            request.allowed = u32::from(verdict != 0);
        }
        request.allowed != 0
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
    /// let flag = KnowledgeInfo { header: Header::of::<KnowledgeInfo>(), id: enshwick::knowledge::KnowledgeId::new(9), kind: KnowledgeInfo::BOOL, scope: KnowledgeInfo::WORLD, reserved: 0, value: 1, flags: 0 };
    /// loader.set_knowledge(&[(None, flag)], &[]);
    /// assert!(loader.grants().is_empty());
    /// ```
    pub fn set_knowledge(
        &self,
        values: &[(Option<Entity>, enshwick_core::abi::KnowledgeInfo)],
        quests: &[KnowledgeId],
    ) {
        table::with((), |s| {
            s.knowledge = values.to_vec();
            s.quests = quests.to_vec();
        });
    }

    /// Sets whether each game query holds, `(character or None for the
    /// world, query, holds)`; a query not set is one the game has not
    /// answered, and a character the fake lists no player for is one not
    /// in the game.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::knowledge::GameQuery;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.set_game_queries(&[(None, GameQuery::new(7), true)]);
    /// ```
    #[cfg(feature = "typed")]
    pub fn set_game_queries(
        &self,
        answers: &[(
            Option<Entity>,
            enshwick_core::content::knowledge::GameQuery,
            bool,
        )],
    ) {
        let answers = answers
            .iter()
            .map(|&(holder, query, holds)| (holder, query.get(), holds))
            .collect();
        table::with((), |s| s.game_queries = answers);
    }

    /// The knowledge grants the mod asked for: character, id, kind, value.
    ///
    /// # Examples
    ///
    /// See [`Installed::set_knowledge`].
    #[must_use]
    pub fn grants(&self) -> Vec<(Option<Entity>, KnowledgeId, u32, u32)> {
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
    pub fn revokes(&self) -> Vec<(Option<Entity>, KnowledgeId, u32, u32)> {
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
    pub fn account_changes(&self) -> Vec<(enshwick_core::PlayerKey, KnowledgeId, u32, u32, bool)> {
        table::with(Vec::new(), |s| s.account_changes.clone())
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
    /// let check = SkillCheck { header: Header::of::<SkillCheck>(), node: enshwick::abi::SkillNode::new(7), gate: SkillCheck::OTHER, unlocked: 0, reserved: [0; 3] };
    /// assert_eq!(loader.skill(&check), SkillCheck::VANILLA);
    /// ```
    #[must_use]
    pub fn skill(&self, check: &SkillCheck) -> u8 {
        let skills = table::with(Vec::new(), |s| s.skills.clone());
        skills
            .iter()
            .filter(|(node, _)| *node == check.node)
            .map(|(_, skill)| {
                // SAFETY: a valid check; `user` is the mod's pointer.
                skill.run(|callback, user| unsafe { callback(check, user) })
            })
            .find(|&answer| answer != SkillCheck::VANILLA)
            .unwrap_or(SkillCheck::VANILLA)
    }

    /// Runs the mod's comfort filters on `comfort` as the loader would, then
    /// its comfort rate (`Rate::Comfort`);
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
        let filtered = filters.iter().fold(comfort.value, |value, filter| {
            let call = Comfort { value, ..*comfort };
            // SAFETY: a valid registration; `user` is the mod's pointer.
            filter.run(|callback, user| unsafe { callback(&raw const call, user) })
        });
        let factor = self
            .rate_of(enshwick_core::abi::RateId::COMFORT)
            .unwrap_or(1.0);
        let scaled = enshwick_core::__fake::scale_whole(i64::from(filtered), factor);
        i32::try_from(scaled).unwrap_or(if scaled < 0 { i32::MIN } else { i32::MAX })
    }

    /// Runs the mod's experience filters on `rows` as the loader would, with
    /// a level table of `cap` levels, level `l` needing `need(l)`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::Entity;
    /// use enshwick::progress::Experience;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let mut rows = [Experience { entity: Entity(1), level: 1, progress: 0, need: 100, gained: 5 }];
    /// loader.experience(&mut rows, 25, |level| level * 100);
    /// assert_eq!(rows[0].gained, 5);
    /// ```
    pub fn experience(
        &self,
        rows: &mut [enshwick_core::progress::Experience],
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
        for filter in &filters {
            let mut pass = ExperiencePass {
                header: Header::of::<ExperiencePass>(),
                rows: rows.as_mut_ptr().cast::<ExperienceRow>(),
                row_count: rows.len(),
                level_cap: cap,
                need_for: Some(need_for),
                token: (&raw mut need).cast(),
            };
            // SAFETY: a valid pass; `user` is the mod's pointer.
            filter.run(|callback, user| unsafe { callback(&raw mut pass, user) });
        }
    }
}
