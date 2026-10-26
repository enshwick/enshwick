use crate::locks::Relock;

use super::*;

unsafe extern "system" fn add_one(
    base: *const u8,
    base_len: usize,
    _user: *mut c_void,
    out: *mut *const u8,
    out_len: *mut usize,
) -> Status {
    // SAFETY: `base` holds `base_len` bytes for the call.
    let mut built = unsafe { core::slice::from_raw_parts(base, base_len) }.to_vec();
    built.push(1);
    let built: &'static [u8] = Box::leak(built.into_boxed_slice());
    // SAFETY: `run_patches` passes `out` pointing at a local
    // `*const u8`, writable for the call.
    unsafe { out.write(built.as_ptr()) };
    // SAFETY: `run_patches` passes `out_len` pointing at a local
    // `usize`, writable for the call.
    unsafe { out_len.write(built.len()) };
    Status::OK
}

#[test]
fn a_rebuild_runs_the_mods_of_the_first_build_although_they_are_off() {
    let key = ResourceKey {
        guid: [0; 16],
        type_hash: 1,
        part: 0,
    };
    let entry = Served {
        key,
        base: key,
        patches: [3, 4]
            .map(|owner| Patch {
                owner: ModIndex::new(owner),
                apply: add_one,
                user: 0,
            })
            .into(),
        keeps_base: AtomicBool::new(true),
        state: Mutex::new(State::default()),
        rebuilding: Mutex::new(()),
    };
    let base = [7u8; 4];
    let at = base.as_ptr().expose_provenance();
    // No loader runs here, so every mod reads as off.
    assert!(run_patches(&entry, at, base.len(), None).is_none());
    let both = [ModIndex::new(3), ModIndex::new(4)];
    let (_, len, by) = run_patches(&entry, at, base.len(), Some(&both)).unwrap();
    assert_eq!((len, by), (6, both.to_vec()));
}

unsafe extern "system" fn no_patch(
    _: *const u8,
    _: usize,
    _: *mut c_void,
    _: *mut *const u8,
    _: *mut usize,
) -> Status {
    Status::OK
}

fn served_by(key: ResourceKey, owners: &[usize]) -> Served {
    Served {
        key,
        base: key,
        patches: owners
            .iter()
            .map(|&owner| Patch {
                owner: ModIndex::new(owner),
                apply: no_patch,
                user: 0,
            })
            .collect(),
        keeps_base: AtomicBool::new(false),
        state: Mutex::new(State::default()),
        rebuilding: Mutex::new(()),
    }
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
