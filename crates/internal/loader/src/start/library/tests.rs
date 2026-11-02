//! Discovery over a `mods/` folder of real DLLs, built from the fixtures
//! below with this checkout's API crate.

use std::path::{Path, PathBuf};
use std::process::Command;

use enshwick::abi::Level;

use super::{Run, libraries};
use crate::errors;

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
name = "imports"
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
pub extern "system" fn enshwick_mod_abi() -> u32 {
    u32::from(enshwick::abi::ABI_MAJOR)
}
"#;

/// What `enshwick_mod_abi` returned before ABI majors: a hash of the
/// layout, here the last one.
const HASHED_ABI: u32 = 0x0dc8_5f01;

/// All four exports; `{abi}` is the version it says and `{query}` its
/// manifest.
const RAW: &str = r#"
use enshwick::abi::{HostApi, Manifest, ModHandle, Status};

#[unsafe(no_mangle)]
pub extern "system" fn enshwick_mod_abi() -> u32 {
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
pub unsafe extern "system" fn enshwick_mod_ready(_: *const HostApi, _: ModHandle) {}
"#;

/// The DLL the `imports` fixture links against, by name only; the tests
/// place it where the search must not look.
const DEPENDENCY: &str = "enshwick-fixture-dependency";

/// A mod that imports a function of [`DEPENDENCY`], a copy of the system's
/// `version.dll`, which it does not ship beside itself.
const IMPORTS: &str = r##"
#[link(name = "{dependency}", kind = "raw-dylib")]
unsafe extern "system" {
    fn GetFileVersionInfoSizeW(file: *const u16, handle: *mut u32) -> u32;
}

#[derive(Debug)]
pub struct Imports;

impl enshwick::Mod for Imports {
    fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
        // SAFETY: a null file name is refused with an error, never read.
        let _size = unsafe { GetFileVersionInfoSizeW(core::ptr::null(), core::ptr::null_mut()) };
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

/// The mod list: `lockers.dll` is off by its id under an older file name,
/// `turned-off.dll` by its file.
const LIST: &str = r#"["fixture.lockers"]
enabled = false
file = "lockers-old.dll"

["fixture.gone"]
enabled = false
file = "turned-off.dll"
"#;

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
            RAW.replace("{abi}", &HASHED_ABI.to_string())
                .replace("{query}", "core::ptr::null()"),
        ),
        (
            "no_manifest",
            RAW.replace("{abi}", "u32::from(enshwick::abi::ABI_MAJOR)")
                .replace("{query}", "core::ptr::null()"),
        ),
        ("imports", IMPORTS.replace("{dependency}", DEPENDENCY)),
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
    crate::log::captured()
        .into_iter()
        .filter(|(_, source, _)| source == crate::settings::LOADER_ID)
        .map(|(level, _, text)| (level, text))
        .collect()
}

/// Fills `mods` from the fixtures in `built`: one mod that runs beside a
/// copy of itself, and one DLL for each other reason to skip.
fn fill_mods(built: &Path, mods: &Path) {
    std::fs::create_dir_all(mods.join(crate::settings::LOADER_ID)).unwrap();
    for (from, to) in [
        ("lanterns", "lanterns.dll"),
        ("lanterns", "lanterns-copy.dll"),
        ("lockers", "lockers.dll"),
        ("half", "half.dll"),
        ("old_abi", "old-abi.dll"),
        ("no_manifest", "no-manifest.dll"),
    ] {
        std::fs::copy(built.join(format!("{from}.dll")), mods.join(to)).unwrap();
    }
    std::fs::write(mods.join("not-pe.dll"), "not a DLL").unwrap();
    std::fs::write(mods.join("turned-off.dll"), "never read").unwrap();
    std::fs::write(
        mods.join(crate::settings::LOADER_ID)
            .join(crate::modlist::LIST_FILE),
        LIST,
    )
    .unwrap();
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
    crate::log::set_levels(crate::settings::Settings {
        log_level: Level::INFO,
        ..crate::settings::Settings::default()
    });
    crate::log::capture();
    let (found, candidates, others) = libraries(&mods, Run::Dry, &mut |_| {});
    let lines = loader_lines();
    crate::log::end_capture();

    let ids: Vec<&str> = candidates.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["fixture.lanterns"], "{lines:#?}");
    assert_eq!(found.len(), 1);
    assert!(others.is_empty());
    let has = |level: Level, line: &str| {
        assert!(
            lines.iter().any(|(l, text)| *l == level && text == line),
            "no {} line {line:?} in {lines:#?}",
            level.name()
        );
    };
    has(
        Level::INFO,
        "turned-off.dll skipped: off in mods/enshwick/mods.toml",
    );
    has(
        Level::INFO,
        "lockers.dll skipped: fixture.lockers is off in mods/enshwick/mods.toml",
    );
    has(
        Level::WARN,
        &format!("half.dll is not an Enshwick mod ({})", errors::BROKEN_DLL),
    );
    has(
        Level::WARN,
        &format!(
            "old-abi.dll skipped: {} ({})",
            enshwick::abi::check_abi(HASHED_ABI, 0).unwrap_err(),
            errors::OTHER_ABI
        ),
    );
    has(
        Level::WARN,
        &format!(
            "no-manifest.dll skipped: the manifest is missing ({})",
            errors::BROKEN_DLL
        ),
    );
    let not_pe = lines
        .iter()
        .find(|(_, text)| text.starts_with("not-pe.dll could not be loaded: "))
        .unwrap_or_else(|| panic!("no line for not-pe.dll in {lines:#?}"));
    assert_eq!(not_pe.0, Level::WARN);
    assert!(
        not_pe.1.ends_with(&format!("({})", errors::BROKEN_DLL)),
        "{not_pe:?}"
    );
    let duplicates: Vec<&str> = lines
        .iter()
        .filter(|(_, text)| text.ends_with(&format!("({})", errors::DUPLICATE_MOD)))
        .map(|(_, text)| text.as_str())
        .collect();
    assert_eq!(duplicates.len(), 1, "{lines:#?}");
    assert!(
        duplicates[0].contains("skipped: fixture.lanterns 0.1.0 is also in lanterns"),
        "{}",
        duplicates[0]
    );

    let list = std::fs::read_to_string(
        mods.join(crate::settings::LOADER_ID)
            .join(crate::modlist::LIST_FILE),
    )
    .unwrap();
    assert!(list.contains("[\"fixture.lanterns\"]"), "{list}");
    assert!(list.contains("file = \"turned-off.dll\""), "{list}");
    for library in &found {
        super::unload(library);
    }
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
    let mods = dir.join("mods");
    let current = dir.join("current");
    std::fs::create_dir_all(&mods).unwrap();
    std::fs::create_dir_all(&current).unwrap();
    let imports = mods.join("imports.dll");
    std::fs::copy(built.join("imports.dll"), &imports).unwrap();
    let dependency = format!("{DEPENDENCY}.dll");
    let system = crate::module::system_dir().unwrap();
    std::fs::copy(system.join("version.dll"), current.join(&dependency)).unwrap();

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

    std::fs::rename(current.join(&dependency), mods.join(&dependency)).unwrap();
    let library = super::load_from(&imports, None).expect("an import beside the mod resolves");
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
    let library = super::load_from(Path::new(&path), None);
    println!("{LOADED}{}", library.is_some());
}
