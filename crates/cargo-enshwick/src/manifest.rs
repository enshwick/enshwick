//! `cargo enshwick manifest`: what a built mod DLL says about itself.

use std::path::Path;

use enshwick::abi::{Manifest, ManifestFlags, Relation, Side, Str};

/// Describes the mod in `dll`; an error when its version is not `expected`.
pub(crate) fn show(dll: &Path, expected: Option<&str>) -> Result<String, String> {
    let manifest = query(dll)?;
    // SAFETY: `query` copied the manifest out of a DLL it never unloads, whose
    // `enshwick_mod_query` export returns a static manifest with static
    // strings and lists that no thread writes.
    let text = unsafe { describe(&manifest) };
    // SAFETY: `query` copied the manifest out of a DLL it never unloads,
    // whose `enshwick_mod_query` export returns static strings.
    let version = unsafe { manifest.version.as_str() }.unwrap_or_default();
    match expected {
        Some(expected) if expected != version => Err(format!(
            "{} is version {version}, expected {expected}\n{text}",
            dll.display()
        )),
        _ => Ok(text),
    }
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
        Side::CLIENT => "client",
        Side::BOTH => "both",
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
        unsafe { core::slice::from_raw_parts(manifest.authors, manifest.author_count) }
    };
    let authors: Vec<String> = authors.iter().map(text).collect();
    let exports = if manifest.exports.is_null() {
        &[][..]
    } else {
        // SAFETY: `exports` is non-null, checked just before; the caller
        // guarantees `export_count` aligned names there for the call.
        unsafe { core::slice::from_raw_parts(manifest.exports, manifest.export_count) }
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
    lines.extend([
        format!("side: {side}"),
        format!(
            "join check: {}",
            if manifest.flags.contains(ManifestFlags::JOIN) {
                "yes"
            } else {
                "no"
            }
        ),
    ]);
    let dependencies = if manifest.dependencies.is_null() {
        &[][..]
    } else {
        // SAFETY: `dependencies` is non-null, checked just before; the caller
        // guarantees `dependency_count` aligned entries there for the call.
        unsafe { core::slice::from_raw_parts(manifest.dependencies, manifest.dependency_count) }
    };
    for dependency in dependencies {
        let relation = match dependency.relation {
            Relation::REQUIRES => "requires",
            Relation::AFTER => "after",
            Relation::BREAKS => "breaks",
            Relation::RECOMMENDS => "recommends",
            Relation::REPLACES => "replaces",
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
    const REQUIRED: usize = size_of::<Manifest>();
    // SAFETY: the caller's contract is the one `read_headed` needs: null or
    // readable for the size its header states, no concurrent writer.
    let manifest = unsafe { enshwick::abi::read_headed(raw, REQUIRED) }
        .ok_or("the mod returned no manifest, or a truncated one")?;
    let header = manifest.header;
    enshwick::abi::check_abi(u32::from(header.abi_major), header.abi_minor)
        .map_err(|why| why.to_string())?;
    Ok(manifest)
}

#[cfg(windows)]
fn query(dll: &Path) -> Result<Manifest, String> {
    use std::os::windows::ffi::OsStrExt as _;

    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    // A bare name would go through the DLL search order, and could load a
    // system DLL of that name instead.
    let dll = std::path::absolute(dll).map_err(|e| format!("{}: {e}", dll.display()))?;
    let wide: Vec<u16> = dll.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: `wide` ends in the NUL chained onto it and lives across the call.
    let module = unsafe { LoadLibraryW(wide.as_ptr()) };
    if module.is_null() {
        return Err(format!("{} could not be loaded", dll.display()));
    }
    let name = format!("{}\0", enshwick::abi::exports::QUERY);
    // SAFETY: `module` was checked non-null and is never freed; `name` ends in
    // the NUL `format!` appended and lives across the call.
    let query = unsafe { GetProcAddress(module, name.as_ptr()) }
        .ok_or_else(|| format!("{} is not an Enshwick mod", dll.display()))?;
    // SAFETY: the ABI gives `exports::QUERY` the signature
    // `extern "system" fn() -> *const Manifest`.
    let query: unsafe extern "system" fn() -> *const Manifest =
        unsafe { core::mem::transmute(query) };
    // SAFETY: `query` is the mod's `exports::QUERY`, which takes no arguments
    // and only returns its static manifest; the DLL is never unloaded.
    let raw = unsafe { query() };
    // SAFETY: the ABI says `exports::QUERY` returns null or a static manifest,
    // which no thread writes; the DLL is never unloaded, so its strings stay.
    unsafe { copy(raw) }
}

#[cfg(not(windows))]
fn query(_dll: &Path) -> Result<Manifest, String> {
    Err("reading a mod DLL needs Windows".into())
}

#[cfg(test)]
mod tests {
    use enshwick::abi::{Dependency, Header, ManifestFlags, Relation, Side, Str};

    use super::*;

    /// A manifest of `id` and `version` and nothing else, as a DLL holds it.
    fn plain(id: &'static str, version: &'static str) -> Manifest {
        Manifest {
            header: Header::of::<Manifest>(),
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
        }
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
    fn a_manifest_of_another_abi_major_is_refused_with_the_release_to_rebuild_for() {
        let mut other = plain("you.other-mod", "1.0.0");
        other.header.abi_major = enshwick::abi::ABI_MAJOR + 1;
        // SAFETY: `other` is a whole local manifest with static strings.
        let why = unsafe { copy(&raw const other) }.unwrap_err();
        assert_eq!(
            why,
            enshwick::abi::check_abi(u32::from(other.header.abi_major), 0)
                .unwrap_err()
                .to_string()
        );
    }

    #[test]
    fn a_manifest_reads_as_lines() {
        static AUTHORS: [Str; 2] = [Str::new("themaiby"), Str::new("a friend")];
        static EXPORTS: [Str; 2] = [Str::new("stall"), Str::new("opened")];
        let relation = |id, range, relation| Dependency {
            id: Str::new(id),
            range: Str::new(range),
            relation,
        };
        let dependencies = [
            relation("you.trading-post", ">=0.1, <1.0", Relation::REQUIRES),
            relation("you.lanterns", "", Relation::AFTER),
            relation("you.lanterns", "", Relation::RECOMMENDS),
            relation("old-stalls", "<0.3", Relation::BREAKS),
            relation("stalls", "", Relation::REPLACES),
        ];
        let manifest = Manifest {
            side: Side::BOTH,
            flags: ManifestFlags::JOIN,
            dependencies: dependencies.as_ptr(),
            dependency_count: dependencies.len(),
            name: Str::new("Market Stalls"),
            description: Str::new("Stalls that sell what players craft"),
            authors: AUTHORS.as_ptr(),
            author_count: AUTHORS.len(),
            license: Str::new("MIT"),
            homepage: Str::new("https://github.com/enshwick/enshwick"),
            exports: EXPORTS.as_ptr(),
            export_count: EXPORTS.len(),
            ..plain("you.market-stalls", "0.2.0")
        };
        assert_eq!(
            // SAFETY: every string and list of `manifest` is a static or a
            // local that outlives the call, never written.
            unsafe { describe(&manifest) },
            "id: you.market-stalls\nversion: 0.2.0\nname: Market Stalls\n\
             description: Stalls that sell what players craft\nauthors: themaiby, a friend\n\
             license: MIT\nhomepage: https://github.com/enshwick/enshwick\n\
             exports: stall, opened\nside: both\njoin check: yes\n\
             requires: you.trading-post >=0.1, <1.0\nafter: you.lanterns\n\
             recommends: you.lanterns\nbreaks: old-stalls <0.3\n\
             replaces: stalls"
        );
    }
}
