//! The game's code as its executable on disk ships it, relocated to where
//! the game runs. The anchors and engine routes search it, so another
//! program's hooks in the code in memory hide no entry, and arming compares
//! each target with it, so Enshwick never hooks over code another program
//! took: the features on that target turn off, naming who took it.
//!
//! The copy lives from the build search until the hooks are armed; who took
//! which code stays for the chat line a joining player reads.

use std::sync::{Arc, Mutex};

use crate::code_watch::lands;
use crate::locks::Relock;

/// Who holds a target whose code no longer reads as shipped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Taken {
    /// The module, by file name, that the target's jump leads into.
    By(String),
    /// The jump leads into no module, or the bytes changed without one.
    Unknown,
}

impl core::fmt::Display for Taken {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::By(module) => write!(f, "taken by {module}"),
            Self::Unknown => f.write_str("taken by something other than Enshwick"),
        }
    }
}

/// The game's shipped `.text` and where the live one starts.
#[derive(Debug)]
struct Shipped {
    start: usize,
    text: Arc<[u8]>,
}

impl Shipped {
    /// The live code the shipped copy stands for, as it reads now.
    fn live(&self) -> &[u8] {
        // SAFETY: `start` is the game's `.text` as `code::text` found it,
        // or a test's code that is never freed, mapped readable for the
        // process and `text.len()` bytes long. Another program's thread may
        // write these bytes at any time, so each check builds the slice anew
        // and reads it at once; a byte has no invalid value.
        unsafe { core::slice::from_raw_parts(self.start as *const u8, self.text.len()) }
    }
}

#[derive(Debug)]
struct State {
    code: Option<Shipped>,
    /// Each holder [`taken`] found, once, in the order found.
    takers: Vec<Taken>,
}

static STATE: Mutex<State> = Mutex::new(State {
    code: None,
    takers: Vec::new(),
});

/// Reads the shipped code of the game mapped at `base` and keeps it for
/// [`taken`] until [`release`]; `None`, logged, when the executable does
/// not read or its code on disk is packed, and the code in memory is all
/// there is.
pub(crate) fn read(base: u64) -> Option<Arc<[u8]>> {
    let (start, len) = crate::code::text(base)?;
    let rva = start.checked_sub(usize::try_from(base).ok()?)?;
    let live = crate::code::text_bytes(base)?;
    let Some((text, ranges)) = crate::code::disk_text(base, rva, len)
        .and_then(|disk| crate::code_watch::shipped(&disk, live).map(|ranges| (disk, ranges)))
    else {
        crate::log!(
            INFO,
            "the executable on disk does not read as the code in memory: the build search reads the code in memory, and hooks are not checked against the shipped code"
        );
        return None;
    };
    if !ranges.is_empty() {
        crate::log!(
            INFO,
            "{} ranges of game code differ from the executable on disk; the build search reads the shipped code",
            ranges.len()
        );
    }
    let text: Arc<[u8]> = text.into();
    STATE.relock().code = Some(Shipped {
        start,
        text: Arc::clone(&text),
    });
    Some(text)
}

/// Keeps `text` as the shipped code of the code at `start`, which a test
/// owns and never frees.
#[cfg(test)]
pub(crate) fn keep(start: usize, text: Vec<u8>) {
    STATE.relock().code = Some(Shipped {
        start,
        text: text.into(),
    });
}

/// A copy of the shipped code [`read`] keeps, for the code watch.
pub(crate) fn copy() -> Option<Vec<u8>> {
    STATE.relock().code.as_ref().map(|code| code.text.to_vec())
}

/// Drops the shipped code once the hooks are armed.
pub(crate) fn release() {
    STATE.relock().code = None;
}

/// Who took the `len` bytes at `start` of the game's code, which a hook
/// would replace; `None` while they read as shipped, or when no shipped
/// code is kept or they lie outside it.
pub(crate) fn taken((start, len): (usize, usize)) -> Option<Taken> {
    let mut state = STATE.relock();
    let code = state.code.as_ref()?;
    let taken = taken_in(
        &code.text,
        code.live(),
        code.start,
        (start, len),
        crate::code::code_at,
        module_of,
    )?;
    if !state.takers.contains(&taken) {
        state.takers.push(taken.clone());
    }
    Some(taken)
}

/// Each holder of code a hook needed, once.
pub(crate) fn takers() -> Vec<Taken> {
    STATE.relock().takers.clone()
}

/// [`taken`] over `shipped` and `live`, the code at `text`, with `read` for
/// the code a jump leads to and `module` for the file name of the module
/// other than the game holding an address.
fn taken_in(
    shipped: &[u8],
    live: &[u8],
    text: usize,
    (start, len): (usize, usize),
    read: impl Fn(usize) -> Option<Vec<u8>>,
    module: impl Fn(usize) -> Option<String>,
) -> Option<Taken> {
    let offset = start.checked_sub(text)?;
    let end = offset.checked_add(len)?;
    let (was, now) = (shipped.get(offset..end)?, live.get(offset..end)?);
    let first = offset + was.iter().zip(now).position(|(was, now)| was != now)?;
    let to = lands(live, text, (first, end - first), read, |at| {
        module(at).is_some()
    });
    Some(to.and_then(module).map_or(Taken::Unknown, Taken::By))
}

/// The file name of the module holding `at`, unless it is the game's own.
fn module_of(at: usize) -> Option<String> {
    use windows_sys::Win32::Foundation::HMODULE;
    use windows_sys::Win32::System::LibraryLoader::{
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        GetModuleFileNameW, GetModuleHandleExW, GetModuleHandleW,
    };

    const PATH: u32 = 1024;

    let mut module: HMODULE = core::ptr::null_mut();
    // SAFETY: with `FROM_ADDRESS` the name is any address, only compared
    // with the loaded modules' ranges; `module` is a writable local, and the
    // unchanged refcount leaves nothing to free.
    let found = unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            core::ptr::without_provenance(at),
            &raw mut module,
        )
    };
    // SAFETY: a null name asks for the executable's own module.
    if found == 0 || module == unsafe { GetModuleHandleW(core::ptr::null()) } {
        return None;
    }
    let mut path = [0u16; PATH as usize];
    // SAFETY: `module` is a loaded module's handle and `path` has room for
    // the `PATH` `u16`s the call may write.
    let written = unsafe { GetModuleFileNameW(module, path.as_mut_ptr(), PATH) };
    let path = String::from_utf16_lossy(path.get(..usize::try_from(written).ok()?)?);
    path.rsplit(['\\', '/'])
        .next()
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests;
