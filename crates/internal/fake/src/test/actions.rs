//! The players' actions in the fake: chat, controls, interactions and the
//! vetoes on crafting and building.

use enshwick_core::__fake::ids;
use enshwick_core::abi::Interaction;

use crate::test::{FakeHost, Installed, table};

pub(crate) mod chat;
pub(crate) mod commands;
#[cfg(feature = "unstable")]
pub(crate) mod controls;

/// What the fake's actions answer, as the test set it.
#[derive(Clone, Debug, Default)]
pub(crate) struct ActionsPresets {
    pub(crate) console_answers: rustc_hash::FxHashMap<String, String>,
    /// Other mods' commands: mod id, the alias each asks, and whether it
    /// loads after the mod under test.
    pub(crate) other_commands: Vec<(String, String, bool)>,
    /// The player's keys by control name, in place of the mod's defaults.
    #[cfg(feature = "unstable")]
    pub(crate) bindings: Vec<(String, usize, Option<enshwick_core::controls::Key>)>,
}

impl FakeHost {
    /// Each player's `PlayerInput` with the game controls held, and the
    /// reflected enum of controls as the type registry holds it, with
    /// `MainhandAction` at bit 0, `ContextualAction` at 5 and `Block` at 30;
    /// what `Input::held` reads, with the `unstable` feature. Its
    /// `EquipmentSlot` names `MeleeWeapon`, `RangedWeapon` and `Shield`, the
    /// members an equip command may name.
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
                Constant::new("QuickUse0", 14),
                Constant::new("QuickUse1", 15),
                Constant::new("Block", 30),
            ]);
        let hash = controls.qualified_hash();
        let equipment =
            Type::new("demo::ecs::EquipmentSlot", Kind::Enum, 1, 1).with_constants(vec![
                Constant::new("MeleeWeapon", 0),
                Constant::new("RangedWeapon", 1),
                Constant::new("Shield", 2),
            ]);
        let equipment_hash = equipment.qualified_hash();
        self.types([controls, equipment])
            .preset(
                &[ids::PLAYER_INPUT],
                &[(ids::PLAYER_INPUT, 1536)],
                &[(ids::PLAYER_INPUT_DIGITAL, 936)],
            )
            .type_hash(ids::PLAYER_INPUT_TYPE, hash)
            .type_hash(ids::EQUIPMENT_SLOT, equipment_hash)
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

/// Names the game action `action` on the fake build with the player's
/// `keys`, in the ABI's key form.
pub(crate) fn set_player_keys(s: &mut table::State, action: &str, keys: [u32; 2]) {
    let player = &mut s.actions.bindings.player;
    player.retain(|(name, _)| name != action);
    player.push((action.to_owned(), keys));
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// Names the game action `action`, its reflected name, on the fake
    /// build, with the `first` and `second` key the player bound to it in
    /// the keyboard and mouse bindings; what `Input::binding` reads from
    /// the next draw on, and the input hints draw. An action never named is
    /// one the build lacks, which the loader refuses.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Side;
    /// use enshwick::controls::Key;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// loader.bind_game_action("ContextualAction", Some(Key::parse("E")?), None);
    /// assert_eq!(loader.game_rebind("ContextualAction"), None);
    /// # Ok::<(), enshwick::controls::ParseKeyError>(())
    /// ```
    #[cfg(feature = "unstable")]
    pub fn bind_game_action(
        &self,
        action: &str,
        first: Option<enshwick_core::controls::Key>,
        second: Option<enshwick_core::controls::Key>,
    ) {
        let bits = |key: Option<enshwick_core::controls::Key>| {
            key.map_or(0, enshwick_core::controls::Key::bits)
        };
        table::with((), |s| {
            set_player_keys(s, action, [bits(first), bits(second)]);
        });
    }

    /// The first key the newest mod rebind of the game action `action`
    /// holds, `None` while no mod rebinds it.
    ///
    /// # Examples
    ///
    /// See [`Installed::bind_game_action`].
    #[cfg(feature = "unstable")]
    #[must_use]
    pub fn game_rebind(&self, action: &str) -> Option<enshwick_core::controls::Key> {
        table::with(None, |s| {
            s.actions
                .bindings
                .rebound(action)
                .and_then(enshwick_core::controls::Key::from_bits)
        })
    }

    /// Asks the mod's interaction filters about `call` as the loader would;
    /// whether every one allows it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Interaction;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let call = Interaction::new(Interaction::OPEN, enshwick::Entity::from_engine(1), enshwick::Entity::from_engine(2));
    /// assert!(loader.interaction(&call));
    /// ```
    #[must_use]
    pub fn interaction(&self, call: &Interaction) -> bool {
        let filters = table::with(Vec::new(), |s| s.actions.interaction_filters.clone());
        filters.iter().all(|filter| {
            // SAFETY: a valid call; `user` is the mod's pointer.
            filter.run(|callback, user| unsafe {
                callback(call, user) != enshwick_core::abi::FILTER_REFUSE
            })
        })
    }

    /// The prompt the mod gave the objects placed from `item`, as the
    /// loader keeps it; `None` for an item without one.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.prompt(enshwick::ItemId::new(0x5e01)), None);
    /// ```
    #[must_use]
    pub fn prompt(
        &self,
        item: enshwick_core::ItemId,
    ) -> Option<enshwick_core::abi::InteractionPrompt> {
        table::with(None, |s| {
            s.actions
                .interaction_prompts
                .iter()
                .find(|p| p.item == item)
                .copied()
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
    /// let craft = enshwick::abi::headed!(CraftCheck { crafting_operator_id: enshwick::Entity::from_engine(2), crafting_station_id: enshwick::Entity::from_engine(0), recipe_id: enshwick::RecipeId::new(1), recipe_amount: 1, reserved: 0 });
    /// assert!(loader.craft(&craft));
    /// ```
    #[must_use]
    pub fn craft(&self, call: &enshwick_core::abi::CraftCheck) -> bool {
        let filters = table::with(Vec::new(), |s| s.actions.craft_filters.clone());
        filters.iter().all(|filter| {
            // SAFETY: a filter the mod registered with its own pointer.
            filter.run(|callback, user| unsafe {
                callback(call, user) != enshwick_core::abi::FILTER_REFUSE
            })
        })
    }

    /// Asks the mod's build filters about `call`, as the cursor check does
    /// after it allowed a placement: `Ok` when the placement goes ahead,
    /// else the reason the first refusing filter gave, if any. `call`'s own
    /// `reason` is not used; the fake host gives each filter a slot. A zero
    /// `account` becomes that of the player whose character `call` names,
    /// as the loader fills it for the server's world. A client's own
    /// prediction runs on a character of its own world, which no server
    /// player names: a client that hosts gives it the
    /// [`host_key`](crate::test::FakeHost::host_key), its own player's, and
    /// one that does not host keeps it zero. The filters run as on the game's worker
    /// threads, where world reads answer [`Status::WRONG_THREAD`](enshwick_core::abi::Status::WRONG_THREAD).
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
    /// let wall = enshwick::abi::headed!(BuildCheck {
    ///     character: enshwick::Entity::from_engine(2),
    ///     item: enshwick::ItemId::new(7),
    ///     position: [0.0; 3],
    ///     reason: core::ptr::null_mut(),
    ///     account: enshwick::PlayerKey::default(),
    /// });
    /// assert_eq!(loader.build(&wall), Ok(()));
    /// ```
    pub fn build(&self, call: &enshwick_core::abi::BuildCheck) -> Result<(), Option<String>> {
        // shortcut: the fake has no worlds or steps, so a hosting fake takes any character no player names for its own prediction, and a respawned character's account comes at once, where the loader gives a character made in the current server step none until the next step; model both if a mod test ever needs those gaps.
        let (filters, account) = table::with((Vec::new(), None), |s| {
            let base = &s.host.base;
            let server = base.serving();
            let player = base.players.iter().find(|p| p.entity == call.character);
            (
                s.actions.build_filters.clone(),
                player
                    .filter(|_| server)
                    .map(|p| p.key)
                    .or_else(|| base.own_key()),
            )
        });
        for filter in filters {
            let mut reason = enshwick_core::abi::Str::new("");
            let mut asked = *call;
            asked.reason = &raw mut reason;
            if asked.account == enshwick_core::PlayerKey::default() {
                asked.account = account.unwrap_or_default();
            }
            let allowed = table::on_worker(|| {
                // SAFETY: a filter the mod registered with its own pointer;
                // `asked` and its `reason` slot live on this stack for the call.
                filter.run(|callback, user| unsafe { callback(&raw const asked, user) })
            });
            if allowed == enshwick_core::abi::FILTER_REFUSE {
                // SAFETY: the refusing filter wrote a text it keeps until its
                // next call on this thread, or left the empty one.
                let reason = unsafe { reason.as_str() }.filter(|r| !r.is_empty());
                return Err(reason.map(str::to_owned));
            }
        }
        Ok(())
    }
}
