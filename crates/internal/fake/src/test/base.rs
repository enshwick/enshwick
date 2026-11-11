//! The base of the fake: build facts, hooks, events, settings, points,
//! calls, pacing and steps, which every domain stands on.

use enshwick_core::__fake::ids;

use crate::test::FakeHost;

pub(crate) mod calls;
pub(crate) mod events;
pub mod facts;
pub(crate) mod hooks;
pub(crate) mod pacing;
pub(crate) mod points;
pub(crate) mod settings;
pub(crate) mod ticks;

impl FakeHost {
    /// The hit, death and equipment change game events, with the hit
    /// history and experience credit a death reads its killer from, and
    /// damage bits, an attack category and an equipment slot numbered unlike
    /// the API's own.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::EngineFacts;
    /// use enshwick::test::facts;
    ///
    /// let host = enshwick::test::FakeHost::new().game_events();
    /// let target = host.offset(facts::HIT_EVENT_TARGET_ID.id())?;
    /// assert!(target + 4 <= host.size(facts::HIT_EVENT.id())?);
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    #[must_use]
    pub fn game_events(self) -> Self {
        const HIT: u64 = 64;
        self.preset(
            &[
                ids::HIT_EVENT,
                ids::ENTITY_DIED_EVENT,
                ids::HIT_HISTORY,
                ids::EXPERIENCE_CREDIT,
                ids::HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT,
            ],
            &[
                (ids::HIT_EVENT, 64),
                (ids::ENTITY_DIED_EVENT, 16),
                (ids::HIT_HISTORY, 152),
                (ids::EXPERIENCE_CREDIT, 8),
                (ids::HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT, 16),
            ],
            &[
                (ids::HIT_EVENT_ROOT_SOURCE_ID, 8),
                (ids::HIT_EVENT_TARGET_ID, 12),
                (ids::HIT_EVENT_SOURCE_ID, 16),
                (ids::HIT_EVENT_ROOT_TARGET_ID, 20),
                (ids::HIT_EVENT_WEAPON_PIDE_ID, 24),
                (ids::HIT_HEALTH_CHANGE, 28),
                (ids::HIT_STAMINA_CHANGE, 32),
                (ids::HIT_DAMAGE_TYPES, 36),
                (ids::HIT_EVENT_ATTACK_CATEGORY, 38),
                (ids::HIT_TARGET_FACTION, 39),
                (ids::ENTITY_DIED_EVENT_DEAD_ENTITY_ID, 8),
                (ids::HIT_HISTORY_LAST_INDEX, 0),
                (ids::HIT_HISTORY_ENTRIES, 8),
                (ids::HIT_HISTORY_END, 8 + 2 * HIT),
                (ids::EXPERIENCE_CREDIT_LAST_PLAYER, 4),
                (ids::HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT_EQUIPMENT_SLOT, 12),
                (ids::DAMAGE_TYPE_MAGICAL_FIRE, 0),
                (ids::DAMAGE_TYPE_PHYSICAL_CUT, 5),
                (ids::ATTACK_CATEGORY_MELEE, 2),
                (ids::EQUIPMENT_SLOT_SHIELD, 40),
            ],
        )
    }

    /// Plays a build that lacks `fact`, whichever preset set it, so a test
    /// of what turns off without it starts from the preset.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::EngineFacts;
    /// use enshwick::test::facts;
    ///
    /// let host = enshwick::test::FakeHost::new()
    ///     .world()
    ///     .without_fact(facts::WORLD_TRANSFORM_ORIENTATION);
    /// assert!(host.offset(facts::WORLD_TRANSFORM_ORIENTATION.id()).is_err());
    /// ```
    #[must_use]
    pub fn without_fact(mut self, fact: facts::Fact) -> Self {
        self.type_hashes.remove(fact.id());
        self.type_sizes.remove(fact.id());
        self.constants.remove(fact.id());
        self
    }
}
