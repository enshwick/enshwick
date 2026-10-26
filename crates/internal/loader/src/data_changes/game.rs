//! Declared changes in the running game: the loader's own patch of each
//! changed item, the conflict reports once every mod ran its init, and the
//! hold a raised stack size sets on the side that keeps the world.

use core::ffi::c_void;
use std::sync::{Mutex, OnceLock};

use enshwick::abi::{Event, Level, Relation, ResourceKey, Status};
use enshwick::content::Guid;
use enshwick::content::changes::{Change, Field};
use enshwick::state::Hold;
use rustc_hash::FxHashMap;

use super::{Declared, HoldStep, Merged, Raised, credited, hold_step, merge};
use crate::host::Loader;
use crate::locks::Relock;
use crate::mod_index::ModIndex;

/// The domain's state: changes as mods declare them, what the end of init
/// sealed, and the stack sizes raised so far.
struct Changes {
    declared: Mutex<Vec<Declared>>,
    sealed: OnceLock<Sealed>,
    raised: Mutex<Option<FxHashMap<ModIndex, Raised>>>,
}

/// What the end of init fixed for the process.
struct Sealed {
    loader: &'static Loader,
    merged: Merged,
    /// The names packs gave the changed entries.
    labels: FxHashMap<Guid, String>,
}

static CHANGES: Changes = Changes {
    declared: Mutex::new(Vec::new()),
    sealed: OnceLock::new(),
    raised: Mutex::new(None),
};

/// What the loader's patch of one item needs: the item and where its stack
/// size sits.
struct ItemPatch {
    item: Guid,
    max_stack_at: usize,
}

/// Records mod `owner`'s `change` from `source`; refused once init is over.
pub(crate) fn declare(owner: ModIndex, change: Change, source: String) -> Status {
    if CHANGES.sealed.get().is_some() {
        return Status::TOO_LATE;
    }
    CHANGES.declared.relock().push(Declared {
        owner,
        change,
        source,
    });
    Status::OK
}

/// Whether the loader follows the world for changes: some mod declared one,
/// or a pack holds the game and may need its hold released.
pub(crate) fn wanted(loader: &Loader) -> bool {
    let declared = CHANGES.sealed.get().map_or_else(
        || !CHANGES.declared.relock().is_empty(),
        |sealed| !sealed.merged.is_empty(),
    );
    declared
        || enshwick::state::holds(&loader.data_dir)
            .iter()
            .any(|(id, _)| loader.mods.iter().any(|m| m.pack && &m.id == id))
}

/// Whether mod `index` is on.
fn on(loader: &Loader, index: ModIndex) -> bool {
    loader
        .mods
        .get(index.get())
        .is_some_and(|record| !record.off.load(std::sync::atomic::Ordering::Acquire))
}

/// The id of mod `index`.
fn id(loader: &Loader, index: ModIndex) -> String {
    loader
        .mods
        .get(index.get())
        .map_or_else(|| index.to_string(), |record| record.id.clone())
}

/// Whether mod `a` requires mod `b`.
fn requires(loader: &Loader, a: ModIndex, b: ModIndex) -> bool {
    let (Some(a), Some(b)) = (loader.mods.get(a.get()), loader.mods.get(b.get())) else {
        return false;
    };
    // SAFETY: a record's manifest is the loader's copy, kept for the process
    // with every string and slice in it.
    let Ok(candidate) = (unsafe { crate::manifest::parse(a.manifest) }) else {
        return false;
    };
    candidate
        .dependencies
        .iter()
        .any(|d| d.relation == Relation::REQUIRES && b.owns(&d.id))
}

/// Merges every declared change, reports the overridden sets, and returns
/// the loader's patches, one per changed item: its key and the patch with
/// its pointer. Called once, when the served resources are sealed.
pub(crate) fn serve(
    loader: &'static Loader,
) -> Vec<(ResourceKey, enshwick::abi::ResourcePatchFn, *mut c_void)> {
    let declared = std::mem::take(&mut *CHANGES.declared.relock());
    let (merged, conflicts) = merge(&declared, |a, b| requires(loader, a, b));
    for conflict in &conflicts {
        let line = conflict.describe(|m| id(loader, m));
        let level = if conflict.intended {
            Level::INFO
        } else {
            Level::WARN
        };
        for owner in [conflict.winner.owner, conflict.overridden.owner] {
            crate::log::write(level, &id(loader, owner), &line);
        }
        let feature = format!("{} of {}", conflict.field, conflict.entry);
        crate::conflicts::note(
            feature.clone(),
            conflict.overridden.owner,
            crate::conflicts::Mode::Exclusive,
            Status::REFUSED,
        );
        crate::conflicts::note(
            feature,
            conflict.winner.owner,
            crate::conflicts::Mode::Exclusive,
            Status::OK,
        );
    }
    let labels: FxHashMap<Guid, String> = declared
        .iter()
        .filter_map(|d| label(&d.source).map(|l| (d.change.entry, l.to_owned())))
        .collect();
    let max_stack_at = enshwick_gamedata::catalogue::constant(
        enshwick_gamedata::catalogue::ITEM_MAX_STACK,
        &loader.reflection,
    )
    .and_then(|at| usize::try_from(at).ok());
    let items: Vec<Guid> = merged.entries(Field::MaxStack).collect();
    let _sealed = CHANGES.sealed.set(Sealed {
        loader,
        merged,
        labels,
    });
    let Some(max_stack_at) = max_stack_at else {
        if !items.is_empty() {
            crate::log!(
                WARN,
                "stack size changes are off: this build does not reflect the items' stack size"
            );
        }
        return Vec::new();
    };
    if !items.is_empty() {
        crate::log!(INFO, "{} items get a changed stack size", items.len());
    }
    items
        .into_iter()
        .map(|item| {
            let patch: &'static ItemPatch = Box::leak(Box::new(ItemPatch { item, max_stack_at }));
            let key = item.key_by_hash(crate::catalogue::types::ITEM_INFO);
            let user = core::ptr::from_ref(patch).cast_mut().cast::<c_void>();
            (key, build_item as enshwick::abi::ResourcePatchFn, user)
        })
        .collect()
}

/// The pack's own name for an entry, from a source such as
/// `items.toml:12 (Wood_Log)`.
fn label(source: &str) -> Option<&str> {
    let (_, rest) = source.split_once(" (")?;
    rest.strip_suffix(')')
}

/// The loader's patch of an item: the base with its changed values.
unsafe extern "system" fn build_item(
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
    let Some(Sealed {
        loader,
        merged,
        labels,
    }) = CHANGES.sealed.get()
    else {
        return;
    };
    let at = patch.max_stack_at;
    let Some(field) = bytes.get_mut(at..at + 2) else {
        crate::log!(WARN, "item {} is too short for its stack size", patch.item);
        return;
    };
    let old = u16::from_le_bytes([field[0], field[1]]);
    let live = |m: ModIndex| on(loader, m);
    let Some(new) = merged.value(patch.item, Field::MaxStack, f64::from(old), live) else {
        return;
    };
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Field::resolve keeps a stack size whole and from 1 to 65535"
    )]
    let new = new as u16;
    field.copy_from_slice(&new.to_le_bytes());
    if new > old {
        let without = |m: ModIndex| {
            merged
                .value(patch.item, Field::MaxStack, f64::from(old), |o| {
                    live(o) && o != m
                })
                .unwrap_or(f64::from(old))
        };
        let owners = merged.owners(patch.item, Field::MaxStack, live);
        let label = labels
            .get(&patch.item)
            .cloned()
            .unwrap_or_else(|| patch.item.to_string());
        let mut raised = CHANGES.raised.relock();
        let raised = raised.get_or_insert_with(FxHashMap::default);
        for owner in credited(&owners, f64::from(new), without) {
            raised
                .entry(owner)
                .and_modify(|r| r.note(patch.item))
                .or_insert_with(|| Raised::new(patch.item, label.clone(), old, new));
        }
    }
}

/// On the side that keeps the world, when it loads: sets the hold of every
/// pack that raised a stack size, and releases that of every other pack
/// still on. A mod with code keeps its own hold.
pub(crate) fn world_event(loader: &Loader, event: Event) {
    if event != Event::WORLD_LOADED {
        return;
    }
    let raised = CHANGES.raised.relock().clone().unwrap_or_default();
    for (index, record) in loader.mods.iter().enumerate() {
        let index = ModIndex::new(index);
        if !on(loader, index) {
            continue;
        }
        let hold = Hold::new(&loader.data_dir.join(&record.id));
        let result = match hold_step(record.pack, raised.get(&index)) {
            HoldStep::Set(reason) => hold.set(&reason),
            HoldStep::Release => hold.clear(),
            HoldStep::Keep => Ok(()),
        };
        if let Err(why) = result {
            crate::log::write(Level::WARN, &record.id, &format!("hold not kept: {why}"));
        }
    }
}

/// Whether the running build reflects what `field` changes.
pub(crate) fn reachable(loader: &Loader, field: Field) -> bool {
    let id = match field {
        Field::MaxStack => enshwick_gamedata::catalogue::ITEM_MAX_STACK,
        _ => return false,
    };
    enshwick_gamedata::catalogue::constant(id, &loader.reflection).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_is_the_name_in_parentheses() {
        assert_eq!(label("items.toml:12 (Wood_Log)"), Some("Wood_Log"));
        assert_eq!(label("items.toml:12"), None);
    }
}
