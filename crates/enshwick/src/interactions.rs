//! Players' interactions with entities.
//!
//! A mod's say before the game opens a chest, moves or sorts its items, lends
//! its stock to crafting, lets a piece be dismantled, or flips its Quick Stash
//! or upgrade switch.
//!
//! The platform asks the filters in the server's world only, on the thread of
//! the system that checks the interaction, and turns a refusal into the
//! game's own refusal, so nothing moves and nothing is lost.
//!
//! # Examples
//!
//! ```
//! use enshwick::interactions::Kind;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     const LOCKED_CHEST: enshwick::ItemId = enshwick::ItemId::new(0x5e01);
//!     // Nobody opens or empties a locked chest.
//!     load.on_interaction(|i| !(i.item == Some(LOCKED_CHEST) && matches!(i.kind, Kind::Open | Kind::Move)))
//! }
//! ```

use core::ffi::c_void;

use crate::abi::Interaction as RawInteraction;
#[cfg(feature = "unstable")]
use crate::abi::Status;
#[cfg(feature = "unstable")]
use crate::content::EngineFacts as _;
use crate::{Entity, ItemId};
#[cfg(feature = "unstable")]
use crate::{Host, WorldError, ecs::ComponentType, engine_ids as ids, resources::Registry};
use crate::{Load, Off};

/// The game's name of the target its prompt offers, among its `TargetType`.
#[cfg(feature = "unstable")]
const INTERACTION_TARGET: &str = "Interaction";

/// The players' prompt targets in the server's world, from
/// [`Load::interaction_targets`]; `Copy`, for any thread, read on the game
/// thread.
///
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let targets = load.interaction_targets()?;
///     let world = load.world()?;
///     load.on_tick(Duration::from_secs(1), move |_| {
///         for player in world.players() {
///             if let Ok(Some(target)) = targets.target(player.entity) {
///                 enshwick::info!("{} looks at {target:?}", player.name);
///             }
///         }
///     })?;
///     Ok(())
/// }
/// ```
#[cfg(feature = "unstable")]
#[derive(Clone, Copy, Debug)]
pub struct Targets(&'static TargetsAt);

#[cfg(feature = "unstable")]
impl crate::script::Bound for Targets {
    const NAME: &'static str = "interactions";
    const DOC: &'static str =
        "Players' interactions with entities, and what each player's prompt offers.";
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[Self::SCRIPT_TARGET];

    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        load.interaction_targets()
    }
}

/// Where `SelectedTargets` keeps the entity of the prompt's target on the
/// running build.
#[cfg(feature = "unstable")]
#[derive(Debug)]
struct TargetsAt {
    host: Host,
    ty: ComponentType,
    entity_at: usize,
}

#[cfg(feature = "unstable")]
static TARGETS: crate::state::ModLocal<TargetsAt> = crate::state::ModLocal::new();

#[cfg(feature = "unstable")]
impl TargetsAt {
    fn resolve(load: &Load<'_>) -> Result<Self, Off> {
        let ty = ComponentType::resolve(load, ids::SELECTED_TARGETS)?;
        let slot = Registry::game()?
            .hashed(load.hash(ids::TARGET_TYPE)?)
            .and_then(|kinds| {
                kinds
                    .constants()
                    .iter()
                    .find(|c| c.name() == INTERACTION_TARGET)
                    .map(crate::resources::Constant::value)
            })
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| Off::new("interactions: the type registry lacks the prompt's target"))?;
        let (targets, info, entity) = (
            load.offset(ids::SELECTED_TARGETS_TARGETS)?,
            load.size(ids::TARGET_INFO)?,
            load.offset(ids::TARGET_INFO_ENTITY)?,
        );
        let entity_at = slot
            .checked_mul(info)
            .and_then(|at| at.checked_add(targets))
            .and_then(|at| at.checked_add(entity));
        let size = load.size(ids::SELECTED_TARGETS)?;
        match entity_at {
            Some(at)
                if at
                    .checked_add(size_of::<u32>())
                    .is_some_and(|end| end <= size) =>
            {
                Ok(Self {
                    host: load.host(),
                    ty,
                    entity_at: at,
                })
            }
            _ => Err(Off::new(format!(
                "interactions: the prompt's target lies past SelectedTargets' {size} bytes"
            ))),
        }
    }
}

#[cfg(feature = "unstable")]
#[crate::script(target)]
impl Targets {
    pub(crate) fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        if let Some(at) = TARGETS.get() {
            return Ok(Self(at));
        }
        let at = TargetsAt::resolve(load)?;
        Ok(Self(TARGETS.get_or_init(|| at)))
    }

    /// The entity whose interaction `player`'s prompt offers now; `None`
    /// when it offers none or for an entity that is no player. Read on the
    /// server's game thread between steps, in the server's world, so a
    /// vanilla client's player reads the same.
    ///
    /// # Errors
    ///
    /// [`WorldError::WrongThread`] off the server's game thread or inside a
    /// step; [`WorldError::Missing`] before a world runs.
    ///
    /// # Examples
    ///
    /// See [`Targets`].
    #[script]
    pub fn target(self, player: Entity) -> Result<Option<Entity>, WorldError> {
        let host = self.0.host;
        WorldError::from_status((host.api().world_quiet)(host.handle()))?;
        let mut out: *const c_void = core::ptr::null();
        let status = (host.api().component)(host.handle(), player, self.0.ty.hash(), &raw mut out);
        if status == Status::MISSING {
            return Ok(None);
        }
        WorldError::from_status(status)?;
        if out.is_null() {
            return Ok(None);
        }
        // SAFETY: `out` is the player's live, non-null SelectedTargets, read
        // right after `world_quiet` answered that this is the game thread at
        // a quiet moment, so no system writes it now; `resolve` checked that
        // the u32 at `entity_at` lies inside the type's size.
        let entity = unsafe { crate::raw::read_at::<u32>(out.cast(), self.0.entity_at) };
        Ok((entity != 0).then_some(Entity(entity)))
    }
}

#[cfg(feature = "unstable")]
impl Load<'_> {
    /// The players' prompt targets in the game server, dedicated or hosted
    /// by the client.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// [`Off`] when the game build lacks the target's read.
    ///
    /// # Examples
    ///
    /// See [`Targets`].
    pub fn interaction_targets(&mut self) -> Result<Targets, Off> {
        Targets::resolve(self)
    }
}

/// What a player does with the target.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[script(name = "InteractionKind")]
#[non_exhaustive]
pub enum Kind {
    /// The game offers to open it.
    Open,
    /// Items move out of or into it, or out of a slot it holds.
    Move,
    /// Its inventory is sorted.
    Sort,
    /// It lends its stock to crafting or building nearby; asked without the
    /// target's item or position.
    Stock,
    /// The cursor offers to pick it up or tear it down.
    Dismantle,
    /// Its Quick Stash switch is flipped.
    QuickStash,
    /// It is upgraded into another piece.
    Upgrade,
}

/// An interaction the game is about to allow, from [`Load::on_interaction`].
///
/// The name is enshwick's: the loader hooks the game functions behind it.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
pub struct Interaction {
    /// What the player does.
    pub kind: Kind,
    /// The player's character.
    pub player: Entity,
    /// The entity acted on.
    pub target: Entity,
    /// The item the target was placed from, when the platform could read it.
    pub item: Option<ItemId>,
    /// Where the target stands, in metres, when the platform could read it.
    pub position: Option<[f64; 3]>,
}

unsafe extern "system" fn run_filter<F>(call: *const RawInteraction, user: *mut c_void) -> u8
where
    F: Fn(&Interaction) -> bool,
{
    // SAFETY: the loader passes a valid call for the duration of this one.
    let Some(call) = (unsafe { call.as_ref() }) else {
        return 1;
    };
    let kind = match call.kind {
        RawInteraction::OPEN => Kind::Open,
        RawInteraction::MOVE => Kind::Move,
        RawInteraction::SORT => Kind::Sort,
        RawInteraction::STOCK => Kind::Stock,
        RawInteraction::DISMANTLE => Kind::Dismantle,
        RawInteraction::QUICK_STASH => Kind::QuickStash,
        RawInteraction::UPGRADE => Kind::Upgrade,
        _ => return 1,
    };
    if call.player.is_none() || call.target.is_none() {
        return 1;
    }
    let known = call.known != 0;
    let interaction = Interaction {
        kind,
        player: call.player,
        target: call.target,
        item: known.then_some(call.item),
        position: known.then_some(call.position),
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let filter = unsafe { &*user.cast::<F>() };
    u8::from(crate::host::guard(
        "panic in an interaction filter",
        || true,
        || filter(&interaction),
    ))
}

#[crate::script(interactions)]
impl Load<'_> {
    /// Runs `filter` before the game allows a player's interaction with an
    /// entity; returning `false` refuses it the game's own way. It runs in
    /// the server's world only, in the dedicated server or a hosting client,
    /// on the thread of the system that checks the interaction: keep it quick
    /// and never block.
    ///
    /// The game asks whether to offer an interaction for every entity in
    /// each player's reach on every frame, so `filter` runs many times a
    /// frame: it must not allocate, lock or log. Moves, sorts, stock draws,
    /// dismantling and upgrades ask once per action. An interaction without
    /// a player's character or without a target is allowed unasked.
    ///
    /// # Errors
    ///
    /// [`Off`] after init or on a build that lacks a piece.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[script]
    pub fn on_interaction<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Interaction) -> bool + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::hand_over(
            filter,
            |user| (host.api().interaction_filter)(host.handle(), run_filter::<F>, user),
            "interaction filter",
        )
    }
}
