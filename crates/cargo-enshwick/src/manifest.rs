//! `cargo enshwick manifest`: what a built mod DLL says about itself.

use std::path::Path;

use enshwick_core::abi::{Manifest, Relation, Side, Str};

/// What a mod DLL must say of itself, each part when given: what a release
/// takes from the crate's `Cargo.toml`.
#[derive(Debug, Default)]
pub(crate) struct Expected<'a> {
    pub(crate) id: Option<&'a str>,
    pub(crate) version: Option<&'a str>,
    /// `server` or `both`, as `[package.metadata.enshwick]` spells it.
    pub(crate) side: Option<&'a str>,
}

/// `cargo enshwick manifest`: describes the mod in `dll`; an error when its
/// version is not `version`.
pub(crate) fn run(dll: &Path, version: Option<&str>) -> anyhow::Result<String> {
    let expected = Expected {
        version,
        ..Expected::default()
    };
    show(dll, &expected).map_err(anyhow::Error::msg)
}

/// Describes the mod in `dll`; an error naming each part that is not what
/// `expected` says.
pub(crate) fn show(dll: &Path, expected: &Expected<'_>) -> Result<String, String> {
    let manifest = query(dll)?;
    // SAFETY: `query` copied the manifest out of a DLL it never unloads and
    // never shuts down, so the strings and lists `enshwick_mod_query`
    // returned stay valid and unwritten (`abi::exports::QUERY`).
    let text = unsafe { describe(&manifest) };
    if let Err(why) = declarations(dll) {
        return Err(format!(
            "{} stays off: its declarations do not read: {why}\n{text}",
            dll.display()
        ));
    }
    // SAFETY: `query` copied the manifest out of a DLL it never unloads and
    // never shuts down, so its strings stay valid.
    let id = unsafe { manifest.id.as_str() }.unwrap_or_default();
    // SAFETY: `query` copied the manifest out of a DLL it never unloads and
    // never shuts down, so its strings stay valid.
    let version = unsafe { manifest.version.as_str() }.unwrap_or_default();
    let wrong = mismatches(id, version, manifest.side, expected);
    if wrong.is_empty() {
        Ok(text)
    } else {
        Err(format!("{}: {}\n{text}", dll.display(), wrong.join("; ")))
    }
}

/// What differs between the manifest's `id`, `version` and `side` and
/// `expected`, a phrase each.
fn mismatches(id: &str, version: &str, side: Side, expected: &Expected<'_>) -> Vec<String> {
    let side = match side {
        Side::SERVER => "server",
        Side::BOTH => "both",
        _ => "unknown",
    };
    [
        ("id", id, expected.id),
        ("version", version, expected.version),
        ("side", side, expected.side),
    ]
    .into_iter()
    .filter_map(|(what, is, wanted)| {
        wanted
            .filter(|wanted| *wanted != is)
            .map(|wanted| format!("{what} is {is}, expected {wanted}"))
    })
    .collect()
}

/// Reads the declarations in the `.enshpt` section of `dll` as the loader
/// does; the reason when the file, its image or the section does not read.
fn declarations(dll: &Path) -> Result<(), String> {
    let bytes = std::fs::read(dll).map_err(|e| e.to_string())?;
    cargo_enshwick::points::from_dll(&bytes)
        .map(drop)
        .map_err(|e| e.to_string())
}

/// The manifest as lines of `key: value`.
///
/// # Safety
///
/// - Every string in `manifest` and its dependencies is readable for its
///   length and unchanged for the call.
/// - `authors`, `exports` and `dependencies` are null or point at
///   `author_count`, `export_count` and `dependency_count` aligned entries,
///   readable and unchanged for the call.
unsafe fn describe(manifest: &Manifest) -> String {
    // SAFETY: the caller guarantees every string of the manifest, its authors
    // and its dependencies readable and unchanged for the call.
    let text = |s: &Str| unsafe { s.as_str() }.unwrap_or("<not UTF-8>").to_owned();
    let side = match manifest.side {
        Side::SERVER => "server",
        Side::BOTH => "both, in the join check",
        _ => "unknown",
    };
    let mut lines = vec![
        format!("id: {}", text(&manifest.id)),
        format!("version: {}", text(&manifest.version)),
    ];
    let authors = if manifest.authors.is_null() {
        &[][..]
    } else {
        // SAFETY: `authors` is non-null, checked just before; the caller
        // guarantees `author_count` aligned names there for the call.
        unsafe { core::slice::from_raw_parts(manifest.authors, manifest.author_count as usize) }
    };
    let authors: Vec<String> = authors.iter().map(text).collect();
    let exports = if manifest.exports.is_null() {
        &[][..]
    } else {
        // SAFETY: `exports` is non-null, checked just before; the caller
        // guarantees `export_count` aligned names there for the call.
        unsafe { core::slice::from_raw_parts(manifest.exports, manifest.export_count as usize) }
    };
    let exports: Vec<String> = exports.iter().map(text).collect();
    for (key, value) in [
        ("name", text(&manifest.name)),
        ("description", text(&manifest.description)),
        ("authors", authors.join(", ")),
        ("license", text(&manifest.license)),
        ("homepage", text(&manifest.homepage)),
        ("exports", exports.join(", ")),
    ] {
        if !value.is_empty() {
            lines.push(format!("{key}: {value}"));
        }
    }
    lines.push(format!("side: {side}"));
    let dependencies = if manifest.dependencies.is_null() {
        &[][..]
    } else {
        // SAFETY: `dependencies` is non-null, checked just before; the caller
        // guarantees `dependency_count` aligned entries there for the call.
        unsafe {
            core::slice::from_raw_parts(manifest.dependencies, manifest.dependency_count as usize)
        }
    };
    for dependency in dependencies {
        let relation = match dependency.relation {
            Relation::REQUIRES => "requires",
            Relation::AFTER => "after",
            Relation::BREAKS => "breaks",
            _ => "unknown relation",
        };
        let range = text(&dependency.range);
        lines.push(
            format!("{relation}: {} {range}", text(&dependency.id))
                .trim_end()
                .to_owned(),
        );
    }
    lines.join("\n")
}

/// Copies the manifest `raw` points at, refused unless it is built for this
/// ABI version.
///
/// # Safety
///
/// - `raw` is null, or points at a manifest readable for the size its header
///   states.
/// - No other thread writes that manifest during the copy.
/// - Its strings and lists stay readable for the rest of the process.
#[cfg(any(windows, test))]
unsafe fn copy(raw: *const Manifest) -> Result<Manifest, String> {
    const REQUIRED: usize = <Manifest as enshwick_core::abi::Headed>::BORN;
    // SAFETY: the caller's contract is the one `read_headed` needs: null or
    // readable for the size its header states, no concurrent writer.
    let manifest = unsafe { enshwick_core::abi::read_headed(raw, REQUIRED) }
        .ok_or("the mod returned no manifest, or a truncated one")?;
    let header = manifest.header;
    enshwick_core::abi::check_abi(header.abi_major, header.abi_minor)
        .map_err(|why| why.to_string())?;
    Ok(manifest)
}

#[cfg(windows)]
fn query(dll: &Path) -> Result<Manifest, String> {
    use std::os::windows::ffi::OsStrExt as _;

    use windows_sys::Win32::System::LibraryLoader::{
        GetProcAddress, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32,
        LoadLibraryExW,
    };

    // A bare name would go through the DLL search order, and could load a
    // system DLL of that name instead; the folder flag also needs a full path.
    let dll = std::path::absolute(dll).map_err(|e| format!("{}: {e}", dll.display()))?;
    let wide: Vec<u16> = dll.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: `wide` ends in the NUL chained onto it and lives across the
    // call. The flags resolve the DLL's imports from its folder and the
    // system's, never from the current directory, `PATH` or this tool's own
    // folder, which the game's search would not see.
    let module = unsafe {
        LoadLibraryExW(
            wide.as_ptr(),
            core::ptr::null_mut(),
            LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
    };
    if module.is_null() {
        return Err(format!(
            "{} could not be loaded: {}",
            dll.display(),
            std::io::Error::last_os_error()
        ));
    }
    let export = |name: &str| {
        let name = format!("{name}\0");
        // SAFETY: `module` was checked non-null and is never freed; `name`
        // ends in the NUL `format!` appended and lives across the call.
        unsafe { GetProcAddress(module, name.as_ptr()) }
            .ok_or_else(|| format!("{} is not an Enshwick mod", dll.display()))
    };
    // SAFETY: the ABI gives `exports::ABI` the signature
    // `extern "system" fn() -> u16`.
    let abi: unsafe extern "system" fn() -> u16 =
        unsafe { core::mem::transmute(export(enshwick_core::abi::exports::ABI)?) };
    // SAFETY: `abi` is the mod's `exports::ABI`, which takes no arguments and
    // returns a constant, the major that decides how the manifest reads.
    enshwick_core::abi::check_major(unsafe { abi() }).map_err(|why| why.to_string())?;
    let query = export(enshwick_core::abi::exports::QUERY)?;
    // SAFETY: the ABI gives `exports::QUERY` the signature
    // `extern "system" fn() -> *const Manifest`.
    let query: unsafe extern "system" fn() -> *const Manifest =
        unsafe { core::mem::transmute(query) };
    // SAFETY: `query` is the mod's `exports::QUERY`, which takes no arguments
    // and returns its manifest; the DLL is never unloaded.
    let raw = unsafe { query() };
    // SAFETY: the ABI says `exports::QUERY` returns null or a manifest valid
    // and unwritten for the process.
    unsafe { copy(raw) }
}

#[cfg(not(windows))]
fn query(_dll: &Path) -> Result<Manifest, String> {
    Err("reading a mod DLL needs Windows".into())
}

#[cfg(test)]
mod tests {
    use enshwick::abi::{Dependency, ManifestFlags, Relation, Side, Str};

    use super::*;

    /// A manifest of `id` and `version` and nothing else, as a DLL holds it.
    fn plain(id: &'static str, version: &'static str) -> Manifest {
        enshwick_abi::headed!(Manifest {
            id: Str::new(id),
            version: Str::new(version),
            side: Side::SERVER,
            flags: ManifestFlags::NONE,
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
        })
    }

    #[test]
    fn a_manifest_without_every_field_of_its_major_is_refused() {
        let mut short = plain("you.old-mod", "1.0.0");
        // SAFETY: `short` is a whole local manifest with static strings.
        let copied = unsafe { copy(&raw const short) }.unwrap();
        // SAFETY: `copied` holds the `'static` strings of `short`.
        assert_eq!(unsafe { copied.id.as_str() }, Some("you.old-mod"));
        short.header.size = u32::try_from(core::mem::offset_of!(Manifest, points)).unwrap();
        // SAFETY: `short` is a local manifest readable for the size its
        // header now states.
        assert!(unsafe { copy(&raw const short) }.is_err());
        // SAFETY: `copy` accepts null and reads nothing.
        assert!(unsafe { copy(core::ptr::null()) }.is_err());
    }

    #[test]
    fn a_manifest_of_an_older_abi_major_is_refused_with_the_release_to_rebuild_for() {
        let mut other = plain("you.other-mod", "1.0.0");
        other.header.abi_major = enshwick::abi::ABI_MAJOR - 1;
        // SAFETY: `other` is a whole local manifest with static strings.
        let why = unsafe { copy(&raw const other) }.unwrap_err();
        assert_eq!(
            why,
            enshwick::abi::check_abi(other.header.abi_major, 0)
                .unwrap_err()
                .to_string()
        );
    }

    #[test]
    fn each_part_unlike_the_crate_is_named() {
        let all = Expected {
            id: Some("you.stalls"),
            version: Some("0.2.0"),
            side: Some("both"),
        };
        assert!(mismatches("you.stalls", "0.2.0", Side::BOTH, &all).is_empty());
        assert_eq!(
            mismatches("you.other", "0.1.0", Side::SERVER, &all),
            [
                "id is you.other, expected you.stalls",
                "version is 0.1.0, expected 0.2.0",
                "side is server, expected both"
            ]
        );
        assert!(mismatches("you.other", "0.1.0", Side::SERVER, &Expected::default()).is_empty());
    }

    #[test]
    fn a_manifest_reads_as_lines() {
        static AUTHORS: [Str; 2] = [Str::new("themaiby"), Str::new("a friend")];
        static EXPORTS: [Str; 2] = [Str::new("content:stall"), Str::new("event:opened")];
        let relation = |id, range, relation| Dependency {
            id: Str::new(id),
            range: Str::new(range),
            relation,
        };
        let dependencies = [
            relation("you.trading-post", ">=0.1, <1.0", Relation::REQUIRES),
            relation("you.lanterns", "", Relation::AFTER),
            relation("you.old-stalls", "<0.3", Relation::BREAKS),
        ];
        let manifest = enshwick_abi::headed!(Manifest {
            side: Side::BOTH,
            flags: ManifestFlags::NONE,
            dependencies: dependencies.as_ptr(),
            dependency_count: u32::try_from(dependencies.len()).unwrap(),
            name: Str::new("Market Stalls"),
            description: Str::new("Stalls that sell what players craft"),
            authors: AUTHORS.as_ptr(),
            author_count: u32::try_from(AUTHORS.len()).unwrap_or(u32::MAX),
            license: Str::new("MIT"),
            homepage: Str::new("https://github.com/enshwick/enshwick"),
            exports: EXPORTS.as_ptr(),
            export_count: u32::try_from(EXPORTS.len()).unwrap(),
            ..plain("you.market-stalls", "0.2.0")
        });
        assert_eq!(
            // SAFETY: every string and list of `manifest` is a static or a
            // local that outlives the call, never written.
            unsafe { describe(&manifest) },
            "id: you.market-stalls\nversion: 0.2.0\nname: Market Stalls\n\
             description: Stalls that sell what players craft\nauthors: themaiby, a friend\n\
             license: MIT\nhomepage: https://github.com/enshwick/enshwick\n\
             exports: content:stall, event:opened\nside: both, in the join check\n\
             requires: you.trading-post >=0.1, <1.0\nafter: you.lanterns\n\
             breaks: you.old-stalls <0.3"
        );
    }
}
