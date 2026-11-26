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
//! game shows one without building permission. A refusal may give a reason:
//! a client that runs the mod shows it under the build cursor, and the
//! server tells the builder once, as a notice, or in chat to a client
//! without Enshwick.
//!
//! # Examples
//!
//! ```
//! use enshwick::vetoes::Verdict;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     const FORBIDDEN_RECIPE: enshwick::RecipeId = enshwick::RecipeId::new(0x51);
//!     load.on_craft(|craft| craft.recipe_id != FORBIDDEN_RECIPE)?;
//!     load.on_build(|placement| {
//!         if placement.position[1] >= 0.0 {
//!             Verdict::Pass
//!         } else {
//!             Verdict::refuse_because("build.below_sea")
//!         }
//!     })
//! }
//! ```

use core::cell::RefCell;
use core::ffi::c_void;
use std::borrow::Cow;

use crate::Off;
use crate::abi::{BuildCheck, CraftCheck, Str};
use crate::game_events::CraftRecipeEvent;
use crate::{Entity, ItemId, PlayerKey};

pub use enshwick_base::vetoes::Verdict;

pub(crate) mod script;

/// A placement the game allows, from [`Load::on_build`](../struct.Load.html#method.on_build). The name is
/// enshwick's: the game's cursor check has none, and a refusal sets the
/// game's `NoPermission` cursor flag.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct Placement {
    /// The building player's character.
    pub player: Entity,
    /// The item placed: a building block, a prop or another placeable.
    pub item: ItemId,
    /// Where, in metres.
    pub position: [f64; 3],
    /// The building player's account, which stays the same when the player
    /// gets a new character, so a filter judges the player without a world
    /// read; `None` where the loader cannot tell: in the prediction of a
    /// client that joined a server, in a hosting client's own prediction
    /// until its server half finds the host's account, and on the server
    /// for a character made in the step under way, until the next step.
    pub account: Option<PlayerKey>,
}

unsafe extern "system" fn run_craft<F>(call: *const CraftCheck, user: *mut c_void) -> u8
where
    F: Fn(&CraftRecipeEvent) -> Verdict,
{
    // SAFETY: the loader passes a valid call for the duration of this one.
    let Some(call) = (unsafe { call.as_ref() }) else {
        return crate::abi::FILTER_PASS;
    };
    let request = enshwick_base::__internal::craft_recipe_event(
        call.crafting_operator_id,
        Entity::from_raw(call.crafting_station_id.get()),
        call.recipe_id,
        call.recipe_amount,
    );
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let filter = unsafe { &*user.cast::<F>() };
    crate::host::verdict(
        crate::host::guard(
            "panic in a craft filter",
            || Verdict::Pass,
            || filter(&request),
        )
        .passes(),
    )
}

thread_local! {
    /// The last refusal's reason, alive until the next build check on this
    /// thread, after the loader copied it.
    static REASON: RefCell<Cow<'static, str>> = const { RefCell::new(Cow::Borrowed("")) };
}

unsafe extern "system" fn run_build<F>(call: *const BuildCheck, user: *mut c_void) -> u8
where
    F: Fn(&Placement) -> Verdict,
{
    // SAFETY: the loader passes a valid call for the duration of this one.
    let Some(call) = (unsafe { call.as_ref() }) else {
        return crate::abi::FILTER_PASS;
    };
    let account = Some(call.account).filter(|&key| key != PlayerKey::default());
    let placement = Placement {
        player: call.character,
        item: call.item,
        position: call.position,
        account,
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let filter = unsafe { &*user.cast::<F>() };
    let verdict = crate::host::guard(
        "panic in a build filter",
        || Verdict::Pass,
        || filter(&placement),
    );
    let Verdict::RefuseBecause(reason) = verdict else {
        return crate::host::verdict(verdict.passes());
    };
    if call.reason.is_null() {
        return crate::abi::FILTER_REFUSE;
    }
    REASON.with_borrow_mut(|kept| {
        *kept = reason;
        // SAFETY: `reason` is non-null (checked before it) and, as
        // `BuildCheck` states, writable for one `Str` during the call; the
        // text lives in this thread's `REASON` until the next check on it,
        // after the loader copied it.
        unsafe { call.reason.write(Str::new(kept)) };
    });
    crate::abi::FILTER_REFUSE
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::on_craft`.
    #[doc(hidden)]
    fn on_craft<F, V>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&CraftRecipeEvent) -> V + Send + Sync + 'static,
        V: Into<Verdict>;

    /// As `Load::on_build`.
    #[doc(hidden)]
    fn on_build<F, V>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Placement) -> V + Send + Sync + 'static,
        V: Into<Verdict>;
}

impl LoadMethods for crate::BaseLoad {
    fn on_craft<F, V>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&CraftRecipeEvent) -> V + Send + Sync + 'static,
        V: Into<Verdict>,
    {
        craft_filter(self, move |request| filter(request).into())
    }

    fn on_build<F, V>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Placement) -> V + Send + Sync + 'static,
        V: Into<Verdict>,
    {
        build_filter(self, move |placement| filter(placement).into())
    }
}

/// Hands `filter` to the loader's craft check, for `Load::on_craft` and its
/// script form.
pub(crate) fn craft_filter<F>(load: &crate::BaseLoad, filter: F) -> Result<(), Off>
where
    F: Fn(&CraftRecipeEvent) -> Verdict + Send + Sync + 'static,
{
    let host = load.host();
    crate::load::hand_over(
        filter,
        |user| (host.api().craft_filter)(host.handle(), Some(run_craft::<F>), user),
        "craft filter",
    )
}

/// Hands `filter` to the loader's build check, for `Load::on_build` and its
/// script form.
pub(crate) fn build_filter<F>(load: &crate::BaseLoad, filter: F) -> Result<(), Off>
where
    F: Fn(&Placement) -> Verdict + Send + Sync + 'static,
{
    let host = load.host();
    crate::load::hand_over(
        filter,
        |user| (host.api().build_filter)(host.handle(), Some(run_build::<F>), user),
        "build filter",
    )
}
