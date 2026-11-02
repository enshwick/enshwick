pub use const_format::concatcp;
pub use serde;

pub use crate::hook::Frame;
pub use crate::script::convert::{argument, record_fields};

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::abi::{HostApi, Level, ManifestFlags, ModHandle, Status};
use crate::{Host, Load, Mod};

pub use crate::content::recipes::derive::{RecipeEdit, modify_recipes_in};
pub use crate::engine_ids::ALL as ENGINE_IDS;
/// The ids the API asks the loader for engine facts by, for the loader and
/// the catalogue to name them from one place.
pub mod ids {
    pub use crate::engine_ids::*;
}

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
    /// The manifest `base` of the mod `id`, exporting `exports`, each
    /// `<kind>:<local name>`, its schema text to start with `head`.
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
    /// Null when building panicked or the section does not read, which
    /// leaves the mod off and the game running. The manifest stays valid
    /// until [`release`](Self::release).
    pub fn query(&self) -> *const crate::abi::Manifest {
        use crate::locks::Relock as _;
        let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut built = self.built.relock();
            if built.is_none() {
                *built = Some(self.build()?);
            }
            built
                .as_ref()
                .map(|b| core::ptr::from_ref::<crate::abi::Manifest>(&b.manifest))
        }));
        built.ok().flatten().unwrap_or(core::ptr::null())
    }

    /// Frees the manifest [`query`](Self::query) built, before a
    /// development reload unloads the mod; the loader holds its own copy.
    pub fn release(&self) {
        use crate::locks::Relock as _;
        let _freed =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.built.relock().take()));
    }

    /// The manifest with the mod's schema text, or `None` when the section
    /// does not read.
    fn build(&self) -> Option<Built> {
        let section = crate::schema_section::Section::parse(section()).ok()?;
        let text = section.schema(self.head, self.id).into_boxed_str();
        let declared: Box<[Box<str>]> = section
            .exports(self.id)
            .into_iter()
            .map(String::into_boxed_str)
            .collect();
        let exports: Box<[crate::abi::Str]> = self
            .exports
            .iter()
            .copied()
            .chain(declared.iter().map(|name| &**name))
            .map(crate::abi::Str::new)
            .collect();
        let manifest = Box::new(crate::abi::Manifest {
            points: crate::abi::Str::new(&text),
            exports: exports.as_ptr(),
            export_count: exports.len(),
            ..*self.base
        });
        Some(Built {
            manifest,
            _text: text,
            _exports: exports,
            _names: declared,
        })
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

/// The most detailed level the loader keeps, read once at init.
static MAX_LEVEL: AtomicU8 = AtomicU8::new(Level::INFO.0);

#[inline]
#[must_use]
pub fn enabled(level: Level) -> bool {
    level.0 <= MAX_LEVEL.load(Ordering::Relaxed)
}

unsafe extern "system" fn refresh_level(
    _event: crate::abi::Event,
    _data: *const core::ffi::c_void,
    _user: *mut core::ffi::c_void,
) {
    if let Some(host) = crate::host() {
        MAX_LEVEL.store(host.log_level().0, Ordering::Relaxed);
    }
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
    pub fn first(&self) -> bool {
        let life = crate::fresh::life();
        self.0.load(Ordering::Relaxed) != life && self.0.swap(life, Ordering::Relaxed) != life
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
    pub fn due(&self, period: Duration) -> Option<u64> {
        static EPOCH: OnceLock<Instant> = OnceLock::new();
        let now = u64::try_from(EPOCH.get_or_init(Instant::now).elapsed().as_millis())
            .unwrap_or(u64::MAX);
        let life = crate::fresh::life();
        if self.life.swap(life, Ordering::Relaxed) != life {
            self.next.store(0, Ordering::Relaxed);
            self.skipped.store(0, Ordering::Relaxed);
        }
        let next = self.next.load(Ordering::Relaxed);
        let period = u64::try_from(period.as_millis()).unwrap_or(u64::MAX);
        if now < next
            || self
                .next
                .compare_exchange(
                    next,
                    now.saturating_add(period),
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_err()
        {
            self.skipped.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        Some(self.skipped.swap(0, Ordering::Relaxed))
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

/// # Safety
///
/// - `api` is null, or points at an aligned [`HostApi`] readable for the size its
///   header states.
/// - That table stays valid and unchanged for the rest of the process,
///   as the loader's process-wide table does.
/// - `manifest` is null, or points at a manifest valid for the call.
pub unsafe fn init<M: Mod>(
    api: *const HostApi,
    me: ModHandle,
    manifest: *const crate::abi::Manifest,
    texts: Option<&'static str>,
    slot: &Slot<M>,
) -> Status {
    // SAFETY: the caller guarantees `manifest` null or valid for the call.
    let Some(manifest) = (unsafe { manifest.as_ref() }) else {
        return Status::FAULTED;
    };
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
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        #[cfg(feature = "test")]
        crate::test::table::note_manifest(api, manifest);
        M::init(&mut Load::of(
            host,
            manifest.flags.contains(ManifestFlags::JOIN),
            texts,
        ))
    }));
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
/// does for a pack folder; the process's host stays as it is.
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
    let outcome =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| init(&mut Load::new(host))));
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

pub fn shutdown<M: Mod>(slot: &'static Slot<M>) {
    let Some(state) = slot.get() else {
        return;
    };
    if let Err(payload) =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| state.shutdown()))
    {
        crate::host::fault_with("panic in shutdown", payload.as_ref());
    }
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
