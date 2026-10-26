//! Rename filters: a mod's say over a player's rename of an entity.

use core::ffi::c_void;

use crate::abi::{RenameCall, Str, read_headed};
use crate::ecs::Entity;
use crate::world::Player;
use crate::{Load, Off};

/// A player's rename of an entity, for [`Load::on_rename`].
///
/// # Examples
///
/// See [`Load::on_rename`].
#[derive(Clone, Debug, PartialEq, Eq)]
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
    F: Fn(&Rename) -> bool,
{
    // SAFETY: the loader passes a valid call for the duration of this one; an
    // all-zero call is valid. Every field is there since the minor that added
    // rename filters.
    let Some(call) = (unsafe { read_headed(call, size_of::<RenameCall>()) }) else {
        return 1;
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let handler = unsafe { &*user.cast::<F>() };
    let rename = Rename {
        entity: Entity(call.entity),
        player: (call.known != 0).then(|| Player::from_info(&call.player)),
        // SAFETY: the loader's text, valid for the call.
        text: unsafe { text_of(call.text) }.to_owned(),
    };
    u8::from(crate::host::guard(
        "panic in a rename filter",
        || true,
        || handler(&rename),
    ))
}

impl Load<'_> {
    /// Runs `filter` before the server applies a player's rename of any
    /// entity, on its game thread; returning `false` refuses the rename. The
    /// game itself checks no permission for renames. A client runs it while
    /// it hosts a game.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses the filter, as after init.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     // Only administrators rename anything.
    ///     load.on_rename(|rename| rename.player.as_ref().is_some_and(|p| p.is_admin()))
    /// }
    /// ```
    pub fn on_rename<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Rename) -> bool + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::hand_over(
            filter,
            |user| (host.api().rename_filter)(host.handle(), run_filter::<F>, user),
            "rename filter",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::{Header, PlayerInfo};

    fn filter_call<F: Fn(&Rename) -> bool>(filter: &F, call: &RenameCall) -> u8 {
        let user = core::ptr::from_ref(filter).cast_mut().cast::<c_void>();
        // SAFETY: a valid call and the filter as user.
        unsafe { run_filter::<F>(call, user) }
    }

    #[test]
    fn a_filter_sees_the_rename_and_may_refuse_it() {
        let mut name = [0u8; 64];
        name[..3].copy_from_slice(b"Ann");
        let filter = |rename: &Rename| rename.player.is_some() && rename.text != "no";
        let mut call = RenameCall {
            header: Header::of::<RenameCall>(),
            entity: 5,
            known: 1,
            player: PlayerInfo {
                header: Header::of::<PlayerInfo>(),
                slot: 1,
                entity: 7,
                key: 42,
                permissions: 0,
                name_len: 3,
                name,
            },
            text: Str::new("Tools"),
        };
        assert_eq!(filter_call(&filter, &call), 1);
        call.text = Str::new("no");
        assert_eq!(filter_call(&filter, &call), 0);
        call.known = 0;
        call.text = Str::new("Tools");
        assert_eq!(filter_call(&filter, &call), 0);
    }
}
