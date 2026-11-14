//! Join rules: a mod's say over an account joining, before its player
//! enters the world.

use core::ffi::c_void;

use crate::Off;
use crate::PlayerKey;
use crate::abi::{JoinCall, read_headed};

/// An account joining the server, for [`Load::on_join`](../struct.Load.html#method.on_join).
///
/// # Examples
///
/// See [`Load::on_join`](../struct.Load.html#method.on_join).
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
pub struct Join {
    /// The account's key, the one its player and the server's ban list
    /// carry; `None` for an account the game gives no key.
    pub player: Option<PlayerKey>,
}

unsafe extern "system" fn run_rule<F>(call: *const JoinCall, user: *mut c_void) -> u8
where
    F: Fn(&Join) -> bool,
{
    // SAFETY: the loader passes a valid call for the duration of this one; an
    // all-zero call is valid. Every field is there since the minor that added
    // join rules.
    let Some(call) = (unsafe { read_headed(call, size_of::<JoinCall>()) }) else {
        return 1;
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let rule = unsafe { &*user.cast::<F>() };
    let join = Join {
        player: (call.player != PlayerKey::new(0)).then_some(call.player),
    };
    u8::from(crate::host::guard(
        "panic in a join rule",
        || true,
        || rule(&join),
    ))
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::on_join`.
    #[doc(hidden)]
    fn on_join<F>(&mut self, rule: F) -> Result<(), Off>
    where
        F: Fn(&Join) -> bool + Send + Sync + 'static;
}

impl LoadMethods for crate::BaseLoad {
    fn on_join<F>(&mut self, rule: F) -> Result<(), Off>
    where
        F: Fn(&Join) -> bool + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::hand_over(
            rule,
            |user| (host.api().join_rule)(host.handle(), run_rule::<F>, user),
            "join rule",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::Header;

    fn rule_call<F: Fn(&Join) -> bool>(rule: &F, player: u64) -> u8 {
        let call = JoinCall {
            header: Header::of::<JoinCall>(),
            player: PlayerKey::new(player),
        };
        let user = core::ptr::from_ref(rule).cast_mut().cast::<c_void>();
        // SAFETY: a valid call and the rule as user.
        unsafe { run_rule::<F>(&raw const call, user) }
    }

    #[test]
    fn a_rule_sees_the_key_or_none_and_may_refuse() {
        let rule = |join: &Join| join.player == Some(PlayerKey::new(7));
        assert_eq!(rule_call(&rule, 7), 1);
        assert_eq!(rule_call(&rule, 8), 0);
        let none = |join: &Join| join.player.is_none();
        assert_eq!(rule_call(&none, 0), 1);
    }
}
