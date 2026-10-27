//! Vetoes: a mod's say before the game crafts or builds. Interactions with
//! entities, from opening a chest to dismantling a piece, have their own
//! filter in [`interactions`](crate::interactions).
//!
//! A craft filter runs in the server's world on its game thread, before the
//! crafting system reads the player's request, where world reads work; a
//! refused request makes nothing and takes nothing. A build filter runs
//! after the game's cursor check allowed a placement, on the cursor
//! systems' worker threads of both sides: the server's check decides, and a
//! client that runs the mod too shows the placement as not allowed, as the
//! game shows one without building permission.
//!
//! # Examples
//!
//! ```
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     const FORBIDDEN_RECIPE: u32 = 0x51;
//!     load.on_craft(|craft| craft.recipe != FORBIDDEN_RECIPE)?;
//!     // Nothing is built below sea level.
//!     load.on_build(|placement| placement.position[1] >= 0.0)
//! }
//! ```

use core::ffi::c_void;

use crate::Entity;
use crate::abi::{BuildCheck, CraftCheck};
use crate::{Load, Off};

/// A player's craft request, from [`Load::on_craft`].
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CraftRequest {
    /// The crafting player's character.
    pub player: Entity,
    /// The station, or `None` for crafting by hand.
    pub station: Option<Entity>,
    /// The recipe id.
    pub recipe: u32,
    /// How many.
    pub amount: u32,
}

/// A placement the game allows, from [`Load::on_build`].
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Placement {
    /// The building player's character.
    pub player: Entity,
    /// The item placed: a building block, a prop or another placeable.
    pub item: u32,
    /// Where, in metres.
    pub position: [f64; 3],
}

unsafe extern "system" fn run_craft<F>(call: *const CraftCheck, user: *mut c_void) -> u8
where
    F: Fn(&CraftRequest) -> bool,
{
    // SAFETY: the loader passes a valid call for the duration of this one.
    let Some(call) = (unsafe { call.as_ref() }) else {
        return 1;
    };
    let request = CraftRequest {
        player: Entity(call.player),
        station: (call.station != 0).then_some(Entity(call.station)),
        recipe: call.recipe,
        amount: call.amount,
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let filter = unsafe { &*user.cast::<F>() };
    u8::from(crate::host::guard(
        "panic in a craft filter",
        || true,
        || filter(&request),
    ))
}

unsafe extern "system" fn run_build<F>(call: *const BuildCheck, user: *mut c_void) -> u8
where
    F: Fn(&Placement) -> bool,
{
    // SAFETY: the loader passes a valid call for the duration of this one.
    let Some(call) = (unsafe { call.as_ref() }) else {
        return 1;
    };
    let placement = Placement {
        player: Entity(call.player),
        item: call.item,
        position: crate::ecs::metres(call.position),
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let filter = unsafe { &*user.cast::<F>() };
    u8::from(crate::host::guard(
        "panic in a build filter",
        || true,
        || filter(&placement),
    ))
}

impl Load<'_> {
    /// Runs `filter` on each player's craft request in the server's world,
    /// before the crafting system reads it; returning `false` refuses it,
    /// and nothing is made or taken. It runs on the game thread, where
    /// [`World`](crate::world::World) reads work, in the dedicated server or
    /// a hosting client.
    ///
    /// # Errors
    ///
    /// [`Off`] after init or on a build without the craft event.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn on_craft<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&CraftRequest) -> bool + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::hand_over(
            filter,
            |user| (host.api().craft_filter)(host.handle(), run_craft::<F>, user),
            "craft filter",
        )
    }

    /// Runs `filter` after the game's cursor check allows a placement;
    /// returning `false` refuses it as the game refuses a placement without
    /// building permission. It runs on both sides, on the cursor systems'
    /// worker threads: the server's answer decides, and a client running the
    /// mod shows the cursor as not allowed. The cursor check runs on every
    /// frame a player aims to build, so `filter` must not allocate, lock or
    /// log, and gives the same answer on both sides.
    ///
    /// # Errors
    ///
    /// [`Off`] after init or on a build without the cursor check.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn on_build<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Placement) -> bool + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::hand_over(
            filter,
            |user| (host.api().build_filter)(host.handle(), run_build::<F>, user),
            "build filter",
        )
    }
}
