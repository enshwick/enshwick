//! Rename filters: a mod's say over a player's rename of an entity.

use core::ffi::c_void;

use crate::Entity;
use crate::Off;
use crate::abi::{RenameCall, Str, read_headed};
use crate::world::Player;
use enshwick_base::vetoes::Verdict;

/// A player's rename of an entity, for [`Load::on_rename`](../struct.Load.html#method.on_rename).
///
/// # Examples
///
/// See [`Load::on_rename`](../struct.Load.html#method.on_rename).
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct Rename {
    /// The entity being renamed.
    pub entity: Entity,
    /// The renaming player, when the server could tell.
    pub player: Option<Player>,
    /// The new name the client sent.
    pub text: String,
}

/// A borrowed string as text, empty when it is not UTF-8.
///
/// # Safety
///
/// - Unless `value` is empty or its pointer null, its pointer is readable
///   for its length in bytes.
/// - Those bytes stay alive and unchanged for `'a`.
unsafe fn text_of<'a>(value: Str) -> &'a str {
    // SAFETY: `Str::as_str` needs the bytes readable and unchanged for `'a`,
    // which this function's `# Safety` bullets guarantee.
    unsafe { value.as_str() }.unwrap_or("")
}

unsafe extern "system" fn run_filter<F>(call: *const RenameCall, user: *mut c_void) -> u8
where
    F: Fn(&Rename) -> Verdict,
{
    // SAFETY: the loader passes a valid call for the duration of this one; an
    // all-zero call is valid. Every field is there since the minor that added
    // rename filters.
    let Some(call) = (unsafe { read_headed(call, size_of::<RenameCall>()) }) else {
        return crate::abi::FILTER_PASS;
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let handler = unsafe { &*user.cast::<F>() };
    let rename = Rename {
        entity: call.entity,
        // SAFETY: the loader passes null or the renaming player, valid for the call.
        player: unsafe { call.player.as_ref() }.map(Player::from_info),
        // SAFETY: the loader's text, valid for the call.
        text: unsafe { text_of(call.text) }.to_owned(),
    };
    crate::host::verdict(
        crate::host::guard(
            "panic in a rename filter",
            || Verdict::Pass,
            || handler(&rename),
        )
        .passes(),
    )
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::on_rename`.
    #[doc(hidden)]
    fn on_rename<F, V>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Rename) -> V + Send + Sync + 'static,
        V: Into<Verdict>;

    /// As `Load::rename_filter`.
    #[doc(hidden)]
    fn rename_filter<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Rename) -> Verdict + Send + Sync + 'static;
}

impl LoadMethods for crate::BaseLoad {
    fn on_rename<F, V>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Rename) -> V + Send + Sync + 'static,
        V: Into<Verdict>,
    {
        self.rename_filter(move |rename| filter(rename).into())
    }

    fn rename_filter<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Rename) -> Verdict + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::hand_over(
            filter,
            |user| (host.api().rename_filter)(host.handle(), Some(run_filter::<F>), user),
            "rename filter",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::PlayerInfo;

    fn filter_call<F: Fn(&Rename) -> Verdict>(filter: &F, call: &RenameCall) -> u8 {
        let user = core::ptr::from_ref(filter).cast_mut().cast::<c_void>();
        // SAFETY: a valid call and the filter as user.
        unsafe { run_filter::<F>(call, user) }
    }

    #[test]
    fn a_filter_sees_the_rename_and_may_refuse_it() {
        let player = enshwick_abi::headed!(PlayerInfo {
            slot: 1,
            ..PlayerInfo::with_name(Entity::from_engine(7), crate::PlayerKey::new(42), 0, "Ann")
        });
        let filter =
            |rename: &Rename| Verdict::from(rename.player.is_some() && rename.text != "no");
        let mut call = enshwick_abi::headed!(RenameCall {
            entity: Entity::from_engine(5),
            player: &raw const player,
            text: Str::new("Tools"),
        });
        assert_eq!(filter_call(&filter, &call), crate::abi::FILTER_PASS);
        call.text = Str::new("no");
        assert_eq!(filter_call(&filter, &call), crate::abi::FILTER_REFUSE);
        call.player = core::ptr::null();
        call.text = Str::new("Tools");
        assert_eq!(filter_call(&filter, &call), crate::abi::FILTER_REFUSE);
    }
}
