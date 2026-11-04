//! Controls: keys a mod declares for its players, and game actions a
//! control follows.
//!
//! Unstable: needs the `unstable` feature and may change in any release.
//!
//! A mod declares each control in init with a name, a label and a default
//! key. The client reads the key only while its player is in the world with
//! no menu, chat or inventory open, never takes it from the game, and calls
//! the mod's handler. A control [linked](Control::link) to a game action
//! fires on the server whenever that player's game action fires, whatever
//! key or button the player bound it to, also for a player without
//! Enshwick. Players rebind controls on the Mods screen; their keys live in
//! `mod-controls.toml` in the game's folder. What changes the world goes to
//! the server as a player action, with [`Actions::run`](crate::actions::Actions::run).
//!
//! # Examples
//!
//! ```
//! use enshwick::controls::{Control, GameAction, Key, Phase};
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let horn = Control::new("horn", "Sound the horn").key(Key::parse("H")?);
//!     load.control(horn, |fired| {
//!         if fired.phase == Phase::Press {
//!             // Play the horn on this client.
//!         }
//!     })?;
//!     let enter = Control::new("enter", "Enter the car").link(GameAction::ContextualAction);
//!     load.control(enter, |fired| {
//!         if let Some(player) = &fired.player {
//!             // On the server: seat `player` if a car is at hand.
//!             let _ = player;
//!         }
//!     })?;
//!     Ok(())
//! }
//! ```

use core::ffi::c_void;
use core::time::Duration;

use crate::abi::{ControlEvent, ControlSpec, Str, read_headed};
use crate::world::Player;
use crate::{Load, Off};

mod key;

pub use key::{Key, ParseKeyError};

/// The longest control name.
pub const MAX_NAME: usize = 32;

/// How long a key stays down before [`Phase::Hold`] fires, unless the
/// control sets its own with [`Control::hold_for`].
pub const HOLD: Duration = Duration::from_millis(500);

/// What happened to a control.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Phase {
    /// Its key went down.
    Press,
    /// Its key came up.
    Release,
    /// Its key stayed down for the control's hold time.
    Hold,
}

impl Phase {
    const fn bit(self) -> u32 {
        match self {
            Self::Press => ControlSpec::PRESS,
            Self::Release => ControlSpec::RELEASE,
            Self::Hold => ControlSpec::HOLD,
        }
    }

    const fn from_bit(bit: u32) -> Option<Self> {
        match bit {
            ControlSpec::PRESS => Some(Self::Press),
            ControlSpec::RELEASE => Some(Self::Release),
            ControlSpec::HOLD => Some(Self::Hold),
            _ => None,
        }
    }
}

macro_rules! game_actions {
    ($($(#[$doc:meta])* $name:ident = $game:literal),* $(,)?) => {
        /// A game action a control can follow, one per member of the game's
        /// player input.
        ///
        /// The members are those of the reflected `PlayerInputType` (client
        /// 520, server 479). The `Tap`, `Hold` and `Release` forms are the
        /// game's own decisions about the same key.
        ///
        /// # Examples
        ///
        /// ```
        /// use enshwick::controls::GameAction;
        ///
        /// assert_eq!(GameAction::ContextualAction.name(), "ContextualAction");
        /// assert_eq!(GameAction::ContextualActionTap.name(), "ContextualAction_Tap");
        /// ```
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum GameAction {
            $($(#[$doc])* $name),*
        }

        impl GameAction {
            /// Every game action, in the game's order.
            ///
            /// # Examples
            ///
            /// ```
            /// use enshwick::controls::GameAction;
            ///
            /// assert!(GameAction::ALL.contains(&GameAction::Jump));
            /// ```
            pub const ALL: &[Self] = &[$(Self::$name),*];

            /// The game's own name of the action, as its reflection gives it.
            ///
            /// # Examples
            ///
            /// See [`GameAction`].
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$name => $game),*
                }
            }
        }
    };
}

game_actions! {
    /// `MainhandAction`.
    MainhandAction = "MainhandAction",
    /// `MainhandAction_Tap`.
    MainhandActionTap = "MainhandAction_Tap",
    /// `MainhandAction_Hold`.
    MainhandActionHold = "MainhandAction_Hold",
    /// `MainhandAction_Release`.
    MainhandActionRelease = "MainhandAction_Release",
    /// `SecondaryBuildingAction`.
    SecondaryBuildingAction = "SecondaryBuildingAction",
    /// `ContextualAction`: the interact key, E by default.
    ContextualAction = "ContextualAction",
    /// `ContextualAction_Tap`.
    ContextualActionTap = "ContextualAction_Tap",
    /// `ContextualAction_Hold`.
    ContextualActionHold = "ContextualAction_Hold",
    /// `AreaLoot`.
    AreaLoot = "AreaLoot",
    /// `SpecialAbility`.
    SpecialAbility = "SpecialAbility",
    /// `SpecialAbility_Tap`.
    SpecialAbilityTap = "SpecialAbility_Tap",
    /// `SpecialAbility_Hold`.
    SpecialAbilityHold = "SpecialAbility_Hold",
    /// `DropItem`.
    DropItem = "DropItem",
    /// `CancelAction`.
    CancelAction = "CancelAction",
    /// `QuickUse0`.
    QuickUse0 = "QuickUse0",
    /// `QuickUse1`.
    QuickUse1 = "QuickUse1",
    /// `QuickUse2`.
    QuickUse2 = "QuickUse2",
    /// `QuickUse3`.
    QuickUse3 = "QuickUse3",
    /// `QuickUse4`.
    QuickUse4 = "QuickUse4",
    /// `QuickUse5`.
    QuickUse5 = "QuickUse5",
    /// `QuickUse6`.
    QuickUse6 = "QuickUse6",
    /// `QuickUse7`.
    QuickUse7 = "QuickUse7",
    /// `QuickUse0_Release`.
    QuickUse0Release = "QuickUse0_Release",
    /// `QuickUse1_Release`.
    QuickUse1Release = "QuickUse1_Release",
    /// `QuickUse2_Release`.
    QuickUse2Release = "QuickUse2_Release",
    /// `QuickUse3_Release`.
    QuickUse3Release = "QuickUse3_Release",
    /// `QuickUse4_Release`.
    QuickUse4Release = "QuickUse4_Release",
    /// `QuickUse5_Release`.
    QuickUse5Release = "QuickUse5_Release",
    /// `QuickUse6_Release`.
    QuickUse6Release = "QuickUse6_Release",
    /// `QuickUse7_Release`.
    QuickUse7Release = "QuickUse7_Release",
    /// `Block`.
    Block = "Block",
    /// `Evade`.
    Evade = "Evade",
    /// `Jump`.
    Jump = "Jump",
    /// `HeadDive`.
    HeadDive = "HeadDive",
    /// `SwimStartDiving`.
    SwimStartDiving = "SwimStartDiving",
    /// `DiveAscend`.
    DiveAscend = "DiveAscend",
    /// `DiveDescend`.
    DiveDescend = "DiveDescend",
    /// `Glider`.
    Glider = "Glider",
    /// `Respawn`.
    Respawn = "Respawn",
    /// `BuildingModeFly`.
    BuildingModeFly = "BuildingModeFly",
    /// `BuildingUndo`.
    BuildingUndo = "BuildingUndo",
    /// `BuildingBlueprintSnapping`.
    BuildingBlueprintSnapping = "BuildingBlueprintSnapping",
    /// `BuildingVoxelGridSnapping`.
    BuildingVoxelGridSnapping = "BuildingVoxelGridSnapping",
    /// `LockCombatTarget`.
    LockCombatTarget = "LockCombatTarget",
    /// `CombatTargetSelect1dLeft`.
    CombatTargetSelect1dLeft = "CombatTargetSelect1dLeft",
    /// `CombatTargetSelect1dRight`.
    CombatTargetSelect1dRight = "CombatTargetSelect1dRight",
    /// `CombatTargetSelect2dLeft`.
    CombatTargetSelect2dLeft = "CombatTargetSelect2dLeft",
    /// `CombatTargetSelect2dRight`.
    CombatTargetSelect2dRight = "CombatTargetSelect2dRight",
    /// `CombatTargetSelect2dUp`.
    CombatTargetSelect2dUp = "CombatTargetSelect2dUp",
    /// `CombatTargetSelect2dDown`.
    CombatTargetSelect2dDown = "CombatTargetSelect2dDown",
    /// `RangedCombatArm`.
    RangedCombatArm = "RangedCombatArm",
    /// `RangedCombatShoot`.
    RangedCombatShoot = "RangedCombatShoot",
    /// `RangedCombatShoot_Tap`.
    RangedCombatShootTap = "RangedCombatShoot_Tap",
    /// `Sprint`.
    Sprint = "Sprint",
    /// `Walk`.
    Walk = "Walk",
    /// `Sneak`.
    Sneak = "Sneak",
}

/// A control as a mod declares it: hand it to [`Load::control`] with its
/// handler.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Debug)]
pub struct Control {
    name: String,
    label: String,
    keys: [Option<Key>; 2],
    link: Option<GameAction>,
    fires: u32,
    hold: Duration,
}

impl Control {
    /// A control named `name`, 1 to 32 of `[a-z0-9-]`, which the controls
    /// page shows as `label`: a text in the player's language, as the mod's
    /// [`Texts`](crate::i18n::Texts) give it. It fires on [`Phase::Press`]
    /// and has no key until [`Control::key`] gives one.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn new(name: &str, label: &str) -> Self {
        Self {
            name: name.to_owned(),
            label: label.to_owned(),
            keys: [None, None],
            link: None,
            fires: ControlSpec::PRESS,
            hold: HOLD,
        }
    }

    /// The default key, which the player may change.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn key(mut self, key: Key) -> Self {
        self.keys[0] = Some(key);
        self
    }

    /// A second default key that fires the control too.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::controls::{Control, Key};
    ///
    /// let horn = Control::new("horn", "Horn").key(Key::parse("H")?).second(Key::parse("Mouse4")?);
    /// # Ok::<(), enshwick::controls::ParseKeyError>(())
    /// ```
    #[must_use]
    pub const fn second(mut self, key: Key) -> Self {
        self.keys[1] = Some(key);
        self
    }

    /// Fires the control on the server whenever the player's game action
    /// `action` fires: the game's own decision, so it follows the player's
    /// binding, a gamepad's too. The handler then gets the player in
    /// [`Fired::player`].
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn link(mut self, action: GameAction) -> Self {
        self.link = Some(action);
        self
    }

    /// The phases the control fires on, in place of [`Phase::Press`] alone.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::controls::{Control, Phase};
    ///
    /// let throttle = Control::new("throttle", "Throttle").fires(&[Phase::Press, Phase::Release]);
    /// ```
    #[must_use]
    pub fn fires(mut self, phases: &[Phase]) -> Self {
        self.fires = phases.iter().fold(0, |bits, phase| bits | phase.bit());
        self
    }

    /// How long the key stays down before [`Phase::Hold`] fires, in place of
    /// [`HOLD`]; a key held down fires it once, never from key repeat.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::controls::{Control, Phase};
    ///
    /// let charge = Control::new("charge", "Charge")
    ///     .fires(&[Phase::Hold])
    ///     .hold_for(core::time::Duration::from_secs(1));
    /// ```
    #[must_use]
    pub const fn hold_for(mut self, hold: Duration) -> Self {
        self.hold = hold;
        self
    }
}

/// A control that fired, as its handler gets it.
///
/// # Examples
///
/// See the [module](self).
#[derive(Debug)]
#[non_exhaustive]
pub struct Fired<'a> {
    /// The control's name.
    pub control: &'a str,
    /// What happened to it.
    pub phase: Phase,
    /// On the server, the player whose game action fired a linked control;
    /// `None` on the client, where the control is the local player's.
    pub player: Option<Player>,
}

type Handler = Box<dyn Fn(&Fired<'_>) + Send + Sync>;

/// Hands one control event to its handler.
///
/// # Safety
///
/// - `event` is null or points at a [`ControlEvent`] readable for the size
///   its header states, with no other writer during the call; its `name`
///   is null or points at its `len` bytes, alive for the call.
/// - `user` is the `Box<Handler>` that [`Load::control`] handed over, which
///   the loader keeps for the process.
unsafe extern "system" fn fire(event: *const ControlEvent, user: *mut c_void) {
    // SAFETY: the caller guarantees `event` is null or a `ControlEvent`
    // readable for the size its header states with no concurrent writer,
    // which is the contract of `read_headed`.
    let Some(event) = (unsafe { read_headed(event, size_of::<ControlEvent>()) }) else {
        return;
    };
    let Some(phase) = Phase::from_bit(event.kind) else {
        return;
    };
    // SAFETY: the caller guarantees `user` is the boxed handler that
    // `hand_over` leaked for the process, aligned, alive and only read.
    let handler = unsafe { &*user.cast::<Handler>() };
    // SAFETY: the caller keeps the name's bytes alive and unchanged for the
    // call, and `control` is not used after it returns.
    let control = unsafe { event.name.as_str() }.unwrap_or("");
    let fired = Fired {
        control,
        phase,
        player: (event.server == 1).then(|| Player::from_info(&event.player)),
    };
    crate::host::guard("panic in a control", || (), || handler(&fired));
}

impl Load<'_> {
    /// Declares `control` with its handler, which runs on the client's
    /// interface thread for a key, and on the server's game thread for a
    /// [linked](Control::link) game action.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed name, a name declared before, or when the
    /// loader refuses the control.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[expect(
        clippy::needless_pass_by_value,
        reason = "a declaration hands its builder over, as `Actions::add` does"
    )]
    pub fn control<F>(&mut self, control: Control, on: F) -> Result<(), Off>
    where
        F: Fn(&Fired<'_>) + Send + Sync + 'static,
    {
        declare(self.host(), &control, on)
    }
}

fn declare<F>(host: crate::Host, control: &Control, on: F) -> Result<(), Off>
where
    F: Fn(&Fired<'_>) + Send + Sync + 'static,
{
    if !is_valid_name(&control.name) {
        return Err(Off::new(format!(
            "control {}: names are 1 to 32 of [a-z0-9-]",
            control.name
        )));
    }
    let spec = ControlSpec {
        header: crate::abi::Header::of::<ControlSpec>(),
        name: Str::new(&control.name),
        label: Str::new(&control.label),
        key: control.keys[0].map_or(0, Key::bits),
        second: control.keys[1].map_or(0, Key::bits),
        link: Str::new(control.link.map_or("", GameAction::name)),
        fires: control.fires,
        hold_ms: u32::try_from(control.hold.as_millis()).unwrap_or(u32::MAX),
    };
    let handler: Handler = Box::new(on);
    crate::load::hand_over(
        handler,
        |user| (host.api().control)(host.handle(), &raw const spec, fire, user),
        &format!("control {}", control.name),
    )
}

/// The mod's controls as a script declares them: the `controls` namespace.
/// A script runs no player actions, so a script's control acts on the
/// side it fires on.
///
/// # Examples
///
/// ```rust,standalone_crate
/// let names: Vec<&str> = enshwick::script::NAMESPACES.iter().map(|n| n.name()).collect();
/// assert!(names.contains(&"controls"));
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Controls {
    host: crate::Host,
}

#[crate::script(declarations)]
impl Controls {
    /// Declares the control `name`, shown as `label`, on `key`, a key name
    /// such as `Shift+E` or empty for none, following the game action
    /// `link` by its game name, such as `ContextualAction`, or empty. It
    /// fires on `phases`, of `press`, `release` and `hold`, or on press
    /// when empty, and holds after `hold_ms`; `on` gets the phase and, on
    /// the server, the player whose game action fired. Allowed while the
    /// mod initialises.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed name, key, game action or phase, a name
    /// declared before, and when the loader refuses the control.
    #[script]
    #[expect(
        clippy::too_many_arguments,
        clippy::needless_pass_by_value,
        reason = "a script declares a control in one call, as Lua takes no builders, and hands \
                  its list over"
    )]
    pub fn declare(
        self,
        name: &str,
        label: &str,
        key: &str,
        link: &str,
        phases: Vec<String>,
        hold_ms: u32,
        on: impl Fn(String, Option<Player>) + Send + Sync + 'static,
    ) -> Result<(), Off> {
        let mut control = Control::new(name, label).hold_for(Duration::from_millis(hold_ms.into()));
        if !key.is_empty() {
            control = control.key(Key::parse(key)?);
        }
        if !link.is_empty() {
            let action = GameAction::ALL
                .iter()
                .find(|a| a.name() == link)
                .ok_or_else(|| Off::new(format!("control {name}: no game action {link}")))?;
            control = control.link(*action);
        }
        if !phases.is_empty() {
            let mut picked = Vec::with_capacity(phases.len());
            for phase in &phases {
                picked.push(match phase.as_str() {
                    "press" => Phase::Press,
                    "release" => Phase::Release,
                    "hold" => Phase::Hold,
                    other => return Err(Off::new(format!("control {name}: no phase {other}"))),
                });
            }
            control = control.fires(&picked);
        }
        declare(self.host, &control, move |fired| {
            let phase = match fired.phase {
                Phase::Press => "press",
                Phase::Release => "release",
                Phase::Hold => "hold",
            };
            on(phase.to_owned(), fired.player.clone());
        })
    }
}

impl crate::script::Bound for Controls {
    const NAME: &'static str = "controls";
    const DOC: &'static str =
        "Keys the mod declares for its players, and game actions they follow.";
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[Self::SCRIPT_DECLARATIONS];

    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(Self { host: load.host() })
    }
}

/// One control's key over time: what each look at it fires. Shared by the
/// loader's key reader and the fake host, so both fire alike.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tracker {
    /// When the key went down, in milliseconds, while it is down.
    down_since: Option<u64>,
    /// Whether [`Phase::Hold`] fired for this press.
    held: bool,
}

impl Tracker {
    /// One look at the key at `now` milliseconds: whether its chord is down
    /// now, and whether it went down and up again since the last look, a
    /// tap shorter than a frame. The phases that fire, as
    /// [`ControlSpec::PRESS`], [`ControlSpec::RELEASE`] and
    /// [`ControlSpec::HOLD`] bits, before the control's own mask.
    #[must_use]
    pub fn look(&mut self, down: bool, tapped: bool, now: u64, hold_ms: u32) -> u32 {
        match (self.down_since, down) {
            (None, true) => {
                *self = Self {
                    down_since: Some(now),
                    held: false,
                };
                ControlSpec::PRESS
            }
            (None, false) if tapped => ControlSpec::PRESS | ControlSpec::RELEASE,
            (Some(since), true)
                if !self.held && now.saturating_sub(since) >= u64::from(hold_ms) =>
            {
                self.held = true;
                ControlSpec::HOLD
            }
            (None, false) | (Some(_), true) => 0,
            (Some(_), false) => {
                *self = Self::default();
                ControlSpec::RELEASE
            }
        }
    }

    /// Whether the key is down, as the last look found it.
    #[must_use]
    pub const fn is_down(&self) -> bool {
        self.down_since.is_some()
    }
}

/// Whether the chord `key` is down: its key is, and exactly its modifiers
/// are, so Shift+E does not fire E.
#[doc(hidden)]
#[must_use]
pub const fn chord_down(key: Key, key_down: bool, modifiers_down: u32) -> bool {
    key_down && modifiers_down == key.modifiers()
}

/// Whether `name` can name a control: 1 to 32 characters of `[a-z0-9-]`.
fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_game_action_carries_the_game_s_own_name() {
        let names: Vec<&str> = GameAction::ALL.iter().map(|a| a.name()).collect();
        assert_eq!(names.len(), 56);
        for expected in [
            "MainhandAction_Release",
            "ContextualAction",
            "ContextualAction_Tap",
            "SpecialAbility_Hold",
            "QuickUse7_Release",
            "RangedCombatShoot_Tap",
            "CombatTargetSelect2dDown",
            "Sneak",
        ] {
            assert!(names.contains(&expected), "{expected}");
        }
        assert!(!names.iter().any(|n| n.contains("__")));
    }

    #[test]
    fn a_held_key_fires_press_once_hold_once_and_release_once() {
        let mut key = Tracker::default();
        let looks: Vec<u32> = [
            (true, 0),
            (true, 100),
            (true, 600),
            (true, 2000),
            (false, 2100),
        ]
        .into_iter()
        .map(|(down, at)| key.look(down, false, at, 500))
        .collect();
        assert_eq!(
            looks,
            [
                ControlSpec::PRESS,
                0,
                ControlSpec::HOLD,
                0,
                ControlSpec::RELEASE
            ]
        );
        assert!(!key.is_down());
    }

    #[test]
    fn a_tap_between_two_looks_still_fires() {
        let mut key = Tracker::default();
        assert_eq!(
            key.look(false, true, 10, 500),
            ControlSpec::PRESS | ControlSpec::RELEASE
        );
        assert_eq!(key.look(false, false, 20, 500), 0);
        assert_eq!(key.look(true, true, 30, 500), ControlSpec::PRESS);
    }

    #[test]
    fn a_chord_fires_only_with_exactly_its_modifiers() {
        let shift_e = Key::parse("Shift+E").unwrap();
        let e = Key::parse("E").unwrap();
        let shift = shift_e.modifiers();
        let ctrl = Key::parse("Ctrl+E").unwrap().modifiers();
        assert!(chord_down(shift_e, true, shift));
        assert!(!chord_down(shift_e, true, shift | ctrl));
        assert!(!chord_down(shift_e, true, 0));
        assert!(!chord_down(e, true, shift));
        assert!(chord_down(e, true, 0));
        assert!(!chord_down(e, false, 0));
    }

    #[test]
    fn a_control_fires_on_the_phases_it_names() {
        let both = Control::new("x", "X").fires(&[Phase::Press, Phase::Hold]);
        assert_eq!(both.fires, ControlSpec::PRESS | ControlSpec::HOLD);
        assert!(is_valid_name("horn-2"));
        assert!(!is_valid_name("Horn"));
        assert!(!is_valid_name(""));
    }
}
