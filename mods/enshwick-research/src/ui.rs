//! What the client's interface does where the loader's screens still need
//! engine facts:
//!
//! - R2, scroll areas: each scroll area's start (who opens it, its kept
//!   state) and its offset whenever it moves, frame to frame, as the wheel,
//!   the scrollbar or the gamepad scroll it;
//! - R3, tab bars: the tab labels and the chosen tab, of the world settings
//!   and every other tab bar of the same function, whenever they change;
//! - R5, text input: the game's text input dialog when it opens, and each
//!   text field's text whenever it changes.
//!
//! Every hook only reads, at the entry of a function, through reads that
//! cannot fault. Each site's first bytes are checked against the build the
//! addresses were found on; a site that differs stays unhooked.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Instant;

use enshwick_loader_api::abi::Registers;
use enshwick_loader_api::{Load, Off};

use crate::memory;

/// What a site is and so what it logs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Kind {
    /// A scroll area's start: `rcx` the area, `dl` vertical, `r8` its state.
    ScrollBegin,
    /// A scroll area's end: `rcx` the area; its state at +0x160.
    ScrollEnd,
    /// A tab bar: `rcx` the chosen tab, `rdx` a slice of labels.
    Tabs,
    /// The text input dialog: `rcx` its state.
    TextDialog,
    /// A text field's start: `rdx` the slice of its text.
    TextField,
}

/// One function looked at.
#[derive(Debug)]
struct Site {
    name: &'static str,
    symbol: &'static str,
    /// Its first bytes on client build 1269579.
    expected: [u8; 8],
    kind: Kind,
}

const SITES: [Site; 5] = [
    Site {
        name: "R2 scroll begin",
        symbol: "research.scroll_begin",
        expected: 0x4154_4157_55c4_8b48_u64.to_le_bytes(),
        kind: Kind::ScrollBegin,
    },
    Site {
        name: "R2 scroll end",
        symbol: "research.scroll_end",
        expected: 0x4157_5653_55c4_8b48_u64.to_le_bytes(),
        kind: Kind::ScrollEnd,
    },
    Site {
        name: "R3 tabs",
        symbol: "research.tabs",
        expected: 0x8d48_5541_55c4_8b48_u64.to_le_bytes(),
        kind: Kind::Tabs,
    },
    Site {
        name: "R5 text dialog",
        symbol: "research.text_dialog",
        expected: 0x4155_4153_55c4_8b48_u64.to_le_bytes(),
        kind: Kind::TextDialog,
    },
    Site {
        name: "R5 text field",
        symbol: "research.text_field",
        expected: 0x7489_4810_245c_8948_u64.to_le_bytes(),
        kind: Kind::TextField,
    },
];

/// A scroll area's kept state's offset at +0 and the area's state at +0x160.
const STATE_OFFSET: usize = 0;
const SCROLL_STATE: usize = 0x160;
/// A text slice: data, then length.
const SLICE_LEN: usize = 8;
/// A UI text: pointer, length, hash; 24 bytes each in a slice of labels.
const TEXT_SIZE: usize = 0x18;
/// The most bytes of a text logged.
const MAX_TEXT: usize = 96;
/// The most tabs logged.
const MAX_TABS: usize = 12;
/// The most distinct calls kept for comparing.
const MAX_KEPT: usize = 512;

/// What was last logged per call site and object, to log changes only.
struct Seen {
    start: Instant,
    base: usize,
    last: Mutex<HashMap<(usize, usize, Kind), String>>,
}

static SEEN: OnceLock<Seen> = OnceLock::new();

/// Links the sites that read as expected; how many.
///
/// # Errors
///
/// [`Off`] when the game's code cannot be read.
pub(crate) fn link(load: &mut Load<'_>) -> Result<usize, Off> {
    let start = load.code()?.as_ptr().addr();
    let _first = SEEN.set(Seen {
        start: Instant::now(),
        base: memory::image_base(),
        last: Mutex::new(HashMap::new()),
    });
    let mut hooked = 0;
    for site in &SITES {
        let address = match load.symbol(site.symbol) {
            Ok(address) => address,
            Err(off) => {
                enshwick_loader_api::warn!("{} off: {off}", site.name);
                continue;
            }
        };
        let Ok(at) = usize::try_from(address) else {
            continue;
        };
        let found = at
            .checked_sub(start)
            .and_then(|offset| load.code().ok()?.get(offset..offset + site.expected.len()));
        if found != Some(&site.expected[..]) {
            enshwick_loader_api::warn!(
                "{} off: its bytes at {at:#x} are {found:02x?}, not those of build 1269579",
                site.name
            );
            continue;
        }
        let kind = site.kind;
        // SAFETY: the site's bytes match build 1269579, where it starts an
        // instruction; the handler only reads the registers.
        match unsafe { load.hook_at(address, move |registers| seen(kind, registers)) } {
            Ok(()) => hooked += 1,
            Err(off) => enshwick_loader_api::warn!("{} off: {off}", site.name),
        }
    }
    Ok(hooked)
}

fn reg(value: u64) -> usize {
    usize::try_from(value).unwrap_or(0)
}

/// The text of `len` bytes at `address`, cut and with control bytes
/// escaped; empty when not mapped.
fn text(address: usize, len: usize) -> String {
    let mut bytes = vec![0u8; len.min(MAX_TEXT)];
    let read = memory::read(address, &mut bytes);
    bytes.truncate(read);
    String::from_utf8_lossy(&bytes).escape_debug().to_string()
}

fn f32_at(address: usize) -> Option<f32> {
    let mut bytes = [0u8; 4];
    (memory::read(address, &mut bytes) == 4).then(|| f32::from_le_bytes(bytes))
}

fn seen(kind: Kind, registers: &mut Registers) {
    let Some(seen) = SEEN.get() else {
        return;
    };
    let caller = memory::read_u64(reg(registers.rsp)).map_or(0, reg);
    let (object, line) = match kind {
        Kind::ScrollBegin => {
            let state = reg(registers.r8);
            let offset = f32_at(state + STATE_OFFSET);
            (
                state,
                format!(
                    "area {:#x} vertical {} state {state:#x} offset {offset:?}",
                    reg(registers.rcx),
                    registers.rdx & 0xff
                ),
            )
        }
        Kind::ScrollEnd => {
            let area = reg(registers.rcx);
            let state = memory::read_u64(area + SCROLL_STATE).map_or(0, reg);
            let offset = f32_at(state + STATE_OFFSET);
            (
                state,
                format!("area {area:#x} state {state:#x} offset {offset:?}"),
            )
        }
        Kind::Tabs => {
            let chosen = reg(registers.rcx);
            let mut byte = [0u8; 1];
            let picked = (memory::read(chosen, &mut byte) == 1).then_some(byte[0]);
            let slice = reg(registers.rdx);
            let labels = memory::read_u64(slice).map_or(0, reg);
            let count = memory::read_u64(slice + 8).map_or(0, reg).min(MAX_TABS);
            let names: Vec<String> = (0..count)
                .map(|i| {
                    let entry = labels + i * TEXT_SIZE;
                    let ptr = memory::read_u64(entry).map_or(0, reg);
                    let len = memory::read_u64(entry + 8).map_or(0, reg);
                    text(ptr, len)
                })
                .collect();
            (chosen, format!("chosen {picked:?} of {count}: {names:?}"))
        }
        Kind::TextDialog => {
            let state = reg(registers.rcx);
            let mut head = [0u8; 0x40];
            let read = memory::read(state, &mut head);
            (
                state,
                format!("state {state:#x} head {:02x?}", &head[..read]),
            )
        }
        Kind::TextField => {
            let slice = reg(registers.rdx);
            let data = memory::read_u64(slice).map_or(0, reg);
            let len = memory::read_u64(slice + SLICE_LEN).map_or(0, reg);
            let cap = memory::read_u64(slice + 16).map_or(0, reg);
            (
                slice,
                format!(
                    "widget {:#x} slice {slice:#x} data {data:#x} len {len} cap {cap}: {:?}",
                    reg(registers.rcx),
                    text(data, len)
                ),
            )
        }
    };
    let key = (caller, object, kind);
    {
        let mut last = seen.last.lock().unwrap_or_else(PoisonError::into_inner);
        if last.get(&key) == Some(&line) {
            return;
        }
        if last.len() >= MAX_KEPT {
            last.clear();
        }
        last.insert(key, line.clone());
    }
    let site = SITES.iter().find(|s| s.kind == kind).map_or("", |s| s.name);
    enshwick_loader_api::info!(
        "{site} at {:.3}s from {}: {line}",
        seen.start.elapsed().as_secs_f64(),
        memory::as_note(caller, seen.base)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_site_is_pinned_and_reads_its_bytes_in_the_client() {
        let pinned = crate::tooltip::tests::pinned();
        for site in &SITES {
            assert!(
                pinned.iter().any(|(name, _, _)| name == site.symbol),
                "{}",
                site.name
            );
        }
        let Ok(path) = std::env::var("ENSH_CLIENT_EXE") else {
            return;
        };
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        let Ok(image) = enshwick_loader_reflect::Image::parse(
            &bytes,
            enshwick_loader_reflect::Layout::File,
            None,
        ) else {
            unreachable!("the client parses");
        };
        for site in &SITES {
            let Some((_, rva, _)) = pinned.iter().find(|(name, _, _)| name == site.symbol) else {
                continue;
            };
            let found = image.read(image.base() + rva, 8);
            assert_eq!(found, Some(&site.expected[..]), "{}", site.name);
        }
    }
}
