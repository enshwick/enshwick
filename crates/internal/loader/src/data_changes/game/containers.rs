//! The loader's patches of the templates that items with changed container
//! slots place: the slots each gets, never fewer than it has.

use core::ffi::c_void;

use enshwick::abi::{ResourceKey, Status};
use enshwick::content::Guid;
use enshwick::content::changes::{Entry, Field};
use enshwick::content::containers::{ContainerLayout, container_slots, resize_container};
use enshwick_gamedata::catalogue;
use rustc_hash::FxHashMap;

use super::{CHANGES, on};
use crate::data_changes::{Key, grown_slots, template_slots};
use crate::host::Loader;

/// The loader's patches of the templates the items with changed container
/// slots place: one per template, for every item that places it.
pub(super) fn container_patches(
    loader: &'static Loader,
) -> Vec<(ResourceKey, enshwick::abi::ResourcePatchFn, *mut c_void)> {
    let Some(sealed) = CHANGES.sealed.get() else {
        return Vec::new();
    };
    let merged = sealed.merged();
    let items: Vec<Guid> = merged
        .entries(Entry::Item)
        .into_iter()
        .filter(|item| merged.keys(*item).iter().any(|k| k.field == Field::Slots))
        .collect();
    if items.is_empty() {
        return Vec::new();
    }
    let facts = catalogue::Reflected(&loader.reflection);
    let at = |id: &str| {
        catalogue::constant(id, &loader.reflection).and_then(|at| usize::try_from(at).ok())
    };
    let placed = at(catalogue::ids::ITEM_INFO_PLACED_ENTITY);
    let own = at(catalogue::ids::ITEM_INFO_PID_ENTITY);
    let (Ok(layout), Some(placed), Some(archive)) = (
        ContainerLayout::resolve(&facts),
        placed,
        crate::archive::game(),
    ) else {
        crate::log!(
            WARN,
            "container slot changes are off: this build does not reflect containers, or the game's archive does not read"
        );
        return Vec::new();
    };
    let mut templates: Vec<(Guid, Vec<Guid>)> = Vec::new();
    for item in items {
        let info = archive.resource(&item.key_by_hash(crate::catalogue::types::ITEM_INFO));
        let holds = |template: &Guid| {
            archive
                .resource(&template.key_by_hash(crate::catalogue::types::TEMPLATE))
                .is_some_and(|data| container_slots(&layout, &data).is_some())
        };
        let template = info.as_deref().and_then(|info| {
            [Some(placed), own]
                .into_iter()
                .flatten()
                .filter_map(|at| info.get(at..at + 16)?.try_into().ok().map(Guid))
                .filter(|template| template.0 != [0; 16])
                .find(holds)
        });
        let Some(template) = template else {
            crate::log!(
                WARN,
                "container slots of {item} are not changed: it is no vanilla item that places a container or carries one"
            );
            continue;
        };
        match templates.iter_mut().find(|(t, _)| *t == template) {
            Some((_, items)) => items.push(item),
            None => templates.push((template, vec![item])),
        }
    }
    let mut patches = Vec::new();
    let mut placed_by = FxHashMap::default();
    for (template, mut items) in templates {
        items.sort_by_key(|item| sealed.slots_order.get(item).copied().unwrap_or(0));
        crate::log!(
            INFO,
            "container slots change on template {template}, which {} places, and so for every item that places it",
            items
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );
        let patch: &'static ContainerPatch = Box::leak(Box::new(ContainerPatch { items, layout }));
        let key = template.key_by_hash(crate::catalogue::types::TEMPLATE);
        for item in &patch.items {
            placed_by.insert(*item, key);
        }
        let user = core::ptr::from_ref(patch).cast_mut().cast::<c_void>();
        patches.push((key, build_container as enshwick::abi::ResourcePatchFn, user));
    }
    let _first = sealed.templates.set(placed_by);
    patches
}

/// What the loader's patch of a container's template needs: the changed
/// items that place it, in load order, and the layout.
struct ContainerPatch {
    items: Vec<Guid>,
    layout: ContainerLayout,
}

/// The loader's patch of a container's template: the base with the slots
/// the changes of the mods still on give it, never fewer than it has.
unsafe extern "system" fn build_container(
    base: *const u8,
    base_len: usize,
    user: *mut c_void,
    out: *mut *const u8,
    out_len: *mut usize,
) -> Status {
    // SAFETY: `user` is the `ContainerPatch` `container_patches` leaked,
    // which lives for the process.
    let patch = unsafe { &*user.cast::<ContainerPatch>() };
    let mut bytes = vec![0; base_len];
    // SAFETY: `ResourcePatchFn` hands `base_len` readable bytes at `base`,
    // unchanged for the call.
    unsafe { enshwick::raw::read_bytes_at(base, 0, &mut bytes) };
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| resized(patch, &bytes)));
    let Ok(Some(built)) = built else {
        return Status::REFUSED;
    };
    // The game reads the resource for the life of the process.
    let built: &'static [u8] = Box::leak(built.into_boxed_slice());
    // SAFETY: `run_patches` passes `out` pointing at a local `*const u8`,
    // writable for the call.
    unsafe { out.write(built.as_ptr()) };
    // SAFETY: `run_patches` passes `out_len` pointing at a local `usize`,
    // writable for the call.
    unsafe { out_len.write(built.len()) };
    Status::OK
}

/// The template `bytes` with the slots its changed items give it.
fn resized(patch: &ContainerPatch, bytes: &[u8]) -> Option<Vec<u8>> {
    let sealed = CHANGES.sealed.get()?;
    let Some(old) = container_slots(&patch.layout, bytes) else {
        crate::log!(
            WARN,
            "container slots of {} are not changed: the template has no inventory of its own",
            patch
                .items
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );
        return Some(bytes.to_vec());
    };
    let values: Vec<(Guid, f64)> = patch
        .items
        .iter()
        .filter_map(|&item| {
            let key = Key::new(item, Field::Slots);
            let value = sealed
                .merged()
                .value(key, f64::from(old), |m| on(sealed.loader, m))?;
            Some((item, value))
        })
        .collect();
    let (Some(new), disagree) = template_slots(&values) else {
        return Some(bytes.to_vec());
    };
    if disagree {
        let all = values
            .iter()
            .map(|(item, value)| format!("{item} {value}"))
            .collect::<Vec<_>>()
            .join(", ");
        crate::log!(
            WARN,
            "items that place one container ask for different slots ({all}): the last in load order counts"
        );
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Field::resolve keeps slots whole and from 8 to 128"
    )]
    let (new, lowered) = grown_slots(old, new as u8);
    if lowered {
        crate::log!(
            WARN,
            "container slots stay at {old}: a container never shrinks, as an upgraded chest moves every item it holds"
        );
    }
    resize_container(&patch.layout, bytes, new)
}
