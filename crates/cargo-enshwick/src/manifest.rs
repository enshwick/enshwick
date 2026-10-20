//! `cargo enshwick manifest`: what a built mod DLL says about itself.

use std::path::Path;

use enshwick_api::abi::{Manifest, ManifestFlags, Relation, Side, Str};

/// Describes the mod in `dll`; an error when its version is not `expected`.
pub(crate) fn show(dll: &Path, expected: Option<&str>) -> Result<String, String> {
    let manifest = query(dll)?;
    let text = describe(manifest);
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
        format!("side: {side}"),
        format!(
            "join check: {}",
            if manifest.flags.contains(ManifestFlags::JOIN) {
                "yes"
            } else {
                "no"
            }
        ),
    ];
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
            Relation::CONFLICTS => "conflicts",
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

#[cfg(windows)]
fn query(dll: &Path) -> Result<&'static Manifest, String> {
    use std::os::windows::ffi::OsStrExt as _;

    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    let wide: Vec<u16> = dll.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: a NUL-terminated path; the DLL stays loaded for the process.
    let module = unsafe { LoadLibraryW(wide.as_ptr()) };
    if module.is_null() {
        return Err(format!("{} could not be loaded", dll.display()));
    }
    let name = format!("{}\0", enshwick_api::abi::exports::QUERY);
    // SAFETY: a loaded module and a NUL-terminated name.
    let query = unsafe { GetProcAddress(module, name.as_ptr()) }
        .ok_or_else(|| format!("{} is not an Enshwick mod", dll.display()))?;
    // SAFETY: a mod's query export has this signature.
    let query: extern "system" fn() -> *const Manifest = unsafe { core::mem::transmute(query) };
    // SAFETY: the export returns its static manifest or null.
    let manifest = unsafe { query().as_ref() }.ok_or("the mod returned no manifest")?;
    if manifest.header.abi_major != enshwick_api::abi::ABI_MAJOR {
        return Err(format!("built for ABI {}", manifest.header.abi_major));
    }
    Ok(manifest)
}

#[cfg(not(windows))]
fn query(_dll: &Path) -> Result<&'static Manifest, String> {
    Err("reading a mod DLL needs Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_reads_as_lines() {
        let manifest = enshwick_api::manifest!(
            id: "survivors-per-altar",
            side: BOTH,
            version: "0.2.0",
            join = true,
            requires: ["flame-altar-zones" => "^0.1"],
            after: ["flame-altar-count"],
        );
        assert_eq!(
            describe(manifest),
            "id: survivors-per-altar\nversion: 0.2.0\nside: both\njoin check: yes\n\
             requires: flame-altar-zones ^0.1\nafter: flame-altar-count"
        );
    }
}
