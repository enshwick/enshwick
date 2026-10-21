//! Reading a mod's manifest into a resolver candidate.

use enshwick_loader_api::abi::{
    ABI_MAJOR, ABI_MINOR, Dependency, Manifest, Relation, Side, Str, UpdateKind, UpdateSource,
    read_headed,
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
    let symbol = |s: enshwick_loader_api::abi::Symbol| enshwick_loader_api::abi::Symbol {
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
        enshwick_loader_api::abi::BuildRow {
            symbols: list(row.symbols, row.symbol_count, symbol),
            constants: list(row.constants, row.constant_count, symbol),
            ..row
        }
    });
    let updates_end = core::mem::offset_of!(Manifest, updates) + size_of::<usize>();
    if manifest.header.covers(updates_end) && !manifest.updates.is_null() {
        // SAFETY: as the caller says; an all-zero source is valid.
        let source = unsafe { read_headed(manifest.updates, size_of::<UpdateSource>()) };
        if let Some(source) = source {
            manifest.updates = Box::leak(Box::new(UpdateSource {
                location: text(source.location),
                component: text(source.component),
                ..source
            }));
        }
    }
    manifest.signed_by = manifest.signed_by.map(text);
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
    /// `Name (id)`, or the id when the manifest names none.
    #[must_use]
    pub fn label(&self, id: &str) -> String {
        enshwick_loader_api::mods::label(&self.name, id)
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
        Relation::REQUIRES
            | Relation::AFTER
            | Relation::BREAKS
            | Relation::RECOMMENDS
            | Relation::REPLACES
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
        let mut m = manifest("safes", "0.1.0");
        m.name = Str::new("Safes");
        m.description = Str::new(" A chest only its owner opens ");
        m.authors = AUTHORS.as_ptr();
        m.author_count = AUTHORS.len();
        m.license = Str::new("MIT");
        m.homepage = Str::new("github.com/themaiby/enshwick");
        // SAFETY: static data.
        let meta = unsafe { meta(&m) };
        assert_eq!(meta.label("safes"), "Safes (safes)");
        assert_eq!(meta.description, "A chest only its owner opens");
        assert_eq!(meta.authors, ["themaiby"]);
        assert!(meta.homepage.is_empty());
        assert_eq!(meta.credits(), "by themaiby; MIT");
        m.header.size = u32::try_from(core::mem::offset_of!(Manifest, name)).unwrap();
        // SAFETY: static data.
        assert_eq!(unsafe { super::meta(&m) }, Meta::default());
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

    #[test]
    fn a_mod_of_a_newer_minor_is_refused_and_an_older_one_loads() {
        let mut newer = manifest("ok", "0.1.0");
        newer.header.abi_minor = ABI_MINOR + 1;
        // SAFETY: static data.
        let why = unsafe { parse(&newer) }.unwrap_err();
        assert!(why.contains("update Enshwick"), "{why}");
        let mut older = manifest("ok", "0.1.0");
        older.header.abi_minor = ABI_MINOR - 1;
        // SAFETY: static data.
        assert!(unsafe { parse(&older) }.is_ok());
    }
}
