use core::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;

use enshwick_core::abi::{HookFlags, Side, Status};
use enshwick_core::raw::{read_at, write_at};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::base::handed::Handed;
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::pending::Pending;
use crate::engine::pinned;

/// The mod texts and the language the client's game shows them in. Both
/// live for the process, so nothing here ends with a hosted game.
pub(super) struct Texts {
    /// Mod texts while mods initialise: tag id, language, text.
    pending: Pending<(u32, u32, String)>,
    /// Mod texts once sealed: tag id to `(language, text)`, read without a
    /// lock.
    pub(super) sealed: OnceLock<FxHashMap<u32, Vec<(u32, &'static str)>>>,
    /// Every tag id the loader answers: the mods' texts, the notices' and
    /// the dialogs'; the text format's one check before it passes a tag on.
    ours: OnceLock<FxHashSet<u32>>,
    /// The offset of a text pool's current language, from the build table.
    pool_language: OnceLock<usize>,
    /// The language the game last asked a text in.
    language: AtomicU32,
}

pub(super) static TEXTS: Texts = Texts {
    pending: Pending::new(),
    sealed: OnceLock::new(),
    ours: OnceLock::new(),
    pool_language: OnceLock::new(),
    language: AtomicU32::new(ENGLISH),
};

/// The English the game falls back to.
pub(super) const ENGLISH: u32 = 1;

/// The language the game shows its texts in, as far as the loader saw.
pub(crate) fn language() -> enshwick_core::i18n::Language {
    enshwick_core::i18n::Language(TEXTS.language.load(Ordering::Relaxed))
}

/// Registers a mod text; refused after init.
pub(crate) fn add_text(id: u32, language: u32, text: &str) -> Status {
    if TEXTS.sealed.get().is_some() {
        return Status::TOO_LATE;
    }
    TEXTS.pending.push((id, language, text.to_owned()));
    Status::OK
}

/// The text of `id` for `language`: its own, else the next language of its
/// lookup order's, else English.
pub(super) fn text_for(id: u32, language: u32) -> Option<Handed> {
    if let Some(text) = crate::base::notices::text(id) {
        return Some(text);
    }
    if let Some(text) = crate::ui::interface::text(id) {
        return Some(Handed::of(text));
    }
    mod_text(id, language).map(Handed::of)
}

/// Whether the mods' texts are sealed, so [`mod_text`] answers.
pub(crate) fn mod_texts_sealed() -> bool {
    TEXTS.sealed.get().is_some()
}

/// A mod's text `id` for `language`, as [`text_for`] picks it: its own, else
/// the next language of its lookup order's, else English; `None` before the
/// texts seal or for a tag no mod gave text.
pub(crate) fn mod_text(id: u32, language: u32) -> Option<&'static str> {
    let texts = TEXTS.sealed.get()?.get(&id)?;
    let pick = |wanted: u32| texts.iter().find(|(l, _)| *l == wanted).map(|(_, t)| *t);
    enshwick_core::i18n::Language(language)
        .lookup_order()
        .find_map(|language| pick(language.0))
        .or_else(|| pick(ENGLISH))
}

/// Answers a text lookup with `text`.
///
/// # Safety
///
/// - `out` is null or the lookup's writable out record of three `u64`,
///   which nothing else touches during the call.
pub(super) unsafe fn answer(out: *mut [u64; 3], text: Handed) -> *mut [u64; 3] {
    if !out.is_null() {
        let record = [0, text.at().expose_provenance() as u64, text.bytes() as u64];
        // SAFETY: `out` is non-null by the check before it, and the caller
        // guarantees the lookup's writable out record: error 0, the UTF-8
        // text and its length.
        unsafe { write_at(out.cast(), 0, record) };
    }
    out
}

/// The switch of the client's text lookup. Each of the three text links has
/// a switch of its own: a build that lacks one still serves mod texts
/// through the others.
static TEXTS_FUSE: Fuse = Fuse::new("mod texts");
/// The switch of the client's English text lookup.
static ENGLISH_FUSE: Fuse = Fuse::new("mod English texts");
/// The switch of the client's text format.
static FORMAT_FUSE: Fuse = Fuse::new("mod texts in the game's text format");

crate::engine::chains::chain_detour! {
    /// The table's `text.lookup(out, pool, id)`: the text of tag `id` in
    /// the pool's current language, the loader's for its own tags.
    fn lookup(out: *mut [u64; 3], pool: *mut u8, id: u32) -> *mut [u64; 3];
    next TEXT_NEXT;
    fuse &TEXTS_FUSE, "the text lookup";
    |onward| {
        let language = match TEXTS.pool_language.get() {
            Some(&offset) if !pool.is_null() => {
                // SAFETY: `pool` is non-null by the check before it and is
                // the lookup's text pool; its u32 current language sits at
                // the build table's `text.pool_language` offset.
                unsafe { read_at::<u32>(pool, offset) }
            }
            _ => ENGLISH,
        };
        TEXTS.language.store(language, Ordering::Relaxed);
        match text_for(id, language) {
            // SAFETY: `out` is the lookup's own out record argument, null or
            // writable for the call.
            Some(text) => unsafe { answer(out, text) },
            None => onward.call(),
        }
    }
}

crate::engine::chains::chain_detour! {
    /// The table's `text.lookup_english(out, pool, id)`: the English text of
    /// tag `id`, the loader's for its own tags.
    fn lookup_english(out: *mut [u64; 3], pool: *mut u8, id: u32) -> *mut [u64; 3];
    next ENGLISH_NEXT;
    fuse &ENGLISH_FUSE, "the English text lookup";
    |onward| {
        match text_for(id, ENGLISH) {
            // SAFETY: `out` is the lookup's own out record argument, null or
            // writable for the call.
            Some(text) => unsafe { answer(out, text) },
            None => onward.call(),
        }
    }
}

crate::engine::chains::chain_detour! {
    /// The table's `text.format(out, id)`, which most of the game's
    /// interface turns tags into text with: the text view of tag `id` into
    /// `out`, its address, length and [`hash`](crate::base::handed::hash).
    /// The game looks a tag up in its own text pool first and answers an
    /// empty text, without asking `text.lookup`, for one the pool lacks, as
    /// every tag of the loader is (client 26430625). Every text the game
    /// shows passes here: the set of the loader's tags is the whole check
    /// before the fuse.
    fn format(out: *mut [u64; 3], id: u32) -> *mut [u64; 3];
    next FORMAT_NEXT;
    |onward| {
        if out.is_null() || !TEXTS.ours.get().is_some_and(|ours| ours.contains(&id)) {
            return onward.call();
        }
        onward.run(&FORMAT_FUSE, "the text format", |onward| {
            let Some(text) = text_for(id, TEXTS.language.load(Ordering::Relaxed)) else {
                return onward.call();
            };
            let view = [
                text.at().expose_provenance() as u64,
                text.bytes() as u64,
                text.hash(),
            ];
            // SAFETY: `out` is non-null by the check before it and is the
            // format's own out record of three `u64`: the text's address,
            // length and hash.
            unsafe { write_at(out.cast(), 0, view) };
            out
        })
    }
}

/// The tag ids [`format`] answers: those of `texts`, the notices and the
/// dialogs.
fn ours(texts: &FxHashMap<u32, Vec<(u32, &'static str)>>) -> FxHashSet<u32> {
    texts
        .keys()
        .copied()
        .chain(crate::base::notices::tag_ids())
        .chain(crate::ui::interface::tag_ids())
        .collect()
}

/// Seals the mod texts and links the client's text lookups when there are any.
pub(super) fn seal_texts(loader: &'static Loader) {
    let pending = TEXTS.pending.take();
    let count = pending.len();
    let mut texts: FxHashMap<u32, Vec<(u32, &'static str)>> = FxHashMap::default();
    for (id, language, text) in pending {
        let text: &'static str = Box::leak(text.into_boxed_str());
        texts.entry(id).or_default().push((language, text));
    }
    let _first = TEXTS.ours.set(ours(&texts));
    // The client always links its lookups, for the loader's notices.
    if TEXTS.sealed.set(texts).is_err() || (count == 0 && loader.side != Side::CLIENT) {
        return;
    }
    // Only the client shows text; a server has no lookups to link.
    let lookups = loader.build.symbol(pinned::text::LOOKUP.id()).is_some()
        && loader
            .build
            .symbol(pinned::text::LOOKUP_ENGLISH.id())
            .is_some();
    if !lookups && loader.side != Side::CLIENT {
        return;
    }
    let Some(offset) = loader.build.constant(pinned::text::POOL_LANGUAGE.id()) else {
        let absent = crate::engine::builds::ABSENT;
        let _off = loader.fuse_off(&TEXTS_FUSE, pinned::text::POOL_LANGUAGE.id(), absent);
        return;
    };
    if let Ok(offset) = usize::try_from(offset) {
        let _first = TEXTS.pool_language.set(offset);
    }
    if link_texts(loader) > 0 {
        crate::log!(INFO, "{count} texts from mods");
    }
}

/// Links the client's text lookup, its English lookup and its text format,
/// each on its own, so a build that lacks one serves mod texts through the
/// others; how many linked.
fn link_texts(loader: &Loader) -> usize {
    let none = HookFlags::NONE;
    let linked = [
        loader.link_fused(
            &TEXTS_FUSE,
            Target::Pinned(pinned::text::LOOKUP),
            &TEXT_NEXT,
            lookup,
            none,
        ),
        loader.link_fused(
            &ENGLISH_FUSE,
            Target::Pinned(pinned::text::LOOKUP_ENGLISH),
            &ENGLISH_NEXT,
            lookup_english,
            none,
        ),
        loader.link_fused(
            &FORMAT_FUSE,
            Target::Pinned(pinned::text::FORMAT),
            &FORMAT_NEXT,
            format,
            none,
        ),
    ];
    linked.into_iter().filter(|&made| made).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::handed::hash;
    use enshwick_base::Relock;

    /// Stands in for the game's text format: the tag id in the hash word.
    unsafe extern "system" fn passed(out: *mut [u64; 3], id: u32) -> *mut [u64; 3] {
        // SAFETY: the tests pass their own writable record.
        unsafe { *out = [0, 0, u64::from(id)] };
        out
    }

    /// One test for the texts' fuse, which the format runs under and a
    /// failed link turns off for the process: the format first, then the
    /// links.
    #[test]
    fn the_text_format_answers_the_loaders_tags_and_the_links_arm_all_or_none() {
        let next: unsafe extern "system" fn(*mut [u64; 3], u32) -> *mut [u64; 3] = passed;
        #[expect(
            clippy::fn_to_numeric_cast_any,
            reason = "a slot holds its next function's address"
        )]
        FORMAT_NEXT.slot().store(next as usize, Ordering::Release);
        let mut texts = FxHashMap::default();
        texts.insert(7, vec![(ENGLISH, "Luau check")]);
        let answered = ours(&texts);
        assert!(
            crate::base::notices::tag_ids()
                .chain(crate::ui::interface::tag_ids())
                .all(|id| answered.contains(&id))
                && answered.len() > 1 + crate::base::notices::tag_ids().count(),
            "every notice and dialog tag is answered"
        );
        assert!(!answered.contains(&0) && !answered.contains(&1));
        assert!(TEXTS.ours.set(answered).is_ok());
        assert!(TEXTS.sealed.set(texts).is_ok());

        let mut out = [0u64; 3];
        // SAFETY: `out` is a writable record of three `u64`.
        let answered = unsafe { format(&raw mut out, 7) };
        assert_eq!(answered, &raw mut out);
        assert_eq!(out[1], 10);
        assert_eq!(out[2], hash(b"Luau check"));
        // SAFETY: the format answered a view of a mod text, which lives for
        // the process, `out[1]` bytes long.
        let shown = unsafe {
            core::slice::from_raw_parts(
                core::ptr::with_exposed_provenance::<u8>(usize::try_from(out[0]).unwrap()),
                10,
            )
        };
        assert_eq!(shown, b"Luau check");

        // SAFETY: `out` is a writable local record of three `u64`, and the
        // slot holds `passed`, which writes only that record.
        unsafe { format(&raw mut out, 8) };
        assert_eq!(out, [0, 0, 8], "a game tag goes to the game");

        let dialog = crate::ui::interface::tag_ids().next().unwrap();
        // SAFETY: `out` is a writable local record of three `u64`, and the
        // slot holds `passed`, which writes only that record.
        unsafe { format(&raw mut out, dialog) };
        assert_eq!(
            out,
            [0, 0, u64::from(dialog)],
            "a dialog tag with no dialog kept goes on too"
        );

        let loader = crate::start::tests::bare_loader();
        assert_eq!(link_texts(&loader), 0, "this build has no text links");
        assert!(TEXTS_FUSE.is_off(), "off by its fuse, not only in the log");
        assert!(
            ENGLISH_FUSE.is_off() && FORMAT_FUSE.is_off(),
            "each link is tried and turned off on its own"
        );
        let mut chains = loader.chains.relock();
        chains.seal();
        assert!(chains.is_empty());
        let listed = crate::engine::diag::parts_off();
        let off = listed.iter().find(|p| p.part == "mod texts");
        assert!(
            off.is_some_and(|p| p.reason.starts_with(pinned::text::LOOKUP.id())),
            "{listed:?}"
        );

        // SAFETY: `out` is a writable local record of three `u64`, and the
        // slot holds `passed`, which writes only that record.
        unsafe { format(&raw mut out, 7) };
        assert_eq!(out, [0, 0, 7], "an off feature's tag goes to the game");
    }
}
