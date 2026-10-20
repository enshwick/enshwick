//! Reading a mod's manifest into a resolver candidate.

use enshwick_api::abi::{ABI_MAJOR, Dependency, Manifest, Relation, Side};
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
        }
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
