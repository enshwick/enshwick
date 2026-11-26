//! Discovery over a `mods/` folder of real DLLs, built from the fixtures
//! below with this checkout's API crate.

use std::path::{Path, PathBuf};
use std::process::Command;

use enshwick_core::abi::Level;

use super::{Run, libraries};
use crate::engine::errors;

/// The fixture crate's manifest; `{api}` is the API crate's folder.
const CARGO_TOML: &str = r#"[package]
name = "discovery-fixtures"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
enshwick = { path = {api}, features = ["test"] }

[[example]]
name = "lanterns"
crate-type = ["cdylib"]

[[example]]
name = "lockers"
crate-type = ["cdylib"]

[[example]]
name = "half"
crate-type = ["cdylib"]

[[example]]
name = "old_abi"
crate-type = ["cdylib"]

[[example]]
name = "no_manifest"
crate-type = ["cdylib"]

[[example]]
name = "bad_id"
crate-type = ["cdylib"]

[[example]]
name = "imports"
crate-type = ["cdylib"]

[[example]]
name = "dependency"
crate-type = ["cdylib"]

[workspace]
"#;

/// A mod as `cargo enshwick new` makes one; `{id}` and `{type}` name it.
/// The fixtures share one package, so each gives its table inline.
const MOD: &str = r##"
#[derive(Debug)]
pub struct {type};

impl enshwick::Mod for {type} {
    fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
        Ok(Self)
    }
}

enshwick::test::declare_mod!(
    {type},
    r#"
    id = "{id}"
    side = "both"
    name = "{type}"
    "#
);
"##;

/// The interface's version alone, without the three functions beside it.
const HALF: &str = r#"
#[unsafe(no_mangle)]
pub extern "system" fn enshwick_mod_abi() -> u16 {
    enshwick::abi::ABI_MAJOR
}
"#;

/// The major a mod built for an older Enshwick returns.
const OLDER_ABI: u16 = enshwick::abi::ABI_MAJOR - 1;

/// All four exports; `{abi}` is the version it says and `{query}` its
/// manifest.
const RAW: &str = r#"
use enshwick::abi::{HostApi, Manifest, ModHandle, Status};

#[unsafe(no_mangle)]
pub extern "system" fn enshwick_mod_abi() -> u16 {
    {abi}
}

#[unsafe(no_mangle)]
pub extern "system" fn enshwick_mod_query() -> *const Manifest {
    {query}
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn enshwick_mod_init(_: *const HostApi, _: ModHandle) -> Status {
    Status::OK
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn enshwick_mod_ready(_: *const HostApi, _: ModHandle) -> Status {
    Status::OK
}
"#;

/// A manifest that reads but does not parse: its id is not of the form mods
/// take, which `declare_mod!` would refuse to compile. The fields it leaves
/// zero are empty strings and lists.
const BAD_ID_MANIFEST: &str = r#"{
    use enshwick::abi::{Manifest, Side, Str};
    let manifest = enshwick::abi::headed!(Manifest {
        id: Str::new("Lanterns"),
        version: Str::new("0.1.0"),
        side: Side::BOTH,
    });
    Box::leak(Box::new(manifest))
}"#;

/// The DLL the `imports` fixture links against, by name only; the tests
/// place it where the search must not look.
const DEPENDENCY: &str = "enshwick-fixture-dependency";

/// The fixture copied as [`DEPENDENCY`]: it exports a function no Windows
/// binding declares, so no crate the fixtures link can bind the `imports`
/// fixture's import to a system DLL instead.
const DEPENDENCY_SOURCE: &str = r#"
#[unsafe(no_mangle)]
pub extern "system" fn enshwick_fixture_dependency() -> u32 {
    7
}
"#;

/// A mod that imports the function of [`DEPENDENCY`], which it does not ship
/// beside itself.
const IMPORTS: &str = r##"
#[link(name = "{dependency}", kind = "raw-dylib")]
unsafe extern "system" {
    fn enshwick_fixture_dependency() -> u32;
}

#[derive(Debug)]
pub struct Imports;

impl enshwick::Mod for Imports {
    fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
        // SAFETY: the function takes nothing and only returns a number.
        let _number = unsafe { enshwick_fixture_dependency() };
        Ok(Self)
    }
}

enshwick::test::declare_mod!(
    Imports,
    r#"
    id = "fixture.imports"
    side = "both"
    name = "Imports"
    "#
);
"##;

/// The target folder this test binary runs from, which the fixtures share so
/// the API crate compiles once across runs.
fn target_dir() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    exe.ancestors()
        .find(|dir| dir.file_name().is_some_and(|name| name == "target"))
        .map_or_else(
            || std::env::temp_dir().join("enshwick-fixtures-target"),
            Path::to_path_buf,
        )
        .join("discovery-fixtures")
}

/// Builds the fixtures in `dir` and returns the folder of their DLLs, or
/// `None` where this process cannot run cargo, as under Wine.
fn build_fixtures(dir: &Path) -> Option<PathBuf> {
    // Under Wine the tests were built on another system, whose cargo cannot
    // read this process's paths.
    if !Path::new(env!("CARGO")).is_absolute() {
        return None;
    }
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let api = workspace.join("crates/enshwick").display().to_string();
    let examples = dir.join("examples");
    std::fs::create_dir_all(&examples).unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), "").unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        CARGO_TOML.replace("{api}", &format!("{api:?}")),
    )
    .unwrap();
    // The workspace's lock holds every version the fixtures need, so they
    // build offline.
    std::fs::copy(workspace.join("Cargo.lock"), dir.join("Cargo.lock")).unwrap();
    let sources = [
        (
            "lanterns",
            MOD.replace("{id}", "fixture.lanterns")
                .replace("{type}", "Lanterns"),
        ),
        (
            "lockers",
            MOD.replace("{id}", "fixture.lockers")
                .replace("{type}", "Lockers"),
        ),
        ("half", HALF.to_owned()),
        (
            "old_abi",
            RAW.replace("{abi}", &OLDER_ABI.to_string())
                .replace("{query}", "core::ptr::null()"),
        ),
        (
            "no_manifest",
            RAW.replace("{abi}", "enshwick::abi::ABI_MAJOR")
                .replace("{query}", "core::ptr::null()"),
        ),
        (
            "bad_id",
            RAW.replace("{abi}", "enshwick::abi::ABI_MAJOR")
                .replace("{query}", BAD_ID_MANIFEST),
        ),
        ("imports", IMPORTS.replace("{dependency}", DEPENDENCY)),
        ("dependency", DEPENDENCY_SOURCE.to_owned()),
    ];
    for (name, source) in sources {
        std::fs::write(examples.join(format!("{name}.rs")), source).unwrap();
    }
    let target = target_dir();
    let output = fixture_build(dir, &target).output().ok()?;
    assert!(
        output.status.success(),
        "the fixtures did not build: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Some(target.join("debug").join("examples"))
}

/// The build of the fixtures in `dir` into `target`, with the compiler beside
/// the cargo that built these tests: run outside `cargo test`, as when a lane
/// runs each test alone, no `RUSTUP_TOOLCHAIN` reaches the inner cargo, and
/// rustup would pick the toolchain by each build script's folder.
fn fixture_build(dir: &Path, target: &Path) -> Command {
    let cargo = Path::new(env!("CARGO"));
    let mut build = Command::new(cargo);
    build
        .args([
            "build",
            "--offline",
            "--quiet",
            "--examples",
            "--manifest-path",
        ])
        .arg(dir.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(target)
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_TARGET_DIR");
    let rustc = cargo.with_file_name(format!("rustc{}", std::env::consts::EXE_SUFFIX));
    if rustc.is_file() {
        build.env("RUSTC", rustc);
    }
    build
}

#[test]
fn the_fixtures_build_with_the_compiler_beside_the_tests_cargo() {
    let build = fixture_build(Path::new("fixtures"), Path::new("target"));
    let rustc = build
        .get_envs()
        .find(|(key, _)| *key == "RUSTC")
        .and_then(|(_, value)| value);
    let beside =
        Path::new(env!("CARGO")).with_file_name(format!("rustc{}", std::env::consts::EXE_SUFFIX));
    if beside.is_file() {
        assert_eq!(rustc, Some(beside.as_os_str()));
    } else {
        assert_eq!(rustc, None, "no compiler beside {}", env!("CARGO"));
    }
}

/// The loader's lines: level and message.
fn loader_lines() -> Vec<(Level, String)> {
    crate::engine::log::captured()
        .into_iter()
        .filter(|(_, source, _)| source == crate::engine::settings::LOADER_ID)
        .map(|(level, _, text)| (level, text))
        .collect()
}

/// Fills `mods` from the fixtures in `built`, one folder each: two mods,
/// one beside a copy of itself and one beside a second mod in its folder,
/// one DLL for each other reason to skip, and mods where none is read: a
/// top-level DLL and one in the loader's folder; beside the two mods, a file
/// that is no PE file before and after each, as a mod's own helper.
fn fill_mods(built: &Path, mods: &Path) {
    for (from, to) in [
        ("lanterns", "lanterns/lanterns.dll"),
        ("lanterns", "lanterns-copy/lanterns.dll"),
        ("lockers", "lockers/lockers.dll"),
        ("lanterns", "lockers/second.dll"),
        ("half", "half/half.dll"),
        ("old_abi", "old-abi/old-abi.dll"),
        ("no_manifest", "no-manifest/no-manifest.dll"),
        ("bad_id", "bad-id/bad-id.dll"),
        ("lockers", "top.dll"),
        ("lanterns", "enshwick/lanterns.dll"),
    ] {
        let to = mods.join(to);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(built.join(format!("{from}.dll")), to).unwrap();
    }
    std::fs::create_dir_all(mods.join("not-pe")).unwrap();
    for file in [
        "not-pe/not-pe.dll",
        "lanterns/a-helper.dll",
        "lockers/z-helper.dll",
    ] {
        std::fs::write(mods.join(file), "not a DLL").unwrap();
    }
}

/// A DLL freed for its exports or ABI still ran while it loaded, so the code
/// watch needs where it was mapped; only a file that never mapped has no
/// range, and a helper after its folder's mod is never loaded at all.
fn assert_only_unloadable_files_unmapped(
    mapped: &[(String, Option<(usize, usize)>)],
    lines: &[(Level, String)],
) {
    let unmapped: Vec<&str> = mapped
        .iter()
        .filter_map(|(file, range)| range.is_none().then_some(file.as_str()))
        .collect();
    assert_eq!(
        unmapped,
        ["lanterns/a-helper.dll", "not-pe/not-pe.dll"],
        "{mapped:?}"
    );
    assert!(
        lines
            .iter()
            .any(|(_, text)| text.starts_with("lanterns/a-helper.dll could not be loaded: ")),
        "a helper that fails beside its mod is logged: {lines:#?}"
    );
}

/// A DLL the discovery refused after loading it, such as one whose manifest
/// does not parse, is unloaded before the discovery returns.
fn assert_not_loaded(dll: &Path) {
    let wide: Vec<u16> = std::os::windows::ffi::OsStrExt::encode_wide(dll.as_os_str())
        .chain([0])
        .collect();
    // SAFETY: `wide` is a null-terminated wide path, read for the call.
    let module =
        unsafe { windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(wide.as_ptr()) };
    assert!(module.is_null(), "{} is still loaded", dll.display());
}

/// Every refused fixture reaches the screens and the status by its folder,
/// with the line the log wrote for it, and the second lanterns file as the
/// one skipped copy, the file the log `line` names.
fn assert_refused_reach_the_screens(
    refused: &[crate::base::resolve::Skipped],
    lines: &[(Level, String)],
    line: &str,
) {
    let [half, second, no_manifest, not_pe, old_abi, bad_id, copy] = refused else {
        panic!("each refused file for the screens and the status: {refused:?}");
    };
    for (skipped, folder) in [
        (half, "half"),
        (second, "lockers"),
        (no_manifest, "no-manifest"),
        (not_pe, "not-pe"),
        (old_abi, "old-abi"),
        (bad_id, "bad-id"),
    ] {
        assert_eq!(
            (
                skipped.id.as_str(),
                skipped.label.as_str(),
                skipped.version.as_deref()
            ),
            (folder, folder, None),
            "{skipped:?}"
        );
        let crate::base::resolve::Skip::Refused { skip } = &skipped.reason else {
            panic!("refused at start: {skipped:?}");
        };
        let line = skip.line(&skipped.file);
        assert!(
            lines.iter().any(|(_, text)| *text == line),
            "the line the log wrote: {skipped:?}"
        );
        assert_eq!(
            skipped.counts_off(|_| true),
            folder != "lockers",
            "a second mod file beside the running Lockers is no mod that is off"
        );
    }
    assert!(
        matches!(
            &not_pe.reason,
            crate::base::resolve::Skip::Refused {
                skip: super::Skip::Unloadable {
                    code: Some(193),
                    ..
                },
                ..
            }
        ),
        "{not_pe:?}"
    );
    let crate::base::resolve::Skip::Refused {
        skip: super::Skip::Manifest(reason),
    } = &bad_id.reason
    else {
        panic!("refused for its manifest: {bad_id:?}");
    };
    assert_eq!(
        reason,
        &crate::base::manifest::ManifestError::BadId("Lanterns".into())
    );
    assert_eq!(
        bad_id.reason.shown(),
        crate::engine::texts::fill("mods.skipped.manifest", &[("reason", &reason.to_string())]),
        "the Mods screen shows why"
    );
    assert!(
        line.starts_with(&copy.file),
        "the copy the log names: {copy:?}"
    );
    assert_eq!(
        (copy.id.as_str(), copy.version.as_deref(), &copy.reason),
        (
            "fixture.lanterns",
            Some("0.1.0"),
            &crate::base::resolve::Skip::Copy {
                id: "fixture.lanterns".into()
            }
        )
    );
}

#[test]
fn each_dll_that_does_not_start_gets_one_line_with_its_reason_and_code() {
    let dir = std::env::temp_dir().join(format!("enshwick-discovery-{}", std::process::id()));
    let _old = std::fs::remove_dir_all(&dir);
    let Some(built) = build_fixtures(&dir.join("fixtures")) else {
        eprintln!(
            "skipped: the cargo that built these tests ({}) does not run in this process, as under Wine",
            env!("CARGO")
        );
        // Under Wine the fixtures are refused before their folder is made.
        if dir.exists() {
            std::fs::remove_dir_all(&dir).unwrap();
        }
        return;
    };
    let mods = dir.join("game").join("mods");
    fill_mods(&built, &mods);

    // Tests sharing the process may have raised the level past news.
    crate::engine::log::set_levels(crate::engine::settings::Settings {
        log_level: Level::INFO,
        ..crate::engine::settings::Settings::default()
    });
    crate::engine::log::capture();
    let mut mapped = Vec::new();
    let all = libraries(&mods, Run::Dry, &mut |file, range| {
        mapped.push((file.to_owned(), range));
    });
    let lines = loader_lines();

    assert_only_unloadable_files_unmapped(&mapped, &lines);
    assert_not_loaded(&mods.join("bad-id").join("bad-id.dll"));

    let (found, candidates, others) = (all.libraries, all.candidates, all.others);
    let ids: Vec<&str> = candidates.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["fixture.lanterns", "fixture.lockers"], "{lines:#?}");
    let files: Vec<&str> = found.iter().map(|l| l.file.as_str()).collect();
    assert_eq!(files[1], "lockers/lockers.dll");
    assert_eq!(found[1].folder, "lockers");
    assert!(others.is_empty());
    let has = |level: Level, line: &str| {
        assert!(
            lines.iter().any(|(l, text)| *l == level && text == line),
            "no {} line {line:?} in {lines:#?}",
            level.name()
        );
    };
    let second = super::Skip::SecondMod {
        other: "lockers/lockers.dll".into(),
    };
    has(Level::WARN, &second.line("lockers/second.dll"));
    has(
        Level::WARN,
        &format!(
            "half/half.dll is not an Enshwick mod ({})",
            errors::BROKEN_DLL
        ),
    );
    has(
        Level::WARN,
        &format!(
            "old-abi/old-abi.dll skipped: {} ({})",
            enshwick_core::abi::check_major(OLDER_ABI).unwrap_err(),
            errors::OTHER_ABI
        ),
    );
    has(
        Level::WARN,
        &format!(
            "no-manifest/no-manifest.dll skipped: the manifest is missing ({})",
            errors::BROKEN_DLL
        ),
    );
    let not_pe = lines
        .iter()
        .find(|(_, text)| text.starts_with("not-pe/not-pe.dll could not be loaded: "))
        .unwrap_or_else(|| panic!("no line for not-pe.dll in {lines:#?}"));
    assert_eq!(not_pe.0, Level::WARN);
    assert!(
        not_pe.1.ends_with(&format!("({})", errors::BROKEN_DLL)),
        "{not_pe:?}"
    );
    let duplicates: Vec<&str> = lines
        .iter()
        .filter(|(_, text)| {
            text.ends_with(&format!("({})", errors::DUPLICATE_MOD)) && text.contains(" is also in ")
        })
        .map(|(_, text)| text.as_str())
        .collect();
    assert_eq!(duplicates.len(), 1, "{lines:#?}");
    assert!(
        duplicates[0].contains("skipped: fixture.lanterns 0.1.0 is also in lanterns"),
        "{}",
        duplicates[0]
    );
    assert_refused_reach_the_screens(&all.refused, &lines, duplicates[0]);

    for library in &found {
        super::unload(library);
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
#[cfg_attr(miri, ignore = "Miri isolates the file system")]
fn each_file_of_the_layout_before_mod_folders_gets_one_line() {
    let dir = std::env::temp_dir().join(format!("enshwick-old-layout-{}", std::process::id()));
    let _old = std::fs::remove_dir_all(&dir);
    let mods = dir.join("mods");
    std::fs::create_dir_all(mods.join("chain")).unwrap();
    std::fs::create_dir_all(mods.join(crate::engine::settings::LOADER_ID)).unwrap();
    std::fs::write(mods.join("lanterns.dll"), "").unwrap();
    std::fs::write(mods.join("enshwick/mods.toml"), "").unwrap();
    std::fs::create_dir_all(dir.join("mod-configs")).unwrap();
    crate::engine::log::capture();
    super::note_old_layout(&mods);
    let lines = loader_lines();
    let old: Vec<&str> = lines
        .iter()
        .filter(|(level, _)| *level == Level::WARN)
        .filter_map(|(_, text)| text.split_once(" is not read: "))
        .map(|(file, _)| file)
        .collect();
    assert_eq!(
        old,
        [
            "mods/lanterns.dll",
            "mods/chain",
            "mods/enshwick/mods.toml",
            "mod-configs/"
        ],
        "{lines:#?}"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The variable that hands [`imports_load_in_this_process`] the mod to load.
const IMPORTS_MOD: &str = "ENSHWICK_TEST_IMPORTS_MOD";

/// What [`imports_load_in_this_process`] prints after its load.
const LOADED: &str = "imports loaded: ";

#[test]
fn a_mods_import_never_resolves_from_the_current_directory() {
    let dir = std::env::temp_dir().join(format!("enshwick-search-{}", std::process::id()));
    let _old = std::fs::remove_dir_all(&dir);
    let Some(built) = build_fixtures(&dir.join("fixtures")) else {
        eprintln!("skipped: this process cannot run cargo, as under Wine");
        if dir.exists() {
            std::fs::remove_dir_all(&dir).unwrap();
        }
        return;
    };
    let folder = dir.join("mods").join("imports");
    let current = dir.join("current");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::create_dir_all(&current).unwrap();
    let imports = folder.join("imports.dll");
    std::fs::copy(built.join("imports.dll"), &imports).unwrap();
    let dependency = format!("{DEPENDENCY}.dll");
    std::fs::copy(built.join("dependency.dll"), current.join(&dependency)).unwrap();

    // The current directory is the process's, so the load runs in a child.
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "start::library::tests::imports_load_in_this_process",
            "--ignored",
            "--nocapture",
        ])
        .current_dir(&current)
        .env(IMPORTS_MOD, &imports)
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&child.stdout);
    assert!(child.status.success(), "{out}");
    assert!(
        out.contains(&format!("{LOADED}false")),
        "the import came from the current directory: {out}"
    );

    std::fs::rename(current.join(&dependency), folder.join(&dependency)).unwrap();
    let library = super::load(&imports, "imports", "imports/imports.dll")
        .0
        .expect("an import beside the mod resolves");
    super::unload(&library);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Loads the mod [`IMPORTS_MOD`] names and prints whether it loaded, for
/// [`a_mods_import_never_resolves_from_the_current_directory`].
#[test]
#[ignore = "runs only as that test's child, in its current directory"]
fn imports_load_in_this_process() {
    let Some(path) = std::env::var_os(IMPORTS_MOD) else {
        return;
    };
    let (library, _) = super::load(Path::new(&path), "imports", "imports/imports.dll");
    println!("{LOADED}{}", library.is_ok());
}
