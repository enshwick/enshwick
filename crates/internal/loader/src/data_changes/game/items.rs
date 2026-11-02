//! The loader's patch of each changed item: its stats and stack size, and
//! the raise and kept size it notes.

use core::ffi::c_void;

use enshwick::abi::Status;
use enshwick::content::changes::Field;
use enshwick_gamedata::catalogue;
use rustc_hash::FxHashMap;

use super::{CHANGES, ItemPatch, on};
use crate::data_changes::{ItemOffsets, Key, Raised, credited, edit_item, served};
use crate::host::Loader;
use crate::locks::Relock;
use crate::mod_index::ModIndex;

/// Each stat with the catalogue ids of its value and of the flag that says
/// an item has it.
pub(super) const STATS: [(Field, &str, &str); 6] = [
    (
        Field::DamageMod,
        catalogue::ids::ITEM_INFO_DAMAGE_MOD,
        catalogue::ids::ITEM_INFO_DAMAGE_SET,
    ),
    (
        Field::SpeedMod,
        catalogue::ids::ITEM_INFO_SPEED_MOD,
        catalogue::ids::ITEM_INFO_DAMAGE_SET,
    ),
    (
        Field::Armor,
        catalogue::ids::ITEM_INFO_ARMOR,
        catalogue::ids::ITEM_INFO_ARMOR_SET,
    ),
    (
        Field::BurnTime,
        catalogue::ids::ITEM_INFO_BURN_TIME,
        catalogue::ids::ITEM_INFO_FUEL_SET,
    ),
    (
        Field::EnergyOutput,
        catalogue::ids::ITEM_INFO_ENERGY_OUTPUT,
        catalogue::ids::ITEM_INFO_FUEL_SET,
    ),
    (
        Field::ComfortAmount,
        catalogue::ids::ITEM_INFO_COMFORT_AMOUNT,
        catalogue::ids::ITEM_INFO_COMFORT_SET,
    ),
];

/// Where an item's values sit on this build.
pub(super) fn item_offsets(loader: &Loader) -> ItemOffsets {
    let at = |id: &str| {
        catalogue::constant(id, &loader.reflection).and_then(|at| usize::try_from(at).ok())
    };
    let armor_count = loader
        .reflection
        .type_info(catalogue::type_name(catalogue::ids::ARMOR_DISTRIBUTION))
        .ok()
        .and_then(|info| usize::try_from(info.size() / 4).ok())
        .unwrap_or(0);
    ItemOffsets {
        max_stack: at(catalogue::ids::ITEM_INFO_MAX_STACK),
        stats: STATS
            .iter()
            .filter_map(|&(field, value, set)| Some((field, at(value)?, at(set)?)))
            .collect(),
        armor_count,
    }
}

/// The loader's patch of an item: the base with its changed values.
pub(super) unsafe extern "system" fn build_item(
    base: *const u8,
    base_len: usize,
    user: *mut c_void,
    out: *mut *const u8,
    out_len: *mut usize,
) -> Status {
    // SAFETY: `user` is the `ItemPatch` `serve` leaked for this key, which
    // lives for the process.
    let patch = unsafe { &*user.cast::<ItemPatch>() };
    let mut bytes = vec![0; base_len];
    // SAFETY: `ResourcePatchFn` hands `base_len` readable bytes at `base`,
    // unchanged for the call.
    unsafe { enshwick::raw::read_bytes_at(base, 0, &mut bytes) };
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        changed_item(patch, &mut bytes);
    }));
    if built.is_err() {
        return Status::REFUSED;
    }
    // The game reads the resource for the life of the process.
    let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
    // SAFETY: `run_patches` passes `out` pointing at a local `*const u8`,
    // writable for the call.
    unsafe { out.write(bytes.as_ptr()) };
    // SAFETY: `run_patches` passes `out_len` pointing at a local `usize`,
    // writable for the call.
    unsafe { out_len.write(bytes.len()) };
    Status::OK
}

/// Writes the changed stack size into the item `bytes`, and notes a raise.
fn changed_item(patch: &ItemPatch, bytes: &mut [u8]) {
    let Some(sealed) = CHANGES.sealed.get() else {
        return;
    };
    let (loader, labels) = (sealed.loader, &sealed.labels);
    let merged = sealed.merged();
    let live = |m: ModIndex| on(loader, m);
    let fields: Vec<Field> = merged
        .keys(patch.item)
        .into_iter()
        .map(|key| key.field)
        .collect();
    let vanilla = stack_at(bytes, patch.offsets);
    let edit = edit_item(bytes, patch.offsets, &fields, |field, base| {
        merged.value(Key::new(patch.item, field), base, live)
    });
    keep_stack(patch, bytes, vanilla);
    for field in &edit.lacking {
        crate::log!(
            WARN,
            "{field} of {} is not changed: the item has none",
            patch.item
        );
    }
    let Some((old, new)) = edit.stack.filter(|(old, new)| new > old) else {
        return;
    };
    let key = Key::new(patch.item, Field::MaxStackSize);
    let without = |m: ModIndex| {
        merged
            .value(key, f64::from(old), |o| live(o) && o != m)
            .unwrap_or(f64::from(old))
    };
    let alone = |m: ModIndex| {
        merged
            .value(key, f64::from(old), |o| live(o) && o == m)
            .unwrap_or(f64::from(old))
    };
    let owners = merged.owners(key, live);
    let label = labels
        .get(&patch.item)
        .cloned()
        .unwrap_or_else(|| patch.item.to_string());
    let mut raised = CHANGES.raised.relock();
    let raised = raised.get_or_insert_with(FxHashMap::default);
    for owner in credited(&owners, f64::from(old), f64::from(new), without, alone) {
        raised
            .entry(owner)
            .and_modify(|r| r.note(patch.item))
            .or_insert_with(|| Raised::new(patch.item, label.clone(), old, new));
    }
}

/// The stack size in `bytes`, an item's static data.
fn stack_at(bytes: &[u8], offsets: &ItemOffsets) -> Option<u16> {
    let at = offsets.max_stack?;
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

/// Serves at least the kept stack size of the patched item in `bytes`,
/// whose game size is `vanilla`, and notes the size served.
fn keep_stack(patch: &ItemPatch, bytes: &mut [u8], vanilla: Option<u16>) {
    let (Some(at), Some(vanilla), Some(merged)) = (
        patch.offsets.max_stack,
        vanilla,
        stack_at(bytes, patch.offsets),
    ) else {
        return;
    };
    let kept = CHANGES
        .kept
        .relock()
        .as_ref()
        .and_then(|kept| kept.of(patch.item));
    let (size, won) = served(merged, kept);
    if won {
        if let Some(slot) = bytes.get_mut(at..at + 2) {
            slot.copy_from_slice(&size.to_le_bytes());
        }
        crate::log!(
            INFO,
            "stacks of {} stay at {size}, not {merged}: a saved world may hold stacks that large",
            patch.item
        );
    }
    CHANGES
        .stacks
        .relock()
        .get_or_insert_with(FxHashMap::default)
        .insert(patch.item, (vanilla, size));
}
