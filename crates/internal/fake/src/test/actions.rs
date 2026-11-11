//! The players' actions in the fake: chat, controls, interactions and the
//! vetoes on crafting and building.

use enshwick_core::__fake::ids;
use enshwick_core::abi::Interaction;

use crate::test::{FakeHost, Installed, table};

pub(crate) mod chat;
pub(crate) mod commands;
#[cfg(feature = "unstable")]
pub(crate) mod controls;

impl FakeHost {
    /// Each player's `PlayerInput` with the game controls held, and the
    /// reflected enum of controls as the type registry holds it, with
    /// `MainhandAction` at bit 0, `ContextualAction` at 5 and `Block` at 30;
    /// what `Input::held` reads, with the `unstable` feature.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::EngineFacts;
    /// use enshwick::test::facts;
    ///
    /// let host = enshwick::test::FakeHost::new().player_input();
    /// let held = host.offset(facts::PLAYER_INPUT_DIGITAL.id())?;
    /// assert!(held + 8 <= host.size(facts::PLAYER_INPUT.id())?);
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    #[must_use]
    pub fn player_input(self) -> Self {
        use enshwick_core::resources::{Constant, Kind, Type};
        let controls =
            Type::new("demo::ecs::PlayerInputType", Kind::Enum, 1, 1).with_constants(vec![
                Constant::new("MainhandAction", 0),
                Constant::new("ContextualAction", 5),
                Constant::new("Block", 30),
            ]);
        let hash = controls.qualified_hash();
        self.types([controls])
            .preset(
                &[ids::PLAYER_INPUT],
                &[(ids::PLAYER_INPUT, 1536)],
                &[(ids::PLAYER_INPUT_DIGITAL, 936)],
            )
            .type_hash(ids::PLAYER_INPUT_TYPE, hash)
    }

    /// Each player's `SelectedTargets`, sixteen 136-byte targets from byte
    /// 0 with the entity at byte 100 of each, and the reflected enum of
    /// targets with `Interaction` at 2; what `Targets::target` reads, with
    /// the `unstable` feature.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::EngineFacts;
    /// use enshwick::test::facts;
    ///
    /// let host = enshwick::test::FakeHost::new().interaction_targets();
    /// assert_eq!(host.size(facts::SELECTED_TARGETS.id())?, 16 * 136 + 8);
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    #[must_use]
    pub fn interaction_targets(self) -> Self {
        use enshwick_core::resources::{Constant, Kind, Type};
        let kinds = Type::new("demo::ecs::TargetType", Kind::Enum, 1, 1).with_constants(vec![
            Constant::new("Combat", 0),
            Constant::new("Teleport", 1),
            Constant::new("Interaction", 2),
        ]);
        let hash = kinds.qualified_hash();
        self.types([kinds])
            .preset(
                &[ids::SELECTED_TARGETS],
                &[
                    (ids::SELECTED_TARGETS, 16 * 136 + 8),
                    (ids::TARGET_INFO, 136),
                ],
                &[
                    (ids::SELECTED_TARGETS_TARGETS, 0),
                    (ids::TARGET_INFO_ENTITY, 100),
                ],
            )
            .type_hash(ids::TARGET_TYPE, hash)
    }
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
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
    ///     player: enshwick::Entity(1),
    ///     target: enshwick::Entity(2),
    ///     item: enshwick::ItemId::new(0),
    ///     known: 0,
    ///     reserved: [0; 3],
    ///     position: [0.0; 3],
    /// };
    /// assert!(loader.interaction(&call));
    /// ```
    #[must_use]
    pub fn interaction(&self, call: &Interaction) -> bool {
        let filters = table::with(Vec::new(), |s| s.interaction_filters.clone());
        filters.iter().all(|filter| {
            // SAFETY: a valid call; `user` is the mod's pointer.
            filter.run(|callback, user| unsafe { callback(call, user) != 0 })
        })
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
    /// let craft = CraftCheck { header: Header::of::<CraftCheck>(), crafting_operator_id: enshwick::Entity(2), crafting_station_id: enshwick::Entity(0), recipe_id: enshwick::RecipeId::new(1), recipe_amount: 1, reserved: 0 };
    /// assert!(loader.craft(&craft));
    /// ```
    #[must_use]
    pub fn craft(&self, call: &enshwick_core::abi::CraftCheck) -> bool {
        let filters = table::with(Vec::new(), |s| s.craft_filters.clone());
        filters.iter().all(|filter| {
            // SAFETY: a filter the mod registered with its own pointer.
            filter.run(|callback, user| unsafe { callback(call, user) != 0 })
        })
    }

    /// Asks the mod's build filters about `call`, as the cursor check does
    /// after it allowed a placement: `Ok` when the placement goes ahead,
    /// else the reason the first refusing filter gave, if any. `call`'s own
    /// `reason` is not used; the fake host gives each filter a slot.
    ///
    /// # Errors
    ///
    /// The refusal, with the reason as the filter gave it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{BuildCheck, Header};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let wall = BuildCheck {
    ///     header: Header::of::<BuildCheck>(),
    ///     player: enshwick::Entity(2),
    ///     item: enshwick::ItemId::new(7),
    ///     position: [0; 3],
    ///     reason: core::ptr::null_mut(),
    /// };
    /// assert_eq!(loader.build(&wall), Ok(()));
    /// ```
    pub fn build(&self, call: &enshwick_core::abi::BuildCheck) -> Result<(), Option<String>> {
        let filters = table::with(Vec::new(), |s| s.build_filters.clone());
        for filter in filters {
            let mut reason = enshwick_core::abi::Str::new("");
            let mut asked = *call;
            asked.reason = &raw mut reason;
            // SAFETY: a filter the mod registered with its own pointer; `asked`
            // and its `reason` slot live on this stack for the call.
            let allowed = filter.run(|callback, user| unsafe { callback(&raw const asked, user) });
            if allowed == 0 {
                // SAFETY: the refusing filter wrote a text it keeps until its
                // next call on this thread, or left the empty one.
                let reason = unsafe { reason.as_str() }.filter(|r| !r.is_empty());
                return Err(reason.map(str::to_owned));
            }
        }
        Ok(())
    }
}
