//! Join rules: a mod's say over an account joining, before its player
//! enters the world.

use core::ffi::c_void;

use crate::Off;
use crate::PlayerKey;
use crate::abi::{JoinCall, read_headed};
use enshwick_base::vetoes::Verdict;

/// An account joining the server, for [`Load::on_join`](../struct.Load.html#method.on_join).
///
/// # Examples
///
/// See [`Load::on_join`](../struct.Load.html#method.on_join).
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct Join {
    /// The account's key, the one its player and the server's ban list
    /// carry; `None` for an account the game gives no key.
    pub player: Option<PlayerKey>,
}

unsafe extern "system" fn run_rule<F>(call: *const JoinCall, user: *mut c_void) -> u8
where
    F: Fn(&Join) -> Verdict,
{
    // SAFETY: the loader passes a valid call for the duration of this one; an
    // all-zero call is valid. Every field is there since the minor that added
    // join rules.
    let Some(call) = (unsafe { read_headed(call, size_of::<JoinCall>()) }) else {
        // A call the rule cannot read refuses, so a broken gate keeps accounts out.
        return crate::abi::FILTER_REFUSE;
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let rule = unsafe { &*user.cast::<F>() };
    let join = Join {
        player: (call.account != PlayerKey::new(0)).then_some(call.account),
    };
    crate::host::verdict(
        crate::host::guard("panic in a join rule", || Verdict::Pass, || rule(&join)).passes(),
    )
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::on_join`.
    #[doc(hidden)]
    fn on_join<F, V>(&mut self, rule: F) -> Result<(), Off>
    where
        F: Fn(&Join) -> V + Send + Sync + 'static,
        V: Into<Verdict>;

    /// As `Load::join_filter`.
    #[doc(hidden)]
    fn join_filter<F>(&mut self, rule: F) -> Result<(), Off>
    where
        F: Fn(&Join) -> Verdict + Send + Sync + 'static;
}

impl LoadMethods for crate::BaseLoad {
    fn on_join<F, V>(&mut self, rule: F) -> Result<(), Off>
    where
        F: Fn(&Join) -> V + Send + Sync + 'static,
        V: Into<Verdict>,
    {
        self.join_filter(move |join| rule(join).into())
    }

    fn join_filter<F>(&mut self, rule: F) -> Result<(), Off>
    where
        F: Fn(&Join) -> Verdict + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::hand_over(
            rule,
            |user| (host.api().join_rule)(host.handle(), Some(run_rule::<F>), user),
            "join rule",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule_call<F: Fn(&Join) -> Verdict>(rule: &F, player: u64) -> u8 {
        let call = enshwick_abi::headed!(JoinCall {
            account: PlayerKey::new(player),
        });
        let user = core::ptr::from_ref(rule).cast_mut().cast::<c_void>();
        // SAFETY: a valid call and the rule as user.
        unsafe { run_rule::<F>(&raw const call, user) }
    }

    #[test]
    fn a_rule_sees_the_key_or_none_and_may_refuse() {
        let rule = |join: &Join| Verdict::from(join.player == Some(PlayerKey::new(7)));
        assert_eq!(rule_call(&rule, 7), crate::abi::FILTER_PASS);
        assert_eq!(rule_call(&rule, 8), crate::abi::FILTER_REFUSE);
        let none = |join: &Join| Verdict::from(join.player.is_none());
        assert_eq!(rule_call(&none, 0), crate::abi::FILTER_PASS);
    }

    #[test]
    fn a_call_the_rule_cannot_read_refuses_the_account() {
        fn let_in(_: &Join) -> Verdict {
            Verdict::Allow
        }
        let rule: fn(&Join) -> Verdict = let_in;
        let user = core::ptr::from_ref(&rule).cast_mut().cast::<c_void>();
        // SAFETY: no call, which `read_headed` refuses, and the rule as user.
        let answer = unsafe { run_rule::<fn(&Join) -> Verdict>(core::ptr::null(), user) };
        assert_eq!(answer, crate::abi::FILTER_REFUSE);
    }
}
