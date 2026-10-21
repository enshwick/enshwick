//! R17: health set by the game's impact programs, outside `ApplyDamage`:
//! heals, revives and scripted kills. The writer every program node goes
//! through takes the attribute, the entity and a value pin into the
//! program's scratch; for the health attribute this logs the entity, the
//! pin's type and the value it sets, for the first lines of a run.

use std::sync::atomic::{AtomicU32, Ordering};

use enshwick_loader_api::{Load, Next, Off};

use crate::memory;

/// The current-health attribute.
const HEALTH: u32 = 0x8eb8_4995;
/// Value pin types: `s32`, `u32`, `bool`, `f32`.
const PIN_TYPES: [(u32, &str); 4] = [
    (0x3294_0bec, "s32"),
    (0x0985_f31e, "u32"),
    (0xc894_953d, "bool"),
    (0xe89f_7410, "f32"),
];
/// The most health writes logged in a run.
const MOST: u32 = 300;

static SEEN: AtomicU32 = AtomicU32::new(0);
static OTHER: AtomicU32 = AtomicU32::new(0);

type Writer = unsafe extern "system" fn(
    out: *mut u8,
    attribute: u32,
    entity: u32,
    value: *const u8,
    program: *const u8,
) -> *mut u8;

enshwick_loader_api::detour! {
    static WRITER: unsafe extern "system" fn(
        out: *mut u8,
        attribute: u32,
        entity: u32,
        value: *const u8,
        program: *const u8,
    ) -> *mut u8 = writer;
}

/// Links the impact programs' attribute writer.
pub(crate) fn link(load: &mut Load<'_>) -> Result<(), Off> {
    load.hook(&WRITER, load.symbol("research.impact_attribute")?)
}

/// How many health writes and other attribute writes were seen.
pub(crate) fn counts() -> (u32, u32) {
    (SEEN.load(Ordering::Relaxed), OTHER.load(Ordering::Relaxed))
}

fn writer(
    next: Next<Writer>,
    out: *mut u8,
    attribute: u32,
    entity: u32,
    value: *const u8,
    program: *const u8,
) -> *mut u8 {
    if attribute == HEALTH {
        let seen = SEEN.fetch_add(1, Ordering::Relaxed);
        if seen < MOST {
            log(seen + 1, entity, value.addr(), program.addr());
        }
    } else {
        OTHER.fetch_add(1, Ordering::Relaxed);
    }
    // SAFETY: the game's own arguments, forwarded unchanged.
    unsafe { next.get()(out, attribute, entity, value, program) }
}

/// The pin's type at `+8`, its scratch offset at `+0xe`, and the program's
/// scratch base at `+0x10`; build 1269579.
fn log(number: u32, entity: u32, value: usize, program: usize) {
    let mut pin = [0u8; 16];
    if memory::read(value, &mut pin) < pin.len() {
        enshwick_loader_api::info!("R17 health write {number}: entity {entity}, unreadable pin");
        return;
    }
    let ty = u32::from_le_bytes([pin[8], pin[9], pin[10], pin[11]]);
    let offset = usize::from(u16::from_le_bytes([pin[14], pin[15]]));
    let name = PIN_TYPES
        .iter()
        .find(|(hash, _)| *hash == ty)
        .map_or("?", |(_, name)| name);
    let raw = memory::read_u64(program + 0x10)
        .and_then(|base| usize::try_from(base).ok())
        .and_then(|base| {
            let mut word = [0u8; 4];
            (memory::read(base + offset, &mut word) == 4).then(|| u32::from_le_bytes(word))
        });
    match raw {
        Some(raw) => enshwick_loader_api::info!(
            "R17 health write {number}: entity {entity}, {name} pin {ty:#010x}, value {} (f32 {})",
            raw.cast_signed(),
            f32::from_bits(raw)
        ),
        None => enshwick_loader_api::info!(
            "R17 health write {number}: entity {entity}, {name} pin {ty:#010x}, value unreadable"
        ),
    }
}
