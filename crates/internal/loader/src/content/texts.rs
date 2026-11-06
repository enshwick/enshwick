use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick::abi::{HookFlags, Side, Status};
use enshwick::raw::{read_at, write_at};

use crate::chains::Next;
use crate::host::{Loader, Target};
use crate::locks::Relock;
use crate::pinned;

/// Mod texts while mods initialise: tag id, language, text.
pub(super) static PENDING_TEXTS: Mutex<Vec<(u32, u32, String)>> = Mutex::new(Vec::new());

/// Mod texts once sealed: tag id to `(language, text)`, read without a lock.
pub(super) static TEXTS: OnceLock<rustc_hash::FxHashMap<u32, Vec<(u32, &'static str)>>> =
    OnceLock::new();

// SAFETY: `TextFn` is the signature of the build table's `text.lookup`, and
// `seal_texts` uses this slot only as that function's next.
pub(super) static TEXT_NEXT: Next<TextFn> = unsafe { Next::new() };

// SAFETY: `TextFn` is the signature of the build table's
// `text.lookup_english`, and `seal_texts` uses this slot only as that
// function's next.
pub(super) static ENGLISH_NEXT: Next<TextFn> = unsafe { Next::new() };

// SAFETY: `FormatFn` is the signature of the build table's `text.format`,
// and `seal_texts` uses this slot only as that function's next.
pub(super) static FORMAT_NEXT: Next<FormatFn> = unsafe { Next::new() };

/// Every tag id the loader answers: the mods' texts, the notices' and the
/// dialogs'; the text format's one check before it passes a tag on.
static OURS: OnceLock<rustc_hash::FxHashSet<u32>> = OnceLock::new();

pub(super) static POOL_LANGUAGE: AtomicUsize = AtomicUsize::new(usize::MAX);

/// The language the game last asked a text in.
pub(super) static LANGUAGE: core::sync::atomic::AtomicU32 =
    core::sync::atomic::AtomicU32::new(ENGLISH);

/// The English the game falls back to.
pub(super) const ENGLISH: u32 = 1;

pub(super) type TextFn =
    unsafe extern "system" fn(out: *mut [u64; 3], pool: *mut u8, id: u32) -> *mut [u64; 3];

/// The game's text format: the text view of tag `id` into `out`, its
/// address, length and [`hash`].
pub(super) type FormatFn = unsafe extern "system" fn(out: *mut [u64; 3], id: u32) -> *mut [u64; 3];

/// The game's hash of a text view: FNV-1a over 64 bits, each byte
/// sign-extended, as the text format of client 26430625 computes it.
pub(crate) const fn hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut i = 0;
    while i < bytes.len() {
        #[expect(clippy::cast_possible_wrap, reason = "the game sign-extends each byte")]
        let byte = bytes[i] as i8;
        #[expect(clippy::cast_sign_loss, reason = "the game sign-extends each byte")]
        let wide = byte as i64 as u64;
        hash = (hash ^ wide).wrapping_mul(0x100_0000_01b3);
        i += 1;
    }
    hash
}

/// The language the game shows its texts in, as far as the loader saw.
pub(crate) fn language() -> enshwick::i18n::Language {
    enshwick::i18n::Language(LANGUAGE.load(Ordering::Relaxed))
}

/// Registers a mod text; refused after init.
pub(crate) fn add_text(id: u32, language: u32, text: &str) -> Status {
    if TEXTS.get().is_some() {
        return Status::TOO_LATE;
    }
    PENDING_TEXTS.relock().push((id, language, text.to_owned()));
    Status::OK
}

/// UTF-8 text the loader hands the game as a text lookup's answer: its
/// address and length.
///
/// The bytes stay readable at that address for as long as the game may keep
/// the view; their maker says how. A view may outlive its text: a notice
/// slot is written over in place while the game may still hold an older
/// view of it, which then reads the slot's newer text (or a mix of the two
/// cut short at the older length) but never freed memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Handed {
    at: *const u8,
    len: usize,
    /// The bytes' [`hash`].
    hash: u64,
}

impl Handed {
    /// The view of `len` bytes at `at`, whose [`hash`] is `hash`.
    pub(crate) const fn new(at: *const u8, len: usize, hash: u64) -> Self {
        Self { at, len, hash }
    }

    /// Where the text starts.
    #[cfg(test)]
    pub(crate) const fn at(self) -> *const u8 {
        self.at
    }

    /// The view of a text that lives for the process.
    pub(crate) const fn of(text: &'static str) -> Self {
        Self::new(text.as_ptr(), text.len(), hash(text.as_bytes()))
    }
}

/// The text of `id` for `language`: its own, else the next language of its
/// lookup order's, else English.
pub(super) fn text_for(id: u32, language: u32) -> Option<Handed> {
    if let Some(text) = crate::notices::text(id) {
        return Some(text);
    }
    if let Some(text) = crate::ui::text(id) {
        return Some(Handed::of(text));
    }
    let texts = TEXTS.get()?.get(&id)?;
    let pick = |wanted: u32| texts.iter().find(|(l, _)| *l == wanted).map(|(_, t)| *t);
    enshwick::i18n::Language(language)
        .lookup_order()
        .find_map(|language| pick(language.0))
        .or_else(|| pick(ENGLISH))
        .map(Handed::of)
}

/// Answers a text lookup with `text`.
///
/// # Safety
///
/// - `out` is null or the lookup's writable out record of three `u64`,
///   which nothing else touches during the call.
pub(super) unsafe fn answer(out: *mut [u64; 3], text: Handed) -> *mut [u64; 3] {
    if !out.is_null() {
        let record = [0, text.at.expose_provenance() as u64, text.len as u64];
        // SAFETY: `out` is non-null by the check before it, and the caller
        // guarantees the lookup's writable out record: error 0, the UTF-8
        // text and its length.
        unsafe { write_at(out.cast(), 0, record) };
    }
    out
}

/// The mod texts' switch.
pub(super) static TEXTS_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("mod texts");

pub(super) unsafe extern "system" fn lookup(
    out: *mut [u64; 3],
    pool: *mut u8,
    id: u32,
) -> *mut [u64; 3] {
    let found = TEXTS_FUSE.run("the text lookup", || {
        let offset = POOL_LANGUAGE.load(Ordering::Relaxed);
        let language = if pool.is_null() || offset == usize::MAX {
            ENGLISH
        } else {
            // SAFETY: `pool` is non-null by the check before it and is the
            // lookup's text pool; its u32 current language sits at the build
            // table's `text.pool_language` offset.
            unsafe { read_at::<u32>(pool, offset) }
        };
        LANGUAGE.store(language, Ordering::Relaxed);
        text_for(id, language)
    });
    match found.flatten() {
        // SAFETY: `out` is the lookup's own out record argument, null or
        // writable for the call.
        Some(text) => unsafe { answer(out, text) },
        // SAFETY: this detour runs only as the `text.lookup` link, so the
        // slot holds the next function of that signature; the game's
        // arguments pass unchanged.
        None => unsafe { TEXT_NEXT.get()(out, pool, id) },
    }
}

pub(super) unsafe extern "system" fn lookup_english(
    out: *mut [u64; 3],
    pool: *mut u8,
    id: u32,
) -> *mut [u64; 3] {
    match TEXTS_FUSE
        .run("the English text lookup", || text_for(id, ENGLISH))
        .flatten()
    {
        // SAFETY: `out` is the lookup's own out record argument, null or
        // writable for the call.
        Some(text) => unsafe { answer(out, text) },
        // SAFETY: this detour runs only as the `text.lookup_english` link, so
        // the slot holds the next function of that signature; the game's
        // arguments pass unchanged.
        None => unsafe { ENGLISH_NEXT.get()(out, pool, id) },
    }
}

/// The loader's link on the game's text format, which most of the game's
/// interface turns tags into text with: it looks a tag up in its own text
/// pool first and answers an empty text, without asking `text.lookup`, for
/// one the pool lacks, as every tag of the loader is (client 26430625).
pub(super) unsafe extern "system" fn format(out: *mut [u64; 3], id: u32) -> *mut [u64; 3] {
    // Every text the game shows passes here: the set is the whole check.
    if !out.is_null()
        && OURS.get().is_some_and(|ours| ours.contains(&id))
        && let Some(text) = TEXTS_FUSE
            .run("the text format", || {
                text_for(id, LANGUAGE.load(Ordering::Relaxed))
            })
            .flatten()
    {
        let view = [
            text.at.expose_provenance() as u64,
            text.len as u64,
            text.hash,
        ];
        // SAFETY: `out` is non-null by the check before it and is the
        // format's own out record of three `u64`: the text's address,
        // length and hash.
        unsafe { write_at(out.cast(), 0, view) };
        return out;
    }
    // SAFETY: this detour runs only as the `text.format` link, so the slot
    // holds the next function of that signature; the game's arguments pass
    // unchanged.
    unsafe { FORMAT_NEXT.get()(out, id) }
}

/// The tag ids [`format`] answers: those of `texts`, the notices and the
/// dialogs.
fn ours(
    texts: &rustc_hash::FxHashMap<u32, Vec<(u32, &'static str)>>,
) -> rustc_hash::FxHashSet<u32> {
    texts
        .keys()
        .copied()
        .chain(crate::notices::tag_ids())
        .chain(crate::ui::tag_ids())
        .collect()
}

/// Seals the mod texts and links the client's text lookups when there are any.
pub(super) fn seal_texts(loader: &'static Loader) {
    let pending = std::mem::take(&mut *PENDING_TEXTS.relock());
    let count = pending.len();
    let mut texts: rustc_hash::FxHashMap<u32, Vec<(u32, &'static str)>> =
        rustc_hash::FxHashMap::default();
    for (id, language, text) in pending {
        let text: &'static str = Box::leak(text.into_boxed_str());
        texts.entry(id).or_default().push((language, text));
    }
    let _first = OURS.set(ours(&texts));
    // The client always links its lookups, for the loader's notices.
    if TEXTS.set(texts).is_err() || (count == 0 && loader.side != Side::CLIENT) {
        return;
    }
    // Only the client shows text; the server has no lookups to link.
    let (Some(_), Some(_), Some(offset)) = (
        loader.build.symbol(pinned::text::LOOKUP.id()),
        loader.build.symbol(pinned::text::LOOKUP_ENGLISH.id()),
        loader.build.constant(pinned::text::POOL_LANGUAGE.id()),
    ) else {
        return;
    };
    POOL_LANGUAGE.store(
        usize::try_from(offset).unwrap_or(usize::MAX),
        Ordering::Relaxed,
    );
    loader.link_feature(
        "mod texts",
        Target::Pinned(pinned::text::LOOKUP),
        &TEXT_NEXT,
        lookup as TextFn,
        HookFlags::NONE,
    );
    loader.link_feature(
        "mod English texts",
        Target::Pinned(pinned::text::LOOKUP_ENGLISH),
        &ENGLISH_NEXT,
        lookup_english as TextFn,
        HookFlags::NONE,
    );
    loader.link_feature(
        "mod texts in the game's text format",
        Target::Pinned(pinned::text::FORMAT),
        &FORMAT_NEXT,
        format as FormatFn,
        HookFlags::NONE,
    );
    crate::log!(INFO, "{count} texts from mods");
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x100_0000_01b3;

    #[test]
    fn the_hash_is_fnv1a_over_bytes_sign_extended_as_the_game_hashes() {
        assert_eq!(hash(b""), BASIS, "the empty text's hash the game writes");
        assert_eq!(hash(b"a"), 0xaf63_dc4c_8601_ec8c, "plain FNV-1a for ASCII");
        assert_eq!(
            hash(&[0x80]),
            (BASIS ^ 0xffff_ffff_ffff_ff80).wrapping_mul(PRIME),
            "a byte over 0x7f is sign-extended before the xor"
        );
        assert_ne!(hash("ж".as_bytes()), {
            let mut plain = BASIS;
            for &b in "ж".as_bytes() {
                plain = (plain ^ u64::from(b)).wrapping_mul(PRIME);
            }
            plain
        });
    }

    /// Stands in for the game's text format: the tag id in the hash word.
    unsafe extern "system" fn passed(out: *mut [u64; 3], id: u32) -> *mut [u64; 3] {
        // SAFETY: the tests pass their own writable record.
        unsafe { *out = [0, 0, u64::from(id)] };
        out
    }

    #[test]
    fn the_text_format_answers_the_loaders_tags_and_passes_the_rest_on() {
        let next: FormatFn = passed;
        #[expect(
            clippy::fn_to_numeric_cast_any,
            reason = "a slot holds its next function's address"
        )]
        FORMAT_NEXT.slot().store(next as usize, Ordering::Release);
        let mut texts = rustc_hash::FxHashMap::default();
        texts.insert(7, vec![(ENGLISH, "Luau check")]);
        assert!(OURS.set(ours(&texts)).is_ok());
        assert!(TEXTS.set(texts).is_ok());

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

        // SAFETY: as before.
        unsafe { format(&raw mut out, 8) };
        assert_eq!(out, [0, 0, 8], "a game tag goes to the game");

        let dialog = crate::ui::tag_ids().next().unwrap();
        // SAFETY: as before.
        unsafe { format(&raw mut out, dialog) };
        assert_eq!(
            out,
            [0, 0, u64::from(dialog)],
            "a dialog tag with no dialog kept goes on too"
        );
    }
}
