//! Text and bytes kept for the process, one copy each, for the names a
//! `&'static` field holds that come from a script, a format or the game.
//!
//! Each distinct value is kept once, so a name made again (a quest's step
//! parts, a world key a menu builds again and again) takes no more memory.

use std::sync::Mutex;

use rustc_hash::{FxBuildHasher, FxHashSet};

use crate::locks::Relock as _;

/// Every text and byte string kept so far.
static KEPT: Mutex<Kept> = Mutex::new(Kept {
    texts: FxHashSet::with_hasher(FxBuildHasher),
    bytes: FxHashSet::with_hasher(FxBuildHasher),
});

struct Kept {
    texts: FxHashSet<&'static str>,
    bytes: FxHashSet<&'static [u8]>,
}

/// `text` for the process, and whether this call kept it first.
#[must_use]
pub fn first_text(text: &str) -> (&'static str, bool) {
    let mut kept = KEPT.relock();
    let texts = &mut kept.texts;
    if let Some(&known) = texts.get(text) {
        return (known, false);
    }
    let new: &'static str = Box::leak(text.into());
    texts.insert(new);
    (new, true)
}

/// `text` for the process.
#[must_use]
pub fn text(text: &str) -> &'static str {
    first_text(text).0
}

/// `bytes` for the process.
#[must_use]
pub fn bytes(bytes: &[u8]) -> &'static [u8] {
    let mut kept = KEPT.relock();
    let all = &mut kept.bytes;
    if let Some(&known) = all.get(bytes) {
        return known;
    }
    let new: &'static [u8] = Box::leak(bytes.into());
    all.insert(new);
    new
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_kept_twice_is_the_same_copy_and_only_the_first_call_is_new() {
        let (first, new) = first_text("intern-test:a");
        let (again, new_again) = first_text(&String::from("intern-test:a"));
        assert!(new && !new_again);
        assert!(core::ptr::eq(first, again));
        assert!(core::ptr::eq(
            bytes(b"\x89PNG"),
            bytes(&[0x89, b'P', b'N', b'G'])
        ));
        assert!(!core::ptr::eq(text("intern-test:b"), first));
    }
}
