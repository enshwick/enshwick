use std::cell::RefCell;
use std::path::PathBuf;

use enshwick_core::abi::{Header, HostApi, Manifest, ManifestFlags, ModHandle, Status, Str};
use enshwick_gamedata::reflect::{Fingerprint, Reflection};

use super::library::InitFn;
use super::*;
use enshwick_base::Relock as _;

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
        .map(|library| unsafe { crate::base::manifest::parse(library.manifest) }.unwrap())
        .collect();
    let (libraries, candidates, copies) =
        library::drop_duplicates(std::path::Path::new("mods"), libraries, candidates);
    let plan = resolve::resolve(&candidates);
    let mods = records(&libraries, &candidates, &plan);
    let skipped = skipped(&libraries, &candidates, &plan, copies);
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
        build: Box::leak(Box::new(crate::engine::builds::empty())),
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
        retry: crate::engine::retry::Retry::default(),
        skipped,
        parts: Parts::new(crate::parts::LAYERS),
    };
    (loader, order)
}

/// A server's loader with no game and no mods, for the tests of other parts.
pub(crate) fn bare_loader() -> Loader {
    loader(&[], false).0
}

/// A server's loader with no game running the plain mods `ids`, in that
/// order, for the tests of other parts that ask about a mod.
pub(crate) fn loader_running(ids: &[&'static str]) -> Loader {
    let mods: Vec<TestMod> = ids.iter().map(|&id| TestMod::plain(id)).collect();
    loader(&mods, false).0
}

/// A loader of `side` with no game and no mods whose build pins every
/// symbol of `ids` and every one of `constants`, for the tests of a part's
/// links.
pub(crate) fn pinning_loader(
    side: Side,
    ids: &[crate::engine::pinned::Symbol],
    constants: &[(crate::engine::pinned::Constant, u64)],
) -> Loader {
    let ids: Vec<&str> = ids.iter().map(|id| id.id()).collect();
    let constants: Vec<(&str, u64)> = constants.iter().map(|&(id, v)| (id.id(), v)).collect();
    Loader {
        side,
        build: Box::leak(Box::new(crate::engine::builds::pinning(&ids, &constants))),
        ..bare_loader()
    }
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
        }
    );
    assert_eq!(skipped.file, "you.safes/you.safes.dll");
    assert!(
        crate::base::off_line::mods_off(&loader).is_empty(),
        "you.safes runs"
    );
    assert_eq!(crate::base::off_line::mods_off_count(&loader), 0);
    assert_eq!(
        crate::base::off_line::mods_counted(&loader),
        1,
        "one mod, two files"
    );
}

#[test]
fn the_status_summary_lists_a_mod_skipped_at_start_and_does_not_count_a_second_copy_off() {
    let (loader, _) = loader(
        &[TestMod::plain("you.safes"), TestMod::plain("you.safes")],
        false,
    );
    assert_eq!(
        crate::base::status::summary(&loader).skipped,
        [crate::engine::diag::SkippedMod {
            id: "you.safes".into(),
            version: "1.0.0".into(),
            file: "you.safes/you.safes.dll".into(),
            reason: "another copy of you.safes loads instead; delete this one".into(),
            shown: resolve::Skip::Copy {
                id: "you.safes".into()
            }
            .shown(),
            off: false,
        }]
    );
}

#[test]
fn the_off_names_and_count_follow_a_mod_that_turns_off() {
    let (loader, _order) = loader(
        &[TestMod::plain("you.safes"), TestMod::plain("you.altars")],
        false,
    );
    assert!(crate::base::off_line::mods_off(&loader).is_empty());
    let altars = loader.mods.iter().find(|m| m.id == "you.altars").unwrap();
    altars.off.store(true, Ordering::Release);
    assert_eq!(crate::base::off_line::mods_off(&loader), ["you.altars"]);
    assert_eq!(crate::base::off_line::mods_off_count(&loader), 1);
    assert_eq!(crate::base::off_line::mods_counted(&loader), 2);
    let names = crate::base::off_line::names(&loader);
    assert_eq!(names.first().map(String::as_str), Some("you.altars"));
    let mut loader = loader;
    loader.skipped.push(resolve::Skipped {
        id: "you.map".into(),
        version: "1.0.0".into(),
        label: "Map".into(),
        file: "you.map/you.map.dll".into(),
        reason: resolve::Skip::Missing { id: "you.x".into() },
    });
    assert_eq!(
        crate::base::off_line::mods_off(&loader),
        ["you.altars", "Map"]
    );
    assert_eq!(crate::base::off_line::mods_off_count(&loader), 2);
    assert_eq!(crate::base::off_line::mods_counted(&loader), 3);
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
        crate::engine::profile::pack_handle(&loader.mods, pack),
        Some(host::handle(pack)),
        "a pack runs in the loader's copy as itself"
    );
    assert_eq!(
        crate::engine::profile::pack_handle(&loader.mods, dll),
        None,
        "a DLL has its own copy; the loader's runs as the loader"
    );
    assert_eq!(
        crate::engine::profile::pack_handle(&loader.mods, ModIndex::new(loader.mods.len())),
        None,
        "an owner past the loaded mods"
    );
}

#[test]
fn a_found_pack_counts_its_files_in_the_join_version_until_it_turns_off() {
    use enshwick_core::packs::MANIFEST_FILE;
    let id = "test.kept-pack";
    let mods = std::env::temp_dir().join(format!("enshwick-packs-{}", std::process::id()));
    let _stale = std::fs::remove_dir_all(&mods);
    let dir = mods.join(id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(MANIFEST_FILE),
        format!("id = \"{id}\"\nversion = \"1.0.0\"\n"),
    )
    .unwrap();
    let found = crate::packs::data_packs::discover(&mods);
    std::fs::remove_dir_all(&mods).unwrap();
    assert_eq!(
        found.iter().map(|f| f.file.as_str()).collect::<Vec<_>>(),
        [format!("{id}/{MANIFEST_FILE}")]
    );
    let (mut loader, _) = loader(&[TestMod::plain(id)], false);
    loader.mods[0].pack = true;
    let version = loader.mods[0].join_version();
    assert!(
        version
            .strip_prefix("1.0.0+")
            .is_some_and(|hash| hash.len() == 8 && hash.bytes().all(|b| b.is_ascii_hexdigit())),
        "{version}"
    );
    loader.turn_off(ModIndex::new(0), "turned off by the test");
    assert_eq!(loader.mods[0].join_version(), "1.0.0");
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
        Some(crate::base::developer_tools::off_reason())
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
