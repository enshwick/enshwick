//! The game's own controls as players use them: which ones a player holds
//! this step, for every player and on a client for its own.
//!
//! The server reads them in its world for every player, vanilla clients
//! included; a client reads its local player's.
//!
//! Unstable: needs the `unstable` feature and may change in any release.
//!
//! The controls are the game's player input, each a [`GameAction`]; the
//! running build's reflection names each bit.
//!
//! The held controls are what the player's client sent, and any client can
//! send any of them, as fast as it likes. Read them for what the player
//! could do with the keys anyway, such as a stance or a hint; never let
//! them decide something that matters, such as rights, rewards or an admin
//! step. That comes from the server's own state, or from an
//! [action](crate::actions), which the server checks on every run.
//!
//! # Examples
//!
//! ```
//! use std::time::Duration;
//!
//! use enshwick::controls::GameAction;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let input = load.input()?;
//!     let world = load.world()?;
//!     load.on_tick(Duration::from_secs(1), move |_| {
//!         for player in world.players() {
//!             if let Ok(Some(held)) = input.held(player.entity)
//!                 && held.contains(GameAction::Block)
//!             {
//!                 enshwick::info!("{} blocks", player.name);
//!             }
//!         }
//!     })?;
//!     Ok(())
//! }
//! ```

use crate::abi::Status;
use crate::content::EngineFacts as _;
use crate::controls::GameAction;
use crate::ecs::ComponentType;
use crate::engine_ids as ids;
use crate::resources::Registry;
use crate::{Entity, Host, Load, Off, WorldError};

/// The bits of the game's `PlayerDigitalInput`, a `Bitmask64`.
const BITS: usize = 64;

/// The players' input in the server's world, from [`Load::input`]; `Copy`,
/// for any thread, read on the game thread.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug)]
pub struct Input(&'static Resolved);

impl crate::script::Bound for Input {
    const NAME: &'static str = "input";
    const DOC: &'static str = "The game's controls each player holds, read in the server's world, and the local player's on a client.";
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[Self::SCRIPT_HELD];

    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        load.input()
    }
}

/// Where `PlayerInput` keeps the held controls on the running build, and
/// the game action each bit stands for.
#[derive(Debug)]
struct Resolved {
    host: Host,
    ty: ComponentType,
    digital_at: usize,
    /// Each bit's action, `None` for a bit no constant of the build names.
    actions: [Option<GameAction>; BITS],
}

static RESOLVED: crate::state::ModLocal<Resolved> = crate::state::ModLocal::new();

impl Resolved {
    fn resolve(load: &Load<'_>) -> Result<Self, Off> {
        let ty = ComponentType::resolve(load, ids::PLAYER_INPUT)?;
        let digital_at = load.offset(ids::PLAYER_INPUT_DIGITAL)?;
        let size = load.size(ids::PLAYER_INPUT)?;
        if digital_at
            .checked_add(size_of::<u64>())
            .is_none_or(|end| end > size)
        {
            return Err(Off::new(format!(
                "input: the held controls at {digital_at} lie past PlayerInput's {size} bytes"
            )));
        }
        let names = Registry::game()?
            .hashed(load.hash(ids::PLAYER_INPUT_TYPE)?)
            .ok_or_else(|| Off::new("input: the type registry lacks the game's controls"))?;
        let mut actions = [None; BITS];
        for constant in names.constants() {
            if let Some(slot) = usize::try_from(constant.value())
                .ok()
                .and_then(|bit| actions.get_mut(bit))
            {
                *slot = GameAction::ALL
                    .iter()
                    .copied()
                    .find(|action| action.name() == constant.name());
            }
        }
        Ok(Self {
            host: load.host(),
            ty,
            digital_at,
            actions,
        })
    }
}

#[crate::script(held)]
impl Input {
    pub(crate) fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        if let Some(resolved) = RESOLVED.get() {
            return Ok(Self(resolved));
        }
        let resolved = Resolved::resolve(load)?;
        Ok(Self(RESOLVED.get_or_init(|| resolved)))
    }

    /// The game's controls `player` holds this step, as their client last
    /// sent them; `None` for an entity that is no player. Read on the
    /// server's game thread between steps, in the server's world, so a
    /// vanilla client's player reads the same. A script gets the list of
    /// the held actions' game names, as a control's `link` names them.
    ///
    /// # Errors
    ///
    /// [`WorldError::WrongThread`] off the server's game thread or inside a
    /// step; [`WorldError::Missing`] before a world runs.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[script]
    pub fn held(self, player: Entity) -> Result<Option<InputMask>, WorldError> {
        let host = self.0.host;
        WorldError::from_status((host.api().world_quiet)(host.handle()))?;
        let mut out: *const core::ffi::c_void = core::ptr::null();
        let status = (host.api().component)(host.handle(), player, self.0.ty.hash(), &raw mut out);
        if status == Status::MISSING {
            return Ok(None);
        }
        WorldError::from_status(status)?;
        if out.is_null() {
            return Ok(None);
        }
        // SAFETY: `out` is the player's live, non-null PlayerInput, read
        // right after `world_quiet` answered that this is the game thread at
        // a quiet moment, so no system writes it now; `resolve` checked that
        // the u64 at `digital_at` lies inside the type's size.
        let bits = unsafe { crate::raw::read_at::<u64>(out.cast(), self.0.digital_at) };
        Ok(Some(InputMask {
            bits,
            actions: &self.0.actions,
        }))
    }

    /// The game's controls the local player holds, as this client's
    /// interface read them in its last step; `None` on a server, outside a
    /// game and while no step ran for a second, as in the pause menu. Only
    /// the gameplay controls: the client-only ones, such as the menu and
    /// map keys, are not in it. Read on any thread.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::controls::GameAction;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let input = load.input()?;
    ///     load.on_frame(move |frame| {
    ///         if input.local_held().is_some_and(|held| held.contains(GameAction::Block)) {
    ///             frame.label("Blocking");
    ///         }
    ///     })
    /// }
    /// ```
    #[script]
    #[must_use]
    pub fn local_held(self) -> Option<InputMask> {
        let host = self.0.host;
        let mut bits = 0u64;
        ((host.api().local_held)(host.handle(), &raw mut bits) == Status::OK).then_some(InputMask {
            bits,
            actions: &self.0.actions,
        })
    }
}

/// The game's controls a player holds, from [`Input::held`].
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputMask {
    bits: u64,
    actions: &'static [Option<GameAction>; BITS],
}

impl InputMask {
    /// Whether `action` is held.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn contains(&self, action: GameAction) -> bool {
        self.iter().any(|held| held == action)
    }

    /// The held actions the running build names, in bit order.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::InputMask;
    ///
    /// fn count(held: InputMask) -> usize {
    ///     held.iter().count()
    /// }
    /// ```
    pub fn iter(&self) -> impl Iterator<Item = GameAction> + use<> {
        let bits = self.bits;
        self.actions
            .iter()
            .enumerate()
            .filter(move |(bit, _)| bits & (1 << bit) != 0)
            .filter_map(|(_, action)| *action)
    }

    /// The raw bits, the game's `Bitmask64` as the client sent it, bits no
    /// constant names included.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::InputMask;
    ///
    /// fn idle(held: InputMask) -> bool {
    ///     held.bits() == 0
    /// }
    /// ```
    #[must_use]
    pub const fn bits(&self) -> u64 {
        self.bits
    }
}

impl crate::script::Typed for InputMask {
    const TY: &'static crate::script::Ty = <Vec<String> as crate::script::Typed>::TY;
}

impl crate::script::IntoScript for InputMask {
    fn into_value(self) -> crate::script::Value {
        crate::script::Value::List(
            self.iter()
                .map(|action| crate::script::Value::String(action.name().to_owned()))
                .collect(),
        )
    }
}

impl Load<'_> {
    /// The players' held game controls in the game server, dedicated or
    /// hosted by the client.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// [`Off`] when the game build lacks the input reads.
    ///
    /// # Examples
    ///
    /// See [`Input`].
    pub fn input(&mut self) -> Result<Input, Off> {
        Input::resolve(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mask_names_only_its_set_bits_the_build_names() {
        static ACTIONS: [Option<GameAction>; BITS] = {
            let mut actions = [None; BITS];
            actions[0] = Some(GameAction::MainhandAction);
            actions[2] = Some(GameAction::MainhandActionHold);
            actions
        };
        let held = InputMask {
            bits: 0b1110,
            actions: &ACTIONS,
        };
        assert!(held.contains(GameAction::MainhandActionHold));
        assert!(!held.contains(GameAction::MainhandAction));
        assert_eq!(held.iter().count(), 1, "bits 1 and 3 have no name");
        assert_eq!(held.bits(), 0b1110);
    }
}
