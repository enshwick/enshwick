//! Reading a mod's manifest into a resolver candidate.

use enshwick_loader_api::abi::{
    ABI_MAJOR, Dependency, Manifest, Relation, Side, Str, UpdateKind, UpdateSource, read_headed,
};
use semver::{Version, VersionReq};

use crate::resolve::{self, Candidate};

/// Where the fields every manifest has end: an older minor lacks `builds`
/// and `updates`, which default to none.
const REQUIRED: usize = core::mem::offset_of!(Manifest, builds);

/// Copies the manifest a mod's `enshwick_mod_query` returned, for the
/// process; the fields of a newer minor than the mod's stay empty.
///
/// # Safety
///
/// `raw` is null or points at the mod's manifest, readable for the size its
/// header says.
///
/// # Errors
///
/// A reason for the log when `raw` is null or shorter than every manifest.
pub unsafe fn read(raw: *const Manifest) -> Result<&'static Manifest, String> {
    if raw.is_null() {
        return Err("the manifest is missing".into());
    }
    // SAFETY: forwarded from the caller; an all-zero manifest is valid.
    let manifest = unsafe { read_headed(raw, REQUIRED) }.ok_or("the manifest is truncated")?;
    Ok(Box::leak(Box::new(manifest)))
}

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
    if !manifest.header.covers(REQUIRED) {
        return Err("the manifest is truncated".into());
    }
    // SAFETY: the caller keeps the manifest's strings alive for the call.
    let id = unsafe { manifest.id.as_str() }.ok_or("the id is not UTF-8")?;
    if !enshwick_loader_api::abi::is_valid_id(id) {
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
    // SAFETY: the caller keeps the manifest's pointers valid for the call; an
    // all-zero source is valid. Every field of a source is there since minor 5.
    let source = unsafe { read_headed(manifest.updates, size_of::<UpdateSource>()) }
        .filter(|source| source.header.abi_major == ABI_MAJOR)
        .ok_or("the update source is from another ABI")?;
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
        UpdateKind::GITHUB | UpdateKind::URL => Ok(source),
        kind => Err(format!("unknown update source kind {}", kind.0)),
    }
}

/// The author keys `manifest` declares with `signed_by:`, checked, and a
/// reason for the log for each key left out because it is not a minisign
/// public key; none for a manifest older than ABI minor 14.
///
/// # Safety
///
/// `manifest` and every string in it are valid for the call.
#[must_use]
pub unsafe fn signed_by(manifest: &Manifest) -> ([Str; 2], Vec<String>) {
    let mut keys = [Str::EMPTY; 2];
    let mut refused = Vec::new();
    let end = core::mem::offset_of!(Manifest, signed_by) + size_of::<[Str; 2]>();
    if !manifest.header.covers(end) {
        return (keys, refused);
    }
    let mut kept = 0;
    for key in &manifest.signed_by {
        // SAFETY: forwarded from the caller.
        match unsafe { key.as_str() } {
            Some("") => {}
            Some(text) if enshwick_loader_api::mods::is_author_key(text) => {
                keys[kept] = *key;
                kept += 1;
            }
            Some(text) => refused.push(format!(
                "the signed_by key {text:?} is not a minisign public key (RW..., 56 characters)"
            )),
            None => refused.push("a signed_by key is not UTF-8".to_owned()),
        }
    }
    (keys, refused)
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
    use enshwick_loader_api::abi::{Header, ManifestFlags, Str};

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
            signed_by: [Str::EMPTY; 2],
        }
    }

    const KEY: &str = "RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF";

    #[test]
    fn author_keys_are_checked_and_bad_ones_left_out() {
        let mut m = manifest("safes", "0.1.0");
        // SAFETY: static data.
        let (keys, refused) = unsafe { signed_by(&m) };
        assert!(keys.iter().all(Str::is_empty) && refused.is_empty());
        m.signed_by = [Str::new("RWnot-a-key"), Str::new(KEY)];
        // SAFETY: static data.
        let (keys, refused) = unsafe { signed_by(&m) };
        // SAFETY: static strings.
        assert_eq!(unsafe { keys[0].as_str() }, Some(KEY));
        assert!(keys[1].is_empty());
        assert_eq!(refused.len(), 1);
        assert!(refused[0].contains("RWnot-a-key"), "{}", refused[0]);
    }

    #[test]
    fn a_manifest_older_than_author_keys_has_none() {
        let mut old = manifest("safes", "0.1.0");
        old.signed_by = [Str::new(KEY), Str::EMPTY];
        old.header.size = u32::try_from(core::mem::offset_of!(Manifest, signed_by)).unwrap();
        // SAFETY: static data.
        let (keys, refused) = unsafe { signed_by(&old) };
        assert!(keys.iter().all(Str::is_empty) && refused.is_empty());
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

    /// A manifest of minor 4: no update source yet.
    #[repr(C)]
    struct Minor4 {
        header: Header,
        id: Str,
        version: Str,
        side: Side,
        flags: ManifestFlags,
        dependencies: *const Dependency,
        dependency_count: usize,
        builds: *const enshwick_loader_api::abi::BuildRow,
        build_count: usize,
    }

    /// A manifest with only the fields every minor has.
    #[repr(C)]
    struct Oldest {
        header: Header,
        id: Str,
        version: Str,
        side: Side,
        flags: ManifestFlags,
        dependencies: *const Dependency,
        dependency_count: usize,
    }

    fn header_of<T>() -> Header {
        Header {
            size: u32::try_from(size_of::<T>()).unwrap(),
            abi_minor: 4,
            ..Header::of::<Manifest>()
        }
    }

    #[test]
    fn a_manifest_of_an_older_minor_loads_with_defaults() {
        let old = Minor4 {
            header: header_of::<Minor4>(),
            id: Str::new("safes"),
            version: Str::new("0.1.0"),
            side: Side::BOTH,
            flags: ManifestFlags::JOIN,
            dependencies: DEPENDENCIES.as_ptr(),
            dependency_count: DEPENDENCIES.len(),
            builds: core::ptr::null(),
            build_count: 0,
        };
        // SAFETY: a valid manifest as long as its header says.
        let manifest = unsafe { read((&raw const old).cast()) }.unwrap();
        assert!(manifest.updates.is_null());
        // SAFETY: static data.
        let candidate = unsafe { parse(manifest) }.unwrap();
        assert_eq!(candidate.id, "safes");
        assert_eq!(candidate.dependencies.len(), 1);
        // SAFETY: static data.
        assert_eq!(unsafe { updates(manifest) }.unwrap().kind, UpdateKind::NONE);
    }

    #[test]
    fn a_manifest_without_builds_loads_with_none() {
        let old = Oldest {
            header: header_of::<Oldest>(),
            id: Str::new("safes"),
            version: Str::new("0.1.0"),
            side: Side::SERVER,
            flags: ManifestFlags::NONE,
            dependencies: core::ptr::null(),
            dependency_count: 0,
        };
        // SAFETY: a valid manifest as long as its header says.
        let manifest = unsafe { read((&raw const old).cast()) }.unwrap();
        assert!(manifest.builds.is_null());
        assert_eq!(manifest.build_count, 0);
        // SAFETY: static data.
        let candidate = unsafe { parse(manifest) }.unwrap();
        assert_eq!(candidate.side, Side::SERVER);
        assert!(candidate.dependencies.is_empty());
    }

    #[test]
    fn a_manifest_shorter_than_every_minor_is_refused() {
        let mut short = manifest("safes", "0.1.0");
        short.header.size =
            u32::try_from(core::mem::offset_of!(Manifest, dependency_count)).unwrap();
        // SAFETY: a valid manifest.
        let reason = unsafe { read(&raw const short) }.unwrap_err();
        assert!(reason.contains("truncated"), "{reason}");
        // SAFETY: null is refused.
        assert!(unsafe { read(core::ptr::null()) }.is_err());
    }

    #[test]
    fn a_manifest_of_a_newer_minor_loads() {
        #[repr(C)]
        struct Newer {
            manifest: Manifest,
            appended: u64,
        }
        let mut newer = Newer {
            manifest: manifest("safes", "0.1.0"),
            appended: 1,
        };
        newer.manifest.header.size = u32::try_from(size_of::<Newer>()).unwrap();
        // SAFETY: a valid manifest as long as its header says.
        let manifest = unsafe { read((&raw const newer).cast()) }.unwrap();
        // SAFETY: static data.
        assert_eq!(unsafe { parse(manifest) }.unwrap().id, "safes");
        assert_eq!(newer.appended, 1);
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
