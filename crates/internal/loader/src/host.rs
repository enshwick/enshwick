//! The loader's side of the API: its state and the `HostApi` table.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick::abi as api_frame;
use enshwick::abi::{
    Event, EventFn, HookFlags, Level, Manifest, ManifestFlags, ModHandle, Side, Status, Str,
};
use enshwick_gamedata::reflect::{Fingerprint, Reflection};

use crate::builds::KnownBuild;
use crate::chains::{Chains, Link, Next};
use crate::locks::Relock;
use crate::mid::Mids;
use crate::mod_index::ModIndex;
use crate::pinned::{Constant, Symbol};

mod api;
mod entries;
mod join;
mod memory;

pub(crate) use api::API;
pub(crate) use entries::mods::{CheckSettingsError, check_settings};
pub(crate) use join::{
    apply_join_values, apply_server_value, join_mods, join_mods_off, join_values,
    report_join_missing, turn_off_mod,
};
pub(crate) use memory::{find, slice};

/// A loaded mod.
#[derive(Debug)]
pub(crate) struct ModRecord {
    /// The mod id.
    pub(crate) id: String,
    /// Its version.
    pub(crate) version: String,
    /// Its DLL relative to the game's folder, with `/` separators.
    pub(crate) file: String,
    /// Its manifest, copied out of its DLL.
    pub(crate) manifest: &'static Manifest,
    /// What its manifest tells players about it.
    pub(crate) meta: crate::manifest::Meta,
    /// Notes on its relations, such as a recommended mod that is missing.
    pub(crate) notes: Vec<String>,
    /// Set when the mod is off: refused at init or faulted later.
    pub(crate) off: AtomicBool,
    /// Why it is off, as the mod or the loader gave it: a text of the mod's
    /// texts as its key and values, or words.
    pub(crate) off_reason: Mutex<Option<String>>,
    /// The settings texts handed to the mod.
    pub(crate) settings: Kept,
    /// The join values the mod last reported, as its settings file holds them.
    pub(crate) synced: Mutex<Option<String>>,
    /// On a client, the joined server's join values for the mod.
    pub(crate) server_synced: Mutex<Option<String>>,
    /// The texts `sync` handed to the mod.
    pub(crate) sync_out: Kept,
    /// The settings schema the mod handed the loader.
    pub(crate) schema: Mutex<Option<enshwick::config::Schema>>,
    /// The mod's check of new settings texts: callback and pointer.
    pub(crate) check: OnceLock<(api_frame::ConfigCheckFn, usize)>,
    /// The texts the mod handed the loader, for its settings' labels.
    pub(crate) texts: Mutex<Option<enshwick::i18n::Texts>>,
    /// Whether it is a data pack, a folder of data the loader runs.
    pub(crate) pack: bool,
    /// The features it reported on or off, in the order it named them.
    pub(crate) features: Mutex<Vec<crate::features::Feature>>,
}

impl ModRecord {
    /// Whether its manifest requires the mod `id`.
    pub(crate) fn requires(&self, id: &str) -> bool {
        // SAFETY: a record's manifest is the loader's copy, kept for the
        // process with every string and slice in it.
        unsafe { crate::manifest::parse(self.manifest) }.is_ok_and(|candidate| {
            candidate
                .dependencies
                .iter()
                .any(|d| d.relation == api_frame::Relation::REQUIRES && d.id == id)
        })
    }

    /// The local names its manifest exports, for other mods to use.
    pub(crate) fn exports(&self) -> Vec<&str> {
        // SAFETY: a record's manifest is the loader's copy, kept for the
        // process with every string and slice in it.
        unsafe { crate::manifest::exports(self.manifest) }
    }

    /// Why it is off, in the reader's language where its texts have the
    /// reason's text; `None` while it runs.
    pub(crate) fn off_reason_shown(&self) -> Option<String> {
        let reason = self.off_reason.relock().clone()?;
        Some(crate::configure::mod_reason(
            self.texts.relock().as_ref(),
            &reason,
        ))
    }

    /// The name players read, from the mod's texts in their language, else
    /// from its manifest; empty when neither names it.
    pub(crate) fn name(&self) -> String {
        self.meta
            .name_in(self.texts.relock().as_ref(), crate::texts::language())
    }

    /// The name players read, or the id when nothing names the mod.
    pub(crate) fn label(&self) -> String {
        enshwick::mods::label(&self.name(), &self.id)
    }

    /// What the mod does, from its texts in the player's language, else
    /// from its manifest.
    pub(crate) fn description(&self) -> String {
        self.meta
            .description_in(self.texts.relock().as_ref(), crate::texts::language())
    }

    /// Whether it takes part in the join check while it runs.
    pub(crate) fn joins_when_on(&self) -> bool {
        self.manifest.flags.contains(ManifestFlags::JOIN)
    }

    /// Whether the mod runs and counts in the join check.
    pub(crate) fn joins(&self) -> bool {
        !self.off.load(Ordering::Acquire) && self.manifest.flags.contains(ManifestFlags::JOIN)
    }

    /// The version the join check compares: a pack's carries the hash of its
    /// files, so other files of one version differ.
    pub(crate) fn join_version(&self) -> String {
        match self.pack.then(|| crate::packs::hash_of(&self.id)).flatten() {
            Some(hash) => format!("{}+{hash}", self.version),
            None => self.version.clone(),
        }
    }
}

/// Texts handed to a mod, kept for the life of the process: the mod may still
/// read one on a thread of its own while another thread asks for a newer one.
/// An equal text handed out before is handed out again, so a file read over
/// and over keeps one copy.
#[derive(Debug, Default)]
pub(crate) struct Kept(Mutex<Vec<Box<str>>>);

impl Kept {
    /// `text`, kept for good.
    pub(crate) fn keep(&self, text: &str) -> Str {
        let mut kept = self.0.relock();
        let at = kept.iter().position(|k| **k == *text).unwrap_or_else(|| {
            kept.push(text.into());
            kept.len() - 1
        });
        // The box's heap text never moves or drops while the store lives,
        // whatever the vector does with the boxes.
        Str::new(&kept[at])
    }
}

/// The handle of the loader itself, which its own copy of the API uses to
/// log and read while it runs data packs.
pub(crate) const LOADER_HANDLE: ModHandle = ModHandle(0);

/// A mod's handler for one event.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Subscription {
    /// The subscribing mod's index.
    pub(crate) owner: ModIndex,
    /// The event.
    pub(crate) event: Event,
    /// The handler, inside the mod's DLL.
    pub(crate) callback: EventFn,
    /// The mod's pointer for the handler, as an address.
    pub(crate) user: usize,
    /// The subscribing mod's reload generation; events of an older one are
    /// dropped.
    pub(crate) generation: u32,
}

/// Everything the loader knows once mods are resolved.
#[derive(Debug)]
pub(crate) struct Loader {
    /// Server or client.
    pub(crate) side: Side,
    /// The game's image base.
    pub(crate) base: u64,
    /// The game build.
    pub(crate) fingerprint: Fingerprint,
    /// The build table row.
    pub(crate) build: &'static KnownBuild,
    /// The game's reflection.
    pub(crate) reflection: Reflection<'static>,
    /// The `mods` folder.
    pub(crate) mods_dir: PathBuf,
    /// The contract of the stub that started the loader; 0 in a dry run.
    pub(crate) stub: u32,
    /// The `mod-configs` folder.
    pub(crate) configs_dir: PathBuf,
    /// The `mod-data` folder.
    pub(crate) data_dir: PathBuf,
    /// Whether developer tools are on, by the marker file read at start.
    pub(crate) developer_tools: bool,
    /// Loaded mods, in load order; handle `n` is index `n - 1`.
    pub(crate) mods: Vec<ModRecord>,
    /// The DLLs that run beside the mods without the API.
    pub(crate) others: Vec<crate::others::OtherDll>,
    /// Every hook chain.
    pub(crate) chains: Mutex<Chains>,
    /// Every mid-function hook.
    pub(crate) mids: Mutex<Mids>,
    /// Reads the caller frame of the running chain.
    pub(crate) caller_frame: fn() -> api_frame::CallerFrame,
    /// Subscriptions while mods initialise.
    pub(crate) subscriptions: Mutex<Vec<Subscription>>,
    /// The subscriptions once every mod is initialised, read without a lock.
    pub(crate) delivery: OnceLock<Box<[Subscription]>>,
    /// What was wrong in the loader settings file at start, a line each.
    pub(crate) settings_problems: Vec<String>,
}

/// The warning line and the status entry of `feature` turned off on `build`
/// because its entry `id` failed for `reason`, as [`Loader::feature_off`]
/// reports them.
pub(crate) fn off_report(
    build: &KnownBuild,
    feature: &str,
    id: &str,
    reason: &str,
) -> (String, String) {
    let reason = build.reason(id, reason);
    (
        crate::builds::off_line(feature, &build.name, id, reason),
        format!("{id} {reason}"),
    )
}

/// Logs the warning line of an [`off_report`] and lists `feature` with its
/// status entry among the loader parts off, on the Mods screen and in the
/// status summary. Always false, for the caller to return.
pub(crate) fn report_off(feature: &str, (line, entry): (String, String)) -> bool {
    crate::log!(WARN, "{line}");
    crate::diag::part_off(feature, &entry);
    false
}

static LOADER: OnceLock<Loader> = OnceLock::new();
/// The events some mod handles, a bit per event id.
static HANDLED: AtomicU64 = AtomicU64::new(0);

/// Whether a mod's handler takes `event`, so work that only raises it may
/// be skipped otherwise. One load, for the game's hot paths.
pub(crate) fn handles(event: Event) -> bool {
    1u64.checked_shl(event.0)
        .is_none_or(|bit| HANDLED.load(Ordering::Relaxed) & bit != 0)
}

fn mark_handled(event: Event) {
    if let Some(bit) = 1u64.checked_shl(event.0) {
        HANDLED.fetch_or(bit, Ordering::Relaxed);
    }
}

/// Makes `loader` the one the API table answers for.
pub(crate) fn install(loader: Loader) -> Option<&'static Loader> {
    LOADER.set(loader).ok()?;
    LOADER.get()
}

pub(crate) fn loader() -> Option<&'static Loader> {
    LOADER.get()
}

/// Where one of the loader's detours goes, and how the code there is
/// checked before [`Loader::link_feature`] links it.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Target<'a> {
    /// A build table symbol, whose code [`Loader::code_holds`] checks.
    Pinned(Symbol),
    /// An address the caller found and checked by name, named for the log:
    /// a reflected system's entry, a function the caller's own code
    /// pattern found, an export of a system library.
    Found(&'a str, Option<u64>),
}

/// A mod's callback with the pointer it registered beside it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Registered<F> {
    /// The owning mod's index.
    pub(crate) owner: ModIndex,
    /// The callback, inside the mod's DLL, which stays loaded.
    pub(crate) callback: F,
    /// The pointer the callback takes, as an address.
    pub(crate) user: usize,
}

impl<F> Registered<F> {
    pub(crate) fn new(owner: ModIndex, callback: F, user: *mut core::ffi::c_void) -> Self {
        Self {
            owner,
            callback,
            user: user.expose_provenance(),
        }
    }

    /// The pointer the mod registered with the callback.
    pub(crate) fn user(&self) -> *mut core::ffi::c_void {
        core::ptr::with_exposed_provenance_mut(self.user)
    }

    /// Whether its mod still runs.
    pub(crate) fn live(&self) -> bool {
        !is_off(self.owner)
    }
}

/// Takes what mods registered during init, for sealing.
pub(crate) fn take<T>(list: &Mutex<Vec<T>>) -> Vec<T> {
    std::mem::take(&mut *list.relock())
}

/// Whether the mod at index `owner` is off; the loader's own links never are.
pub(crate) fn is_off(owner: ModIndex) -> bool {
    owner != ModIndex::LOADER
        && loader().is_none_or(|loader| {
            loader
                .mods
                .get(owner.get())
                .is_none_or(|m| m.off.load(Ordering::Acquire))
        })
}

impl Loader {
    /// The address of the build table's symbol `id` in the running game.
    pub(crate) fn address(&self, id: Symbol) -> Option<u64> {
        self.address_by_name(id.id())
    }

    /// The build table's constant `id` as an offset or a size.
    pub(crate) fn offset(&self, id: Constant) -> Option<usize> {
        self.offset_by_name(id.id())
    }

    /// The address of the symbol `name`, an id put together at run time.
    pub(crate) fn address_by_name(&self, name: &str) -> Option<u64> {
        self.build.symbol(name).map(|rva| self.base + rva)
    }

    /// The constant `name`, an id put together at run time, as an offset or
    /// a size.
    pub(crate) fn offset_by_name(&self, name: &str) -> Option<usize> {
        self.build
            .constant(name)
            .and_then(|value| usize::try_from(value).ok())
    }

    /// Links one of the loader's own detours on `target`, ahead of every
    /// mod's link there.
    pub(crate) fn link<F: Copy>(
        &self,
        target: u64,
        next: &'static Next<F>,
        detour: F,
        flags: HookFlags,
    ) -> Status {
        let link = Link {
            owner: ModIndex::LOADER,
            detour: Next::<F>::address_of(detour),
            next: next.slot(),
            priority: i32::MIN,
            flags,
            hop: None,
        };
        self.chains.relock().add(target, link)
    }

    /// Whether the code at symbol `id` is what the table expects, checked
    /// before a hook patches it or a detour relies on it, by
    /// [`crate::builds::code_holds`]: on the build the table was made for,
    /// the symbol's pattern matches where its code starts, when its pattern
    /// finds it directly, its first eight bytes are the table's
    /// `<id>.bytes`, when the table pins them, and its whole function holds
    /// the table's body, when the table has one. A table carried over already
    /// made every check while it found the symbol, so there only the
    /// presence of a check counts. False for a symbol the build lacks.
    pub(crate) fn code_holds(&self, id: Symbol) -> bool {
        let Some(address) = self.address(id) else {
            return false;
        };
        let exact = self.build.kind == crate::builds::Kind::Exact;
        let head = self.build.head(id.id()).map(|(pattern, at)| {
            !exact || {
                let Some(start) = address
                    .checked_sub(at)
                    .and_then(|a| usize::try_from(a).ok())
                else {
                    return false;
                };
                let mut code = vec![0u8; pattern.len()];
                // SAFETY: `start` is the table's `id` less the distance its
                // pattern starts before it, code inside the game's image,
                // which stays mapped and readable for the life of the
                // process; a pattern spans code of the function it was made
                // from, which the table checks against its build.
                unsafe { crate::memory::read_bytes(start, 0, &mut code) };
                pattern.matches(&code)
            }
        });
        let bytes = self.build.constant(&format!("{id}.bytes")).map(|expected| {
            !exact || {
                let Ok(code) = usize::try_from(address) else {
                    return false;
                };
                // SAFETY: `code` is the table's `id`, eight bytes of code
                // inside the game's image, which stays mapped and
                // readable for the life of the process and is not
                // patched before this check.
                let found: u64 = unsafe { crate::memory::read(code, 0) };
                found == expected
            }
        });
        let body = self.build.body(id.id()).map(|body| {
            !exact || {
                let Ok(start) = usize::try_from(address) else {
                    return false;
                };
                let mut code = vec![0u8; body.len()];
                // SAFETY: `start` is the table's `id`, the first byte of a
                // function of `body.len()` bytes inside the game's image,
                // which stays mapped and readable for the life of the
                // process and is not patched before this check.
                unsafe { crate::memory::read_bytes(start, 0, &mut code) };
                body.holds(&code)
            }
        });
        crate::builds::code_holds(&[head, bytes, body])
    }

    /// [`Loader::link`] for a feature, after checking its target: a table
    /// symbol's code must hold by [`Loader::code_holds`]. A target this
    /// build lacks, code that does not hold or a refused link turns only
    /// `feature` off, named by [`Loader::feature_off`]. True when linked.
    pub(crate) fn link_feature<F: Copy>(
        &self,
        feature: &str,
        target: Target<'_>,
        next: &'static Next<F>,
        detour: F,
        flags: HookFlags,
    ) -> bool {
        let (id, address) = match target {
            Target::Pinned(symbol) => (
                symbol.id(),
                crate::builds::hook_target(self.address(symbol), || self.code_holds(symbol)),
            ),
            Target::Found(id, address) => (id, crate::builds::hook_target(address, || true)),
        };
        let address = match address {
            Ok(address) => address,
            Err(why) => return self.feature_off(feature, id, &why.to_string()),
        };
        let status = self.link(address, next, detour, flags);
        if !status.is_ok() {
            return self.feature_off(feature, id, &format!("refused the link ({})", status.0));
        }
        crate::diag::name(address, id);
        true
    }

    /// Turns `feature` off on this build because its entry `id` failed for
    /// `reason`: one warning that names the build, and the part off on the
    /// Mods screen and in the status summary. Always false, for the caller
    /// to return.
    pub(crate) fn feature_off(&self, feature: &str, id: &str, reason: &str) -> bool {
        report_off(feature, off_report(self.build, feature, id, reason))
    }

    /// The mod named `id`, as a player or an administrator names it.
    pub(crate) fn find_mod(&self, id: &str) -> Option<(usize, &ModRecord)> {
        self.mods.iter().enumerate().find(|(_, m)| m.id == id)
    }

    /// The mod of handle `me`; while it is on, the table function it is
    /// inside counts against it for a reload, whenever it calls.
    fn record(&self, me: ModHandle) -> Option<(ModIndex, &ModRecord)> {
        let index = ModIndex::from_handle(me)?;
        let record = self.mods.get(index.get())?;
        if !record.off.load(Ordering::Relaxed) {
            crate::reload::note(index.get());
        }
        Some((index, record))
    }

    /// Turns a mod off: unlinks its hooks and logs why.
    pub(crate) fn turn_off(&self, index: ModIndex, reason: &str) {
        let Some(record) = self.mods.get(index.get()) else {
            return;
        };
        if record.off.swap(true, Ordering::AcqRel) {
            return;
        }
        *record.off_reason.relock() = Some(reason.to_owned());
        let shown = record.off_reason_shown().unwrap_or_default();
        crate::log::write(
            Level::ERROR,
            &record.id,
            &format!("off: {shown} ({})", crate::errors::MOD_OFF),
        );
        if record.pack {
            crate::packs::forget(&record.id);
        }
        crate::diag::turned_off(&record.id, &shown);
        // A poisoned lock still holds the chains: the mod must be wired past.
        let chains = self.chains.relock();
        let faulted = |owner: ModIndex| {
            owner != ModIndex::LOADER
                && self
                    .mods
                    .get(owner.get())
                    .is_none_or(|m| m.off.load(Ordering::Acquire))
        };
        chains.rewire(faulted);
        drop(chains);
        crate::points::rearm(faulted);
        if record.joins_when_on() {
            crate::advertise::wake();
        }
    }

    /// Ends subscribing: from now on events go to the subscriptions made so
    /// far, grouped by event in subscription order, so a raise walks its own.
    pub(crate) fn seal_subscriptions(&self) {
        let mut made = std::mem::take(&mut *self.subscriptions.relock());
        made.sort_by_key(|s| s.event.0);
        for subscription in &made {
            mark_handled(subscription.event);
        }
        let _first = self.delivery.set(made.into_boxed_slice());
    }

    /// Whether a mod subscribed to `event`.
    pub(crate) fn subscribed(&self, event: Event) -> bool {
        let item_data = matches!(
            event,
            Event::WORLD_LOADED | Event::WORLD_SAVED | Event::SHUTDOWN_STARTED
        ) && (crate::item_data::wanted()
            || crate::stats::wanted()
            || crate::data_changes::wanted(self)
            || crate::rate_links::recipes_rated());
        item_data
            || self
                .delivery
                .get()
                .is_some_and(|d| d.iter().any(|s| s.event == event))
            || crate::reload::late().is_some_and(|late| late.iter().any(|s| s.event == event))
    }

    /// Raises `event` with `data` for every mod still on, or for mod `only`.
    pub(crate) fn emit(&self, event: Event, only: Option<usize>, data: *const core::ffi::c_void) {
        // The world followers act on these three events alone.
        if only.is_none()
            && matches!(
                event,
                Event::WORLD_LOADED | Event::WORLD_SAVED | Event::SHUTDOWN_STARTED
            )
        {
            crate::game_settings::world_event(self, event);
            crate::knowledge::changes::world_event(self, event);
            crate::item_data::world_event(self, event);
            crate::stats::world_event(self, event);
            crate::data_changes::world_event(self, event);
            crate::rate_links::world_event(event);
        }
        let Some(delivery) = self.delivery.get() else {
            return;
        };
        let late = crate::reload::late();
        let late = late.as_ref().map_or(&[][..], |late| late.as_slice());
        for subscription in of_event(delivery, event).iter().chain(late).filter(|s| {
            s.event == event
                && only.is_none_or(|only| only == s.owner.get())
                && self
                    .mods
                    .get(s.owner.get())
                    .is_some_and(|m| !m.off.load(Ordering::Acquire))
                && s.generation == crate::reload::generation(s.owner.get())
        }) {
            // SAFETY: the handler lives in the mod's DLL, which stays loaded;
            // the safe layer catches its panics.
            crate::profile::call(
                subscription.owner,
                crate::profile::Feature::Events,
                || unsafe {
                    (subscription.callback)(
                        event,
                        data,
                        core::ptr::with_exposed_provenance_mut(subscription.user),
                    );
                },
            );
        }
    }

    /// The mod's own build rows that match the running build.
    fn own_rows(
        &self,
        index: usize,
    ) -> impl Iterator<Item = &'static enshwick::abi::BuildRow> + '_ {
        let manifest = self.mods.get(index).map(|record| record.manifest);
        let rows: &'static [enshwick::abi::BuildRow] = match manifest {
            Some(m) if !m.builds.is_null() => {
                // SAFETY: `manifest::read` refused every manifest shorter
                // than `Manifest`, so `builds` and `build_count` were read
                // from the mod, and `manifest::own` copied the rows into
                // leaked loader memory: `build_count` rows, `'static`.
                unsafe { core::slice::from_raw_parts(m.builds, m.build_count) }
            }
            _ => &[],
        };
        rows.iter().filter(|row| {
            row.side == self.side
                && row.fingerprint.guid == self.fingerprint.guid()
                && row.fingerprint.age == self.fingerprint.age()
        })
    }

    fn resolve_symbol(&self, index: usize, name: &str) -> Option<u64> {
        let own = self.own_rows(index).find_map(|row| {
            // SAFETY: a manifest's build row points at `symbol_count` symbols in
            // the mod's statics, and the mod's DLL stays loaded for the process.
            let symbols = unsafe { slice(row.symbols, row.symbol_count) };
            find(symbols, name)
        });
        own.or_else(|| self.build.symbol(name))
            .map(|rva| self.base + rva)
    }

    fn resolve_constant(&self, index: usize, name: &str) -> Option<u64> {
        let own = self.own_rows(index).find_map(|row| {
            // SAFETY: a manifest's build row points at `constant_count`
            // constants in the mod's statics, and the mod's DLL stays loaded for
            // the process.
            let constants = unsafe { slice(row.constants, row.constant_count) };
            find(constants, name)
        });
        own.or_else(|| self.build.constant(name))
            .or_else(|| crate::catalogue::constant(name, &self.reflection))
    }
}

/// The loader and the calling mod, when the mod is on.
/// What an entry point answers when the loader's own code in it panicked.
trait Fallback {
    const FALLBACK: Self;
}

impl Fallback for Status {
    const FALLBACK: Self = Self::REFUSED;
}

impl Fallback for () {
    const FALLBACK: Self = ();
}

impl Fallback for Level {
    const FALLBACK: Self = Self::INFO;
}

impl Fallback for u8 {
    const FALLBACK: Self = 0;
}

impl Fallback for u32 {
    const FALLBACK: Self = 0;
}

impl Fallback for u64 {
    const FALLBACK: Self = 0;
}

/// Runs the body of the entry point `name`, so a panic in the loader's own
/// code never unwinds into the mod that called it: the call is refused and
/// the fault logged.
fn entry<R: Fallback>(name: &'static str, body: impl FnOnce() -> R) -> R {
    crate::reload::entered(name, || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(body))
            .unwrap_or_else(|_| entry_fault(name))
    })
}

#[cold]
fn entry_fault<R: Fallback>(name: &str) -> R {
    crate::log!(ERROR, "{name}: the loader panicked; the call is refused");
    R::FALLBACK
}

/// The calling mod of an entry point, with its index and record: refused
/// with [`Status::BAD_ARGS`] for a handle the loader never gave out and with
/// [`Status::FAULTED`] for a mod that is off, the one answer every entry
/// point gives a mod that is off.
fn calling(me: ModHandle) -> Result<(&'static Loader, ModIndex, &'static ModRecord), Status> {
    let loader = loader().ok_or(Status::BAD_ARGS)?;
    let (index, record) = loader.record(me).ok_or(Status::BAD_ARGS)?;
    if record.off.load(Ordering::Acquire) {
        return Err(Status::FAULTED);
    }
    Ok((loader, index, record))
}

/// [`calling`], for an entry point that needs only the loader; the
/// loader's own handle passes too.
fn caller(me: ModHandle) -> Result<&'static Loader, Status> {
    if me == LOADER_HANDLE {
        crate::reload::note_initialising();
        return loader().ok_or(Status::BAD_ARGS);
    }
    calling(me).map(|(loader, _, _)| loader)
}

/// [`calling`], for an entry point that needs the loader and the index.
fn caller_index(me: ModHandle) -> Result<(&'static Loader, ModIndex), Status> {
    calling(me).map(|(loader, index, _)| (loader, index))
}

/// The handle of the mod at `index`.
#[must_use]
pub(crate) fn handle(index: ModIndex) -> ModHandle {
    index.handle()
}

impl core::ops::Index<ModIndex> for [ModRecord] {
    type Output = ModRecord;

    fn index(&self, index: ModIndex) -> &ModRecord {
        &self[index.get()]
    }
}

impl core::ops::Index<ModIndex> for Vec<ModRecord> {
    type Output = ModRecord;

    fn index(&self, index: ModIndex) -> &ModRecord {
        &self[index.get()]
    }
}

#[cfg(test)]
mod headed_tests;

/// The subscriptions to `event` among `sealed`, which are sorted by event.
fn of_event(sealed: &[Subscription], event: Event) -> &[Subscription] {
    let first = sealed.partition_point(|s| s.event.0 < event.0);
    let end = sealed.partition_point(|s| s.event.0 <= event.0);
    sealed.get(first..end).unwrap_or_default()
}

#[cfg(test)]
mod tests;
