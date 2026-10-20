//! Players' interactions with entities: a mod's say before the game opens a
//! chest, moves or sorts its items, lends its stock to crafting, lets a piece
//! be dismantled, or flips its Quick Stash or upgrade switch.
//!
//! The platform asks the filters in the server's world only, on the thread of
//! the system that checks the interaction, and turns a refusal into the
//! game's own refusal, so nothing moves and nothing is lost.
//!
//! # Examples
//!
//! ```
//! use enshwick_loader_api::interactions::Kind;
//!
//! fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
//!     const LOCKED_CHEST: u32 = 0x5e01;
//!     // Nobody opens or empties a locked chest.
//!     load.on_interaction(|i| !(i.item == Some(LOCKED_CHEST) && matches!(i.kind, Kind::Open | Kind::Move)))
//! }
//! ```

use core::ffi::c_void;

use crate::abi::{HostApi, Interaction as RawInteraction};
use crate::ecs::Entity;
use crate::{Load, Off};

/// What a player does with the target.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interaction {
    /// What the player does.
    pub kind: Kind,
    /// The player's character.
    pub player: Entity,
    /// The entity acted on.
    pub target: Entity,
    /// The item the target was placed from, when the platform could read it.
    pub item: Option<u32>,
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
    let known = call.known != 0;
    let interaction = Interaction {
        kind,
        player: Entity(call.player),
        target: Entity(call.target),
        item: known.then_some(call.item),
        position: known.then_some(call.position),
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let filter = unsafe { &*user.cast::<F>() };
    let allowed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| filter(&interaction)))
        .unwrap_or_else(|_| {
            if let Some(host) = crate::host() {
                host.fault("panic in an interaction filter");
            }
            true
        });
    u8::from(allowed)
}

impl Load<'_> {
    /// Runs `filter` before the game allows a player's interaction with an
    /// entity; returning `false` refuses it the game's own way. It runs in
    /// the server's world only, in the dedicated server or a hosting client,
    /// on the thread of the system that checks the interaction: keep it quick
    /// and never block.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, on a build that lacks a piece, or with a loader
    /// older than the API.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn on_interaction<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Interaction) -> bool + Send + Sync + 'static,
    {
        let host = self.host();
        let end = core::mem::offset_of!(HostApi, interaction_filter) + size_of::<usize>();
        if !host.api().header.covers(end) {
            return Err(Off::new("interaction filters need a newer loader"));
        }
        let user = Box::into_raw(Box::new(filter));
        let status = (host.api().interaction_filter)(host.handle(), run_filter::<F>, user.cast());
        if status.is_ok() {
            Ok(())
        } else {
            // SAFETY: refused, so the loader keeps no copy of the pointer.
            drop(unsafe { Box::from_raw(user) });
            Err(Off::new(format!(
                "interaction filters refused by the loader (status {})",
                status.0
            )))
        }
    }
}
