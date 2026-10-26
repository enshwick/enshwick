//! Reading a mod's manifest into a resolver candidate.

use enshwick::abi::{ABI_MAJOR, ABI_MINOR, Dependency, Manifest, Relation, Side, Str, read_headed};
use semver::{Version, VersionReq};

use crate::resolve::{self, Candidate};

/// The ABI minor from which a mod id must carry its author,
/// `<author>.<name>`; an older mod's id without one still loads, with a
/// warning, until every published mod is rebuilt.
const AUTHOR_IDS: u16 = 70;

/// Where the fields every manifest has end: an older minor lacks `builds`,
/// which defaults to none.
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
    // SAFETY: the manifest's pointers are valid as the caller says.
    Ok(Box::leak(Box::new(unsafe { own(manifest) })))
}

/// `manifest` with every string and list it points at copied into the
/// loader's memory, so it outlives its DLL, which a development reload
/// unloads. A string that is not UTF-8 keeps pointing into the DLL; the
/// checks refuse such a mod.
///
/// # Safety
///
/// Every pointer and string in `manifest` is valid for the call.
unsafe fn own(mut manifest: Manifest) -> Manifest {
    fn text(s: Str) -> Str {
        // SAFETY: the caller keeps the string valid for the call.
        match unsafe { s.as_str() } {
            Some(text) => Str::new(Box::leak(text.to_owned().into_boxed_str())),
            None => s,
        }
    }
    fn list<T: Copy>(ptr: *const T, count: usize, each: impl Fn(T) -> T) -> *const T {
        if ptr.is_null() || count == 0 {
            return ptr;
        }
        // SAFETY: the caller keeps `count` entries at `ptr` valid for the call.
        let items = unsafe { core::slice::from_raw_parts(ptr, count) };
        let owned: Vec<T> = items.iter().map(|&item| each(item)).collect();
        Box::leak(owned.into_boxed_slice()).as_ptr()
    }
    let symbol = |s: enshwick::abi::Symbol| enshwick::abi::Symbol {
        name: text(s.name),
        ..s
    };
    manifest.id = text(manifest.id);
    manifest.version = text(manifest.version);
    manifest.dependencies = list(manifest.dependencies, manifest.dependency_count, |d| {
        Dependency {
            id: text(d.id),
            range: text(d.range),
            ..d
        }
    });
    manifest.builds = list(manifest.builds, manifest.build_count, |row| {
        enshwick::abi::BuildRow {
            symbols: list(row.symbols, row.symbol_count, symbol),
            constants: list(row.constants, row.constant_count, symbol),
            ..row
        }
    });
    manifest.name = text(manifest.name);
    manifest.description = text(manifest.description);
    manifest.authors = list(manifest.authors, manifest.author_count, text);
    manifest.license = text(manifest.license);
    manifest.homepage = text(manifest.homepage);
    manifest
}

/// Validates `manifest` and turns it into a candidate.
///
/// # Safety
///
/// `manifest` and every pointer and string in it are valid for the call.
///
/// # Errors
///
/// A reason for the log when the manifest is malformed, from another ABI
/// major, or from a newer minor than the loader's: a mod meets only loaders
/// that know every entry it calls, and an older minor's mod is read as far
/// as it knows each struct.
pub unsafe fn parse(manifest: &Manifest) -> Result<Candidate, String> {
    if manifest.header.abi_major != ABI_MAJOR {
        return Err(format!(
            "built for ABI {}, the loader speaks {ABI_MAJOR}",
            manifest.header.abi_major
        ));
    }
    if manifest.header.abi_minor > ABI_MINOR {
        return Err(format!(
            "built for ABI {ABI_MAJOR}.{}, newer than the loader's {ABI_MAJOR}.{ABI_MINOR}: update Enshwick",
            manifest.header.abi_minor
        ));
    }
    if !manifest.header.covers(REQUIRED) {
        return Err("the manifest is truncated".into());
    }
    // SAFETY: the caller keeps the manifest's strings alive for the call.
    let id = unsafe { manifest.id.as_str() }.ok_or("the id is not UTF-8")?;
    if id == crate::settings::LOADER_ID {
        return Err(format!("the id {id:?} is the loader's own"));
    }
    let without_author =
        manifest.header.abi_minor < AUTHOR_IDS && enshwick::abi::is_valid_former_id(id);
    if !enshwick::abi::is_valid_id(id) && !without_author {
        return Err(format!(
            "the id {id:?} is not <author>.<name>, each of [a-z0-9-]"
        ));
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

/// What a manifest tells players about its mod.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Meta {
    /// The name players read; empty for none.
    pub name: String,
    /// One sentence on what the mod does.
    pub description: String,
    /// The authors' names.
    pub authors: Vec<String>,
    /// An SPDX licence expression.
    pub license: String,
    /// Where players read about the mod.
    pub homepage: String,
}

impl Meta {
    /// The name players read, or the id when the manifest names none.
    #[must_use]
    pub fn label(&self, id: &str) -> String {
        enshwick::mods::label(&self.name, id)
    }

    /// `by A, B; MIT; https://...`: what is known of authors, licence and
    /// homepage, empty when nothing is.
    #[must_use]
    pub fn credits(&self) -> String {
        let mut parts = Vec::new();
        if !self.authors.is_empty() {
            parts.push(format!("by {}", self.authors.join(", ")));
        }
        if !self.license.is_empty() {
            parts.push(self.license.clone());
        }
        if !self.homepage.is_empty() {
            parts.push(self.homepage.clone());
        }
        parts.join("; ")
    }
}

/// The metadata `manifest` declares; empty for a manifest older than ABI
/// minor 36. Text that is not UTF-8, or a homepage that is not an address,
/// is left out.
///
/// # Safety
///
/// `manifest` and every pointer and string in it are valid for the call.
#[must_use]
pub unsafe fn meta(manifest: &Manifest) -> Meta {
    let end = core::mem::offset_of!(Manifest, homepage) + size_of::<Str>();
    if !manifest.header.covers(end) {
        return Meta::default();
    }
    // SAFETY: forwarded from the caller.
    let text = |s: &Str| unsafe { s.as_str() }.unwrap_or_default().trim().to_owned();
    let authors = if manifest.authors.is_null() {
        &[][..]
    } else {
        // SAFETY: the manifest points at `author_count` names.
        unsafe { core::slice::from_raw_parts(manifest.authors, manifest.author_count) }
    };
    let homepage = text(&manifest.homepage);
    Meta {
        name: text(&manifest.name),
        description: text(&manifest.description),
        authors: authors
            .iter()
            .map(text)
            .filter(|name| !name.is_empty())
            .collect(),
        license: text(&manifest.license),
        homepage: if homepage.starts_with("https://") || homepage.starts_with("http://") {
            homepage
        } else {
            String::new()
        },
    }
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
        Relation::REQUIRES
            | Relation::AFTER
            | Relation::BREAKS
            | Relation::RECOMMENDS
            | Relation::REPLACES
            | Relation::WAS
    ) {
        return Err(format!("unknown relation {} to {id}", raw.relation.0));
    }
    if id == crate::settings::LOADER_ID
        && matches!(raw.relation, Relation::REPLACES | Relation::WAS)
    {
        return Err(format!("{id} is the loader's own id; no mod takes it over"));
    }
    Ok(resolve::Dependency {
        id: id.to_owned(),
        range,
        relation: raw.relation,
    })
}

#[cfg(test)]
mod tests {
    use enshwick::abi::{Header, ManifestFlags, Str};

    use super::*;

    static DEPENDENCIES: [Dependency; 1] = [Dependency {
        id: Str::new("you.lockers"),
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
            name: Str::EMPTY,
            description: Str::EMPTY,
            authors: core::ptr::null(),
            author_count: 0,
            license: Str::EMPTY,
            homepage: Str::EMPTY,
        }
    }

    static AUTHORS: [Str; 2] = [Str::new("themaiby"), Str::new(" ")];

    #[test]
    fn metadata_is_read_and_cleaned() {
        let mut m = manifest("you.trading-post", "0.1.0");
        m.name = Str::new("Trading Post");
        m.description = Str::new(" A chest only its owner opens ");
        m.authors = AUTHORS.as_ptr();
        m.author_count = AUTHORS.len();
        m.license = Str::new("MIT");
        m.homepage = Str::new("github.com/enshwick/enshwick");
        // SAFETY: static data.
        let meta = unsafe { meta(&m) };
        assert_eq!(meta.label("you.trading-post"), "Trading Post");
        assert_eq!(meta.description, "A chest only its owner opens");
        assert_eq!(meta.authors, ["themaiby"]);
        assert!(meta.homepage.is_empty());
        assert_eq!(meta.credits(), "by themaiby; MIT");
        m.header.size = u32::try_from(core::mem::offset_of!(Manifest, name)).unwrap();
        // SAFETY: static data.
        assert_eq!(unsafe { super::meta(&m) }, Meta::default());
    }

    /// A manifest of minor 4: no names yet.
    #[repr(C)]
    struct Minor4 {
        header: Header,
        id: Str,
        version: Str,
        side: Side,
        flags: ManifestFlags,
        dependencies: *const Dependency,
        dependency_count: usize,
        builds: *const enshwick::abi::BuildRow,
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
            id: Str::new("you.trading-post"),
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
        assert!(manifest.name.is_empty());
        // SAFETY: static data.
        let candidate = unsafe { parse(manifest) }.unwrap();
        assert_eq!(candidate.id, "you.trading-post");
        assert_eq!(candidate.dependencies.len(), 1);
    }

    #[test]
    fn a_manifest_without_builds_loads_with_none() {
        let old = Oldest {
            header: header_of::<Oldest>(),
            id: Str::new("you.trading-post"),
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
        let mut short = manifest("you.trading-post", "0.1.0");
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
            manifest: manifest("you.trading-post", "0.1.0"),
            appended: 1,
        };
        newer.manifest.header.size = u32::try_from(size_of::<Newer>()).unwrap();
        // SAFETY: a valid manifest as long as its header says.
        let manifest = unsafe { read((&raw const newer).cast()) }.unwrap();
        // SAFETY: static data.
        assert_eq!(unsafe { parse(manifest) }.unwrap().id, "you.trading-post");
        assert_eq!(newer.appended, 1);
    }

    #[test]
    fn a_good_manifest_parses() {
        // SAFETY: static data.
        let candidate = unsafe { parse(&manifest("you.market-stalls", "0.1.2")) }.unwrap();
        assert_eq!(candidate.id, "you.market-stalls");
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
        assert!(id.contains("not <author>.<name>"), "{id}");
        // SAFETY: static data.
        let version = unsafe { parse(&manifest("you.ok", "one")) }.unwrap_err();
        assert!(version.starts_with("version"), "{version}");
        // SAFETY: static data.
        let loader = unsafe { parse(&manifest("enshwick", "0.1.0")) }.unwrap_err();
        assert!(loader.contains("loader's own"), "{loader}");
        let mut other = manifest("you.ok", "0.1.0");
        other.header.abi_major = ABI_MAJOR + 1;
        // SAFETY: static data.
        let major = unsafe { parse(&other) }.unwrap_err();
        assert!(major.contains(&format!("ABI {}", ABI_MAJOR + 1)), "{major}");
    }

    #[test]
    fn a_former_id_parses_and_the_loader_s_own_id_is_never_taken_over() {
        static FORMER: [Dependency; 1] = [Dependency {
            id: Str::new("market-stalls"),
            range: Str::EMPTY,
            relation: Relation::WAS,
        }];
        static LOADER: [Dependency; 1] = [Dependency {
            id: Str::new("enshwick"),
            range: Str::EMPTY,
            relation: Relation::REPLACES,
        }];
        let mut renamed = manifest("you.market-stalls", "0.2.0");
        renamed.dependencies = FORMER.as_ptr();
        // SAFETY: static data.
        let candidate = unsafe { parse(&renamed) }.unwrap();
        assert_eq!(candidate.dependencies[0].relation, Relation::WAS);
        renamed.dependencies = LOADER.as_ptr();
        // SAFETY: static data.
        let why = unsafe { parse(&renamed) }.unwrap_err();
        assert!(why.contains("loader's own"), "{why}");
    }

    #[test]
    fn an_id_without_an_author_loads_only_from_an_older_minor() {
        let mut old = manifest("lanterns", "0.1.0");
        old.header.abi_minor = AUTHOR_IDS - 1;
        // SAFETY: static data.
        assert_eq!(unsafe { parse(&old) }.unwrap().id, "lanterns");
        old.header.abi_minor = AUTHOR_IDS;
        // SAFETY: static data.
        let why = unsafe { parse(&old) }.unwrap_err();
        assert!(why.contains("not <author>.<name>"), "{why}");
    }

    #[test]
    fn a_mod_of_a_newer_minor_is_refused_and_an_older_one_loads() {
        let mut newer = manifest("you.ok", "0.1.0");
        newer.header.abi_minor = ABI_MINOR + 1;
        // SAFETY: static data.
        let why = unsafe { parse(&newer) }.unwrap_err();
        assert!(why.contains("update Enshwick"), "{why}");
        let mut older = manifest("you.ok", "0.1.0");
        older.header.abi_minor = ABI_MINOR - 1;
        // SAFETY: static data.
        assert!(unsafe { parse(&older) }.is_ok());
    }
}
