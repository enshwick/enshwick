//! The updates of installed mods that mods publish for the Mods screen, each
//! with the publishing mod's answer to a pick.

use std::sync::{Mutex, PoisonError};

use enshwick_loader_api::abi::{Status, Str, UpdatePickFn};
use enshwick_loader_api::updates::Update;

/// Each publishing mod's updates, by mod index, in the order they arrived.
static UPDATES: Mutex<Vec<(usize, Vec<Update>)>> = Mutex::new(Vec::new());
/// Each mod's answer to a pick: mod index, callback and its pointer.
static PICKS: Mutex<Vec<(usize, UpdatePickFn, usize)>> = Mutex::new(Vec::new());

/// Replaces mod `owner`'s updates with the ones in `text`.
pub(crate) fn set(owner: usize, text: &str) -> Status {
    let Ok(updates) = enshwick_loader_api::updates::from_toml(text) else {
        return Status::BAD_ARGS;
    };
    let mut all = UPDATES.lock().unwrap_or_else(PoisonError::into_inner);
    all.retain(|(index, _)| *index != owner);
    if !updates.is_empty() {
        all.push((owner, updates));
    }
    Status::OK
}

/// Makes `callback` mod `owner`'s answer to a pick; refused the second time.
pub(crate) fn on_pick(owner: usize, callback: UpdatePickFn, user: usize) -> Status {
    let mut picks = PICKS.lock().unwrap_or_else(PoisonError::into_inner);
    if picks.iter().any(|(index, _, _)| *index == owner) {
        return Status::REFUSED;
    }
    picks.push((owner, callback, user));
    Status::OK
}

/// Every update published by the mods that are on, with its mod.
pub(crate) fn all(on: impl Fn(usize) -> bool) -> Vec<(usize, Update)> {
    UPDATES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|(owner, _)| on(*owner))
        .flat_map(|(owner, updates)| updates.iter().map(|update| (*owner, update.clone())))
        .collect()
}

/// Tells mod `owner` the player picked the update of mod `id`; false when
/// it hears no picks.
pub(crate) fn pick(owner: usize, id: &str) -> bool {
    let found = PICKS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .find(|(index, _, _)| *index == owner)
        .map(|&(_, callback, user)| (callback, user));
    let Some((callback, user)) = found.filter(|_| {
        !crate::host::loader()
            .and_then(|loader| loader.mods.get(owner))
            .is_some_and(|m| m.off.load(std::sync::atomic::Ordering::Acquire))
    }) else {
        return false;
    };
    // SAFETY: the mod's callback with the pointer it registered, kept for
    // the process, and an id valid for the call; the API's wrapper catches
    // panics.
    crate::profile::call(owner, crate::profile::Feature::Interface, || unsafe {
        callback(Str::new(id), core::ptr::with_exposed_provenance_mut(user));
    });
    true
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicUsize, Ordering};

    use enshwick_loader_api::updates::to_toml;

    use super::*;

    static COUNT: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "system" fn noted(id: Str, _user: *mut core::ffi::c_void) {
        // SAFETY: an id valid for the call.
        if unsafe { id.as_str() } == Some("lanterns") {
            COUNT.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn updates_replace_each_other_per_mod_and_picks_reach_the_mod() {
        let text = to_toml(&[Update::new("lanterns", "1.0.0")]);
        assert_eq!(set(7, &text), Status::OK);
        assert_eq!(set(8, &text), Status::OK);
        assert_eq!(set(7, &to_toml(&[])), Status::OK);
        assert_eq!(set(9, "not a list = ["), Status::BAD_ARGS);
        let shown = all(|owner| owner == 7 || owner == 8);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].0, 8);
        assert!(all(|_| false).is_empty());
        let user = 0;
        assert_eq!(on_pick(8, noted, user), Status::OK);
        assert_eq!(on_pick(8, noted, user), Status::REFUSED);
        assert!(pick(8, "lanterns"));
        assert!(!pick(7, "lanterns"));
        assert_eq!(COUNT.load(Ordering::Relaxed), 1);
    }
}
