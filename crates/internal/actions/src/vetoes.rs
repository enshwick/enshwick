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
//! use enshwick::vetoes::BuildVerdict;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     const FORBIDDEN_RECIPE: enshwick::RecipeId = enshwick::RecipeId::new(0x51);
//!     load.on_craft(|craft| craft.recipe_id != FORBIDDEN_RECIPE)?;
//!     load.on_build(|placement| {
//!         if placement.position[1] >= 0.0 {
//!             BuildVerdict::Allow
//!         } else {
//!             BuildVerdict::refuse_because("build.below_sea")
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
use crate::script::{Answer, Error, Ty, Value};
use crate::{Entity, ItemId};

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
}

unsafe extern "system" fn run_craft<F>(call: *const CraftCheck, user: *mut c_void) -> u8
where
    F: Fn(&CraftRecipeEvent) -> bool,
{
    // SAFETY: the loader passes a valid call for the duration of this one.
    let Some(call) = (unsafe { call.as_ref() }) else {
        return 1;
    };
    let request = enshwick_base::__internal::craft_recipe_event(
        call.crafting_operator_id,
        Entity::from_raw(call.crafting_station_id.0),
        call.recipe_id,
        call.recipe_amount,
    );
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let filter = unsafe { &*user.cast::<F>() };
    u8::from(crate::host::guard(
        "panic in a craft filter",
        || true,
        || filter(&request),
    ))
}

/// What a build filter answers for a placement, from [`Load::on_build`](../struct.Load.html#method.on_build).
///
/// From Luau a filter answers `false` to refuse, a string to refuse with
/// that reason, and nil or `true` to allow; any other value refuses, and a
/// script error allows. The name is enshwick's.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BuildVerdict {
    /// The placement goes on as the game's check decided.
    Allow,
    /// The placement is refused, as one without building permission.
    Refuse,
    /// The placement is refused, and the builder reads why: a key of the
    /// mod's texts, shown in the player's language, or plain words, at most
    /// 120 bytes.
    RefuseBecause(Cow<'static, str>),
}

impl BuildVerdict {
    /// A refusal the builder reads `reason` for; a `&'static str` costs no
    /// allocation, as the cursor check needs.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn refuse_because(reason: impl Into<Cow<'static, str>>) -> Self {
        Self::RefuseBecause(reason.into())
    }
}

impl From<bool> for BuildVerdict {
    fn from(allowed: bool) -> Self {
        if allowed { Self::Allow } else { Self::Refuse }
    }
}

impl Answer for BuildVerdict {
    const TY: &'static Ty = &Ty::Data;

    fn from_answer(answer: Result<Value, Error>) -> Self {
        match answer {
            Ok(Value::Nil | Value::Bool(true)) | Err(_) => Self::Allow,
            Ok(Value::String(reason)) => Self::RefuseBecause(reason.into()),
            // A mistyped answer refuses, so the author sees it at once.
            Ok(_) => Self::Refuse,
        }
    }
}

thread_local! {
    /// The last refusal's reason, alive until the next build check on this
    /// thread, after the loader copied it.
    static REASON: RefCell<Cow<'static, str>> = const { RefCell::new(Cow::Borrowed("")) };
}

unsafe extern "system" fn run_build<F>(call: *const BuildCheck, user: *mut c_void) -> u8
where
    F: Fn(&Placement) -> BuildVerdict,
{
    // SAFETY: the loader passes a valid call for the duration of this one.
    let Some(call) = (unsafe { call.as_ref() }) else {
        return 1;
    };
    let placement = Placement {
        player: call.player,
        item: call.item,
        position: crate::ecs::metres(call.position),
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let filter = unsafe { &*user.cast::<F>() };
    let verdict = crate::host::guard(
        "panic in a build filter",
        || BuildVerdict::Allow,
        || filter(&placement),
    );
    let reason = match verdict {
        BuildVerdict::Allow => return 1,
        BuildVerdict::Refuse => return 0,
        BuildVerdict::RefuseBecause(reason) => reason,
    };
    let covered = call
        .header
        .covers(core::mem::offset_of!(BuildCheck, reason) + size_of::<*mut Str>());
    if !covered || call.reason.is_null() {
        return 0;
    }
    REASON.with_borrow_mut(|kept| {
        *kept = reason;
        // SAFETY: `reason` is non-null (checked before it) and, as
        // `BuildCheck` states, writable for one `Str` during the call; the
        // text lives in this thread's `REASON` until the next check on it,
        // after the loader copied it.
        unsafe { call.reason.write(Str::new(kept)) };
    });
    0
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::on_craft`.
    #[doc(hidden)]
    fn on_craft<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&CraftRecipeEvent) -> bool + Send + Sync + 'static;

    /// As `Load::on_build`.
    #[doc(hidden)]
    fn on_build<F, V>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Placement) -> V + Send + Sync + 'static,
        V: Into<BuildVerdict>;
}

impl LoadMethods for crate::BaseLoad {
    fn on_craft<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&CraftRecipeEvent) -> bool + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::hand_over(
            filter,
            |user| (host.api().craft_filter)(host.handle(), run_craft::<F>, user),
            "craft filter",
        )
    }

    fn on_build<F, V>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Placement) -> V + Send + Sync + 'static,
        V: Into<BuildVerdict>,
    {
        build_filter(self, move |placement| filter(placement).into())
    }
}

/// Hands `filter` to the loader's build check, for `Load::on_build` and its
/// script form.
pub(crate) fn build_filter<F>(load: &crate::BaseLoad, filter: F) -> Result<(), Off>
where
    F: Fn(&Placement) -> BuildVerdict + Send + Sync + 'static,
{
    let host = load.host();
    crate::load::hand_over(
        filter,
        |user| (host.api().build_filter)(host.handle(), run_build::<F>, user),
        "build filter",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_s_mistyped_answer_refuses_and_its_error_allows() {
        let answer = BuildVerdict::from_answer;
        assert_eq!(answer(Ok(Value::Nil)), BuildVerdict::Allow);
        assert_eq!(answer(Ok(Value::Bool(true))), BuildVerdict::Allow);
        assert_eq!(answer(Ok(Value::Bool(false))), BuildVerdict::Refuse);
        assert_eq!(
            answer(Ok(Value::String("build.below_sea".to_owned()))),
            BuildVerdict::refuse_because("build.below_sea")
        );
        assert_eq!(answer(Ok(Value::Number(1.0))), BuildVerdict::Refuse);
        assert_eq!(answer(Err(Error::new("boom"))), BuildVerdict::Allow);
    }
}
