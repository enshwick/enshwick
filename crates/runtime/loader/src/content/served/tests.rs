use enshwick_base::Relock;
use enshwick_core::content::Guid;

use enshwick_core::abi::Status;

use super::patches::{Build, run_patches, write_differences};
use super::*;

unsafe extern "system" fn add_one(
    base: *const u8,
    base_len: u32,
    _user: *mut c_void,
    out: *mut *mut u8,
    out_len: *mut u32,
) -> Status {
    // SAFETY: `base` holds `base_len` bytes for the call.
    let mut built = unsafe { core::slice::from_raw_parts(base, base_len as usize) }.to_vec();
    built.push(1);
    // SAFETY: `run_patches` passes `out` and `out_len` pointing at locals,
    // writable for the call.
    unsafe { hand_out(built, out, out_len) }
}

#[test]
fn a_rebuild_runs_the_mods_of_the_first_build_although_they_are_off() {
    let key = ResourceKey {
        guid: Guid([0; 16]),
        type_hash: 1,
        part: 0,
    };
    let patches = [3, 4]
        .map(|owner| Patch::new(ModIndex::new(owner), add_one, 0))
        .into();
    let entry = Served::new(key, key, patches, true);
    let base = [7u8; 4];
    let at = base.as_ptr().expose_provenance();
    // No loader runs here, so every mod reads as off.
    assert!(run_patches(&entry, at, base.len(), None, Build::Game).is_none());
    let both = [ModIndex::new(3), ModIndex::new(4)];
    let built = run_patches(&entry, at, base.len(), Some(&both), Build::Game).unwrap();
    assert_eq!((built.len, built.by), (6, both.to_vec()));
}

#[test]
fn a_change_of_a_new_resource_chains_onto_its_maker_in_either_order() {
    let key = |guid: u8| ResourceKey {
        guid: Guid([guid; 16]),
        type_hash: 1,
        part: 0,
    };
    let patch = |owner: usize| Patch::new(ModIndex::new(owner), no_patch, 0);
    let owners = |served: &Served| served.patches.iter().map(|p| p.owner).collect::<Vec<_>>();
    let (new, base, other) = (key(2), key(1), key(3));
    for maker_first in [true, false] {
        let mut pending = Vec::new();
        let (first, second) = if maker_first {
            ((base, 1), (new, 2))
        } else {
            ((new, 2), (base, 1))
        };
        assert_eq!(add(&mut pending, new, first.0, patch(first.1)), Ok(()));
        assert_eq!(add(&mut pending, new, second.0, patch(second.1)), Ok(()));
        assert_eq!(pending[0].base, base, "maker first: {maker_first}");
        assert_eq!(owners(&pending[0]), [ModIndex::new(1), ModIndex::new(2)]);
        assert_eq!(
            add(&mut pending, new, other, patch(3)),
            Err(ServeResourceError::OtherBase),
            "a second maker"
        );
    }
}

#[test]
fn a_renamer_taken_off_a_guid_leaves_its_changes_waiting_for_the_owner_s_maker() {
    let key = |guid: u8| ResourceKey {
        guid: Guid([guid; 16]),
        type_hash: 1,
        part: 0,
    };
    let patch = |owner: usize| Patch::new(ModIndex::new(owner), no_patch, 0);
    let owners = |served: &Served| served.patches.iter().map(|p| p.owner).collect::<Vec<_>>();
    let (claimed, game, other) = (key(2), key(1), key(3));
    let mut pending = Vec::new();
    assert_eq!(add(&mut pending, claimed, game, patch(1)), Ok(()));
    assert_eq!(add(&mut pending, claimed, claimed, patch(2)), Ok(()));
    assert_eq!(add(&mut pending, other, game, patch(1)), Ok(()));
    take_off(&mut pending, &[ModIndex::new(1)], claimed.guid);
    assert_eq!(pending[0].base, claimed, "the changes wait for a maker");
    assert_eq!(owners(&pending[0]), [ModIndex::new(2)]);
    assert_eq!(owners(&pending[1]), [ModIndex::new(1)], "another key stays");
    assert_eq!(add(&mut pending, claimed, other, patch(3)), Ok(()));
    assert_eq!(pending[0].base, other, "the owner's maker comes first");
    let mut alone = Vec::new();
    assert_eq!(add(&mut alone, claimed, game, patch(1)), Ok(()));
    take_off(&mut alone, &[ModIndex::new(1)], claimed.guid);
    assert!(alone.is_empty(), "a resource with no patch goes");
}

unsafe extern "system" fn no_patch(
    _: *const u8,
    _: u32,
    _: *mut c_void,
    _: *mut *mut u8,
    _: *mut u32,
) -> Status {
    Status::OK
}

fn served_by(key: ResourceKey, owners: &[usize]) -> Served {
    let patches = owners
        .iter()
        .map(|&owner| Patch::new(ModIndex::new(owner), no_patch, 0))
        .collect();
    Served::new(key, key, patches, false)
}

#[test]
fn a_resource_counts_as_served_only_while_a_mod_is_on_and_it_built() {
    let key = recipe_registry();
    let served = [served_by(key, &[1, 2])];
    assert!(serves(&served, &key, |owner| owner == ModIndex::new(2)));
    assert!(!serves(&served, &key, |_| false));
    served[0].state.relock().failed = true;
    assert!(!serves(&served, &key, |_| true));
    let other = ResourceKey { part: 1, ..key };
    assert!(!serves(&[served_by(other, &[1])], &key, |_| true));
}

#[test]
fn a_resource_whose_every_patch_panicked_no_longer_counts_as_served() {
    let key = recipe_registry();
    let served = [served_by(key, &[1, 2])];
    served[0].patches[0].on.store(false, Ordering::Release);
    assert!(serves(&served, &key, |_| true), "the other patch serves it");
    served[0].patches[1].on.store(false, Ordering::Release);
    assert!(!serves(&served, &key, |_| true));
    assert!(!owner_on(&served[0]));
}

#[test]
fn a_rebuild_writes_only_the_bytes_that_differ() {
    let mut held = vec![1u8, 2, 3, 4, 5];
    let new = [1u8, 9, 3, 4, 7];
    let changed = write_differences(
        held.as_mut_ptr().expose_provenance(),
        new.as_ptr().expose_provenance(),
        new.len(),
    );
    assert_eq!(changed, 2);
    assert_eq!(held, new);
    let again = write_differences(
        held.as_mut_ptr().expose_provenance(),
        new.as_ptr().expose_provenance(),
        new.len(),
    );
    assert_eq!(again, 0);
}

#[test]
fn the_rate_patch_runs_last_and_keeps_the_base() {
    let key = recipe_registry();
    let mut served = vec![served_by(key, &[1])];
    let change = core::ptr::without_provenance_mut::<c_void>(1);
    assert_eq!(
        serve_changes(&mut served, vec![(key, add_one, change)], false),
        1
    );
    assert_eq!(serve_rates(&mut served, Some(no_patch)), 1);
    let entry = &served[0];
    let owners: Vec<ModIndex> = entry.patches.iter().map(|p| p.owner).collect();
    assert_eq!(
        owners,
        [ModIndex::LOADER, ModIndex::new(1), ModIndex::LOADER]
    );
    assert_eq!(entry.patches[0].user, 1, "the data change first");
    assert_eq!(entry.patches[2].user, 0, "the rates last");
    assert!(entry.keeps_base.load(Ordering::Relaxed));
}

#[test]
fn a_change_a_mod_sets_later_keeps_the_base_of_a_new_resource() {
    let key = recipe_registry();
    let mut served = Vec::new();
    let change = core::ptr::without_provenance_mut::<c_void>(1);
    assert_eq!(
        serve_changes(&mut served, vec![(key, add_one, change)], true),
        1
    );
    assert!(served[0].keeps_base.load(Ordering::Relaxed));
    let mut plain = Vec::new();
    assert_eq!(
        serve_changes(&mut plain, vec![(key, add_one, change)], false),
        1
    );
    assert!(!plain[0].keeps_base.load(Ordering::Relaxed));
}

#[test]
fn the_rate_patch_serves_the_registry_alone_or_not_at_all() {
    let mut served = Vec::new();
    assert_eq!(serve_rates(&mut served, None), 0);
    assert!(served.is_empty());
    assert_eq!(serve_rates(&mut served, Some(no_patch)), 1);
    assert_eq!(served[0].key, recipe_registry());
    assert_eq!(served[0].patches.len(), 1);
    assert!(served[0].keeps_base.load(Ordering::Relaxed));
}

#[test]
fn the_served_resources_link_all_three_or_none_and_say_why_they_are_off() {
    let loader = crate::start::tests::bare_loader();
    assert!(
        !link_resources(&loader),
        "this build has no resource database"
    );
    assert!(
        RESOURCES_FUSE.is_off(),
        "off by its fuse, not only in the log"
    );
    let mut chains = loader.chains.relock();
    chains.seal();
    assert!(chains.is_empty(), "no link of the resources is made");
    let listed = crate::engine::diag::parts_off();
    let off = listed.iter().find(|p| p.part == "served resources");
    assert!(
        off.is_some_and(|p| p.reason.starts_with(pinned::resource::OPEN.id())),
        "{listed:?}"
    );
}

/// Builds `base` with the loader patch `apply` and its `user`, inside a
/// game build that lends its output a buffer.
fn build_with(apply: ResourcePatchFn, user: *mut c_void, base: &[u8]) -> Status {
    let _build = enter_build(recipe_registry());
    let (mut out, mut out_len) = (core::ptr::null_mut::<u8>(), 0u32);
    // SAFETY: `user` is the value `own_patch` made with `apply`, `base` is
    // readable for its length, and `out` and `out_len` are writable locals.
    unsafe {
        apply(
            base.as_ptr(),
            u32::try_from(base.len()).unwrap(),
            user,
            &raw mut out,
            &raw mut out_len,
        )
    }
}

#[test]
fn a_stand_in_that_panics_is_refused_alone_and_the_others_keep_building() {
    let stand_in = |builder: Builder| own_patch(BuilderPatch::new(builder, STAND_INS));
    let (broken, broken_user) = stand_in(Box::new(|_| panic!("broken stand-in")));
    let (sound, sound_user) = stand_in(Box::new(Some));
    assert_eq!(build_with(broken, broken_user, &[1]), Status::FAULTED);
    assert_eq!(
        build_with(sound, sound_user, &[1]),
        Status::OK,
        "another stand-in keeps building after one panicked"
    );
    assert!(
        crate::engine::diag::parts_off()
            .iter()
            .any(|p| p.part == STAND_INS && p.reason.contains("broken stand-in")),
        "the panic still shows"
    );
}

#[test]
fn a_panic_in_the_served_work_leaves_the_resource_links_wired() {
    let ran = SERVING_FUSE.run("a resource build", || panic!("broken build"));
    assert!(ran.is_none());
    assert!(SERVING_FUSE.is_off());
    assert!(
        !core::ptr::eq(&raw const SERVING_FUSE, &raw const RESOURCES_FUSE),
        "the work turns off a switch no link carries, so a served handle never reaches the game"
    );
    let listed = crate::engine::diag::parts_off();
    assert!(
        listed
            .iter()
            .any(|p| p.part == "served resource builds" && p.reason.contains("broken build")),
        "{listed:?}"
    );
}

#[test]
fn a_served_key_is_named_by_the_catalogue_type_of_its_hash() {
    use crate::engine::catalogue::types;

    let guid = Guid([1; 16]);
    let named = |type_hash| {
        key_name(&ResourceKey {
            guid,
            type_hash,
            part: 0,
        })
    };
    assert_eq!(named(types::ITEM_REGISTRY), "the item registry");
    assert_eq!(named(types::RECIPE_REGISTRY), "the recipe registry");
    assert_eq!(named(types::WORKSHOP_REGISTRY), "the workshop registry");
    assert_eq!(named(types::UI_BUNDLE), "the UI bundle");
    for (type_hash, kind) in [
        (
            enshwick_gamedata::catalogue::type_hash(
                enshwick_core::content::types::TEMPLATE_COLLECTION_RESOURCE.id(),
            )
            .unwrap(),
            "template collection",
        ),
        (types::ITEM_INFO, "item"),
        (types::TEMPLATE, "template"),
        (types::UI_IMAGE, "image"),
    ] {
        assert_eq!(named(type_hash), format!("{kind} {guid}"));
    }
}

#[test]
fn a_base_builds_from_its_whole_length_where_all_of_it_reads_copying_it_from_writable_pages() {
    use crate::engine::pages::protect;
    use windows_sys::Win32::System::Memory::{PAGE_NOACCESS, PAGE_READONLY};

    const PAGE: usize = 4096;
    let key = ResourceKey {
        guid: Guid([2; 16]),
        type_hash: 1,
        part: 3,
    };
    // The pages stay for the test process.
    let pages = crate::engine::pages::allocate(None, 3 * PAGE).expect("fresh pages");
    let start = pages.as_ptr().expose_provenance();
    // SAFETY: the pages are the test's own read-write allocation of
    // `3 * PAGE` bytes, used by no one else.
    unsafe { pages.as_ptr().wrapping_add(2 * PAGE - 1).write(7) };
    let mut copy = Vec::new();
    let held_at = held(&key, (start, 16), 2 * PAGE, &mut copy);
    assert_eq!(
        held_at,
        (copy.as_ptr().expose_provenance(), 2 * PAGE),
        "read-write pages, copied"
    );
    assert_eq!((copy.len(), copy.last()), (2 * PAGE, Some(&7)));
    // SAFETY: the pages are the test's own allocation, used by no one else.
    unsafe { protect(start, 2 * PAGE, PAGE_READONLY) }.expect("two pages read-only");
    // SAFETY: the third page is the test's own allocation, used by no one
    // else.
    unsafe { protect(start + 2 * PAGE, PAGE, PAGE_NOACCESS) }.expect("a page that does not read");
    let mut copy = Vec::new();
    assert_eq!(
        held(&key, (start, 16), 2 * PAGE, &mut copy),
        (start, 2 * PAGE),
        "held whole"
    );
    assert!(copy.is_empty(), "read in place");
    assert_eq!(
        held(&key, (start, 16), 3 * PAGE, &mut Vec::new()),
        (start, 16),
        "past the pages that read"
    );
    assert_eq!(
        held(&key, (start + PAGE, 16), 16, &mut Vec::new()),
        (start + PAGE, 16),
        "not cut"
    );
}
