//! Reading a mod's manifest into a resolver candidate.

use enshwick::abi::{ABI_VERSION, Dependency, Manifest, Relation, Side, Str, read_headed};
use semver::{Version, VersionReq};

use crate::resolve::{self, Candidate};

/// Where the fields of a manifest end.
const REQUIRED: usize = size_of::<Manifest>();

/// Why a mod's manifest is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ManifestError {
    /// The DLL hands out no manifest.
    #[error("the manifest is missing")]
    Missing,
    /// The manifest is shorter than this ABI needs.
    #[error("the manifest is truncated")]
    Truncated,
    /// The mod is built for another ABI version.
    #[error("{}", enshwick::abi::OTHER_ABI)]
    OtherAbi,
    /// A text of the manifest is not UTF-8.
    #[error("the {0} is not UTF-8")]
    NotUtf8(&'static str),
    /// The mod claims the loader's own id.
    #[error("the id {0:?} is the loader's own")]
    LoadersId(String),
    /// The id is not of the form mods take.
    #[error("the id {0:?} is not <author>.<name>, each of [a-z0-9-]")]
    BadId(String),
    /// The version does not parse.
    #[error("version {version:?}: {reason}")]
    BadVersion {
        /// The version as the manifest gives it.
        version: String,
        /// Why it does not parse.
        reason: String,
    },
    /// The side is none the loader knows.
    #[error("unknown side {0}")]
    UnknownSide(u8),
    /// A dependency's range does not parse.
    #[error("range {range:?} of {id}: {reason}")]
    BadRange {
        /// The dependency's id.
        id: String,
        /// The range as the manifest gives it.
        range: String,
        /// Why it does not parse.
        reason: String,
    },
    /// A dependency names a relation the loader does not know.
    #[error("unknown relation {relation} to {id}")]
    UnknownRelation {
        /// The dependency's id.
        id: String,
        /// The relation's raw value.
        relation: u8,
    },
    /// A rename or replacement names the loader's own id.
    #[error("{0} is the loader's own id; no mod takes it over")]
    TakesLoadersId(String),
}

/// Copies the manifest a mod's `enshwick_mod_query` returned, for the
/// process.
///
/// # Safety
///
/// - `raw` is null or points at the mod's manifest, readable for the size its
///   header says; no alignment is needed.
/// - Every string and list the manifest points at is readable and unchanged
///   for the call.
/// - No other thread writes the manifest during the call.
///
/// # Errors
///
/// A reason for the log when `raw` is null or shorter than every manifest.
pub unsafe fn read(raw: *const Manifest) -> Result<&'static Manifest, ManifestError> {
    if raw.is_null() {
        return Err(ManifestError::Missing);
    }
    // SAFETY: `raw` is non-null, checked just before, and the caller
    // guarantees it readable for the size its header says, with no
    // concurrent writer.
    let manifest = unsafe { read_headed(raw, REQUIRED) }.ok_or(ManifestError::Truncated)?;
    // SAFETY: `manifest` copies the mod's pointers, and the caller guarantees
    // every string and list they point at readable and unchanged for the call.
    Ok(Box::leak(Box::new(unsafe { own(manifest) })))
}

/// `manifest` with every string and list it points at copied into the
/// loader's memory, so it outlives its DLL, which a development reload
/// unloads. A string that is not UTF-8 keeps pointing into the DLL; the
/// checks refuse such a mod.
///
/// # Safety
///
/// - Every string in `manifest`, its dependencies, build rows and authors
///   is readable for its length and unchanged for the call.
/// - Every list pointer in them is null or points at its count of aligned
///   entries, readable and unchanged for the call.
unsafe fn own(mut manifest: Manifest) -> Manifest {
    fn text(s: Str) -> Str {
        // SAFETY: `text` runs only inside `own`, on strings of its manifest,
        // which `own`'s caller guarantees readable and unchanged for the call.
        match unsafe { s.as_str() } {
            Some(text) => Str::new(Box::leak(text.to_owned().into_boxed_str())),
            None => s,
        }
    }
    fn list<T: Copy>(ptr: *const T, count: usize, each: impl Fn(T) -> T) -> *const T {
        if ptr.is_null() || count == 0 {
            return ptr;
        }
        // SAFETY: `ptr` is non-null and `count` nonzero, checked just before;
        // `list` runs only inside `own`, on lists of its manifest, which `own`'s
        // caller guarantees hold `count` aligned entries, readable and
        // unchanged for the call.
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
    manifest.points = text(manifest.points);
    manifest.homepage = text(manifest.homepage);
    manifest.exports = list(manifest.exports, manifest.export_count, text);
    manifest
}

/// The local names `manifest` exports, those that are UTF-8.
///
/// # Safety
///
/// - `exports` is null or points at `export_count` aligned strings, each
///   readable for its length, unchanged for the call.
#[must_use]
pub unsafe fn exports(manifest: &Manifest) -> Vec<&str> {
    if manifest.exports.is_null() || manifest.export_count == 0 {
        return Vec::new();
    }
    // SAFETY: `exports` is non-null and the count nonzero, checked just
    // before; the caller guarantees `export_count` aligned strings there,
    // readable and unchanged for the call.
    let names = unsafe { core::slice::from_raw_parts(manifest.exports, manifest.export_count) };
    names
        .iter()
        // SAFETY: the caller guarantees every name readable for its length
        // and unchanged for the call.
        .filter_map(|name| unsafe { name.as_str() })
        .collect()
}

/// Why a pack whose `pack.toml` asks for the Enshwick releases
/// `requirement` does not run on this one; `None` when it runs, or when it
/// asks for none.
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the Windows build reads packs")
)]
pub(crate) fn enshwick_refusal(requirement: &str) -> Option<String> {
    if requirement.is_empty() {
        return None;
    }
    let req = match VersionReq::parse(requirement) {
        Ok(req) => req,
        Err(e) => {
            return Some(format!(
                "enshwick = \"{requirement}\" is no version requirement: {e}"
            ));
        }
    };
    let this = Version::parse(enshwick::VERSION).ok()?;
    (!req.matches(&this)).then(|| {
        format!(
            "made for Enshwick {requirement}, this is Enshwick {}",
            enshwick::VERSION
        )
    })
}

/// Validates `manifest` and turns it into a candidate.
///
/// # Safety
///
/// - Every string in `manifest` and its dependencies is readable for its
///   length and unchanged for the call.
/// - `dependencies` is null or points at `dependency_count` aligned
///   entries, readable and unchanged for the call.
///
/// # Errors
///
/// A reason for the log when the manifest is malformed or built for another
/// ABI version.
pub unsafe fn parse(manifest: &Manifest) -> Result<Candidate, ManifestError> {
    if manifest.header.abi_version != ABI_VERSION {
        return Err(ManifestError::OtherAbi);
    }
    if !manifest.header.covers(REQUIRED) {
        return Err(ManifestError::Truncated);
    }
    // SAFETY: the caller guarantees the manifest's strings readable and
    // unchanged for the call.
    let id = unsafe { manifest.id.as_str() }.ok_or(ManifestError::NotUtf8("id"))?;
    if id == crate::settings::LOADER_ID {
        return Err(ManifestError::LoadersId(id.to_owned()));
    }
    if !enshwick::abi::is_valid_id(id) {
        return Err(ManifestError::BadId(id.to_owned()));
    }
    // SAFETY: the caller guarantees the manifest's strings readable and
    // unchanged for the call.
    let version = unsafe { manifest.version.as_str() }.ok_or(ManifestError::NotUtf8("version"))?;
    let version: Version =
        version
            .parse()
            .map_err(|e: semver::Error| ManifestError::BadVersion {
                version: version.to_owned(),
                reason: e.to_string(),
            })?;
    if !matches!(manifest.side, Side::SERVER | Side::CLIENT | Side::BOTH) {
        return Err(ManifestError::UnknownSide(manifest.side.0));
    }
    let raw = if manifest.dependencies.is_null() || manifest.dependency_count == 0 {
        &[][..]
    } else {
        // SAFETY: `dependencies` is non-null and the count nonzero, checked
        // just before; the caller guarantees `dependency_count` aligned entries
        // there, readable and unchanged for the call.
        unsafe { core::slice::from_raw_parts(manifest.dependencies, manifest.dependency_count) }
    };
    let dependencies = raw
        .iter()
        // SAFETY: the caller guarantees every dependency's strings readable
        // and unchanged for the call.
        .map(|d| unsafe { dependency(d) })
        .collect::<Result<_, _>>()?;
    Ok(Candidate {
        id: id.to_owned(),
        version,
        side: manifest.side,
        dependencies,
    })
}

/// Why the mod `asker` may not use the `kind` `name`, `<owner>:<what>`,
/// whose owner's manifest exports `exports`.
///
/// A kind is a call, point, service, event or stat. `None` when it may: a
/// name of its own, or one the owner exports.
#[must_use]
pub fn unexported(kind: &str, name: &str, asker: &str, exports: &[&str]) -> Option<String> {
    let (owner, what) = name.split_once(':')?;
    if owner == asker || exports.contains(&what) {
        return None;
    }
    let offered = if exports.is_empty() {
        "it exports nothing".to_owned()
    } else {
        format!("it exports {}", exports.join(", "))
    };
    Some(format!(
        "{kind} {name}: {owner} does not export {what}; {offered}"
    ))
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
    /// The name players read in `language`: the mod's text
    /// [`MOD_NAME`](enshwick::i18n::MOD_NAME) in `texts`, else the manifest's
    /// name; empty when neither names the mod.
    #[must_use]
    pub fn name_in(
        &self,
        texts: Option<&enshwick::i18n::Texts>,
        language: enshwick::i18n::Language,
    ) -> String {
        texts
            .and_then(|t| t.get(enshwick::i18n::MOD_NAME, language))
            .map_or_else(|| self.name.clone(), str::to_owned)
    }

    /// What the mod does in `language`: the mod's text
    /// [`MOD_DESCRIPTION`](enshwick::i18n::MOD_DESCRIPTION) in `texts`, else
    /// the manifest's description.
    #[must_use]
    pub fn description_in(
        &self,
        texts: Option<&enshwick::i18n::Texts>,
        language: enshwick::i18n::Language,
    ) -> String {
        texts
            .and_then(|t| t.get(enshwick::i18n::MOD_DESCRIPTION, language))
            .map_or_else(|| self.description.clone(), str::to_owned)
    }

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

/// The metadata `manifest` declares. Text that is not UTF-8, or a homepage
/// that is not an address, is left out.
///
/// # Safety
///
/// - Every string in `manifest` is readable for its length and unchanged
///   for the call.
/// - `authors` is null or points at `author_count` aligned strings, readable
///   and unchanged for the call.
#[must_use]
pub unsafe fn meta(manifest: &Manifest) -> Meta {
    // SAFETY: the closure reads only the manifest's own strings and its
    // authors, which the caller guarantees readable and unchanged for the call.
    let text = |s: &Str| unsafe { s.as_str() }.unwrap_or_default().trim().to_owned();
    let authors = if manifest.authors.is_null() {
        &[][..]
    } else {
        // SAFETY: `authors` is non-null, checked just before; the caller
        // guarantees `author_count` aligned strings there for the call.
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
/// - The dependency's `id` and `range` are readable for their lengths and
///   unchanged for the call.
unsafe fn dependency(raw: &Dependency) -> Result<resolve::Dependency, ManifestError> {
    // SAFETY: the caller guarantees `id` readable and unchanged for the call.
    let id = unsafe { raw.id.as_str() }.ok_or(ManifestError::NotUtf8("dependency id"))?;
    // SAFETY: the caller guarantees `range` readable and unchanged for the
    // call.
    let range = unsafe { raw.range.as_str() }.ok_or(ManifestError::NotUtf8("version range"))?;
    let range = if range.is_empty() {
        None
    } else {
        Some(
            range
                .parse::<VersionReq>()
                .map_err(|e| ManifestError::BadRange {
                    id: id.to_owned(),
                    range: range.to_owned(),
                    reason: e.to_string(),
                })?,
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
        return Err(ManifestError::UnknownRelation {
            id: id.to_owned(),
            relation: raw.relation.0,
        });
    }
    if id == crate::settings::LOADER_ID && raw.relation == Relation::REPLACES {
        return Err(ManifestError::TakesLoadersId(id.to_owned()));
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
            points: Str::EMPTY,
            exports: core::ptr::null(),
            export_count: 0,
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
        // SAFETY: every string and list in `m` points at `'static` literals
        // and statics, readable and never written.
        let meta = unsafe { meta(&m) };
        assert_eq!(meta.label("you.trading-post"), "Trading Post");
        assert_eq!(meta.description, "A chest only its owner opens");
        assert_eq!(meta.authors, ["themaiby"]);
        assert!(meta.homepage.is_empty());
        assert_eq!(meta.credits(), "by themaiby; MIT");
    }

    #[test]
    fn a_manifest_without_every_field_of_its_major_is_refused() {
        let mut short = manifest("you.trading-post", "0.1.0");
        short.header.size = u32::try_from(core::mem::offset_of!(Manifest, points)).unwrap();
        // SAFETY: `short` is a whole local manifest, its strings and lists
        // pointing at `'static` data.
        let reason = unsafe { read(&raw const short) }.unwrap_err().to_string();
        assert!(reason.contains("truncated"), "{reason}");
        // SAFETY: `read` returns an error for a null pointer before it reads.
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
        // SAFETY: `newer` is a local manifest readable for the size its header
        // says, its strings and lists pointing at `'static` data.
        let manifest = unsafe { read((&raw const newer).cast()) }.unwrap();
        // SAFETY: every string and list in the manifest points at `'static`
        // literals and statics, readable and never written.
        assert_eq!(unsafe { parse(manifest) }.unwrap().id, "you.trading-post");
        assert_eq!(newer.appended, 1);
    }

    #[test]
    fn a_good_manifest_parses() {
        // SAFETY: every string and list in the manifest points at `'static`
        // literals and statics, readable and never written.
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
        // SAFETY: every string and list in the manifest points at `'static`
        // literals and statics, readable and never written.
        let id = unsafe { parse(&manifest("Bad Id", "0.1.0")) }
            .unwrap_err()
            .to_string();
        assert!(id.contains("not <author>.<name>"), "{id}");
        // SAFETY: every string and list in the manifest points at `'static`
        // literals and statics, readable and never written.
        let version = unsafe { parse(&manifest("you.ok", "one")) }
            .unwrap_err()
            .to_string();
        assert!(version.starts_with("version"), "{version}");
        // SAFETY: every string and list in the manifest points at `'static`
        // literals and statics, readable and never written.
        let loader = unsafe { parse(&manifest("enshwick", "0.1.0")) }
            .unwrap_err()
            .to_string();
        assert!(loader.contains("loader's own"), "{loader}");
    }

    #[test]
    fn an_old_form_id_is_replaced_and_the_loader_s_own_id_never_is() {
        static FORMER: [Dependency; 1] = [Dependency {
            id: Str::new("market-stalls"),
            range: Str::EMPTY,
            relation: Relation::REPLACES,
        }];
        static LOADER: [Dependency; 1] = [Dependency {
            id: Str::new("enshwick"),
            range: Str::EMPTY,
            relation: Relation::REPLACES,
        }];
        let mut renamed = manifest("you.market-stalls", "0.2.0");
        renamed.dependencies = FORMER.as_ptr();
        // SAFETY: every string and list in the manifest points at `'static`
        // literals and statics, readable and never written.
        let candidate = unsafe { parse(&renamed) }.unwrap();
        assert_eq!(candidate.dependencies[0].relation, Relation::REPLACES);
        renamed.dependencies = LOADER.as_ptr();
        // SAFETY: every string and list in the manifest points at `'static`
        // literals and statics, readable and never written.
        let why = unsafe { parse(&renamed) }.unwrap_err().to_string();
        assert!(why.contains("loader's own"), "{why}");
    }

    #[test]
    fn a_pack_made_for_another_enshwick_is_refused_with_its_requirement() {
        let this = format!("={}", enshwick::VERSION);
        assert_eq!(enshwick_refusal(""), None);
        assert_eq!(enshwick_refusal(&this), None);
        assert_eq!(enshwick_refusal(">=1.0.0-alpha.1, <2"), None);
        assert_eq!(
            enshwick_refusal("=0.9.0"),
            Some(format!(
                "made for Enshwick =0.9.0, this is Enshwick {}",
                enshwick::VERSION
            ))
        );
        let bad = enshwick_refusal("one").unwrap();
        assert!(
            bad.starts_with("enshwick = \"one\" is no version requirement"),
            "{bad}"
        );
    }

    #[test]
    fn an_id_without_an_author_is_refused() {
        let authorless = manifest("lanterns", "0.1.0");
        // SAFETY: every string and list in the manifest points at `'static`
        // literals and statics, readable and never written.
        let why = unsafe { parse(&authorless) }.unwrap_err().to_string();
        assert!(why.contains("not <author>.<name>"), "{why}");
    }

    #[test]
    fn exports_are_copied_with_the_manifest() {
        static EXPORTS: [Str; 2] = [Str::new("price-of"), Str::new("sold")];
        let mut exporting = manifest("you.trading-post", "0.1.0");
        exporting.exports = EXPORTS.as_ptr();
        exporting.export_count = EXPORTS.len();
        // SAFETY: `exporting` is a whole local manifest, its strings and lists
        // pointing at `'static` data.
        let copied = unsafe { read(&raw const exporting) }.unwrap();
        assert_ne!(copied.exports, EXPORTS.as_ptr());
        // SAFETY: `read` copied every string and list for the process.
        assert_eq!(unsafe { exports(copied) }, ["price-of", "sold"]);
        let silent = manifest("you.lanterns", "0.1.0");
        // SAFETY: every string and list in the manifest points at `'static`
        // literals and statics, readable and never written.
        assert!(unsafe { exports(&silent) }.is_empty());
    }

    #[test]
    fn another_mod_uses_only_what_the_owner_exports() {
        let exports = ["price-of", "sold"];
        let asker = "you.market-stalls";
        assert_eq!(
            unexported("call", "you.trading-post:price-of", asker, &exports),
            None
        );
        assert_eq!(
            unexported("event", "you.market-stalls:opened", asker, &[]),
            None
        );
        assert_eq!(
            unexported("point", "you.trading-post:margin", asker, &exports).as_deref(),
            Some(
                "point you.trading-post:margin: you.trading-post does not export margin; \
                 it exports price-of, sold"
            )
        );
        let none = unexported("service", "you.trading-post:prices", asker, &[]).unwrap();
        assert!(none.ends_with("it exports nothing"), "{none}");
        assert_eq!(
            unexported("stat", "you.trading-post:luck", asker, &exports).as_deref(),
            Some(
                "stat you.trading-post:luck: you.trading-post does not export luck; \
                 it exports price-of, sold"
            )
        );
    }

    #[test]
    fn a_mod_of_any_other_abi_version_is_refused_with_the_release_to_rebuild_for() {
        for other in [ABI_VERSION.wrapping_add(1), ABI_VERSION.wrapping_sub(1), 4] {
            let mut built = manifest("you.ok", "0.1.0");
            built.header.abi_version = other;
            // SAFETY: every string and list in the manifest points at `'static`
            // literals and statics, readable and never written.
            let why = unsafe { parse(&built) }.unwrap_err().to_string();
            assert_eq!(
                why,
                format!(
                    "built for another Enshwick: rebuild it for Enshwick {}",
                    enshwick::VERSION
                )
            );
        }
        let current = manifest("you.ok", "0.1.0");
        // SAFETY: every string and list in the manifest points at `'static`
        // literals and statics, readable and never written.
        assert!(unsafe { parse(&current) }.is_ok());
    }

    #[test]
    fn a_mod_s_texts_name_it_in_the_player_s_language() {
        use enshwick::i18n::{Language, Texts};
        let meta = Meta {
            name: "Better Altars".to_owned(),
            description: "More altars.".to_owned(),
            ..Meta::default()
        };
        let texts = Texts::parse(
            "[en]\n\"mod.name\" = \"Better Altars\"\n[uk]\n\"mod.name\" = \"Кращі олтарі\"\n\"mod.description\" = \"Більше олтарів.\"\n",
        )
        .unwrap();
        assert_eq!(meta.name_in(Some(&texts), Language::UK_UA), "Кращі олтарі");
        assert_eq!(
            meta.description_in(Some(&texts), Language::UK_UA),
            "Більше олтарів."
        );
        assert_eq!(
            meta.description_in(Some(&texts), Language::FALLBACK),
            "More altars."
        );
        assert_eq!(meta.name_in(None, Language::UK_UA), "Better Altars");
    }
}
