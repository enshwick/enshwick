//! Bases past 16: the grown bases objects, their window records from create
//! to destroy, and every other site the window needs, found and armed all
//! together or not at all.
//!
//! Both create functions allocate the object with one size immediate, which
//! grows to [`window::BLOCK`]; their hooks fill the window records, and the
//! destroy hooks free them first. A miss of any site leaves the game its
//! 16 bases, with the zones still raised. Server 399 and client 426, where
//! the bodies are the same.

use core::ops::Range;
use core::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use enshwick::abi::{HookFlags, Side};
use enshwick_gamedata::Resolved;

use super::Fields;
use super::lifecycle::{self, Code, CtorFn, SlotFreeFn, SlotInitFn, TeardownFn};
use super::loops;
use crate::base_groups::window;
use crate::builds::Unhooked;
use crate::chains::Next;
use crate::code::{CodePatch, Verdict};
use crate::host::{Loader, Target};
use crate::mod_index::ModIndex;
use crate::pinned::{Constant, Symbol, bases};

/// Bases past 16, as the log and the Mods screen name them when a build
/// lacks a site.
const PAST_16: &str = "bases past 16";
/// `mov edx, imm32`, which each create loads the object's size with.
const MOV_EDX: u8 = 0xBA;
/// The base count the window's code reads, `[obj+0x24]` in the count
/// getter and the cleanups.
pub(super) const COUNT_AT: usize = 0x24;

type CreateFn = unsafe extern "system" fn(allocator: *mut u8, part: *mut u8) -> *mut u8;
type DestroyFn = unsafe extern "system" fn(obj: *mut u8) -> u64;

/// Why a site of the window is not found.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub(super) enum SiteError {
    /// The build table lacks an entry.
    #[error("{0} is not in this build")]
    Missing(&'static str),
    /// A create does not load the size the table names exactly once.
    #[error("a create does not load the size {size:#x} once")]
    Size { size: u32 },
    /// The client's base list calls another getter than the count's.
    #[error("the base list's count getter is not the expected one")]
    Getter,
    /// The build table's base layout is not the one the window is laid out
    /// for.
    #[error("the bases' records are {stride:#x} apart with the count at {count:#x}")]
    Layout { stride: usize, count: usize },
}

/// The build table entries the window reads, with its symbols as offsets
/// into the code that starts at `code_rva` from the image base.
pub(super) struct Pins<'a> {
    pub(super) table: &'a Resolved,
    pub(super) code_rva: u64,
}

impl Pins<'_> {
    /// Where symbol `id` lies in the code.
    pub(super) fn at(&self, id: Symbol) -> Result<usize, SiteError> {
        self.table
            .symbol(id.id())
            .and_then(|rva| rva.checked_sub(self.code_rva))
            .and_then(|at| usize::try_from(at).ok())
            .ok_or(SiteError::Missing(id.id()))
    }

    /// The code of symbol `id`, as far as its pattern covers it.
    fn body(&self, id: Symbol) -> Result<Range<usize>, SiteError> {
        let at = self.at(id)?;
        let (pattern, _) = self
            .table
            .head(id.id())
            .ok_or(SiteError::Missing(id.id()))?;
        Ok(at..at + pattern.len())
    }

    /// Constant `id` as a 32-bit value.
    fn value(&self, id: Constant) -> Result<u32, SiteError> {
        self.table
            .constant(id.id())
            .and_then(|value| u32::try_from(value).ok())
            .ok_or(SiteError::Missing(id.id()))
    }
}

/// Every site of the window.
#[derive(Debug)]
pub(super) struct Sites {
    code: Code,
    server_create: u64,
    client_create: u64,
    server_destroy: u64,
    client_destroy: u64,
    patches: Vec<CodePatch>,
    pub(super) loops: loops::Sites,
}

impl Sites {
    fn targets(&self) -> impl Iterator<Item = u64> + '_ {
        [
            self.server_create,
            self.client_create,
            self.server_destroy,
            self.client_destroy,
        ]
        .into_iter()
        .chain(self.loops.targets())
    }

    /// The game's code the window needs.
    pub(super) fn code(&self) -> Code {
        self.code
    }
}

static SITES: OnceLock<Sites> = OnceLock::new();
/// Set at link when every site of ours is linked and every patch
/// registered: a target another owner hooks does not stand for ours.
static LINKED: AtomicBool = AtomicBool::new(false);
/// Set at arming when every site is linked, hooked and patched.
static ARMED: AtomicBool = AtomicBool::new(false);
/// Set at arming, armed or not.
static SETTLED: AtomicBool = AtomicBool::new(false);
/// The window's own switch, apart from the lookups': a panic in the
/// lifecycle stops making windows, and an object made before still frees
/// its window.
static LIFECYCLE: crate::fuse::Fuse = crate::fuse::Fuse::new("bases past 16");

// SAFETY: `CreateFn` is the signature of the create the table's
// `bases.server_create` pins, and `link` uses this slot only as that
// function's next.
static SERVER_CREATE_NEXT: Next<CreateFn> = unsafe { Next::new() };
// SAFETY: `CreateFn` is the signature of the create the table's
// `bases.client_create` pins, and `link` uses this slot only as that
// function's next.
static CLIENT_CREATE_NEXT: Next<CreateFn> = unsafe { Next::new() };
// SAFETY: `DestroyFn` is the signature of the destroy the table's
// `bases.server_destroy` pins, and `link` uses this slot only as that
// function's next.
static SERVER_DESTROY_NEXT: Next<DestroyFn> = unsafe { Next::new() };
// SAFETY: `DestroyFn` is the signature of the destroy the table's
// `bases.client_destroy` pins, and `link` uses this slot only as that
// function's next.
static CLIENT_DESTROY_NEXT: Next<DestroyFn> = unsafe { Next::new() };

/// Whether bases past 16 are armed.
pub(super) fn armed() -> bool {
    ARMED.load(Ordering::Acquire)
}

/// Whether arming has settled the window, armed or not.
pub(super) fn settled() -> bool {
    SETTLED.load(Ordering::Acquire)
}

/// The window's sites, while it is armed.
pub(super) fn sites() -> Option<&'static Sites> {
    SITES.get().filter(|_| armed())
}

/// The patch that grows the object the create whose code is `create`
/// allocates, at its one `mov edx, size`.
fn size_patch(
    code: &[u8],
    start: usize,
    create: Range<usize>,
    size: u32,
) -> Result<CodePatch, SiteError> {
    let mut expected = [MOV_EDX, 0, 0, 0, 0];
    expected[1..].copy_from_slice(&size.to_le_bytes());
    let body = code.get(create.clone()).ok_or(SiteError::Size { size })?;
    let mut found = body
        .windows(expected.len())
        .enumerate()
        .filter(|(_, w)| *w == expected);
    let (Some((at, _)), None) = (found.next(), found.next()) else {
        return Err(SiteError::Size { size });
    };
    let grown = u32::try_from(window::BLOCK).unwrap_or(u32::MAX);
    let mut replacement = expected;
    replacement[1..].copy_from_slice(&grown.to_le_bytes());
    Ok(CodePatch {
        owner: ModIndex::LOADER,
        address: start + create.start + at,
        expected: expected.into(),
        replacement: replacement.into(),
        needs: Box::default(),
    })
}

/// Finds every site in `code`, which starts at `start`, from the build
/// table's `pins`, for a base layout of `fields`; the client's base list
/// only on a `client`.
fn find(
    code: &[u8],
    start: usize,
    client: bool,
    fields: Fields,
    pins: &Pins<'_>,
) -> Result<Sites, SiteError> {
    if fields.stride != window::RECORD || fields.count != COUNT_AT {
        return Err(SiteError::Layout {
            stride: fields.stride,
            count: fields.count,
        });
    }
    let patches = vec![
        size_patch(
            code,
            start,
            pins.body(bases::SERVER_CREATE)?,
            pins.value(bases::SERVER_SIZE)?,
        )?,
        size_patch(
            code,
            start,
            pins.body(bases::CLIENT_CREATE)?,
            pins.value(bases::CLIENT_SIZE)?,
        )?,
    ];
    let loops = loops::find(code, start, client, pins)?;
    let patches: Vec<CodePatch> = patches.into_iter().chain(loops.patches()).collect();
    let at = |id: Symbol| pins.at(id).map(|offset| (start + offset) as u64);
    let address = |id: Symbol| pins.at(id).map(|offset| start + offset);
    let code_of = Code {
        // SAFETY: the table's `bases.ctor`, `fn(obj) -> obj`, whose whole
        // code its pattern holds.
        ctor: unsafe { core::mem::transmute::<usize, CtorFn>(address(bases::CTOR)?) },
        // SAFETY: the table's `bases.teardown`, `fn(first record)`, whose
        // whole code its pattern holds.
        teardown: unsafe { core::mem::transmute::<usize, TeardownFn>(address(bases::TEARDOWN)?) },
        // SAFETY: the table's `bases.slot_init`, the stock slot init the
        // server's create calls.
        slot_init: unsafe { core::mem::transmute::<usize, SlotInitFn>(address(bases::SLOT_INIT)?) },
        // SAFETY: the table's `bases.slot_free`, the stock map free the
        // server's destroy calls.
        slot_free: unsafe { core::mem::transmute::<usize, SlotFreeFn>(address(bases::SLOT_FREE)?) },
    };
    Ok(Sites {
        code: code_of,
        server_create: at(bases::SERVER_CREATE)?,
        client_create: at(bases::CLIENT_CREATE)?,
        server_destroy: at(bases::SERVER_DESTROY)?,
        client_destroy: at(bases::CLIENT_DESTROY)?,
        patches,
        loops,
    })
}

/// Finds and links every site and registers the size patches, when a mod
/// asked for more than 16 bases, for a base layout of `fields`; a miss is
/// logged with the build and leaves 16.
pub(super) fn link(loader: &'static Loader, fields: Fields) {
    let build = &loader.build.name;
    let (Some(text), Some(code)) = (
        crate::code::text(loader.base),
        crate::code::text_bytes(loader.base),
    ) else {
        return;
    };
    let pins = Pins {
        table: loader.build.resolved(),
        code_rva: text.0 as u64 - loader.base,
    };
    let sites = match find(code, text.0, loader.side == Side::CLIENT, fields, &pins) {
        Ok(sites) => SITES.get_or_init(|| sites),
        Err(SiteError::Missing(id)) => {
            loader.feature_off(PAST_16, id, &Unhooked::Absent.to_string());
            return;
        }
        Err(reason) => {
            crate::log!(WARN, "bases keep 16 on {build}: {reason}");
            return;
        }
    };
    let status = crate::code::register_all(sites.patches.clone(), text);
    if !status.is_ok() {
        crate::log!(
            WARN,
            "bases keep 16 on {build}: the size patches were refused ({})",
            status.0
        );
        return;
    }
    let none = HookFlags::NONE;
    let at = |symbol| Target::Pinned(symbol);
    let refused = [
        loader.link_feature(
            "server bases create",
            at(bases::SERVER_CREATE),
            &SERVER_CREATE_NEXT,
            create_server,
            none,
        ),
        loader.link_feature(
            "client bases create",
            at(bases::CLIENT_CREATE),
            &CLIENT_CREATE_NEXT,
            create_client,
            none,
        ),
        loader.link_feature(
            "server bases destroy",
            at(bases::SERVER_DESTROY),
            &SERVER_DESTROY_NEXT,
            destroy_server,
            none,
        ),
        loader.link_feature(
            "client bases destroy",
            at(bases::CLIENT_DESTROY),
            &CLIENT_DESTROY_NEXT,
            destroy_client,
            none,
        ),
    ]
    .into_iter()
    .chain(loops::link(loader, &sites.loops))
    .filter(|linked| !linked)
    .count();
    LINKED.store(refused == 0, Ordering::Release);
}

/// Settles, before arming writes anything, whether bases past 16 are armed:
/// every site linked by us, among the `hooked` targets, and every size
/// patch written.
pub(super) fn arming(patches: &[CodePatch], verdicts: &[Verdict], hooked: &[u64]) {
    let armed = LINKED.load(Ordering::Acquire)
        && SITES.get().is_some_and(|sites| {
            sites.targets().all(|at| hooked.contains(&at))
                && sites.patches.iter().all(|mine| {
                    patches
                        .iter()
                        .zip(verdicts)
                        .any(|(p, v)| p.address == mine.address && *v == Verdict::Write)
                })
        });
    if SITES.get().is_some() && !armed {
        crate::log!(
            WARN,
            "bases keep 16: a hook or a size patch of bases past 16 is not armed"
        );
    }
    ARMED.store(armed, Ordering::Release);
    SETTLED.store(true, Ordering::Release);
}

/// Fills the window of an object a create has just made.
///
/// # Safety
///
/// - `obj` is null or the object the create has just returned, on its
///   thread.
unsafe fn created(obj: *mut u8) {
    if obj.is_null() {
        return;
    }
    if let Some(sites) = sites() {
        // SAFETY: the caller guarantees the fresh object, grown by the size
        // patch since the window is armed, which no one else has yet.
        let _done = LIFECYCLE.run("a bases create", || unsafe {
            lifecycle::init(obj, &sites.code)
        });
    }
}

/// Frees the window of an object a destroy is about to free, and forgets
/// its side table.
///
/// # Safety
///
/// - `obj` is null or the live object the destroy got, on its thread.
unsafe fn destroying(obj: *mut u8) {
    let Some(sites) = SITES.get() else {
        return;
    };
    super::members::forget_object(obj);
    // SAFETY: the caller guarantees the live object the game destroys, or
    // null, which `destroy` finds windowed never; it ignores the fuse, so an
    // object windowed before a panic still frees its window.
    let freed = std::panic::catch_unwind(|| unsafe { lifecycle::destroy(obj, &sites.code) });
    if freed.is_err() {
        LIFECYCLE.blow("a bases destroy");
    }
}

unsafe extern "system" fn create_server(allocator: *mut u8, part: *mut u8) -> *mut u8 {
    // SAFETY: this detour runs only as the server create's link, so the
    // slot holds the next function of that signature; the game's arguments
    // pass unchanged.
    let obj = unsafe { SERVER_CREATE_NEXT.get()(allocator, part) };
    // SAFETY: the object the create has just returned, or null.
    unsafe { created(obj) };
    obj
}

unsafe extern "system" fn create_client(allocator: *mut u8, part: *mut u8) -> *mut u8 {
    // SAFETY: this detour runs only as the client create's link, so the
    // slot holds the next function of that signature; the game's arguments
    // pass unchanged.
    let obj = unsafe { CLIENT_CREATE_NEXT.get()(allocator, part) };
    // SAFETY: the object the create has just returned, or null.
    unsafe { created(obj) };
    obj
}

unsafe extern "system" fn destroy_server(obj: *mut u8) -> u64 {
    // SAFETY: the object the destroy got, or null.
    unsafe { destroying(obj) };
    // SAFETY: this detour runs only as the server destroy's link, so the
    // slot holds the next function of that signature; the game's argument
    // passes unchanged.
    unsafe { SERVER_DESTROY_NEXT.get()(obj) }
}

unsafe extern "system" fn destroy_client(obj: *mut u8) -> u64 {
    // SAFETY: the object the destroy got, or null.
    unsafe { destroying(obj) };
    // SAFETY: this detour runs only as the client destroy's link, so the
    // slot holds the next function of that signature; the game's argument
    // passes unchanged.
    unsafe { CLIENT_DESTROY_NEXT.get()(obj) }
}

#[cfg(test)]
mod tests {
    use enshwick_gamedata::reflect::{Image, Layout};
    use enshwick_gamedata::{Side as TableSide, exact};

    use super::*;
    use crate::bases::fake::FIELDS;

    /// Every site is where the table of each game executable that
    /// `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` name puts it, each size patch
    /// finds its one `mov edx, size` in its create, and the client's count
    /// getter is in the client only; skipped without them.
    #[test]
    fn every_site_is_found_in_the_game_s_executables() {
        let tables = crate::builds::load();
        for (variable, side) in [
            ("ENSH_SERVER_EXE", TableSide::Server),
            ("ENSH_CLIENT_EXE", TableSide::Client),
        ] {
            let Some(path) = std::env::var_os(variable) else {
                continue;
            };
            let bytes = std::fs::read(path).unwrap();
            let image = Image::parse(&bytes, Layout::File, None).unwrap();
            let table = exact(&tables, side, image.fingerprint().unwrap()).unwrap();
            let text = image.section(".text").unwrap();
            let range = image.range(text);
            let code = image.read(range.start, range.end - range.start).unwrap();
            let pins = Pins {
                table: &table.resolve(),
                code_rva: range.start - image.base(),
            };
            let client = side == TableSide::Client;
            let sites = find(code, 0, client, FIELDS, &pins)
                .unwrap_or_else(|why| panic!("{variable}: {why}"));
            assert_eq!(sites.patches.len(), 2 + usize::from(client), "{variable}");
            assert_eq!(sites.targets().count(), 14, "{variable}");
        }
    }

    #[test]
    fn a_layout_other_than_the_window_s_is_refused_before_any_search() {
        let other = Fields {
            stride: 0x138,
            ..FIELDS
        };
        let tables = crate::builds::load();
        let pins = Pins {
            table: &tables[0].resolve(),
            code_rva: 0,
        };
        assert_eq!(
            find(&[], 0, false, other, &pins).map(|_| ()),
            Err(SiteError::Layout {
                stride: 0x138,
                count: 0x24
            })
        );
    }

    #[test]
    fn a_grown_size_is_the_one_load_of_the_vanilla_size_in_its_create() {
        let mut code = vec![0x90; 0x60];
        code[0x30..0x35].copy_from_slice(&[0xBA, 0x70, 0x20, 0, 0]);
        let grown = size_patch(&code, 0x1000, 0x20..0x40, 0x2070).unwrap();
        assert_eq!(grown.address, 0x1030);
        assert_eq!(*grown.expected, [0xBA, 0x70, 0x20, 0, 0]);
        assert_eq!(*grown.replacement, [0xBA, 0x30, 0x30, 0x01, 0]);
        assert!(size_patch(&code, 0x1000, 0x20..0x40, 0x16e0).is_err());
        assert!(size_patch(&code, 0x1000, 0x40..0x60, 0x2070).is_err());
        code[0x10..0x15].copy_from_slice(&[0xBA, 0x70, 0x20, 0, 0]);
        assert!(
            size_patch(&code, 0x1000, 0x00..0x40, 0x2070).is_err(),
            "twice"
        );
    }
}
