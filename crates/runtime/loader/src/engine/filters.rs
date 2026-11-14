//! The callbacks mods register on one loader feature, asked in load order:
//! registered during init, sealed when the feature links, and each asked
//! only while its mod runs, through the profiler as that feature.

use core::ffi::c_void;
use core::ops::ControlFlow;

use crate::engine::loader::Registered;
use crate::engine::mod_index::ModIndex;
use crate::engine::profile::Feature;
use crate::engine::registry::{JoinFeatureError, Registry, Sealed};

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

    /// Adds mod `owner`'s `callback` when `available` finds what the
    /// feature needs on this build, asked only before the feature links.
    ///
    /// # Errors
    ///
    /// [`JoinFeatureError::TooLate`] once the feature linked, else
    /// [`JoinFeatureError::Unavailable`] when `available` says no.
    pub(crate) fn register_when(
        &self,
        available: impl FnOnce() -> bool,
        owner: ModIndex,
        callback: F,
        user: *mut c_void,
    ) -> Result<(), JoinFeatureError> {
        if self.is_sealed() {
            return Err(JoinFeatureError::TooLate);
        }
        if !available() {
            return Err(JoinFeatureError::Unavailable);
        }
        Ok(self.register(owner, callback, user)?)
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
    pub(crate) fn has_sealed(&self) -> bool {
        self.sealed().is_some_and(|sealed| !sealed.is_empty())
    }

    /// Asks the sealed callbacks as [`ask`] does; nothing before the
    /// feature linked.
    #[inline]
    pub(crate) fn ask<R>(
        &self,
        ask: impl FnMut(Callback<'_, F>) -> ControlFlow<R>,
    ) -> Option<(&Registered<F>, R)> {
        self::ask(self.sealed().unwrap_or_default(), self.feature, ask)
    }

    /// The first callback that refuses, `allows` answering for each; all of
    /// `allows` runs as the mod's callback, so it holds only the call.
    #[inline]
    pub(crate) fn refuser(
        &self,
        mut allows: impl FnMut(&F, *mut c_void) -> bool,
    ) -> Option<&Registered<F>> {
        self.ask(|callback| {
            if callback.call(&mut allows) {
                ControlFlow::Continue(())
            } else {
                ControlFlow::Break(())
            }
        })
        .map(|(refuser, ())| refuser)
    }

    /// Calls every callback whose mod runs, in order; all of `call` runs as
    /// the mod's callback, so it holds only the call.
    #[inline]
    pub(crate) fn each(&self, mut call: impl FnMut(&F, *mut c_void)) {
        let _none: Option<(&Registered<F>, ())> = self.ask(|callback| {
            callback.call(&mut call);
            ControlFlow::Continue(())
        });
    }
}

/// One live mod's callback as [`ask`] hands it out: reached only through
/// [`Callback::call`], so the mod's code always runs profiled as its own,
/// and the loader's work around the call does not.
#[derive(Debug)]
pub(crate) struct Callback<'a, F: 'static> {
    registered: &'a Registered<F>,
    feature: Feature,
}

impl<F> Callback<'_, F> {
    /// Runs `call` with the callback and its user pointer, profiled as the
    /// mod's callback for the feature.
    #[inline]
    pub(crate) fn call<A>(&self, call: impl FnOnce(&F, *mut c_void) -> A) -> A {
        let registered = self.registered;
        crate::engine::profile::call(registered.owner, self.feature, || {
            call(&registered.callback, registered.user())
        })
    }
}

/// Calls `ask` on each of `callbacks` whose mod runs, in order, and stops at
/// the first that breaks: that callback and its answer. The one way the
/// loader asks mods' filters, so the off rule ([`Registered::live`]) and the
/// profiling ([`Callback::call`]) live once.
#[inline]
pub(crate) fn ask<'a, F: 'static, R>(
    callbacks: impl IntoIterator<Item = &'a Registered<F>>,
    feature: Feature,
    mut ask: impl FnMut(Callback<'_, F>) -> ControlFlow<R>,
) -> Option<(&'a Registered<F>, R)> {
    callbacks
        .into_iter()
        .filter(|registered| registered.live())
        .find_map(|registered| {
            match ask(Callback {
                registered,
                feature,
            }) {
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
        assert!(!filters.has_sealed());
        assert_eq!(filters.seal().len(), 1);
        assert!(filters.has_sealed());
        assert_eq!(
            filters.register(RUNS, 9, core::ptr::null_mut()),
            Err(Sealed)
        );
    }

    #[test]
    fn a_registration_is_too_late_once_sealed_before_the_build_is_asked() {
        let filters = filters(&[]);
        let null = core::ptr::null_mut();
        assert_eq!(
            filters.register_when(|| false, RUNS, 1, null),
            Err(JoinFeatureError::Unavailable)
        );
        assert_eq!(filters.register_when(|| true, RUNS, 2, null), Ok(()));
        assert_eq!(filters.seal().len(), 1, "an unavailable one is not kept");
        let mut asked = false;
        let late = filters.register_when(
            || {
                asked = true;
                true
            },
            RUNS,
            3,
            null,
        );
        assert_eq!(late, Err(JoinFeatureError::TooLate));
        assert!(!asked);
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
    fn only_the_call_runs_as_the_mods_callback_not_the_loaders_work_around_it() {
        use crate::engine::profile::{call, current_callback};
        let filters = filters(&[RUNS]);
        let _sealed = filters.seal();
        let mut seen = Vec::new();
        // The loader's own callback marks as none, so an outer mod's mark
        // shows where the loader's work runs.
        let outer = ModIndex::new(7);
        call(outer, Feature::Chat, || {
            let _none: Option<(&Registered<u8>, ())> = filters.ask(|callback| {
                seen.push(current_callback());
                seen.push(callback.call(|_, _| current_callback()));
                seen.push(current_callback());
                ControlFlow::Continue(())
            });
        });
        let around = Some((outer.get(), Feature::Chat));
        assert_eq!(seen, [around, None, around]);
    }

    #[test]
    fn each_reaches_every_callback_in_order_and_a_break_carries_its_answer() {
        let filters = filters(&[RUNS, RUNS, RUNS]);
        let _sealed = filters.seal();
        let mut asked = Vec::new();
        filters.each(|&callback, _| asked.push(callback));
        assert_eq!(asked, [0, 1, 2]);
        let answer = filters.ask(|callback| {
            if callback.call(|&callback, _| callback) == 2 {
                ControlFlow::Break("two")
            } else {
                ControlFlow::Continue(())
            }
        });
        assert_eq!(answer.map(|(r, a)| (r.callback, a)), Some((2, "two")));
    }
}
