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
pub(super) static TEXTS: OnceLock<std::collections::HashMap<u32, Vec<(u32, &'static str)>>> =
    OnceLock::new();

// SAFETY: `TextFn` is the signature of the build table's `text.lookup`, and
// `seal_texts` uses this slot only as that function's next.
pub(super) static TEXT_NEXT: Next<TextFn> = unsafe { Next::new() };

// SAFETY: `TextFn` is the signature of the build table's
// `text.lookup_english`, and `seal_texts` uses this slot only as that
// function's next.
pub(super) static ENGLISH_NEXT: Next<TextFn> = unsafe { Next::new() };

pub(super) static POOL_LANGUAGE: AtomicUsize = AtomicUsize::new(usize::MAX);

/// The language the game last asked a text in.
pub(super) static LANGUAGE: core::sync::atomic::AtomicU32 =
    core::sync::atomic::AtomicU32::new(ENGLISH);

/// The English the game falls back to.
pub(super) const ENGLISH: u32 = 1;

pub(super) type TextFn =
    unsafe extern "system" fn(out: *mut [u64; 3], pool: *mut u8, id: u32) -> *mut [u64; 3];

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
}

impl Handed {
    /// The view of `len` bytes at `at`.
    pub(crate) const fn new(at: *const u8, len: usize) -> Self {
        Self { at, len }
    }

    /// Where the text starts.
    #[cfg(test)]
    pub(crate) const fn at(self) -> *const u8 {
        self.at
    }

    /// The view of a text that lives for the process.
    pub(crate) const fn of(text: &'static str) -> Self {
        Self::new(text.as_ptr(), text.len())
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

/// Seals the mod texts and links the client's text lookups when there are any.
pub(super) fn seal_texts(loader: &'static Loader) {
    let pending = std::mem::take(&mut *PENDING_TEXTS.relock());
    let count = pending.len();
    let mut texts: std::collections::HashMap<u32, Vec<(u32, &'static str)>> =
        std::collections::HashMap::new();
    for (id, language, text) in pending {
        let text: &'static str = Box::leak(text.into_boxed_str());
        texts.entry(id).or_default().push((language, text));
    }
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
    crate::log!(INFO, "{count} texts from mods");
}
