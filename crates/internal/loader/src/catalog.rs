//! The catalogs mods publish for the Mods screen: updates of installed mods
//! and mods the player may add, each with the mod's answer to a pick.

use std::sync::{Mutex, PoisonError};

use enshwick_loader_api::abi::{CatalogPickFn, Status, Str};
use enshwick_loader_api::catalog::Entry;

/// Each publishing mod's entries, by mod index, in the order they arrived.
static ENTRIES: Mutex<Vec<(usize, Vec<Entry>)>> = Mutex::new(Vec::new());
/// Each mod's answer to a pick: mod index, callback and its pointer.
static PICKS: Mutex<Vec<(usize, CatalogPickFn, usize)>> = Mutex::new(Vec::new());

/// Replaces mod `owner`'s catalog with the one in `text`.
pub(crate) fn set(owner: usize, text: &str) -> Status {
    let Ok(entries) = enshwick_loader_api::catalog::from_toml(text) else {
        return Status::BAD_ARGS;
    };
    let mut all = ENTRIES.lock().unwrap_or_else(PoisonError::into_inner);
    all.retain(|(index, _)| *index != owner);
    if !entries.is_empty() {
        all.push((owner, entries));
    }
    Status::OK
}

/// Makes `callback` mod `owner`'s answer to a pick; refused the second time.
pub(crate) fn on_pick(owner: usize, callback: CatalogPickFn, user: usize) -> Status {
    let mut picks = PICKS.lock().unwrap_or_else(PoisonError::into_inner);
    if picks.iter().any(|(index, _, _)| *index == owner) {
        return Status::REFUSED;
    }
    picks.push((owner, callback, user));
    Status::OK
}

/// Every entry of the catalogs of the mods that are on, with its mod.
pub(crate) fn entries(on: impl Fn(usize) -> bool) -> Vec<(usize, Entry)> {
    ENTRIES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|(owner, _)| on(*owner))
        .flat_map(|(owner, entries)| entries.iter().map(|entry| (*owner, entry.clone())))
        .collect()
}

/// Tells mod `owner` the player picked entry `id`; false when it hears no
/// picks.
pub(crate) fn pick(owner: usize, id: &str) -> bool {
    let found = PICKS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .find(|(index, _, _)| *index == owner)
        .map(|&(_, callback, user)| (callback, user));
    let Some((callback, user)) = found else {
        return false;
    };
    // SAFETY: the mod's callback with the pointer it registered, kept for
    // the process, and an id valid for the call; the API's wrapper catches
    // panics.
    unsafe { callback(Str::new(id), core::ptr::with_exposed_provenance_mut(user)) };
    true
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    static COUNT: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "system" fn noted(id: Str, _user: *mut core::ffi::c_void) {
        // SAFETY: an id valid for the call.
        if unsafe { id.as_str() } == Some("lanterns") {
            COUNT.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn catalogs_replace_each_other_per_mod_and_picks_reach_the_mod() {
        let text = enshwick_loader_api::catalog::to_toml(&[Entry::new("lanterns", "1.0.0")]);
        assert_eq!(set(7, &text), Status::OK);
        assert_eq!(set(8, &text), Status::OK);
        assert_eq!(
            set(7, &enshwick_loader_api::catalog::to_toml(&[])),
            Status::OK
        );
        assert_eq!(set(9, "not a catalog = ["), Status::BAD_ARGS);
        let shown = entries(|owner| owner == 7 || owner == 8);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].0, 8);
        assert!(entries(|_| false).is_empty());
        let user = 0;
        assert_eq!(on_pick(8, noted, user), Status::OK);
        assert_eq!(on_pick(8, noted, user), Status::REFUSED);
        assert!(pick(8, "lanterns"));
        assert!(!pick(7, "lanterns"));
        assert_eq!(COUNT.load(Ordering::Relaxed), 1);
    }
}
