//! The callbacks mods register on one loader feature, asked in load order:
//! registered during init, sealed when the feature links, and each asked
//! only while its mod runs, through the profiler as that feature.

use core::ffi::c_void;
use core::ops::ControlFlow;

use crate::engine::loader::Registered;
use crate::engine::mod_index::ModIndex;
use crate::engine::profile::Feature;
use crate::engine::registry::{Registry, Sealed};

/// The callbacks of type `F` mods register on one feature, such as the
/// join rules or the damage filters.
#[derive(Debug)]
pub(crate) struct Filters<F: 'static> {
    registry: Registry<Registered<F>>,
    feature: Feature,
}

impl<F> Filters<F> {
    /// No callback yet; each runs profiled as `feature`.
    pub(crate) const fn new(feature: Feature) -> Self {
        Self {
            registry: Registry::new(),
            feature,
        }
    }

    /// Adds mod `owner`'s `callback`, called with `user`.
    ///
    /// # Errors
    ///
    /// [`Sealed`] once the feature linked.
    pub(crate) fn register(
        &self,
        owner: ModIndex,
        callback: F,
        user: *mut c_void,
    ) -> Result<(), Sealed> {
        self.registry.add(Registered::new(owner, callback, user))
    }

    /// Whether the feature linked, so no callback can join.
    pub(crate) fn is_sealed(&self) -> bool {
        self.registry.is_sealed()
    }

    /// Seals the callbacks and returns them, for the feature's link.
    pub(crate) fn seal(&self) -> &[Registered<F>] {
        self.registry.seal()
    }

    /// The sealed callbacks; `None` before the feature linked.
    #[inline]
    pub(crate) fn sealed(&self) -> Option<&[Registered<F>]> {
        self.registry.sealed()
    }

    /// Whether any callback is sealed in, so the feature has work.
    #[inline]
    pub(crate) fn any(&self) -> bool {
        self.sealed().is_some_and(|sealed| !sealed.is_empty())
    }

    /// Asks the sealed callbacks as [`ask`] does; nothing before the
    /// feature linked.
    #[inline]
    pub(crate) fn ask<R>(
        &self,
        ask: impl FnMut(&F, *mut c_void) -> ControlFlow<R>,
    ) -> Option<(&Registered<F>, R)> {
        self::ask(self.sealed().unwrap_or_default(), self.feature, ask)
    }

    /// The first callback that refuses, `allows` answering for each.
    #[inline]
    pub(crate) fn refuser(
        &self,
        mut allows: impl FnMut(&F, *mut c_void) -> bool,
    ) -> Option<&Registered<F>> {
        self.ask(|callback, user| {
            if allows(callback, user) {
                ControlFlow::Continue(())
            } else {
                ControlFlow::Break(())
            }
        })
        .map(|(refuser, ())| refuser)
    }

    /// Calls every callback whose mod runs, in order.
    #[inline]
    pub(crate) fn each(&self, mut call: impl FnMut(&F, *mut c_void)) {
        let _none: Option<(&Registered<F>, ())> = self.ask(|callback, user| {
            call(callback, user);
            ControlFlow::Continue(())
        });
    }
}

/// Calls `ask` on each of `callbacks` whose mod runs, in order, profiled as
/// mod's callback for `feature`, and stops at the first that breaks: that
/// callback and its answer. The one way the loader asks mods' filters, so
/// the off rule ([`Registered::live`]) and the profiling live once.
#[inline]
pub(crate) fn ask<'a, F: 'a, R>(
    callbacks: impl IntoIterator<Item = &'a Registered<F>>,
    feature: Feature,
    mut ask: impl FnMut(&F, *mut c_void) -> ControlFlow<R>,
) -> Option<(&'a Registered<F>, R)> {
    callbacks
        .into_iter()
        .filter(|registered| registered.live())
        .find_map(|registered| {
            let answer = crate::engine::profile::call(registered.owner, feature, || {
                ask(&registered.callback, registered.user())
            });
            match answer {
                ControlFlow::Break(answer) => Some((registered, answer)),
                ControlFlow::Continue(()) => None,
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The loader's own links always run; without a running loader every
    /// mod counts as off.
    const RUNS: ModIndex = ModIndex::LOADER;
    const OFF: ModIndex = ModIndex::new(5);

    /// Callbacks numbered from 0, one per owner.
    fn filters(owners: &[ModIndex]) -> Filters<u8> {
        let filters = Filters::new(Feature::Vetoes);
        for (callback, &owner) in (0u8..).zip(owners) {
            let registered = filters.register(owner, callback, core::ptr::null_mut());
            assert_eq!(registered, Ok(()));
        }
        filters
    }

    #[test]
    fn nothing_is_asked_before_the_feature_links_and_a_sealed_one_refuses_more() {
        let filters = filters(&[RUNS]);
        let mut asked = 0;
        filters.each(|_, _| asked += 1);
        assert_eq!(asked, 0);
        assert!(!filters.any());
        assert_eq!(filters.seal().len(), 1);
        assert!(filters.any());
        assert_eq!(
            filters.register(RUNS, 9, core::ptr::null_mut()),
            Err(Sealed)
        );
    }

    #[test]
    fn the_first_refusing_callback_answers_and_later_ones_are_not_asked() {
        let filters = filters(&[RUNS, RUNS, RUNS]);
        let _sealed = filters.seal();
        let mut asked = Vec::new();
        let refuser = filters.refuser(|&callback, _| {
            asked.push(callback);
            callback != 1
        });
        assert_eq!(refuser.map(|r| r.callback), Some(1));
        assert_eq!(asked, [0, 1]);
    }

    #[test]
    fn a_callback_of_a_mod_that_is_off_is_never_asked() {
        let filters = filters(&[OFF, RUNS, OFF]);
        let _sealed = filters.seal();
        let mut asked = Vec::new();
        assert!(
            filters
                .refuser(|&callback, _| {
                    asked.push(callback);
                    false
                })
                .is_some_and(|r| r.callback == 1)
        );
        filters.each(|&callback, _| asked.push(callback));
        assert_eq!(asked, [1, 1]);
    }

    #[test]
    fn each_reaches_every_callback_in_order_and_a_break_carries_its_answer() {
        let filters = filters(&[RUNS, RUNS, RUNS]);
        let _sealed = filters.seal();
        let mut asked = Vec::new();
        filters.each(|&callback, _| asked.push(callback));
        assert_eq!(asked, [0, 1, 2]);
        let answer = filters.ask(|&callback, _| {
            if callback == 2 {
                ControlFlow::Break("two")
            } else {
                ControlFlow::Continue(())
            }
        });
        assert_eq!(answer.map(|(r, a)| (r.callback, a)), Some((2, "two")));
    }
}
