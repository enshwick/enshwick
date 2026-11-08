pub use const_format::concatcp;
pub use serde;

pub use crate::hook::Frame;
pub use crate::script::convert::{argument, argument_or_default, record_fields};

/// A copy of `this`, for a `#[script]` method that takes `self` by value.
#[must_use]
pub fn owned<T: Clone>(this: &T) -> T {
    this.clone()
}

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::abi::{HostApi, Level, ModHandle, Side, Status};
use crate::{Host, Load, Mod};

pub use crate::content::recipes::derive::{RecipeEdit, modify_recipes_in};
pub use crate::engine_ids::ALL as ENGINE_IDS;
/// The ids the API asks the loader for engine facts by, for the loader and
/// the catalogue to name them from one place.
pub mod ids {
    pub use crate::engine_ids::*;
}

mod copy;
pub use copy::copy_out;

/// The rules the loader and the fake host refuse a mod's requests by.
pub mod rules {
    pub use crate::rules::*;
}

/// Links an `InventorySetup` holds, for the inventory walks of the API
/// and the loader (server 399, client 426).
pub const INVENTORY_LINKS: usize = 16;
/// Slots a sub-inventory holds, for the same walks.
pub const INVENTORY_SLOTS: u8 = 8;
/// The flag bit of an attribute set that marks it initialised, for the
/// vitals reads of the API and the loader.
pub const ATTRIBUTE_SET_INITIALISED: u8 = 1;

/// Each experience source with its catalogue id, for the loader's
/// experience rates.
pub const EXPERIENCE_SOURCES: &[(crate::game_events::ExperienceSourceType, &str)] =
    &crate::game_events::ExperienceSourceType::IDS;

/// The bytes of `text` and a closing NUL, an entry of the `.enshpt`
/// section that [`__schema_entry!`](crate::__schema_entry) writes.
///
/// # Panics
///
/// When `N` is not one more than the length of `text`, at compile time.
#[must_use]
pub const fn entry<const N: usize>(text: &str) -> [u8; N] {
    let source = text.as_bytes();
    assert!(source.len() + 1 == N, "an entry is its text and a NUL");
    let mut out = [0u8; N];
    let mut i = 0;
    while i < source.len() {
        out[i] = source[i];
        i += 1;
    }
    out
}

/// The bytes of this binary's `.enshpt` section: the schema entries of
/// every point, event and call declared without `of`, and the head of each
/// `declare_mod!`, separated by NULs.
#[must_use]
pub fn section() -> &'static [u8] {
    // The linker sorts `.enshpt$<x>` by `<x>` into one `.enshpt`, so the
    // entries in `$m` lie between the markers in `$a` and `$z`.
    #[cfg(windows)]
    let (start, stop) = {
        #[unsafe(link_section = ".enshpt$a")]
        static START: [u8; 1] = [0];
        #[unsafe(link_section = ".enshpt$z")]
        static STOP: [u8; 1] = [0];
        (START.as_ptr(), STOP.as_ptr())
    };
    #[cfg(not(windows))]
    let (start, stop) = {
        // The section must exist for the linker to define its bounds.
        #[used]
        #[cfg_attr(
            target_vendor = "apple",
            unsafe(link_section = "__DATA,__enshpt,regular,no_dead_strip")
        )]
        #[cfg_attr(not(target_vendor = "apple"), unsafe(link_section = "enshpt"))]
        static PRESENT: [u8; 0] = [];
        unsafe extern "Rust" {
            #[cfg_attr(
                target_vendor = "apple",
                link_name = "\x01section$start$__DATA$__enshpt"
            )]
            #[cfg_attr(not(target_vendor = "apple"), link_name = "__start_enshpt")]
            static START: [u8; 0];
            #[cfg_attr(target_vendor = "apple", link_name = "\x01section$end$__DATA$__enshpt")]
            #[cfg_attr(not(target_vendor = "apple"), link_name = "__stop_enshpt")]
            static STOP: [u8; 0];
        }
        (
            core::ptr::addr_of!(START).cast::<u8>(),
            core::ptr::addr_of!(STOP).cast::<u8>(),
        )
    };
    let len = stop.addr().saturating_sub(start.addr());
    // SAFETY: the linker places the whole section, `len` bytes of
    // immutable statics, between `start` and `stop` in this image, which
    // stays mapped while its code runs. The pointer takes exposed
    // provenance, since the bytes belong to many statics, not to `START`.
    unsafe {
        core::slice::from_raw_parts(
            core::ptr::with_exposed_provenance::<u8>(start.expose_provenance()),
            len,
        )
    }
}

/// A mod's manifest, completed from the binary's `.enshpt` section at the
/// loader's first query.
///
/// The section gives the schemas of the mod's exported points, events and
/// calls, and their exports and its exported services', `<kind>:<local
/// name>`, join the content and stat exports of its `Cargo.toml`.
#[derive(Debug)]
pub struct Manifested {
    base: &'static crate::abi::Manifest,
    id: &'static str,
    head: &'static str,
    exports: &'static [&'static str],
    built: std::sync::Mutex<Option<Built>>,
}

/// The completed manifest and the schema text and exports it points at.
#[derive(Debug)]
struct Built {
    manifest: Box<crate::abi::Manifest>,
    /// Why the mod stays off because its section does not read, as a reason
    /// in the loader's texts, when the manifest has no schemas or exports.
    unread: Option<String>,
    _text: Box<str>,
    _exports: Box<[crate::abi::Str]>,
    _names: Box<[Box<str>]>,
}

// SAFETY: `Send` is missing only for the raw pointers inside `Manifest` and
// `Str`. Each points at memory no thread writes once `build` returns: the
// text, exports and name boxes this `Built` owns, whose heap addresses stay put
// when it moves, the section and the `declare_mod!` statics of the mod's
// image (`Manifested::build`). Moving `Built` to another thread therefore
// moves only read-only views, and the mutex in `Manifested` orders its one
// free in `release` after every build.
unsafe impl Send for Built {}

impl Manifested {
    /// The manifest `base` of the mod `id`, its schema text to start with
    /// `head`, exporting the content and stat kinds `exports`.
    #[must_use]
    pub const fn new(
        base: &'static crate::abi::Manifest,
        id: &'static str,
        head: &'static str,
        exports: &'static [&'static str],
    ) -> Self {
        Self {
            base,
            id,
            head,
            exports,
            built: std::sync::Mutex::new(None),
        }
    }

    /// The manifest, built at the first call after the start or a
    /// [`release`](Self::release): the section is read and the schema text
    /// allocated then, never on later calls. Any thread may call it; it
    /// takes a lock and calls nothing that could call it again.
    ///
    /// Null when building panicked, which leaves the mod off and the game
    /// running. When the section does not read, the manifest has no
    /// schemas or exports, and [`init`] refuses the mod with the reason.
    /// The manifest stays valid until [`release`](Self::release).
    pub fn query(&self) -> *const crate::abi::Manifest {
        self.with_built(|built| core::ptr::from_ref::<crate::abi::Manifest>(&built.manifest))
            .unwrap_or(core::ptr::null())
    }

    /// `read` over the manifest, built first when it is not; `None` when
    /// building or `read` panicked.
    fn with_built<R>(&self, read: impl FnOnce(&Built) -> R) -> Option<R> {
        use crate::locks::Relock as _;
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            read(self.built.relock().get_or_insert_with(|| self.build()))
        }))
        .ok()
    }

    /// The manifest with the mod's schema text and exports; when the section
    /// does not read, with neither, keeping why.
    fn build(&self) -> Built {
        let (text, names, unread): (Box<str>, Box<[Box<str>]>, _) =
            match crate::schema_section::Section::parse(section()) {
                Ok(section) => (
                    section.schema(self.head, self.id).into_boxed_str(),
                    self.exports
                        .iter()
                        .map(|&name| name.to_owned())
                        .chain(section.exports(self.id))
                        .map(String::into_boxed_str)
                        .collect(),
                    None,
                ),
                Err(error) => (Box::default(), Box::default(), Some(unread(&error))),
            };
        let exports: Box<[crate::abi::Str]> = names
            .iter()
            .map(|name| crate::abi::Str::new(name))
            .collect();
        let manifest = Box::new(crate::abi::Manifest {
            points: crate::abi::Str::new(&text),
            exports: exports.as_ptr(),
            export_count: exports.len(),
            ..*self.base
        });
        Built {
            manifest,
            unread,
            _text: text,
            _exports: exports,
            _names: names,
        }
    }
}

/// Why a mod whose section does not read for `error` stays off, as a reason
/// in the loader's texts: an older format is the mod's to rebuild, a newer
/// one the loader's to update.
fn unread(error: &crate::schema_section::ReadSectionError) -> String {
    use crate::schema_section::{FORMAT, ReadSectionError};
    match *error {
        ReadSectionError::Format(format) => crate::i18n::reason::encode(
            if format < FORMAT {
                "mods.off.section.older"
            } else {
                "mods.off.section.newer"
            },
            &[
                ("format", format.to_string()),
                ("reads", FORMAT.to_string()),
            ],
        ),
        ReadSectionError::Damaged(byte) | ReadSectionError::NotText(byte) => {
            crate::i18n::reason::encode("mods.off.section.damaged", &[("byte", byte.to_string())])
        }
    }
}

/// The first non-empty text of `texts`, or empty.
#[must_use]
pub const fn first_text(texts: &[&'static str]) -> &'static str {
    let mut i = 0;
    while i < texts.len() {
        if !texts[i].is_empty() {
            return texts[i];
        }
        i += 1;
    }
    ""
}

/// `id`, a dependency's content id, once the loader lets this mod name
/// it, for `deps!`.
///
/// # Errors
///
/// [`Off`](crate::Off) when the loader refuses the id, as one the
/// dependency does not export.
pub fn dependency_id(id: crate::content::Id) -> Result<crate::content::Id, crate::Off> {
    crate::content::named(id)
}

/// Whether `name` is `<id>:<what>`, a name of the mod `id` itself.
#[must_use]
pub const fn is_own(id: &str, name: &str) -> bool {
    let (id, name) = (id.as_bytes(), name.as_bytes());
    name.len() > id.len() && name[id.len()] == b':' && same(name.split_at(id.len()).0, id)
}

/// Whether `code` is a language's code in text files, as
/// [`Language::code`](crate::i18n::Language::code) gives it, for `texts!`.
#[must_use]
pub const fn is_language_code(code: &str) -> bool {
    let all = crate::i18n::Language::ALL;
    let mut i = 0;
    while i < all.len() {
        if same(all[i].code().as_bytes(), code.as_bytes()) {
            return true;
        }
        i += 1;
    }
    false
}

/// Language codes joined by `], [`, built in a constant, for the
/// messages of `texts!`.
#[derive(Clone, Copy, Debug)]
pub struct Codes {
    bytes: [u8; Self::CAPACITY],
    len: usize,
}

impl Codes {
    /// Room for every code of [`Language::ALL`](crate::i18n::Language::ALL)
    /// with its separator.
    const CAPACITY: usize = 512;

    /// The codes as text, empty for none.
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match core::str::from_utf8(self.bytes.split_at(self.len).0) {
            Ok(text) => text,
            Err(_) => "",
        }
    }

    /// `code` added after the others.
    ///
    /// # Panics
    ///
    /// When the codes outgrow [`Codes::CAPACITY`], at compile time.
    const fn with(mut self, code: &str) -> Self {
        let code = code.as_bytes();
        let separator: &[u8] = if self.len == 0 { b"" } else { b"], [" };
        assert!(
            self.len + separator.len() + code.len() <= Self::CAPACITY,
            "the language codes outgrow their buffer"
        );
        let mut i = 0;
        while i < separator.len() {
            self.bytes[self.len] = separator[i];
            self.len += 1;
            i += 1;
        }
        let mut i = 0;
        while i < code.len() {
            self.bytes[self.len] = code[i];
            self.len += 1;
            i += 1;
        }
        self
    }
}

/// The languages of [`Language::REQUIRED`](crate::i18n::Language::REQUIRED)
/// that `codes`, the languages a key has a text in, lack, for `texts!`.
#[must_use]
pub const fn missing_languages(codes: &[&str]) -> Codes {
    let required = crate::i18n::Language::REQUIRED;
    let mut missing = Codes {
        bytes: [0; Codes::CAPACITY],
        len: 0,
    };
    let mut i = 0;
    while i < required.len() {
        let wanted = required[i].code();
        let mut found = false;
        let mut j = 0;
        while j < codes.len() {
            if same(codes[j].as_bytes(), wanted.as_bytes()) {
                found = true;
            }
            j += 1;
        }
        if !found {
            missing = missing.with(wanted);
        }
        i += 1;
    }
    missing
}

/// Whether two byte strings are equal, in a constant.
const fn same(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Cargo's `authors`, separated by `:`, trimmed and without empty names.
const fn author_names<const N: usize>(cargo: &'static str) -> ([crate::abi::Str; N], usize) {
    let mut out = [crate::abi::Str::EMPTY; N];
    let mut count = 0;
    let mut rest = cargo.as_bytes();
    while !rest.is_empty() {
        let mut cut = 0;
        while cut < rest.len() && rest[cut] != b':' {
            cut += 1;
        }
        let (name, tail) = rest.split_at(cut);
        // Cut at an ASCII byte and trimmed of ASCII, so still UTF-8.
        if let Ok(name) = core::str::from_utf8(name.trim_ascii())
            && !name.is_empty()
        {
            if count < N {
                out[count] = crate::abi::Str::new(name);
            }
            count += 1;
        }
        rest = match tail.split_first() {
            Some((_, after)) => after,
            None => tail,
        };
    }
    (out, count)
}

/// How many authors [`authors`] finds.
#[must_use]
pub const fn author_count(cargo: &'static str) -> usize {
    author_names::<0>(cargo).1
}

/// The authors' names for the manifest.
#[must_use]
pub const fn authors<const N: usize>(cargo: &'static str) -> [crate::abi::Str; N] {
    author_names::<N>(cargo).0
}

/// Compiles only for a `Copy` hook argument.
pub const fn hook_argument<T: Copy>() {}

/// Compiles only for a `Copy` hook return value.
pub const fn hook_return<T: Copy>() {}

#[must_use]
pub const fn or_default<T: Copy>(given: &[T], default: T) -> T {
    match given {
        [value] => *value,
        _ => default,
    }
}

pub fn fault_in(hook: &str, payload: &(dyn core::any::Any + Send)) {
    crate::host::fault_with(&format!("panic in hook {hook}"), payload);
}

/// The most detailed level the loader keeps, read at init and again at each
/// change of the loader's settings ([`refresh_levels`]).
static MAX_LEVEL: AtomicU8 = AtomicU8::new(Level::INFO.0);

/// Set once the mod's `init` has returned in this mod life.
static INITIALISED: crate::fresh::Fresh<()> = crate::fresh::Fresh::new();

/// Whether the mod's `init` has returned, so a slow first read runs inside
/// the game's frame rather than at start.
pub(crate) fn initialised() -> bool {
    INITIALISED.get().is_some()
}

/// The kept word for a data pack's `level` read in this mod life: the life
/// above the level's byte, so a word of an earlier life, or 0, reads as
/// unread.
fn tagged_level(level: Level) -> u64 {
    crate::fresh::life() << 8 | u64::from(level.0)
}

/// The level a kept word holds, if it was read in this mod life.
fn kept_level(word: u64) -> Option<u8> {
    (word >> 8 == crate::fresh::life() & (u64::MAX >> 8)).then_some(word.to_le_bytes()[0])
}

/// Each data pack's kept level word, by handle: chunk `k` holds the `2^k`
/// handles from `2^k - 1` on and is made at the first line of a pack in it,
/// so a line finds its pack's word with one bit scan, no lock and no hash.
struct PackLevels([OnceLock<Box<[AtomicU64]>>; usize::BITS as usize]);

impl PackLevels {
    /// The chunk of `pack` and its place there; `None` past every chunk.
    fn place(pack: ModHandle) -> Option<(usize, usize)> {
        let n = pack.0.checked_add(1)?;
        let chunk = n.ilog2() as usize;
        Some((chunk, n - (1 << chunk)))
    }

    /// The word of `pack`, its chunk made at the first ask.
    #[inline]
    fn word(&self, pack: ModHandle) -> Option<&AtomicU64> {
        let (chunk, at) = Self::place(pack)?;
        let words = self.0.get(chunk)?;
        match words.get() {
            Some(words) => words.get(at),
            None => Self::make(words, chunk).get(at),
        }
    }

    /// Chunk `chunk`, made in `words`.
    #[cold]
    fn make(words: &OnceLock<Box<[AtomicU64]>>, chunk: usize) -> &[AtomicU64] {
        words.get_or_init(|| (0..1usize << chunk).map(|_| AtomicU64::new(0)).collect())
    }

    /// Every word made so far, with its handle.
    fn made(&self) -> impl Iterator<Item = (ModHandle, &AtomicU64)> {
        self.0
            .iter()
            .enumerate()
            .filter_map(|(chunk, words)| Some((chunk, words.get()?)))
            .flat_map(|(chunk, words)| {
                (0..)
                    .zip(words.iter())
                    .map(move |(at, word)| (ModHandle((1 << chunk) + at - 1), word))
            })
    }
}

/// The data packs' levels the loader's own copy of the API keeps.
static PACK_LEVELS: PackLevels = PackLevels([const { OnceLock::new() }; usize::BITS as usize]);

#[inline]
#[must_use]
pub fn enabled(level: Level) -> bool {
    let max = match crate::host::pack() {
        Some(pack) => pack_level(pack),
        None => MAX_LEVEL.load(Ordering::Relaxed),
    };
    level.0 <= max
}

/// The level of the data pack `pack`, asked of the loader at its first line
/// and after [`refresh_levels`] only; out of line, so a mod's check stays
/// one load.
#[inline(never)]
fn pack_level(pack: Host) -> u8 {
    let Some(kept) = PACK_LEVELS.word(pack.handle()) else {
        return pack.log_level().0;
    };
    let word = kept.load(Ordering::Relaxed);
    match kept_level(word) {
        Some(level) => level,
        None => first_pack_level(pack, kept, word),
    }
}

/// Asks the loader for `pack`'s level and keeps it in `kept`, which held
/// `word`.
#[cold]
fn first_pack_level(pack: Host, kept: &AtomicU64, word: u64) -> u8 {
    let level = pack.log_level();
    // A refresh that landed since the load is newer than this read.
    match kept.compare_exchange(
        word,
        tagged_level(level),
        Ordering::Relaxed,
        Ordering::Relaxed,
    ) {
        Ok(_) => level.0,
        Err(newer) => kept_level(newer).unwrap_or(level.0),
    }
}

/// Rereads the levels this copy of the API keeps from the loader: the
/// process host's, and in the loader's own copy every data pack's.
///
/// A mod's copy runs it on each
/// [`Event::LOADER_SETTINGS_CHANGED`](crate::abi::Event::LOADER_SETTINGS_CHANGED);
/// the loader's own copy subscribes to nothing, so the loader calls it before
/// it raises that event.
pub fn refresh_levels() {
    let Some(own) = as_loader(crate::host) else {
        return;
    };
    MAX_LEVEL.store(own.log_level().0, Ordering::Relaxed);
    for (pack, kept) in PACK_LEVELS.made() {
        kept.store(tagged_level((own.api().log_level)(pack)), Ordering::Relaxed);
    }
}

unsafe extern "system" fn refresh_level(
    _event: crate::abi::Event,
    _data: *const core::ffi::c_void,
    _user: *mut core::ffi::c_void,
) {
    refresh_levels();
}

/// The state of the call site `site` for the data pack this thread runs,
/// so the first pack to reach a site of the loader's copy does not silence
/// it for the others; `site` itself outside a pack.
fn for_pack<T: Default + Send + Sync + 'static>(site: &'static T) -> &'static T {
    type Sites = rustc_hash::FxHashMap<(usize, usize), &'static (dyn core::any::Any + Send + Sync)>;
    static SITES: std::sync::LazyLock<std::sync::Mutex<Sites>> =
        std::sync::LazyLock::new(std::sync::Mutex::default);
    thread_local! {
        // The states this thread already found, so a line after the first
        // takes no lock.
        static SEEN: core::cell::RefCell<Sites> = core::cell::RefCell::default();
    }
    let Some(pack) = crate::host::pack() else {
        return site;
    };
    let key = (core::ptr::from_ref(site).addr(), pack.handle().0);
    let seen = SEEN.with_borrow(|seen| seen.get(&key).copied());
    // The lock is taken outside the borrow: a poisoned lock logs, and the
    // line may reach this function again on this thread.
    let state = seen.unwrap_or_else(|| {
        let state = *crate::Relock::relock(&*SITES)
            .entry(key)
            .or_insert_with(|| Box::leak(Box::new(T::default())));
        SEEN.with_borrow_mut(|seen| seen.insert(key, state));
        state
    });
    state.downcast_ref().unwrap_or(site)
}

/// A call site that logs its first time in each mod life: holds the
/// life it logged in.
#[derive(Debug)]
pub struct Once(AtomicU64);

impl Once {
    #[must_use]
    pub const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    #[inline]
    #[must_use]
    pub fn first(&'static self) -> bool {
        let this = for_pack(self);
        let life = crate::fresh::life();
        this.0.load(Ordering::Relaxed) != life && this.0.swap(life, Ordering::Relaxed) != life
    }
}

impl Default for Once {
    fn default() -> Self {
        Self::new()
    }
}

/// A call site that logs at most once per period, counting what it skips.
#[derive(Debug)]
pub struct Every {
    /// The mod life the counts belong to.
    life: AtomicU64,
    next: AtomicU64,
    skipped: AtomicU64,
}

impl Every {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            life: AtomicU64::new(0),
            next: AtomicU64::new(0),
            skipped: AtomicU64::new(0),
        }
    }

    /// The lines skipped since the last one when a line is due now.
    #[must_use]
    pub fn due(&'static self, period: Duration) -> Option<u64> {
        static EPOCH: OnceLock<Instant> = OnceLock::new();
        let this = for_pack(self);
        let now = u64::try_from(EPOCH.get_or_init(Instant::now).elapsed().as_millis())
            .unwrap_or(u64::MAX);
        let life = crate::fresh::life();
        if this.life.swap(life, Ordering::Relaxed) != life {
            this.next.store(0, Ordering::Relaxed);
            this.skipped.store(0, Ordering::Relaxed);
        }
        let next = this.next.load(Ordering::Relaxed);
        let period = u64::try_from(period.as_millis()).unwrap_or(u64::MAX);
        if now < next
            || this
                .next
                .compare_exchange(
                    next,
                    now.saturating_add(period),
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_err()
        {
            this.skipped.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        Some(this.skipped.swap(0, Ordering::Relaxed))
    }
}

impl Default for Every {
    fn default() -> Self {
        Self::new()
    }
}

pub fn log(level: Level, message: core::fmt::Arguments<'_>) {
    if !enabled(level) {
        return;
    }
    if let Some(host) = crate::host() {
        match message.as_str() {
            Some(text) => host.log(level, text),
            None => host.log(level, &message.to_string()),
        }
    }
}

/// Where `declare_mod!` keeps the mod's state once `init` made it, for
/// the process in the game and for one install under the fake host.
#[derive(Debug)]
pub struct Slot<M>(crate::fresh::Fresh<M>);

impl<M> Slot<M> {
    #[must_use]
    pub const fn new() -> Self {
        Self(crate::fresh::Fresh::new())
    }

    #[must_use]
    pub fn get(&'static self) -> Option<&'static M> {
        self.0.get()
    }
}

impl<M> Default for Slot<M> {
    fn default() -> Self {
        Self::new()
    }
}

/// The end of the loader's line for a mod it turned off, its `MOD_OFF`
/// code, so the fake host writes the same line; a loader test holds the
/// two equal.
pub const MOD_OFF: &str = "E2003, see https://enshwick.com/errors/e2003/";

/// # Safety
///
/// - `api` is null, or points at an aligned [`HostApi`] readable for the size its
///   header states.
/// - That table stays valid and unchanged for the rest of the process,
///   as the loader's process-wide table does.
pub unsafe fn init<M: Mod>(
    api: *const HostApi,
    me: ModHandle,
    manifested: &Manifested,
    texts: Option<&'static str>,
    slot: &Slot<M>,
) -> Status {
    let Some((manifest, unread)) = manifested.with_built(|built| {
        (
            core::ptr::from_ref::<crate::abi::Manifest>(&built.manifest),
            built.unread.clone(),
        )
    }) else {
        return Status::FAULTED;
    };
    // SAFETY: the built manifest is never freed once built, so it stays
    // valid for the process.
    let manifest = unsafe { &*manifest };
    // SAFETY: `Host::new` needs `api` null or a table readable for its
    // header's size that outlives the process; this function's `# Safety`
    // bullets guarantee both.
    let Some(host) = (unsafe { Host::new(api, me) }) else {
        return Status::BAD_ARGS;
    };
    let host = host.install();
    crate::host::log_panics();
    MAX_LEVEL.store(host.log_level().0, Ordering::Relaxed);
    let _watched = host.subscribe(
        crate::abi::Event::LOADER_SETTINGS_CHANGED,
        refresh_level,
        core::ptr::null_mut(),
    );
    if let Some(reason) = unread {
        host.fault(&reason);
        return Status::REFUSED;
    }
    #[cfg(feature = "test")]
    if let Some(refused) = crate::test::table::note_manifest(api, manifest) {
        host.fault(&refused);
        return Status::REFUSED;
    }
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        M::init(&mut Load::of(host, manifest.side == Side::BOTH, texts))
    }));
    let _set = INITIALISED.set(());
    match outcome {
        Ok(Ok(state)) => match slot.0.set(state) {
            Ok(()) => Status::OK,
            Err(state) => {
                let _dropped =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(state)));
                Status::REFUSED
            }
        },
        Ok(Err(off)) => {
            host.fault(off.to_reason());
            Status::REFUSED
        }
        Err(payload) => {
            let message = crate::host::panic_text(payload.as_ref());
            host.fault(&format!("init panicked: {message}"));
            Status::FAULTED
        }
    }
}

/// Runs `init` with a [`Load`] for the data pack `me`, as the loader
/// does for a pack folder; [`host`](crate::host()) answers the pack for the
/// call, and the process's host stays as it is.
///
/// # Safety
///
/// - `api` is null, or points at an aligned [`HostApi`] readable for the size its
///   header states.
/// - That table stays valid and unchanged for the rest of the process,
///   as the loader's process-wide table does.
pub unsafe fn init_pack(
    api: *const HostApi,
    me: ModHandle,
    init: impl FnOnce(&mut Load<'_>) -> Result<(), crate::Off>,
) -> Status {
    // SAFETY: `Host::new` needs `api` null or a table readable for its
    // header's size that outlives the process; this function's `# Safety`
    // bullets guarantee both.
    let Some(host) = (unsafe { Host::new(api, me) }) else {
        return Status::BAD_ARGS;
    };
    // A pack that runs again reads its level afresh; the loader refreshes it
    // on a change of its settings (`refresh_levels`).
    if let Some(kept) = PACK_LEVELS.word(me) {
        kept.store(0, Ordering::Relaxed);
    }
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        host.scope(|| init(&mut Load::new(host)))
    }));
    match outcome {
        Ok(Ok(())) => Status::OK,
        Ok(Err(off)) => {
            host.fault(off.to_reason());
            Status::REFUSED
        }
        Err(payload) => {
            let message = crate::host::panic_text(payload.as_ref());
            host.fault(&format!("init panicked: {message}"));
            Status::FAULTED
        }
    }
}

/// Makes the table `api` with the handle `me` this process's host, for
/// the loader's own copy of the API, which runs data packs.
///
/// # Safety
///
/// - `api` is null, or points at an aligned [`HostApi`] readable for the size its
///   header states.
/// - That table stays valid and unchanged for the rest of the process,
///   as the loader's process-wide table does.
pub unsafe fn install_host(api: *const HostApi, me: ModHandle) {
    // SAFETY: `Host::new` needs `api` null or a table readable for its
    // header's size that outlives the process; this function's `# Safety`
    // bullets guarantee both.
    if let Some(host) = unsafe { Host::new(api, me) } {
        MAX_LEVEL.store(host.install().log_level().0, Ordering::Relaxed);
    }
}

/// Runs `call`, the loader's call into code of the data pack `me`, with
/// [`host`](crate::host()) answering the pack on this thread, so the pack's
/// logs, settings reads and faults go out under its own handle.
///
/// # Safety
///
/// - `api` is null, or points at an aligned [`HostApi`] readable for the size its
///   header states.
/// - That table stays valid and unchanged for the rest of the process,
///   as the loader's process-wide table does.
pub unsafe fn as_pack<R>(api: *const HostApi, me: ModHandle, call: impl FnOnce() -> R) -> R {
    // SAFETY: `Host::new` needs `api` null or a table readable for its
    // header's size that outlives the process; this function's `# Safety`
    // bullets guarantee both.
    match unsafe { Host::new(api, me) } {
        Some(host) => host.scope(call),
        None => call(),
    }
}

/// Runs `call`, the loader's call into its own code or a mod DLL's, as the
/// process's host, also inside a data pack's call.
///
/// A panic or a line there is then not the pack's.
pub fn as_loader<R>(call: impl FnOnce() -> R) -> R {
    crate::host::running_as(None, call)
}

pub fn ready<M: Mod>(slot: &'static Slot<M>) {
    let (Some(state), Some(host)) = (slot.get(), crate::host()) else {
        return;
    };
    if let Err(payload) =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| state.ready(host)))
    {
        crate::host::fault_with("panic in ready", payload.as_ref());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_handle_has_its_own_place_and_the_chunks_double() {
        let places: Vec<_> = (0..7)
            .map(|h| PackLevels::place(ModHandle(h)).unwrap())
            .collect();
        assert_eq!(
            places,
            [(0, 0), (1, 0), (1, 1), (2, 0), (2, 1), (2, 2), (2, 3)]
        );
        assert_eq!(
            PackLevels::place(ModHandle(usize::MAX - 1)),
            Some((63, (1 << 63) - 1))
        );
        assert_eq!(PackLevels::place(ModHandle(usize::MAX)), None);
    }

    #[test]
    fn every_made_word_lists_under_its_own_handle() {
        let levels = PackLevels([const { OnceLock::new() }; usize::BITS as usize]);
        for h in [2, 5] {
            levels
                .word(ModHandle(h))
                .unwrap()
                .store(h as u64, Ordering::Relaxed);
        }
        let made: Vec<_> = levels
            .made()
            .map(|(h, w)| (h.0, w.load(Ordering::Relaxed)))
            .collect();
        assert_eq!(made, [(1, 0), (2, 2), (3, 0), (4, 0), (5, 5), (6, 0)]);
    }

    /// A site state whose making panics, as a panic in `Box::new` would.
    #[cfg(feature = "test")]
    #[derive(Debug)]
    struct Refused;

    #[cfg(feature = "test")]
    impl Default for Refused {
        fn default() -> Self {
            panic!("no state")
        }
    }

    #[cfg(feature = "test")]
    #[test]
    fn a_pack_s_site_after_a_poisoned_lock_logs_the_lock_and_works() {
        static BROKEN: Refused = Refused;
        static SITE: Once = Once::new();
        let loader = crate::test::FakeHost::new().install();
        // SAFETY: the fake host's table lives for the process.
        unsafe { install_host(loader.api(), ModHandle(9)) };
        let panicked = std::panic::catch_unwind(|| {
            // SAFETY: the fake host's table lives for the process.
            unsafe {
                as_pack(loader.api(), ModHandle(2), || {
                    let _state = for_pack(&BROKEN);
                });
            }
        });
        assert!(panicked.is_err());
        // SAFETY: the fake host's table lives for the process.
        let first = unsafe { as_pack(loader.api(), ModHandle(2), || SITE.first()) };
        assert!(first);
        assert!(
            loader
                .sent()
                .iter()
                .any(|(by, _, line)| *by == ModHandle(2) && line.contains("panicked holding")),
            "{:?}",
            loader.sent()
        );
    }
}
