//! Reading a mod's manifest into a resolver candidate.

use enshwick_api::abi::{
    ABI_MAJOR, Dependency, Manifest, Relation, Side, UpdateKind, UpdateSource,
};
use semver::{Version, VersionReq};

use crate::resolve::{self, Candidate};

/// Validates `manifest` and turns it into a candidate.
///
/// # Safety
///
/// `manifest` and every pointer and string in it are valid for the call.
///
/// # Errors
///
/// A reason for the log when the manifest is malformed or from another ABI.
pub unsafe fn parse(manifest: &Manifest) -> Result<Candidate, String> {
    if manifest.header.abi_major != ABI_MAJOR {
        return Err(format!(
            "built for ABI {}, the loader speaks {ABI_MAJOR}",
            manifest.header.abi_major
        ));
    }
    if !manifest.header.covers(size_of::<Manifest>()) {
        return Err("the manifest is truncated".into());
    }
    // SAFETY: the caller keeps the manifest's strings alive for the call.
    let id = unsafe { manifest.id.as_str() }.ok_or("the id is not UTF-8")?;
    if !enshwick_api::abi::is_valid_id(id) {
        return Err(format!("the id {id:?} is not [a-z0-9-]+"));
    }
    if id == crate::settings::LOADER_ID {
        return Err(format!("the id {id:?} is the loader's own"));
    }
    // SAFETY: as above.
    let version = unsafe { manifest.version.as_str() }.ok_or("the version is not UTF-8")?;
    let version: Version = version
        .parse()
        .map_err(|e| format!("version {version:?}: {e}"))?;
    if !matches!(manifest.side, Side::SERVER | Side::CLIENT | Side::BOTH) {
        return Err(format!("unknown side {}", manifest.side.0));
    }
    let raw = if manifest.dependencies.is_null() || manifest.dependency_count == 0 {
        &[][..]
    } else {
        // SAFETY: the manifest points at `dependency_count` entries.
        unsafe { core::slice::from_raw_parts(manifest.dependencies, manifest.dependency_count) }
    };
    let dependencies = raw
        .iter()
        // SAFETY: as above, for every dependency's strings.
        .map(|d| unsafe { dependency(d) })
        .collect::<Result<_, _>>()?;
    Ok(Candidate {
        id: id.to_owned(),
        version,
        side: manifest.side,
        dependencies,
    })
}

/// The update source `manifest` declares, checked; [`UpdateSource::NONE`]
/// for a manifest without one or older than ABI minor 5.
///
/// # Safety
///
/// `manifest` and every pointer and string in it are valid for the call.
///
/// # Errors
///
/// A reason for the log when the source is malformed; the mod still loads.
pub unsafe fn updates(manifest: &Manifest) -> Result<UpdateSource, String> {
    let end = core::mem::offset_of!(Manifest, updates) + size_of::<usize>();
    if !manifest.header.covers(end) || manifest.updates.is_null() {
        return Ok(UpdateSource::NONE);
    }
    // SAFETY: the caller keeps the manifest's pointers valid for the call.
    let source = unsafe { &*manifest.updates };
    if source.header.abi_major != ABI_MAJOR || !source.header.covers(size_of::<UpdateSource>()) {
        return Err("the update source is from another ABI".into());
    }
    // SAFETY: as above, for the source's strings.
    let location = unsafe { source.location.as_str() }.ok_or("the update location is not UTF-8")?;
    // SAFETY: as above.
    let component =
        unsafe { source.component.as_str() }.ok_or("the update component is not UTF-8")?;
    match source.kind {
        UpdateKind::NONE => Ok(UpdateSource::NONE),
        UpdateKind::GITHUB if !is_repository(location) => Err(format!(
            "the update repository {location:?} is not owner/name"
        )),
        UpdateKind::GITHUB if !is_component(component) => Err(format!(
            "the update component {component:?} is not [a-z0-9_-]+"
        )),
        UpdateKind::URL if !location.starts_with("https://") || location.len() <= 8 => {
            Err(format!("the update URL {location:?} is not https://"))
        }
        UpdateKind::GITHUB | UpdateKind::URL => Ok(*source),
        kind => Err(format!("unknown update source kind {}", kind.0)),
    }
}

/// Whether `text` is a GitHub `owner/name`.
fn is_repository(text: &str) -> bool {
    let part = |p: &str| {
        !p.is_empty()
            && p.len() <= 100
            && p.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    };
    text.split_once('/')
        .is_some_and(|(owner, name)| part(owner) && part(name))
}

/// Whether `text` is a release-please component: a Cargo package name.
fn is_component(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}

/// # Safety
///
/// The dependency's strings are valid for the call.
unsafe fn dependency(raw: &Dependency) -> Result<resolve::Dependency, String> {
    // SAFETY: forwarded from the caller.
    let id = unsafe { raw.id.as_str() }.ok_or("a dependency id is not UTF-8")?;
    // SAFETY: forwarded from the caller.
    let range = unsafe { raw.range.as_str() }.ok_or("a version range is not UTF-8")?;
    let range = if range.is_empty() {
        None
    } else {
        Some(
            range
                .parse::<VersionReq>()
                .map_err(|e| format!("range {range:?} of {id}: {e}"))?,
        )
    };
    if !matches!(
        raw.relation,
        Relation::REQUIRES | Relation::AFTER | Relation::CONFLICTS
    ) {
        return Err(format!("unknown relation {} to {id}", raw.relation.0));
    }
    Ok(resolve::Dependency {
        id: id.to_owned(),
        range,
        relation: raw.relation,
    })
}

#[cfg(test)]
mod tests {
    use enshwick_api::abi::{Header, ManifestFlags, Str};

    use super::*;

    static DEPENDENCIES: [Dependency; 1] = [Dependency {
        id: Str::new("flame-altar-zones"),
        range: Str::new("^0.1"),
        relation: Relation::REQUIRES,
    }];

    fn manifest(id: &'static str, version: &'static str) -> Manifest {
        Manifest {
            header: Header::of::<Manifest>(),
            id: Str::new(id),
            version: Str::new(version),
            side: Side::BOTH,
            flags: ManifestFlags::JOIN,
            dependencies: DEPENDENCIES.as_ptr(),
            dependency_count: DEPENDENCIES.len(),
            builds: core::ptr::null(),
            build_count: 0,
            updates: core::ptr::null(),
        }
    }

    static GITHUB: UpdateSource = UpdateSource::github("themaiby/enshwick", "enshwick-safes");
    static NO_OWNER: UpdateSource = UpdateSource::github("enshwick", "enshwick-safes");
    static NO_COMPONENT: UpdateSource = UpdateSource::github("themaiby/enshwick", "");
    static HTTP: UpdateSource = UpdateSource::url("http://example.com/latest.json");
    static HTTPS: UpdateSource = UpdateSource::url("https://example.com/latest.json");

    fn with(source: &'static UpdateSource) -> Result<UpdateSource, String> {
        let mut m = manifest("safes", "0.1.0");
        m.updates = source;
        // SAFETY: static data.
        unsafe { updates(&m) }
    }

    #[test]
    fn update_sources_are_checked() {
        // SAFETY: static data.
        let none = unsafe { updates(&manifest("safes", "0.1.0")) }.unwrap();
        assert_eq!(none.kind, UpdateKind::NONE);
        assert_eq!(with(&GITHUB).unwrap().kind, UpdateKind::GITHUB);
        assert_eq!(with(&HTTPS).unwrap().kind, UpdateKind::URL);
        assert!(with(&NO_OWNER).unwrap_err().contains("owner/name"));
        assert!(with(&NO_COMPONENT).unwrap_err().contains("component"));
        assert!(with(&HTTP).unwrap_err().contains("https"));
    }

    #[test]
    fn an_older_manifest_has_no_update_source() {
        let mut old = manifest("safes", "0.1.0");
        old.updates = &raw const GITHUB;
        old.header.size = u32::try_from(core::mem::offset_of!(Manifest, updates)).unwrap();
        // SAFETY: static data.
        let source = unsafe { updates(&old) }.unwrap();
        assert_eq!(source.kind, UpdateKind::NONE);
    }

    #[test]
    fn a_good_manifest_parses() {
        // SAFETY: static data.
        let candidate = unsafe { parse(&manifest("survivors-per-altar", "0.1.2")) }.unwrap();
        assert_eq!(candidate.id, "survivors-per-altar");
        assert_eq!(candidate.version, Version::new(0, 1, 2));
        assert_eq!(
            candidate.dependencies[0].range,
            Some("^0.1".parse().unwrap())
        );
    }

    #[test]
    fn bad_manifests_say_why() {
        // SAFETY: static data.
        let id = unsafe { parse(&manifest("Bad Id", "0.1.0")) }.unwrap_err();
        assert!(id.contains("not [a-z0-9-]+"), "{id}");
        // SAFETY: static data.
        let version = unsafe { parse(&manifest("ok", "one")) }.unwrap_err();
        assert!(version.starts_with("version"), "{version}");
        // SAFETY: static data.
        let loader = unsafe { parse(&manifest("enshwick", "0.1.0")) }.unwrap_err();
        assert!(loader.contains("loader's own"), "{loader}");
        let mut other = manifest("ok", "0.1.0");
        other.header.abi_major = 2;
        // SAFETY: static data.
        assert!(unsafe { parse(&other) }.unwrap_err().contains("ABI 2"));
    }
}
