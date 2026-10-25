//! Which argument of the client's item tooltip names the hovered item: a
//! look at the entry of the tooltip, of the item viewers that build its
//! stat rows, and of the function that draws the rows, logging what each
//! call is handed whenever it changes.
//!
//! Every hook only reads. Each site's first bytes are checked against the
//! build the addresses were found on; a site that differs stays unhooked.

use core::fmt::Write as _;
use core::hash::Hasher as _;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Instant;

use enshwick_loader_api::abi::Registers;
use enshwick_loader_api::{Load, Off};

use crate::memory;

/// What a site is handed, and so what it logs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// The tooltip's entry: `rcx` its description, `dl` a flag.
    Show,
    /// The tooltip's body: `rcx` a copy of the description.
    Body,
    /// An item viewer: up to four arguments in registers, more on the stack.
    Viewer,
    /// The stat rows: `rcx` a slice of rows, `rdx` the options.
    Rows,
}

/// One function looked at.
#[derive(Debug)]
struct Site {
    name: &'static str,
    /// The symbol the manifest pins for the client build.
    symbol: &'static str,
    /// Its first bytes on that build.
    expected: [u8; 16],
    kind: Kind,
}

/// The sites, client build 1269579. The tooltip entry has eight callers,
/// one per window that shows items; the viewers are every caller of the
/// row drawer.
const SITES: [Site; 10] = [
    Site {
        name: "tooltip",
        symbol: "research.tooltip",
        expected: [
            0x48, 0x89, 0x5c, 0x24, 0x08, 0x48, 0x89, 0x7c, 0x24, 0x10, 0x55, 0x48, 0x8d, 0xac,
            0x24, 0x00,
        ],
        kind: Kind::Show,
    },
    Site {
        name: "tooltip_body",
        symbol: "research.tooltip_body",
        expected: [
            0x40, 0x55, 0x53, 0x56, 0x57, 0x41, 0x54, 0x41, 0x55, 0x41, 0x56, 0x41, 0x57, 0x48,
            0x8d, 0xac,
        ],
        kind: Kind::Body,
    },
    Site {
        name: "stat_rows",
        symbol: "research.stat_rows",
        expected: [
            0x48, 0x8b, 0xc4, 0x48, 0x89, 0x50, 0x10, 0x48, 0x89, 0x48, 0x08, 0x55, 0x53, 0x56,
            0x57, 0x41,
        ],
        kind: Kind::Rows,
    },
    Site {
        name: "viewer_compare",
        symbol: "research.viewer_compare",
        expected: [
            0x40, 0x55, 0x56, 0x57, 0x41, 0x54, 0x41, 0x55, 0x41, 0x57, 0x48, 0x8d, 0xac, 0x24,
            0xe8, 0xf6,
        ],
        kind: Kind::Viewer,
    },
    Site {
        name: "viewer_b562c0",
        symbol: "research.viewer_b562c0",
        expected: [
            0x48, 0x8b, 0xc4, 0x55, 0x56, 0x41, 0x54, 0x41, 0x55, 0x41, 0x56, 0x41, 0x57, 0x48,
            0x8d, 0xa8,
        ],
        kind: Kind::Viewer,
    },
    Site {
        name: "viewer_b56b80",
        symbol: "research.viewer_b56b80",
        expected: [
            0x48, 0x8b, 0xc4, 0x55, 0x53, 0x57, 0x41, 0x54, 0x41, 0x55, 0x41, 0x57, 0x48, 0x8d,
            0xa8, 0xa8,
        ],
        kind: Kind::Viewer,
    },
    Site {
        name: "viewer_beb220",
        symbol: "research.viewer_beb220",
        expected: [
            0x48, 0x89, 0x5c, 0x24, 0x18, 0x48, 0x89, 0x74, 0x24, 0x20, 0x55, 0x57, 0x41, 0x56,
            0x48, 0x8d,
        ],
        kind: Kind::Viewer,
    },
    Site {
        name: "viewer_bebb60",
        symbol: "research.viewer_bebb60",
        expected: [
            0x48, 0x89, 0x74, 0x24, 0x18, 0x48, 0x89, 0x7c, 0x24, 0x20, 0x55, 0x41, 0x54, 0x41,
            0x55, 0x41,
        ],
        kind: Kind::Viewer,
    },
    Site {
        name: "viewer_c3da50",
        symbol: "research.viewer_c3da50",
        expected: [
            0x48, 0x8b, 0xc4, 0x48, 0x89, 0x58, 0x20, 0x55, 0x48, 0x8d, 0xa8, 0xd8, 0xfe, 0xff,
            0xff, 0x48,
        ],
        kind: Kind::Viewer,
    },
    Site {
        name: "viewer_b55140",
        symbol: "research.viewer_b55140",
        expected: [
            0x48, 0x8b, 0xc4, 0x55, 0x53, 0x56, 0x57, 0x48, 0x8d, 0xa8, 0x38, 0xfc, 0xff, 0xff,
            0x48, 0x81,
        ],
        kind: Kind::Viewer,
    },
];

/// The bytes of the tooltip's description: its entry copies this many.
const DESCRIPTION: usize = 0x180;
/// The bytes shown behind each argument of a viewer.
const ARGUMENT: usize = 0x80;
/// The bytes of one stat row.
const ROW: usize = 0x28;
/// The most rows shown of one call.
const MOST_ROWS: usize = 16;
/// The bytes shown behind a pointer found in a description.
const POINTEE: usize = 0x40;
/// The most pointers in a description followed.
const MOST_POINTEES: usize = 12;
/// Stack arguments of a viewer shown, from `rsp + 0x28` at its entry.
const STACK_ARGUMENTS: usize = 4;

/// The tooltip's shared state: when it started, the marks the player set,
/// and how many dumps it may still write.
#[derive(Debug)]
struct Shared {
    start: Instant,
    base: usize,
    interval_ms: u64,
    max_dumps: u32,
    dumps: AtomicU32,
    last: [AtomicU64; SITES.len()],
    last_at: [AtomicU64; SITES.len()],
    mark_seen: [AtomicU32; SITES.len()],
}

static SHARED: OnceLock<Shared> = OnceLock::new();

/// The marks set so far.
static MARK: AtomicU32 = AtomicU32::new(0);
/// The values the last mark looks for, and how many there are.
static LOOKED_FOR: [AtomicU32; 4] = [const { AtomicU32::new(0) }; 4];
static LOOKED_FOR_COUNT: AtomicU32 = AtomicU32::new(0);

/// Hooks every site whose bytes match; returns how many it hooked.
pub(crate) fn link(load: &mut Load<'_>, interval_ms: u64, max_dumps: u32) -> Result<usize, Off> {
    let start = load.code()?.as_ptr().addr();
    let _first = SHARED.set(Shared {
        start: Instant::now(),
        base: memory::image_base(),
        interval_ms,
        max_dumps,
        dumps: AtomicU32::new(0),
        last: Default::default(),
        last_at: Default::default(),
        mark_seen: Default::default(),
    });
    let mut hooked = 0;
    for (index, site) in SITES.iter().enumerate() {
        let address = match load.symbol(site.symbol) {
            Ok(address) => address,
            Err(off) => {
                enshwick_loader_api::warn!("R1 {} off: {off}", site.name);
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
                "R1 {} off: its bytes at {at:#x} are {found:02x?}, not those of build 1269579",
                site.name
            );
            continue;
        }
        let kind = site.kind;
        // SAFETY: the site's bytes match build 1269579, where it starts an
        // instruction; the handler only reads the registers.
        match unsafe { load.hook_at(address, move |registers| seen(index, kind, registers)) } {
            Ok(()) => hooked += 1,
            Err(off) => enshwick_loader_api::warn!("R1 {} off: {off}", site.name),
        }
    }
    Ok(hooked)
}

/// Starts a new section of the log: every site dumps its next call, and
/// marks the `values` it finds.
pub(crate) fn mark(values: &[u32]) -> u32 {
    let count = values.len().min(LOOKED_FOR.len());
    for (slot, &value) in LOOKED_FOR.iter().zip(&values[..count]) {
        slot.store(value, Ordering::Relaxed);
    }
    #[expect(clippy::cast_possible_truncation, reason = "at most four values")]
    LOOKED_FOR_COUNT.store(count as u32, Ordering::Release);
    if let Some(shared) = SHARED.get() {
        shared.dumps.store(0, Ordering::Relaxed);
    }
    MARK.fetch_add(1, Ordering::AcqRel) + 1
}

/// How many dumps were written since the last mark, and the cap.
pub(crate) fn dumps() -> Option<(u32, u32)> {
    SHARED
        .get()
        .map(|shared| (shared.dumps.load(Ordering::Relaxed), shared.max_dumps))
}

/// One region read for a call.
struct Region {
    label: String,
    address: usize,
    bytes: Vec<u8>,
}

impl Region {
    fn read(label: impl Into<String>, address: usize, size: usize) -> Self {
        let mut bytes = vec![0u8; size];
        let read = memory::read(address, &mut bytes);
        bytes.truncate(read);
        Self {
            label: label.into(),
            address,
            bytes,
        }
    }
}

/// What one call is handed.
struct Call {
    from: usize,
    registers: Vec<(&'static str, u64)>,
    regions: Vec<Region>,
}

fn reg(value: u64) -> usize {
    usize::try_from(value).unwrap_or(0)
}

/// Reads what a call of `kind` is handed.
fn call(kind: Kind, registers: &Registers) -> Call {
    let rsp = reg(registers.rsp);
    let from = memory::read_u64(rsp).map_or(0, reg);
    match kind {
        Kind::Show | Kind::Body => Call {
            from,
            registers: vec![("rcx", registers.rcx), ("rdx", registers.rdx & 0xff)],
            regions: vec![Region::read("rcx", reg(registers.rcx), DESCRIPTION)],
        },
        Kind::Viewer => {
            let mut stack = Vec::with_capacity(STACK_ARGUMENTS);
            for n in 0..STACK_ARGUMENTS {
                stack.push(memory::read_u64(rsp + 0x28 + n * 8).unwrap_or(0));
            }
            let mut registers = vec![
                ("rcx", registers.rcx),
                ("rdx", registers.rdx),
                ("r8", registers.r8),
                ("r9", registers.r9),
            ];
            let regions = registers
                .iter()
                .filter(|(_, value)| memory::plausible(reg(*value)))
                .map(|&(name, value)| Region::read(name, reg(value), ARGUMENT))
                .collect();
            registers.extend(
                ["stack+28", "stack+30", "stack+38", "stack+40"]
                    .into_iter()
                    .zip(stack),
            );
            Call {
                from,
                registers,
                regions,
            }
        }
        Kind::Rows => {
            let slice = Region::read("slice", reg(registers.rcx), 0x10);
            let mut regions = Vec::with_capacity(3);
            if let (Some(rows), Some(count)) = (
                slice.bytes.get(0..8).map(le_u64),
                slice.bytes.get(8..16).map(le_u64),
            ) {
                let count = reg(count & 0xffff_ffff).min(MOST_ROWS);
                regions.push(Region::read("rows", reg(rows), count * ROW));
            }
            regions.insert(0, slice);
            regions.push(Region::read("options", reg(registers.rdx), 0x20));
            Call {
                from,
                registers: vec![("rcx", registers.rcx), ("rdx", registers.rdx)],
                regions,
            }
        }
    }
}

fn le_u64(bytes: &[u8]) -> u64 {
    let mut word = [0u8; 8];
    word.copy_from_slice(bytes);
    u64::from_le_bytes(word)
}

/// A hash of what the call is handed, which changes when any of it does.
fn signature(call: &Call) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    hasher.write_usize(call.from);
    for (_, value) in &call.registers {
        hasher.write_u64(*value);
    }
    for region in &call.regions {
        hasher.write(&region.bytes);
    }
    hasher.finish().max(1)
}

/// The hook at site `index`: dumps the call when what it is handed changed,
/// at most once per interval, and always after a new mark.
fn seen(index: usize, kind: Kind, registers: &mut Registers) {
    let Some(shared) = SHARED.get() else {
        return;
    };
    if shared.dumps.load(Ordering::Relaxed) >= shared.max_dumps {
        return;
    }
    let call = call(kind, registers);
    let mark = MARK.load(Ordering::Acquire);
    let marked = shared.mark_seen[index].swap(mark, Ordering::AcqRel) != mark;
    let sig = signature(&call);
    let changed = shared.last[index].swap(sig, Ordering::AcqRel) != sig;
    if !marked && !changed {
        return;
    }
    let now = u64::try_from(shared.start.elapsed().as_millis()).unwrap_or(u64::MAX);
    if !marked
        && now.saturating_sub(shared.last_at[index].load(Ordering::Relaxed)) < shared.interval_ms
    {
        // Seen as new again on the next call, once the interval is over.
        shared.last[index].store(0, Ordering::Relaxed);
        return;
    }
    shared.last_at[index].store(now, Ordering::Relaxed);
    let number = shared.dumps.fetch_add(1, Ordering::AcqRel) + 1;
    let looked_for: Vec<u32> = LOOKED_FOR
        [..(LOOKED_FOR_COUNT.load(Ordering::Acquire) as usize).min(LOOKED_FOR.len())]
        .iter()
        .map(|v| v.load(Ordering::Relaxed))
        .collect();
    let site = &SITES[index];
    let text = dump(&call, site, kind, shared.base, mark, number, &looked_for);
    enshwick_loader_api::info!("{text}");
    if number == shared.max_dumps {
        enshwick_loader_api::info!(
            "R1 {number} dumps since the last mark: no more until the next mark"
        );
    }
}

/// The lines of one dump.
fn dump(
    call: &Call,
    site: &Site,
    kind: Kind,
    base: usize,
    mark: u32,
    number: u32,
    looked_for: &[u32],
) -> String {
    let mut text = format!(
        "R1 dump {number} (mark {mark}) {} called from {}:",
        site.name,
        memory::as_note(call.from, base)
    );
    for (name, value) in &call.registers {
        let _infallible = write!(text, " {name}={value:#x}");
    }
    for region in &call.regions {
        if region.bytes.is_empty() {
            let _infallible = write!(
                text,
                "\n  {} at {:#x}: not readable",
                region.label, region.address
            );
            continue;
        }
        for line in memory::words(&region.label, &region.bytes) {
            text.push('\n');
            text.push_str(&line);
        }
        let found = memory::matches(&region.bytes, looked_for);
        if !found.is_empty() {
            let _infallible = write!(
                text,
                "\n  {} holds the marked values: {}",
                region.label,
                found.join(", ")
            );
        }
        if kind == Kind::Show && region.label == "rcx" {
            follow(&mut text, region, base, looked_for);
        }
    }
    text
}

/// Appends what the pointers in a description point at, one level deep.
fn follow(text: &mut String, region: &Region, base: usize, looked_for: &[u32]) {
    let pointers = region
        .bytes
        .as_chunks::<8>()
        .0
        .iter()
        .enumerate()
        .map(|(n, word)| (n * 8, reg(u64::from_le_bytes(*word))))
        .filter(|&(_, value)| memory::plausible(value) && memory::in_image(value, base).is_none())
        .take(MOST_POINTEES);
    for (offset, pointer) in pointers {
        let pointee = Region::read(format!("[rcx+{offset:03x}]"), pointer, POINTEE);
        if pointee.bytes.is_empty() {
            continue;
        }
        for line in memory::words(&pointee.label, &pointee.bytes) {
            text.push('\n');
            text.push_str(&line);
        }
        let found = memory::matches(&pointee.bytes, looked_for);
        if !found.is_empty() {
            let _infallible = write!(
                text,
                "\n  {} holds the marked values: {}",
                pointee.label,
                found.join(", ")
            );
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn a_description_dump_names_the_site_and_the_marked_values() {
        let mut description = vec![0u8; DESCRIPTION];
        description[8..12].copy_from_slice(&0x0c01u32.to_le_bytes());
        description[0x9c..0x9e].copy_from_slice(&3u16.to_le_bytes());
        let registers = Registers {
            rcx: description.as_ptr().addr() as u64,
            rdx: 0x101,
            ..Registers::default()
        };
        let call = call(Kind::Show, &registers);
        assert_eq!(call.regions[0].bytes, description);
        let text = dump(&call, &SITES[0], Kind::Show, 0, 2, 7, &[3]);
        assert!(
            text.starts_with("R1 dump 7 (mark 2) tooltip called from 0x0:"),
            "{text}"
        );
        assert!(text.contains(" rdx=0x1"), "{text}");
        assert!(
            text.contains("  rcx+000: 00000000 00000000 00000c01"),
            "{text}"
        );
        assert!(
            text.contains("rcx holds the marked values: u16 3 at +0x9c"),
            "{text}"
        );
    }

    #[test]
    fn rows_are_read_through_the_slice() {
        let rows = vec![7u8; 2 * ROW];
        let mut slice = [0u8; 16];
        slice[..8].copy_from_slice(&(rows.as_ptr().addr() as u64).to_le_bytes());
        slice[8..].copy_from_slice(&2u64.to_le_bytes());
        let registers = Registers {
            rcx: slice.as_ptr().addr() as u64,
            ..Registers::default()
        };
        let call = call(Kind::Rows, &registers);
        let labels: Vec<&str> = call.regions.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, ["slice", "rows", "options"]);
        assert_eq!(call.regions[1].bytes, rows);
        assert!(call.regions[2].bytes.is_empty());
    }

    #[test]
    fn the_signature_follows_the_bytes() {
        let mut description = vec![0u8; DESCRIPTION];
        let registers = Registers {
            rcx: description.as_ptr().addr() as u64,
            ..Registers::default()
        };
        let first = signature(&call(Kind::Show, &registers));
        description[0x40] = 1;
        let registers = Registers {
            rcx: description.as_ptr().addr() as u64,
            ..Registers::default()
        };
        assert_ne!(first, signature(&call(Kind::Show, &registers)));
    }

    /// The client rows the manifest pins, by symbol.
    pub(crate) fn pinned() -> Vec<(String, u64, enshwick_loader_api::abi::Fingerprint)> {
        use enshwick_loader_api::Mod as _;

        let manifest = crate::Research::manifest();
        // SAFETY: the manifest macro points at static rows of these counts.
        let builds = unsafe { core::slice::from_raw_parts(manifest.builds, manifest.build_count) };
        let mut pinned = Vec::new();
        for build in builds
            .iter()
            .filter(|b| b.side == enshwick_loader_api::abi::Side::CLIENT)
        {
            // SAFETY: as above.
            let symbols = unsafe { core::slice::from_raw_parts(build.symbols, build.symbol_count) };
            for symbol in symbols {
                // SAFETY: a static string.
                let name = unsafe { symbol.name.as_str() }.unwrap_or_default();
                pinned.push((name.to_owned(), symbol.rva, build.fingerprint));
            }
        }
        pinned
    }

    /// The client rows the manifest pins for the build of `image`, by symbol.
    pub(crate) fn pinned_in(image: &enshwick_loader_reflect::Image<'_>) -> Vec<(String, u64)> {
        let build = image.fingerprint();
        pinned()
            .into_iter()
            .filter(|(_, _, f)| {
                build == Some(enshwick_loader_reflect::Fingerprint::new(f.guid, f.age))
            })
            .map(|(name, rva, _)| (name, rva))
            .collect()
    }

    #[test]
    fn every_client_build_pins_each_site_once() {
        let pinned = pinned();
        let mut builds: Vec<([u8; 16], u32)> =
            pinned.iter().map(|(_, _, f)| (f.guid, f.age)).collect();
        builds.sort_unstable();
        builds.dedup();
        assert!(!builds.is_empty());
        for (guid, age) in builds {
            for site in &SITES {
                let rows = pinned
                    .iter()
                    .filter(|(name, _, f)| name == site.symbol && f.guid == guid && f.age == age)
                    .count();
                assert_eq!(rows, 1, "{} on client build {age}", site.name);
            }
        }
    }

    /// The bytes each site expects are the client's that `ENSH_CLIENT_EXE`
    /// names; skipped without it.
    #[test]
    fn sites_read_their_bytes_in_the_client() {
        let Ok(path) = std::env::var("ENSH_CLIENT_EXE") else {
            return;
        };
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        let image = enshwick_loader_reflect::Image::parse(
            &bytes,
            enshwick_loader_reflect::Layout::File,
            None,
        );
        let Ok(image) = image else {
            unreachable!("the client parses");
        };
        let pinned = pinned_in(&image);
        for site in &SITES {
            let rva = pinned.iter().find(|(name, _)| name == site.symbol);
            let Some((_, rva)) = rva else {
                unreachable!("{} is pinned for this client build", site.name);
            };
            let found = image.read(image.base() + rva, 16);
            assert_eq!(found, Some(&site.expected[..]), "{}", site.name);
        }
    }
}
