use std::cell::RefCell;
use std::path::PathBuf;

use enshwick::abi::{Header, HostApi, Manifest, ManifestFlags, ModHandle, Status, Str};
use enshwick_gamedata::reflect::{Fingerprint, Reflection};

use super::library::InitFn;
use super::*;
use crate::locks::Relock as _;

thread_local! {
    /// The export calls of this test's mods, by stage and handle.
    static CALLS: RefCell<Vec<(&'static str, usize)>> = const { RefCell::new(Vec::new()) };
}

fn called() -> Vec<(&'static str, usize)> {
    CALLS.with_borrow(Clone::clone)
}

unsafe extern "system" fn init(_: *const HostApi, handle: ModHandle) -> Status {
    CALLS.with_borrow_mut(|calls| calls.push(("init", handle.0)));
    Status::OK
}

unsafe extern "system" fn failing_init(_: *const HostApi, handle: ModHandle) -> Status {
    CALLS.with_borrow_mut(|calls| calls.push(("init", handle.0)));
    Status::FAULTED
}

unsafe extern "system" fn on_ready(_: *const HostApi, handle: ModHandle) {
    CALLS.with_borrow_mut(|calls| calls.push(("ready", handle.0)));
}

/// A mod whose `init` answers as `init` does.
struct TestMod {
    id: &'static str,
    flags: ManifestFlags,
    init: InitFn,
}

impl TestMod {
    const fn plain(id: &'static str) -> Self {
        Self {
            id,
            flags: ManifestFlags::NONE,
            init,
        }
    }

    fn manifest(&self) -> &'static Manifest {
        Box::leak(Box::new(Manifest {
            header: Header::of::<Manifest>(),
            id: Str::new(self.id),
            version: Str::new("1.0.0"),
            side: Side::SERVER,
            flags: self.flags,
            dependencies: core::ptr::null(),
            dependency_count: 0,
            builds: core::ptr::null(),
            build_count: 0,
            name: Str::EMPTY,
            description: Str::EMPTY,
            authors: core::ptr::null(),
            author_count: 0,
            license: Str::EMPTY,
            homepage: Str::EMPTY,
            points: Str::EMPTY,
            exports: core::ptr::null(),
            export_count: 0,
        }))
    }
}

/// A loader of `mods` with no game, and the libraries in its load order.
fn loader(mods: &[TestMod], developer_tools: bool) -> (Loader, Vec<Library>) {
    let libraries: Vec<Library> = mods
        .iter()
        .map(|test| Library {
            folder: test.id.to_owned(),
            file: format!("{0}/{0}.dll", test.id),
            module: core::ptr::null_mut(),
            pack: false,
            range: None,
            manifest: test.manifest(),
            init: test.init,
            ready: on_ready,
        })
        .collect();
    let candidates: Vec<Candidate> = libraries
        .iter()
        // SAFETY: each manifest is one `TestMod::manifest` leaked, its
        // strings `'static` literals and its lists null.
        .map(|library| unsafe { crate::manifest::parse(library.manifest) }.unwrap())
        .collect();
    let plan = resolve::resolve(&candidates);
    let mods = records(&libraries, &candidates, &plan);
    let skipped = skipped(&libraries, &candidates, &plan);
    let mut slots: Vec<Option<Library>> = libraries.into_iter().map(Some).collect();
    let order = plan
        .order
        .iter()
        .map(|&i| slots[i].take().unwrap())
        .collect();
    let loader = Loader {
        side: Side::SERVER,
        base: 0,
        fingerprint: Fingerprint::new([0; 16], 0),
        build: Box::leak(Box::new(crate::builds::empty())),
        reflection: Reflection::default(),
        mods_dir: PathBuf::new(),
        developer_tools,
        mods,
        others: Vec::new(),
        chains: Mutex::new(Chains::default()),
        mids: Mutex::default(),
        caller_frame: read_frame,
        subscriptions: Mutex::default(),
        delivery: OnceLock::new(),
        settings_problems: Vec::new(),
        newer_release: OnceLock::new(),
        retry: crate::retry::Retry::default(),
        skipped,
    };
    (loader, order)
}

/// A server's loader with no game and no mods, for the tests of other parts.
pub(crate) fn bare_loader() -> Loader {
    loader(&[], false).0
}

#[test]
fn a_second_copy_skipped_at_start_is_kept_with_its_reason_but_its_running_mod_is_not_off() {
    let (loader, order) = loader(
        &[TestMod::plain("you.safes"), TestMod::plain("you.safes")],
        false,
    );
    assert_eq!(order.len(), 1);
    assert_eq!(
        loader.mods.len(),
        1,
        "only the copy that loads has a record"
    );
    let [skipped] = loader.skipped.as_slice() else {
        panic!("{:?}", loader.skipped);
    };
    assert_eq!(
        (
            skipped.id.as_str(),
            skipped.label.as_str(),
            skipped.version.as_str()
        ),
        ("you.safes", "you.safes", "1.0.0")
    );
    assert_eq!(
        skipped.reason,
        resolve::Skip::Copy {
            id: "you.safes".into(),
            first: 0
        }
    );
    assert_eq!(skipped.file, "you.safes/you.safes.dll");
    assert!(
        crate::off_line::mods_off(&loader).is_empty(),
        "you.safes runs"
    );
    assert_eq!(crate::off_line::mods_off_count(&loader), 0);
}

/// The handle of the mod `id` in `loader`'s load order.
fn handle_of(loader: &Loader, id: &str) -> usize {
    let index = loader.mods.iter().position(|m| m.id == id).unwrap();
    host::handle(ModIndex::new(index)).0
}

fn is_off(loader: &Loader, id: &str) -> bool {
    loader
        .mods
        .iter()
        .find(|m| m.id == id)
        .unwrap()
        .off
        .load(Ordering::Acquire)
}

const DEVELOPER: TestMod = TestMod {
    id: "you.probe",
    flags: ManifestFlags::DEVELOPER,
    init,
};

#[test]
fn a_pack_s_callback_runs_as_the_pack_and_a_dll_s_as_the_loader() {
    let (mut loader, _) = loader(
        &[TestMod::plain("you.lanterns"), TestMod::plain("you.pack")],
        false,
    );
    let at = |id: &str| ModIndex::new(loader.mods.iter().position(|m| m.id == id).unwrap());
    let (dll, pack) = (at("you.lanterns"), at("you.pack"));
    loader.mods[pack.get()].pack = true;
    assert_eq!(
        crate::profile::pack_handle(&loader.mods, pack),
        Some(host::handle(pack)),
        "a pack runs in the loader's copy as itself"
    );
    assert_eq!(
        crate::profile::pack_handle(&loader.mods, dll),
        None,
        "a DLL has its own copy; the loader's runs as the loader"
    );
    assert_eq!(
        crate::profile::pack_handle(&loader.mods, ModIndex::new(loader.mods.len())),
        None,
        "an owner past the loaded mods"
    );
}

#[test]
fn a_developer_mod_is_off_before_its_init_without_developer_tools() {
    let (loader, order) = loader(&[TestMod::plain("you.lanterns"), DEVELOPER], false);
    initialise(&loader, &order.iter().collect::<Vec<_>>(), None);
    assert_eq!(called(), [("init", handle_of(&loader, "you.lanterns"))]);
    assert!(is_off(&loader, "you.probe"));
    assert!(!is_off(&loader, "you.lanterns"));
    let probe = loader.mods.iter().find(|m| m.id == "you.probe").unwrap();
    assert_eq!(
        probe.off_reason.relock().as_deref(),
        Some(crate::developer_tools::off_reason())
    );
}

#[test]
fn a_developer_mod_gets_its_init_with_developer_tools_on() {
    let (loader, order) = loader(&[DEVELOPER], true);
    initialise(&loader, &order.iter().collect::<Vec<_>>(), None);
    assert_eq!(called(), [("init", handle_of(&loader, "you.probe"))]);
    assert!(!is_off(&loader, "you.probe"));
}

#[test]
fn a_mod_turned_off_another_way_before_init_gets_no_init() {
    let (loader, order) = loader(
        &[TestMod::plain("you.lanterns"), TestMod::plain("you.stalls")],
        false,
    );
    let lanterns = loader
        .mods
        .iter()
        .position(|m| m.id == "you.lanterns")
        .unwrap();
    loader.turn_off(ModIndex::new(lanterns), "turned off by the test");
    initialise(&loader, &order.iter().collect::<Vec<_>>(), None);
    assert_eq!(called(), [("init", handle_of(&loader, "you.stalls"))]);
    assert!(is_off(&loader, "you.lanterns"));
}

#[test]
fn a_mod_whose_init_fails_is_off() {
    let failing = TestMod {
        init: failing_init,
        ..TestMod::plain("you.broken")
    };
    let (loader, order) = loader(&[failing], false);
    initialise(&loader, &order.iter().collect::<Vec<_>>(), None);
    assert!(is_off(&loader, "you.broken"));
    let broken = &loader.mods[0];
    assert_eq!(
        broken.off_reason.relock().as_deref(),
        Some(format!("init returned {}", Status::FAULTED.0).as_str())
    );
}

#[test]
fn ready_skips_the_mods_that_are_off() {
    let (loader, order) = loader(
        &[TestMod::plain("you.lanterns"), TestMod::plain("you.stalls")],
        false,
    );
    let order: Vec<&Library> = order.iter().collect();
    initialise(&loader, &order, None);
    let stalls = loader
        .mods
        .iter()
        .position(|m| m.id == "you.stalls")
        .unwrap();
    loader.turn_off(ModIndex::new(stalls), "turned off by the test");
    CALLS.with_borrow_mut(Vec::clear);
    ready(&loader, &order, None);
    assert_eq!(called(), [("ready", handle_of(&loader, "you.lanterns"))]);
}
