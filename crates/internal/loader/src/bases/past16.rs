//! Bases past 16: the grown bases objects, their window records from create
//! to destroy, and every other site the window needs, found and armed all
//! together or not at all.
//!
//! Both create functions allocate the object with one size immediate, which
//! grows to [`window::BLOCK`]; their hooks fill the window records, and the
//! destroy hooks free them first. A miss of any site leaves the game its
//! 16 bases, with the zones still raised. Server 399 and client 426, where
//! the bodies are the same.

use core::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use enshwick::abi::{HookFlags, Side};

use super::Fields;
use super::lifecycle::{self, Code, CtorFn, SlotFreeFn, SlotInitFn, TeardownFn};
use super::loops;
use crate::base_groups::window;
use crate::chains::Next;
use crate::code::{CodePatch, Verdict};
use crate::host::Loader;
use crate::mod_index::ModIndex;

/// The server object's create: `fn(allocator, world part) -> obj`, null
/// when it fails.
const SERVER_CREATE: &str = "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 48 89 7C 24 20 41 56 \
                             48 83 EC 50 48 8B 01 48 8B EA";
/// The client object's create, of the same shape.
const CLIENT_CREATE: &str =
    "48 89 6C 24 10 48 89 74 24 18 57 48 83 EC 50 48 8B 01 48 8B EA 45 33 C9";
/// Where each create loads the object's size, `mov edx, size`, and the
/// sizes.
const SERVER_SIZE_AT: usize = 0x30;
const CLIENT_SIZE_AT: usize = 0x1d;
const SERVER_SIZE: u32 = 0x2070;
const CLIENT_SIZE: u32 = 0x16e0;
const MOV_EDX: u8 = 0xBA;
/// Where the server's create calls the stock slot init.
const SLOT_INIT_AT: usize = 0x15f;
/// The constructor, a leaf.
const CTOR: &str = "45 33 D2 48 8D 51 30 4C 89 11 48 8D 42 60 44 88 51 08 4C 8D 1D ?? ?? ?? ??";
/// The teardown of 16 records from `rcx`.
const TEARDOWN: &str = "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 48 89 7C 24 20 41 56 \
                        48 83 EC 20 48 8D B1 00 13 00 00";
/// The server object's destroy, `fn(obj)`, null-safe, and where it calls
/// the stock map free.
const SERVER_DESTROY: &str = "40 56 48 83 EC 30 48 8B F1 48 85 C9 0F 84 ?? ?? ?? ?? \
                              48 89 5C 24 40 48 89 6C 24 48";
const SLOT_FREE_AT: usize = 0xa8;
/// The client object's destroy.
const CLIENT_DESTROY: &str = "41 57 48 83 EC 30 4C 8B F9 48 85 C9 0F 84 ?? ?? ?? ?? \
                              48 89 5C 24 40 48 89 6C 24 48";
const CALL: u8 = 0xE8;
/// The base count the window's code reads, `[obj+0x24]` in the count
/// getter and the cleanups.
pub(super) const COUNT_AT: usize = 0x24;

type CreateFn = unsafe extern "system" fn(allocator: *mut u8, part: *mut u8) -> *mut u8;
type DestroyFn = unsafe extern "system" fn(obj: *mut u8) -> u64;

/// Why a site of the window is not found.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub(super) enum SiteError {
    /// A pattern does not match once.
    #[error("{0}")]
    Pattern(String),
    /// The code ends before the bytes at an offset.
    #[error("the code ends before {0:#x}")]
    Cut(usize),
    /// An anchor holds no call.
    #[error("{at:#x} holds {found:#04x}, not a call")]
    NotACall { at: usize, found: u8 },
    /// A call leaves the code section.
    #[error("the call at {0:#x} leaves the code")]
    CallLeaves(usize),
    /// A create allocates another size than the game's.
    #[error("a create loads {found:02X?}, not the size {size:#x}")]
    Size { found: Vec<u8>, size: u32 },
    /// An anchor holds another instruction than the expected load.
    #[error("{at:#x} holds {found:02X?}, not the expected load")]
    NotALoad { at: usize, found: Vec<u8> },
    /// The client's base list calls another getter than the count's.
    #[error("the base list's count getter is not the expected one")]
    Getter,
    /// The build table's base layout is not the one the window is laid out
    /// for.
    #[error("the bases' records are {stride:#x} apart with the count at {count:#x}")]
    Layout { stride: usize, count: usize },
}

impl From<String> for SiteError {
    fn from(why: String) -> Self {
        Self::Pattern(why)
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

// SAFETY: `CreateFn` is the signature of the create the `SERVER_CREATE`
// pattern finds, and `link` uses this slot only as that function's next.
static SERVER_CREATE_NEXT: Next<CreateFn> = unsafe { Next::new() };
// SAFETY: `CreateFn` is the signature of the create the `CLIENT_CREATE`
// pattern finds, and `link` uses this slot only as that function's next.
static CLIENT_CREATE_NEXT: Next<CreateFn> = unsafe { Next::new() };
// SAFETY: `DestroyFn` is the signature of the destroy the `SERVER_DESTROY`
// pattern finds, and `link` uses this slot only as that function's next.
static SERVER_DESTROY_NEXT: Next<DestroyFn> = unsafe { Next::new() };
// SAFETY: `DestroyFn` is the signature of the destroy the `CLIENT_DESTROY`
// pattern finds, and `link` uses this slot only as that function's next.
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

/// The offset `call` at `at` in `code` jumps to, after checking its opcode.
pub(super) fn call_target(code: &[u8], at: usize) -> Result<usize, SiteError> {
    let bytes: [u8; 5] = code
        .get(at..at + 5)
        .and_then(|b| b.try_into().ok())
        .ok_or(SiteError::Cut(at))?;
    if bytes[0] != CALL {
        return Err(SiteError::NotACall {
            at,
            found: bytes[0],
        });
    }
    let rel = i32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
    isize::try_from(rel)
        .ok()
        .and_then(|rel| (at + 5).checked_add_signed(rel))
        .filter(|&to| to < code.len())
        .ok_or(SiteError::CallLeaves(at))
}

/// The patch that grows the object a create at `create` allocates, whose
/// `mov edx, size` sits `at` bytes in.
fn size_patch(
    code: &[u8],
    start: usize,
    create: usize,
    at: usize,
    size: u32,
) -> Result<CodePatch, SiteError> {
    let mut expected = [MOV_EDX, 0, 0, 0, 0];
    expected[1..].copy_from_slice(&size.to_le_bytes());
    let found = code
        .get(create + at..create + at + expected.len())
        .ok_or(SiteError::Cut(create + at))?;
    if found != expected {
        return Err(SiteError::Size {
            found: found.to_vec(),
            size,
        });
    }
    let grown = u32::try_from(window::BLOCK).unwrap_or(u32::MAX);
    let mut replacement = expected;
    replacement[1..].copy_from_slice(&grown.to_le_bytes());
    Ok(CodePatch {
        owner: ModIndex::LOADER,
        address: start + create + at,
        expected: expected.into(),
        replacement: replacement.into(),
        needs: Box::default(),
    })
}

/// Finds every site in `code`, which starts at `start`, for a base layout
/// of `fields`; the client's base list only on a `client`.
fn find(code: &[u8], start: usize, client: bool, fields: Fields) -> Result<Sites, SiteError> {
    if fields.stride != window::RECORD || fields.count != COUNT_AT {
        return Err(SiteError::Layout {
            stride: fields.stride,
            count: fields.count,
        });
    }
    let find = |pattern: &str| crate::code::find(code, pattern).map_err(SiteError::from);
    let server_create = find(SERVER_CREATE)?;
    let client_create = find(CLIENT_CREATE)?;
    let server_destroy = find(SERVER_DESTROY)?;
    let client_destroy = find(CLIENT_DESTROY)?;
    let ctor = find(CTOR)?;
    let teardown = find(TEARDOWN)?;
    let slot_init = call_target(code, server_create + SLOT_INIT_AT)?;
    let slot_free = call_target(code, server_destroy + SLOT_FREE_AT)?;
    let patches = vec![
        size_patch(code, start, server_create, SERVER_SIZE_AT, SERVER_SIZE)?,
        size_patch(code, start, client_create, CLIENT_SIZE_AT, CLIENT_SIZE)?,
    ];
    let loops = loops::find(code, start, client)?;
    let patches: Vec<CodePatch> = patches.into_iter().chain(loops.patches()).collect();
    let at = |offset: usize| (start + offset) as u64;
    let address = |offset: usize| start + offset;
    let code_of = Code {
        // SAFETY: the constructor its pattern finds, `fn(obj) -> obj`.
        ctor: unsafe { core::mem::transmute::<usize, CtorFn>(address(ctor)) },
        // SAFETY: the teardown its pattern finds, `fn(first record)`.
        teardown: unsafe { core::mem::transmute::<usize, TeardownFn>(address(teardown)) },
        // SAFETY: the stock slot init the server's create calls at
        // `SLOT_INIT_AT`.
        slot_init: unsafe { core::mem::transmute::<usize, SlotInitFn>(address(slot_init)) },
        // SAFETY: the stock map free the server's destroy calls at
        // `SLOT_FREE_AT`.
        slot_free: unsafe { core::mem::transmute::<usize, SlotFreeFn>(address(slot_free)) },
    };
    Ok(Sites {
        code: code_of,
        server_create: at(server_create),
        client_create: at(client_create),
        server_destroy: at(server_destroy),
        client_destroy: at(client_destroy),
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
    let sites = match find(code, text.0, loader.side == Side::CLIENT, fields) {
        Ok(sites) => SITES.get_or_init(|| sites),
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
    let refused = [
        (
            "server bases create",
            loader.link(
                sites.server_create,
                &SERVER_CREATE_NEXT,
                create_server,
                HookFlags::NONE,
            ),
        ),
        (
            "client bases create",
            loader.link(
                sites.client_create,
                &CLIENT_CREATE_NEXT,
                create_client,
                HookFlags::NONE,
            ),
        ),
        (
            "server bases destroy",
            loader.link(
                sites.server_destroy,
                &SERVER_DESTROY_NEXT,
                destroy_server,
                HookFlags::NONE,
            ),
        ),
        (
            "client bases destroy",
            loader.link(
                sites.client_destroy,
                &CLIENT_DESTROY_NEXT,
                destroy_client,
                HookFlags::NONE,
            ),
        ),
    ]
    .into_iter()
    .chain(loops::link(loader, &sites.loops))
    .filter(|(_, status)| !status.is_ok())
    .inspect(|(name, status)| {
        crate::log!(
            WARN,
            "bases keep 16 on {build}: {name} refused ({})",
            status.0
        );
    })
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
    use super::*;
    use crate::bases::fake::FIELDS;

    #[test]
    fn a_call_anchor_resolves_to_its_target_and_anything_else_is_refused() {
        let mut code = vec![0xCC; 0x40];
        code[0x10..0x15].copy_from_slice(&[0xE8, 0x0B, 0, 0, 0]);
        code[0x20..0x25].copy_from_slice(&[0xE8, 0xF0, 0xFF, 0xFF, 0xFF]);
        assert_eq!(call_target(&code, 0x10), Ok(0x20));
        assert_eq!(call_target(&code, 0x20), Ok(0x15));
        assert_eq!(
            call_target(&code, 0x00),
            Err(SiteError::NotACall { at: 0, found: 0xCC })
        );
        code[0x30..0x35].copy_from_slice(&[0xE8, 0x00, 0x10, 0, 0]);
        assert_eq!(call_target(&code, 0x30), Err(SiteError::CallLeaves(0x30)));
        assert_eq!(call_target(&code, 0x3e), Err(SiteError::Cut(0x3e)));
    }

    /// Every site is found once in the game's executables that
    /// `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` name, the client's base list
    /// in the client only; skipped without them.
    #[test]
    fn every_site_is_found_in_the_game_s_executables() {
        use enshwick_gamedata::reflect::{Image, Layout};
        for (variable, client) in [("ENSH_SERVER_EXE", false), ("ENSH_CLIENT_EXE", true)] {
            let Some(path) = std::env::var_os(variable) else {
                continue;
            };
            let bytes = std::fs::read(path).unwrap();
            let image = Image::parse(&bytes, Layout::File, None).unwrap();
            let text = image.section(".text").unwrap();
            let range = image.range(text);
            let code = image.read(range.start, range.end - range.start).unwrap();
            let sites =
                find(code, 0, client, FIELDS).unwrap_or_else(|why| panic!("{variable}: {why}"));
            assert_eq!(sites.patches.len(), 2 + usize::from(client), "{variable}");
            assert_eq!(sites.targets().count(), 14, "{variable}");
            assert!(
                client || find(code, 0, true, FIELDS).is_err(),
                "{variable} has no client base list"
            );
        }
    }

    #[test]
    fn a_layout_other_than_the_window_s_is_refused_before_any_search() {
        let other = Fields {
            stride: 0x138,
            ..FIELDS
        };
        assert_eq!(
            find(&[], 0, false, other).map(|_| ()),
            Err(SiteError::Layout {
                stride: 0x138,
                count: 0x24
            })
        );
    }

    #[test]
    fn a_grown_size_checks_the_vanilla_size_first() {
        let mut code = vec![0x90; 0x60];
        code[0x30..0x35].copy_from_slice(&[0xBA, 0x70, 0x20, 0, 0]);
        let grown = size_patch(&code, 0x1000, 0, 0x30, SERVER_SIZE).unwrap();
        assert_eq!(grown.address, 0x1030);
        assert_eq!(*grown.expected, [0xBA, 0x70, 0x20, 0, 0]);
        assert_eq!(*grown.replacement, [0xBA, 0x30, 0x30, 0x01, 0]);
        assert!(matches!(
            size_patch(&code, 0x1000, 0, 0x30, CLIENT_SIZE),
            Err(SiteError::Size { .. })
        ));
    }
}
