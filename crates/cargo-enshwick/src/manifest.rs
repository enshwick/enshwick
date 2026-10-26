//! `cargo enshwick manifest`: what a built mod DLL says about itself.

use std::path::Path;

use enshwick::abi::{Manifest, ManifestFlags, Relation, Side, Str};

/// Describes the mod in `dll`; an error when its version is not `expected`.
pub(crate) fn show(dll: &Path, expected: Option<&str>) -> Result<String, String> {
    let manifest = query(dll)?;
    let text = describe(&manifest);
    // SAFETY: the manifest's strings are static data in the loaded DLL.
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
fn describe(manifest: &Manifest) -> String {
    // SAFETY: a manifest's strings are static data next to it.
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
    let meta_end = core::mem::offset_of!(Manifest, homepage) + size_of::<Str>();
    if manifest.header.covers(meta_end) {
        let authors = if manifest.authors.is_null() {
            &[][..]
        } else {
            // SAFETY: the manifest points at `author_count` static names.
            unsafe { core::slice::from_raw_parts(manifest.authors, manifest.author_count) }
        };
        let authors: Vec<String> = authors.iter().map(text).collect();
        for (key, value) in [
            ("name", text(&manifest.name)),
            ("description", text(&manifest.description)),
            ("authors", authors.join(", ")),
            ("license", text(&manifest.license)),
            ("homepage", text(&manifest.homepage)),
        ] {
            if !value.is_empty() {
                lines.push(format!("{key}: {value}"));
            }
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
        // SAFETY: the manifest points at `dependency_count` static entries.
        unsafe { core::slice::from_raw_parts(manifest.dependencies, manifest.dependency_count) }
    };
    for dependency in dependencies {
        let relation = match dependency.relation {
            Relation::REQUIRES => "requires",
            Relation::AFTER => "after",
            Relation::BREAKS => "breaks",
            Relation::RECOMMENDS => "recommends",
            Relation::REPLACES => "replaces",
            Relation::WAS => "was",
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

/// Copies the manifest `raw` points at, which may come from an older or a
/// newer minor: the fields the mod's minor lacks stay empty.
///
/// # Safety
///
/// `raw` is null or points at a manifest readable for the size its header
/// says, with static strings and lists.
#[cfg(any(windows, test))]
unsafe fn copy(raw: *const Manifest) -> Result<Manifest, String> {
    // Every minor has the fields before `builds`.
    const REQUIRED: usize = core::mem::offset_of!(Manifest, builds);
    // SAFETY: forwarded from the caller; an all-zero manifest is valid.
    let manifest = unsafe { enshwick::abi::read_headed(raw, REQUIRED) }
        .ok_or("the mod returned no manifest, or a truncated one")?;
    if manifest.header.abi_major != enshwick::abi::ABI_MAJOR {
        return Err(format!("built for ABI {}", manifest.header.abi_major));
    }
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
    // SAFETY: a NUL-terminated path; the DLL stays loaded for the process.
    let module = unsafe { LoadLibraryW(wide.as_ptr()) };
    if module.is_null() {
        return Err(format!("{} could not be loaded", dll.display()));
    }
    let name = format!("{}\0", enshwick::abi::exports::QUERY);
    // SAFETY: a loaded module and a NUL-terminated name.
    let query = unsafe { GetProcAddress(module, name.as_ptr()) }
        .ok_or_else(|| format!("{} is not an Enshwick mod", dll.display()))?;
    // SAFETY: a mod's query export has this signature.
    let query: extern "system" fn() -> *const Manifest = unsafe { core::mem::transmute(query) };
    // SAFETY: the export returns its static manifest or null.
    unsafe { copy(query()) }
}

#[cfg(not(windows))]
fn query(_dll: &Path) -> Result<Manifest, String> {
    Err("reading a mod DLL needs Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_of_an_older_minor_is_copied_up_to_its_size() {
        let mut old = *enshwick::manifest!(id: "old-mod", side: SERVER, version: "1.0.0");
        old.header.size = u32::try_from(core::mem::offset_of!(Manifest, builds)).unwrap();
        // SAFETY: a valid manifest, as long as its header says.
        let copied = unsafe { copy(&raw const old) }.unwrap();
        assert!(copied.name.is_empty());
        assert_eq!(
            describe(&copied),
            "id: old-mod
version: 1.0.0
side: server
join check: no"
        );
        old.header.size = 8;
        // SAFETY: as above.
        assert!(unsafe { copy(&raw const old) }.is_err());
        // SAFETY: null is refused.
        assert!(unsafe { copy(core::ptr::null()) }.is_err());
    }

    #[test]
    fn a_manifest_reads_as_lines() {
        let manifest = enshwick::manifest!(
            id: "survivors-per-altar",
            side: BOTH,
            version: "0.2.0",
            name: "Survivors per Altar",
            description: "More survivors per altar",
            authors: ["themaiby", "a friend"],
            license: "MIT",
            homepage: "https://github.com/enshwick/enshwick",
            join: true,
            requires: ["flame-altar-zones" => ">=0.1, <1.0"],
            after: ["flame-altar-count"],
            recommends: ["flame-altar-count" => ""],
            breaks: ["old-survivors" => "<0.3"],
            replaces: ["survivors"],
            was: ["survivor-copies"],
        );
        assert_eq!(
            describe(manifest),
            "id: survivors-per-altar\nversion: 0.2.0\nname: Survivors per Altar\n\
             description: More survivors per altar\nauthors: themaiby, a friend\n\
             license: MIT\nhomepage: https://github.com/enshwick/enshwick\n\
             side: both\njoin check: yes\n\
             requires: flame-altar-zones >=0.1, <1.0\nafter: flame-altar-count\n\
             recommends: flame-altar-count\nbreaks: old-survivors <0.3\n\
             replaces: survivors\nwas: survivor-copies"
        );
    }
}
