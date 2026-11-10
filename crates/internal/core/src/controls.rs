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
//! the server as a player action, with [`Actions::run`](crate::actions::Actions::run)
//! or [`Control::runs`]. A mod of side server offers its controls with
//! [`Control::runs`]: the server sends them to the clients that join.
//!
//! # Examples
//!
//! ```
//! # extern crate enshwick_core as enshwick;
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
mod script;

pub use key::{Key, ParseKeyError};
#[doc(hidden)]
pub use script::ControlFns;

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
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
        /// # extern crate enshwick_core as enshwick;
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
            /// # extern crate enshwick_core as enshwick;
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
    action: Option<String>,
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
            action: None,
        }
    }

    /// Runs the mod's [action](crate::actions) `action` on the server, for
    /// the player, each time the control fires on a client: the server checks
    /// who may run it, as for any action, and shows the reply as a notice.
    /// The handler still runs wherever the mod's code does.
    ///
    /// A mod of side server needs it: the server sends its controls to each
    /// client that joins, whose Mods screen lists them under the mod's key bindings
    /// and whose player may rebind them, though the client runs none of the
    /// mod's code. The label is then a key of the mod's texts, shown in each
    /// player's language.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::actions::{Action, Outcome};
    /// use enshwick::controls::{Control, Key};
    /// use enshwick::i18n::Texts;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let actions = load.actions(Texts::default())?;
    ///     actions.add(Action::new("day", "Make it day").admins(), |_| Outcome::done("Morning"))?;
    ///     let day = Control::new("day", "Make it day").key(Key::parse("F8")?).runs("day");
    ///     load.control(day, |_| {})
    /// }
    /// ```
    #[must_use]
    pub fn runs(mut self, action: &str) -> Self {
        self.action = Some(action.to_owned());
        self
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
    /// # extern crate enshwick_core as enshwick;
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

    /// The phases the control fires on, in place of [`Phase::Press`] alone;
    /// an empty list keeps [`Phase::Press`].
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::controls::{Control, Phase};
    ///
    /// let throttle = Control::new("throttle", "Throttle").fires(&[Phase::Press, Phase::Release]);
    /// ```
    #[must_use]
    pub fn fires(mut self, phases: &[Phase]) -> Self {
        if !phases.is_empty() {
            self.fires = phases.iter().fold(0, |bits, phase| bits | phase.bit());
        }
        self
    }

    /// How long the key stays down before [`Phase::Hold`] fires, in place of
    /// [`HOLD`]; a key held down fires it once, never from key repeat.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
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
    /// [linked](Control::link) game action; a client hosting a game may run
    /// both at once.
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

pub(crate) fn declare<F>(host: crate::Host, control: &Control, on: F) -> Result<(), Off>
where
    F: Fn(&Fired<'_>) + Send + Sync + 'static,
{
    if !is_valid_name(&control.name) {
        return Err(Off::new(format!(
            "control {}: names are 1 to 32 of [a-z0-9-]",
            control.name
        )));
    }
    let action = control.action.as_deref().unwrap_or("");
    if control.action.is_some() && !is_valid_name(action) {
        return Err(Off::new(format!(
            "control {}: action {action}: ids are 1 to 32 of [a-z0-9-]",
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
        action: Str::new(action),
    };
    let handler: Handler = Box::new(on);
    crate::load::hand_over(
        handler,
        |user| (host.api().control)(host.handle(), &raw const spec, fire, user),
        &format!("control {}", control.name),
    )
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
    /// One look at the key at `now` milliseconds, whether its chord is down
    /// now: the phases that fire, as [`ControlSpec::PRESS`],
    /// [`ControlSpec::RELEASE`] and [`ControlSpec::HOLD`] bits, before the
    /// control's own mask.
    #[must_use]
    pub fn look(&mut self, down: bool, now: u64, hold_ms: u32) -> u32 {
        match (self.down_since, down) {
            (None, true) => {
                *self = Self {
                    down_since: Some(now),
                    held: false,
                };
                ControlSpec::PRESS
            }
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

/// Fills `out` with the slots of each control on `bound` that are down, a bit
/// per slot, so the loader's key reader and the fake host fire alike. On
/// entry `out` holds the last look's slots. `held(control, slot, key)` gives
/// the modifiers held while that key is down, and `None` while it is up or
/// the control's mod is off, so an off control neither fires nor takes a key.
///
/// A control comes down when one of its keys does with at least its
/// modifiers held, and of the controls on that key only those with the most
/// modifiers do: G fires while Shift is held, Shift+G beats G, and Shift+G
/// and Ctrl+G both fire with Shift and Ctrl held. A held combination that
/// cannot be bound, such as Alt+F4, brings nothing down. A control stays down
/// while the key that brought it down does, whatever modifiers change
/// meanwhile, and no other control on that key comes down until it is let go.
#[doc(hidden)]
pub fn chords(
    bound: &[[u32; 2]],
    mut held: impl FnMut(usize, usize, Key) -> Option<u32>,
    out: &mut Vec<u8>,
) {
    // Bits of `out` above the two slots, used only inside one look.
    const WAS: u8 = 0x80;
    const RISES: u8 = 0b1100;
    // Indexed by key code, which `Key` keeps under 1024.
    let mut taken = [false; 1024];
    let mut best = [0u32; 1024];
    out.resize(bound.len(), 0);
    for (control, keys) in bound.iter().enumerate() {
        let was = out[control];
        let mut now = if was == 0 { 0 } else { WAS };
        for (slot, &bits) in keys.iter().enumerate() {
            if was & 1 << slot != 0
                && let Some(key) = Key::from_bits(bits)
                && held(control, slot, key).is_some()
            {
                now |= 1 << slot;
                taken[key.code() as usize] = true;
            }
        }
        out[control] = now;
    }
    for (control, keys) in bound.iter().enumerate() {
        if out[control] & WAS != 0 {
            continue;
        }
        for (slot, &bits) in keys.iter().enumerate() {
            let Some(key) = Key::from_bits(bits) else {
                continue;
            };
            let code = key.code() as usize;
            if !taken[code]
                && let Some(mods) = held(control, slot, key)
                && key.modifiers() & !mods == 0
                && Key::from_bits(key.code() | mods).is_some()
            {
                out[control] |= 4 << slot;
                best[code] = best[code].max(key.modifiers().count_ones());
            }
        }
    }
    for (control, keys) in bound.iter().enumerate() {
        let rises = out[control] & RISES;
        out[control] &= 0b11;
        for (slot, &bits) in keys.iter().enumerate() {
            if rises & 4 << slot != 0
                && let Some(key) = Key::from_bits(bits)
                && key.modifiers().count_ones() == best[key.code() as usize]
            {
                out[control] |= 1 << slot;
            }
        }
    }
}

#[doc(hidden)]
#[must_use]
pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Whether `fires` names at least one phase and nothing else. Shared by the
/// loader and the fake host, which refuse a control that would never fire.
#[doc(hidden)]
#[must_use]
pub const fn valid_phases(fires: u32) -> bool {
    let all = ControlSpec::PRESS | ControlSpec::RELEASE | ControlSpec::HOLD;
    fires != 0 && fires & !all == 0
}

/// Each phase bit of `kind`, in the order its calls go out: press, hold,
/// release. Shared by the loader and the fake host.
#[doc(hidden)]
pub fn phases(kind: u32) -> impl Iterator<Item = u32> {
    [ControlSpec::PRESS, ControlSpec::HOLD, ControlSpec::RELEASE]
        .into_iter()
        .filter(move |bit| kind & bit != 0)
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
        .map(|(down, at)| {
            let fired = key.look(down, at, 500);
            assert_eq!(key.is_down(), down);
            fired
        })
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

    fn bits(names: &[&str]) -> [u32; 2] {
        let mut keys = [0; 2];
        for (slot, name) in names.iter().enumerate() {
            keys[slot] = Key::parse(name).unwrap().bits();
        }
        keys
    }

    /// The slots of the controls on `bound` that are down when `down` are
    /// held, their modifiers held together as on a keyboard, after a look
    /// that found `was`, with the controls `live` gives on.
    fn look_with(
        bound: &[&[&str]],
        was: &[u8],
        down: &[&str],
        live: impl Fn(usize) -> bool,
    ) -> Vec<u8> {
        let bound: Vec<[u32; 2]> = bound.iter().map(|names| bits(names)).collect();
        // Parsed apart, since a held combination such as Alt+F4 is no `Key`.
        let (mut codes, mut mods) = (Vec::new(), 0);
        for name in down {
            let (with, key) = name.rsplit_once('+').unwrap_or(("", name));
            codes.push(Key::parse(key).unwrap().code());
            if !with.is_empty() {
                mods |= Key::parse(&format!("{with}+G")).unwrap().modifiers();
            }
        }
        let mut out = was.to_vec();
        chords(
            &bound,
            |control, _, key| (live(control) && codes.contains(&key.code())).then_some(mods),
            &mut out,
        );
        out
    }

    fn look(bound: &[&[&str]], was: &[u8], down: &[&str]) -> Vec<u8> {
        look_with(bound, was, down, |_| true)
    }

    #[test]
    fn a_plain_key_fires_while_a_modifier_is_held() {
        assert_eq!(look(&[&["G"]], &[0], &["Shift+G"]), [1]);
        assert_eq!(look(&[&["G"]], &[0], &["Ctrl+Alt+G"]), [1]);
        assert_eq!(look(&[&["Shift+G"]], &[0], &["G"]), [0]);
        assert_eq!(look(&[&["G"]], &[0], &[]), [0]);
    }

    #[test]
    fn the_control_with_the_most_held_modifiers_wins_its_key() {
        let bound: &[&[&str]] = &[&["G"], &["Shift+G"], &["Shift+Ctrl+G"]];
        let up = [0; 3];
        assert_eq!(look(bound, &up, &["G"]), [1, 0, 0]);
        assert_eq!(look(bound, &up, &["Shift+G"]), [0, 1, 0]);
        assert_eq!(look(bound, &up, &["Shift+Ctrl+G"]), [0, 0, 1]);
        assert_eq!(look(bound, &up, &["Ctrl+G"]), [1, 0, 0]);
    }

    #[test]
    fn controls_with_disjoint_modifiers_on_one_key_fire_apart_and_tie_together() {
        let bound: &[&[&str]] = &[&["Shift+G"], &["Ctrl+G"]];
        let up = [0; 2];
        assert_eq!(look(bound, &up, &["Shift+G"]), [1, 0]);
        assert_eq!(look(bound, &up, &["Ctrl+G"]), [0, 1]);
        assert_eq!(look(bound, &up, &["Shift+Ctrl+G"]), [1, 1]);
        assert_eq!(look(bound, &up, &["G"]), [0, 0]);
    }

    #[test]
    fn a_modifier_change_while_the_key_is_held_keeps_the_same_control_down() {
        let bound: &[&[&str]] = &[&["G"], &["Shift+G"]];
        assert_eq!(look(bound, &[1, 0], &["Shift+G"]), [1, 0]);
        assert_eq!(look(bound, &[0, 1], &["G"]), [0, 1]);
        assert_eq!(look(bound, &[1, 0], &[]), [0, 0]);
    }

    #[test]
    fn either_key_of_a_control_brings_it_down() {
        let bound: &[&[&str]] = &[&["F", "G"], &["H"]];
        assert_eq!(look(bound, &[0, 0], &["Shift+G"]), [0b10, 0]);
        assert_eq!(look(bound, &[0b10, 0], &["F", "H"]), [0, 1]);
    }

    #[test]
    fn a_control_of_a_mod_that_is_off_neither_outranks_nor_holds_a_key() {
        let bound: &[&[&str]] = &[&["Shift+G"], &["G"]];
        let first_off = |control| control != 0;
        assert_eq!(look_with(bound, &[0, 0], &["Shift+G"], first_off), [0, 1]);
        assert_eq!(look_with(bound, &[1, 0], &["Shift+G"], first_off), [0, 1]);
    }

    #[test]
    fn a_control_stays_down_only_through_the_key_that_brought_it_down() {
        let bound: &[&[&str]] = &[&["F", "Shift+G"], &["G"]];
        assert_eq!(look(bound, &[0b01, 0], &["F", "G"]), [0b01, 1]);
        assert_eq!(look(bound, &[0b01, 1], &["G"]), [0, 1]);
    }

    #[test]
    fn a_held_combination_that_cannot_be_bound_brings_nothing_down() {
        let bound: &[&[&str]] = &[&["F4"], &["Tab"], &["Ctrl+F4"]];
        let up = [0; 3];
        assert_eq!(look(bound, &up, &["Alt+F4"]), [0, 0, 0]);
        assert_eq!(look(bound, &up, &["F4", "Alt+Tab"]), [0, 0, 0]);
        assert_eq!(look(bound, &up, &["Ctrl+Alt+F4"]), [0, 0, 0]);
        assert_eq!(look(bound, &up, &["Ctrl+F4"]), [0, 0, 1]);
    }

    #[test]
    fn a_control_fires_on_the_phases_it_names() {
        let both = Control::new("x", "X").fires(&[Phase::Press, Phase::Hold, Phase::Press]);
        assert_eq!(both.fires, ControlSpec::PRESS | ControlSpec::HOLD);
        let none = Control::new("x", "X").fires(&[Phase::Release]).fires(&[]);
        assert_eq!(
            none.fires,
            ControlSpec::RELEASE,
            "an empty list keeps the phases"
        );
        assert_eq!(
            Control::new("x", "X").fires(&[]).fires,
            ControlSpec::PRESS,
            "as a script's empty list"
        );
        assert!(valid_phases(ControlSpec::HOLD | ControlSpec::RELEASE));
        assert!(!valid_phases(0), "a control that never fires");
        assert!(!valid_phases(ControlSpec::PRESS | 8));
        assert!(is_valid_name("horn-2"));
        assert!(!is_valid_name("Horn"));
        assert!(!is_valid_name(""));
    }
}
