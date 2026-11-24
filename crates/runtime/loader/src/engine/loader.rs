//! The loader's state: the build, the loaded mods with their records, the
//! hook chains and the subscriptions, installed once and read by every
//! part.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick_core::abi as api_frame;
use enshwick_core::abi::{Event, EventFn, HookFlags, Level, Manifest, Side};
use enshwick_gamedata::reflect::{Fingerprint, Reflection};

use crate::engine::builds::KnownBuild;
use crate::engine::chains::{Chains, Link, Next};
use crate::engine::facts::{Fact, Facts};
use crate::engine::fuse::{Fuse, Off};
use crate::engine::mid::Mids;
use crate::engine::mod_index::ModIndex;
use crate::engine::pinned::{Constant, Symbol};
use crate::engine::registry::JoinFeatureError;
use enshwick_base::Relock;

/// A loaded mod: what the engine needs of it. What a layer above keeps of
/// it is one of its facts, read through [`ModRecord::fact`].
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
    /// Set when the mod is off: refused at init or faulted later.
    pub(crate) off: AtomicBool,
    /// Why it is off, as the mod or the loader gave it: a text of the mod's
    /// texts as its key and values, or words.
    pub(crate) off_reason: Mutex<Option<String>>,
    /// Set while one of its code patches waits for a thread to leave it: its
    /// detours and mid-function hooks are bypassed, the rest of it runs.
    pub(crate) waiting: AtomicBool,
    /// The texts the mod handed the loader, which its off reason and its
    /// features' lines are read in.
    pub(crate) texts: Mutex<Option<enshwick_core::i18n::Texts>>,
    /// Whether it is a data pack, whose code the loader's own copy of the
    /// API runs.
    pub(crate) pack: bool,
    /// The features it reported on or off, in the order it named them.
    pub(crate) features: Mutex<Vec<crate::engine::features::Feature>>,
    /// What the layers above keep of it, one fact per type, made by the
    /// parts with the record.
    pub(crate) facts: Facts,
}

impl ModRecord {
    /// What the layer that owns `T` keeps of the mod; `None` when no part
    /// made one.
    pub(crate) fn fact<T: Fact>(&self) -> Option<&T> {
        self.facts.get()
    }

    /// Takes its report that its feature `name` is off for `off`, or on when
    /// `off` is `None`, for the status summary, the Mods screen and the off
    /// line at join; logs the change, if any. It locks the texts, then the
    /// features, from any thread a build or a mod's call runs on, so no
    /// code holds the features while it locks the texts.
    pub(crate) fn report_feature(&self, name: &str, off: Option<&str>) {
        let texts = self.texts.relock();
        let changed =
            crate::engine::features::report(&mut self.features.relock(), name, off, texts.as_ref());
        drop(texts);
        if let Some(line) = changed {
            let level = if off.is_some() {
                Level::WARN
            } else {
                Level::INFO
            };
            crate::engine::log::write(level, &self.id, &line);
        }
    }

    /// Its folder in `mods/`, which holds its installation hold.
    pub(crate) fn folder(&self) -> &str {
        let file = self.file.strip_prefix("mods/").unwrap_or(&self.file);
        file.split('/').next().unwrap_or(file)
    }
}

impl ModRecord {
    /// Why it is off, in the reader's language where its texts have the
    /// reason's text; `None` while it runs.
    pub(crate) fn off_reason_shown(&self) -> Option<String> {
        let reason = self.off_reason.relock().clone()?;
        Some(crate::engine::texts::mod_reason(
            self.texts.relock().as_ref(),
            &reason,
        ))
    }

    /// Whether it runs on both sides, so it takes part in the join check
    /// while it runs.
    pub(crate) fn joins_when_on(&self) -> bool {
        self.manifest.side == Side::BOTH
    }

    /// Whether the mod runs and counts in the join check.
    pub(crate) fn joins(&self) -> bool {
        !self.off.load(Ordering::Acquire) && self.joins_when_on()
    }
}

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
    /// Whether developer tools are on, by the marker file read at start.
    pub(crate) developer_tools: bool,
    /// Loaded mods, in load order; handle `n` is index `n - 1`.
    pub(crate) mods: Vec<ModRecord>,
    /// The DLLs that run beside the mods without the API.
    pub(crate) others: Vec<crate::engine::others::OtherDll>,
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
    /// Set once the signature check finds a loader file without the release
    /// signature.
    pub(crate) unofficial: AtomicBool,
    /// How many mods were turned off so far, so a reader can tell that
    /// one was without walking every mod.
    pub(crate) offs: AtomicU64,
    /// The code patches a thread kept out at arming, tried again later.
    pub(crate) retry: crate::engine::retry::Retry,
    /// Every part of the loader's layers, which the start hands down.
    pub(crate) parts: &'static crate::engine::parts::Parts,
    /// What the layers above keep of the whole run, one fact per type, made
    /// by the start.
    pub(crate) facts: Facts,
}

/// Wires every armed chain of the running loader past the mods that are off
/// or waiting and the loader features that are off.
pub(crate) fn rewire() {
    if let Some(loader) = loader() {
        loader.rewire();
    }
}

static LOADER: OnceLock<Loader> = OnceLock::new();

/// What the loader keeps of its mods' start. Mods subscribe and run their
/// `init` once per process, so no part forgets it.
#[derive(Debug)]
struct State {
    /// The events some mod handles, a bit per event id.
    handled: AtomicU64,
    /// The index of the mod whose `init` runs now, `usize::MAX` for none.
    initialising: AtomicUsize,
}

static STATE: State = State {
    handled: AtomicU64::new(0),
    initialising: AtomicUsize::new(usize::MAX),
};

/// Whether a mod's handler takes `event`, so work that only raises it may
/// be skipped otherwise. One load, for the game's hot paths.
pub(crate) fn handles(event: Event) -> bool {
    1u64.checked_shl(event.0)
        .is_none_or(|bit| STATE.handled.load(Ordering::Relaxed) & bit != 0)
}

fn mark_handled(event: Event) {
    if let Some(bit) = 1u64.checked_shl(event.0) {
        STATE.handled.fetch_or(bit, Ordering::Relaxed);
    }
}

/// Makes `loader` the one the API table answers for.
pub(crate) fn install(loader: Loader) -> Option<&'static Loader> {
    LOADER.set(loader).ok()?;
    LOADER.get()
}

pub(crate) fn loader() -> Option<&'static Loader> {
    #[cfg(test)]
    if let Some(loader) = TEST_LOADER.get() {
        return Some(loader);
    }
    LOADER.get()
}

#[cfg(test)]
std::thread_local! {
    /// The loader one test runs, seen only on its own thread, so a test
    /// that runs a loader leaves the process's one, and every other test,
    /// alone when the tests share a process.
    static TEST_LOADER: core::cell::Cell<Option<&'static Loader>> =
        const { core::cell::Cell::new(None) };
}

/// Makes `loader` the one [`loader`] answers on this test's thread.
#[cfg(test)]
pub(crate) fn install_for_test(loader: Loader) -> &'static Loader {
    let loader: &'static Loader = Box::leak(Box::new(loader));
    TEST_LOADER.set(Some(loader));
    loader
}

/// The running loader with what a part's link found of the build, kept in
/// `found`; `None` until both are there.
pub(crate) fn linked<T>(found: &'static OnceLock<T>) -> Option<(&'static Loader, &'static T)> {
    Some((loader()?, found.get()?))
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

/// Whether the mod at index `owner` is off; the loader's own links never are.
pub(crate) fn is_off(owner: ModIndex) -> bool {
    loader().map_or(owner != ModIndex::LOADER, |loader| {
        loader.passed(owner, false)
    })
}

/// Whether the detours and mid-function hooks of `owner` are passed by: it
/// is off, or waiting for a code patch. Only atomic loads, for hook calls.
pub(crate) fn bypassed(owner: ModIndex) -> bool {
    loader().map_or(owner != ModIndex::LOADER, |loader| {
        loader.passed(owner, true)
    })
}

/// Marks `owner` waiting for a code patch; the loader never waits. A plain
/// store, so the settle may call it while it holds the chains.
pub(crate) fn wait(owner: ModIndex) {
    if let Some(record) = loader().and_then(|l| l.mods.get(owner.get())) {
        record.waiting.store(true, Ordering::Release);
    }
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

    /// Whether the code at symbol `id` is what the table expects, checked
    /// before a hook patches it or a detour relies on it, by
    /// [`crate::engine::builds::code_holds`]: the anchors made every check of a
    /// symbol while they found it (its pattern where its code starts, its
    /// `<id>.bytes`, its whole function's body), so the presence of a check
    /// counts; so does an engine route that found the symbol, whose own
    /// match checks its code. False for a symbol the build lacks.
    pub(crate) fn code_holds(&self, id: Symbol) -> bool {
        if self.address(id).is_none() {
            return false;
        }
        let checked = |present: bool| present.then_some(true);
        crate::engine::builds::code_holds(&[
            checked(self.build.head(id.id()).is_some()),
            checked(self.build.constant(&format!("{id}.bytes")).is_some()),
            checked(self.build.body(id.id()).is_some()),
            checked(self.build.routed(id.id())),
        ])
    }

    /// Links one of the loader's own detours of `feature` on `target`, ahead
    /// of every mod's link there, after checking the target: a table
    /// symbol's code must hold by [`Loader::code_holds`]. A feature arms all
    /// its links or none: a target this build lacks, code that does not hold
    /// or a refused link turns the whole feature off by [`Fuse::turn_off`],
    /// its links made before are dropped when the hooks arm, and its later
    /// links are not made. True when linked.
    pub(crate) fn link_feature<F: Copy>(
        &self,
        feature: &'static Fuse,
        target: Target<'_>,
        next: &'static Next<F>,
        detour: F,
        flags: HookFlags,
    ) -> bool {
        let Some((id, address)) = self.checked_target(feature, target) else {
            return false;
        };
        let link = Link {
            owner: ModIndex::LOADER,
            detour: Next::<F>::address_of(detour),
            next: next.slot(),
            priority: i32::MIN,
            flags,
            hop: None,
            feature: Some(feature),
        };
        let added = self.chains.relock().add(address, link);
        self.linked(feature, id, address, added)
    }

    /// Links one of the loader's own mid-function `handler`s of `feature` at
    /// `target`, ahead of every mod's handler there, checked and armed all
    /// or none as [`Loader::link_feature`] links a detour. Declare the handler
    /// with [`mid_handler!`](crate::engine::mid::mid_handler). True when
    /// linked.
    pub(crate) fn link_mid(
        &self,
        feature: &'static Fuse,
        target: Target<'_>,
        handler: api_frame::MidHookFn,
        flags: HookFlags,
    ) -> bool {
        let Some((id, address)) = self.checked_target(feature, target) else {
            return false;
        };
        let link = crate::engine::mid::MidLink {
            owner: ModIndex::LOADER,
            handler,
            user: 0,
            priority: i32::MIN,
            flags,
        };
        let added = self.mids.relock().add_fused(address, link, Some(feature));
        self.linked(feature, id, address, added)
    }

    /// The address of `target` for a link of `feature`, after its checks: a
    /// table symbol's code must hold by [`Loader::code_holds`]. `None` while
    /// the feature is off, and when a check fails, which turns it off.
    fn checked_target<'a>(
        &self,
        feature: &'static Fuse,
        target: Target<'a>,
    ) -> Option<(&'a str, u64)> {
        if feature.is_off() {
            return None;
        }
        let (id, address) = match target {
            Target::Pinned(symbol) => (
                symbol.id(),
                crate::engine::builds::hook_target(self.address(symbol), || {
                    self.code_holds(symbol)
                }),
            ),
            Target::Found(id, address) => {
                (id, crate::engine::builds::hook_target(address, || true))
            }
        };
        match address {
            Ok(address) => Some((id, address)),
            Err(why) => {
                let _off = self.fuse_off(feature, id, &why.to_string());
                None
            }
        }
    }

    /// Whether the link of `feature` on `address`, the entry `id`, was made
    /// as `added` says; a refused link turns the feature off.
    fn linked(
        &self,
        feature: &'static Fuse,
        id: &str,
        address: u64,
        added: Result<(), crate::engine::chains::LinkError>,
    ) -> bool {
        if let Err(error) = added {
            return self.fuse_off(feature, id, &format!("refused the link ({error})"));
        }
        crate::engine::diag::name(address, id);
        true
    }

    /// Turns `feature` off on this build because its entry `id` failed for
    /// `reason`, by [`Fuse::turn_off`]: one warning that names the build, the
    /// feature off on the Mods screen and in the status summary, and its
    /// links wired past. Always false, for the caller to return.
    pub(crate) fn fuse_off(&self, feature: &'static Fuse, id: &str, reason: &str) -> bool {
        let reason = self.build.reason(id, reason);
        feature.turn_off(&Off::build(feature.feature(), &self.build.name, id, reason));
        false
    }

    /// Turns off every loader feature with a detour or a mid-function hook
    /// on `target`, the entry `id` that could not be armed for `reason`, so
    /// the feature's links on other targets are passed too and no feature
    /// runs half armed.
    pub(crate) fn unarmed(&self, target: u64, id: &str, reason: &str) {
        let mut features = self.chains.relock().features_at(target);
        features.extend(self.mids.relock().features_at(target));
        for feature in features {
            let _off = self.fuse_off(feature, id, reason);
        }
    }

    /// Wires every armed chain past the mods that are off or waiting for a
    /// code patch and the loader features that are off.
    pub(crate) fn rewire(&self) {
        // A poisoned lock still holds the chains: they must be wired past.
        let chains = self.chains.relock();
        chains.rewire(|owner| self.passed(owner, true));
    }

    /// Whether the hooks and points of `owner` are passed by: the mod is
    /// off or unknown, or with `waiting`, it waits for a code patch. The
    /// loader's own links never are. The one rule [`is_off`] and
    /// [`bypassed`] follow too; only atomic loads, for hook calls.
    #[inline]
    pub(crate) fn passed(&self, owner: ModIndex, waiting: bool) -> bool {
        owner != ModIndex::LOADER
            && self.mods.get(owner.get()).is_none_or(|m| {
                m.off.load(Ordering::Acquire) || (waiting && m.waiting.load(Ordering::Acquire))
            })
    }

    /// Whether a mod's registration may still join: every one is too late
    /// once the loader delivers its first events. The one place that rule
    /// lives; a feature's own seal may close it earlier.
    ///
    /// # Errors
    ///
    /// [`JoinFeatureError::TooLate`] after the delivery.
    pub(crate) fn before_delivery(&self) -> Result<(), JoinFeatureError> {
        match self.delivery.get() {
            Some(_) => Err(JoinFeatureError::TooLate),
            None => Ok(()),
        }
    }

    /// The mod named `id`, as a player or an administrator names it.
    pub(crate) fn find_mod(&self, id: &str) -> Option<(usize, &ModRecord)> {
        self.mods.iter().enumerate().find(|(_, m)| m.id == id)
    }

    /// How many mods were turned off so far.
    pub(crate) fn offs(&self) -> u64 {
        self.offs.load(Ordering::Acquire)
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
        let _count = self.offs.fetch_add(1, Ordering::AcqRel);
        let shown = record.off_reason_shown().unwrap_or_default();
        crate::engine::log::write(
            Level::ERROR,
            &record.id,
            &enshwick_base::log_files::off(&shown, crate::engine::errors::MOD_OFF),
        );
        crate::engine::diag::turned_off(&record.id, &shown);
        self.rewire();
        self.parts.turned(self, record);
    }

    /// Turns a mod the loader turned off for a world back on: relinks its
    /// hooks and points and logs it. Only for a mod whose code never ran
    /// while it was off, so nothing of it changed.
    pub(crate) fn turn_back_on(&self, index: ModIndex) {
        let Some(record) = self.mods.get(index.get()) else {
            return;
        };
        if !record.off.swap(false, Ordering::AcqRel) {
            return;
        }
        *record.off_reason.relock() = None;
        crate::engine::log::write(Level::INFO, &record.id, "back on");
        self.rewire();
        self.parts.turned(self, record);
    }

    /// What the layer that owns `T` keeps of the whole run; `None` when the
    /// start made none.
    pub(crate) fn fact<T: Fact>(&self) -> Option<&T> {
        self.facts.get()
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
        let followed = is_world_event(event)
            && self
                .parts
                .iter()
                .filter_map(|part| part.follows)
                .any(|follows| follows(self));
        followed
            || self
                .delivery
                .get()
                .is_some_and(|d| d.iter().any(|s| s.event == event))
    }

    /// The subscriptions to `event` of mods still on, or of mod `only`.
    fn live(&self, event: Event, only: Option<usize>) -> impl Iterator<Item = &Subscription> {
        let delivery = self.delivery.get().map_or(&[][..], |d| of_event(d, event));
        delivery.iter().filter(move |s| {
            s.event == event
                && only.is_none_or(|only| only == s.owner.get())
                && self
                    .mods
                    .get(s.owner.get())
                    .is_some_and(|m| !m.off.load(Ordering::Acquire))
        })
    }

    /// Whether a mod still on hears `event`, so work that only feeds it
    /// stops once its last subscriber is turned off.
    pub(crate) fn hears(&self, event: Event) -> bool {
        handles(event) && self.live(event, None).next().is_some()
    }

    /// Raises `event` with `data` for every mod still on, or for mod `only`.
    pub(crate) fn emit(&self, event: Event, only: Option<usize>, data: *const core::ffi::c_void) {
        if only.is_none() && is_world_event(event) {
            self.parts.world_event(self, event);
        }
        for subscription in self.live(event, only) {
            crate::engine::profile::call(
                subscription.owner,
                crate::engine::profile::Feature::Events,
                // SAFETY: the handler lives in the mod's DLL, which stays
                // loaded; the safe layer catches its panics.
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
}

/// Whether `event` is one of the three the parts that follow the world act
/// on: loaded, saved and shutdown.
fn is_world_event(event: Event) -> bool {
    matches!(
        event,
        Event::WORLD_LOADED | Event::WORLD_SAVED | Event::SHUTDOWN_STARTED
    )
}

/// Runs `init` of mod `index`, which [`in_init`] then answers for.
pub(crate) fn initialising<R>(index: usize, init: impl FnOnce() -> R) -> R {
    STATE.initialising.store(index, Ordering::Release);
    let outcome = init();
    STATE.initialising.store(usize::MAX, Ordering::Release);
    outcome
}

/// Whether mod `index` is running its `init` now.
pub(crate) fn in_init(index: usize) -> bool {
    STATE.initialising.load(Ordering::Acquire) == index
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
mod tests;

/// The subscriptions to `event` among `sealed`, which are sorted by event.
fn of_event(sealed: &[Subscription], event: Event) -> &[Subscription] {
    let first = sealed.partition_point(|s| s.event.0 < event.0);
    let end = sealed.partition_point(|s| s.event.0 <= event.0);
    sealed.get(first..end).unwrap_or_default()
}
