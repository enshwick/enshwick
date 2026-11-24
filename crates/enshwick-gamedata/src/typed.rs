//! The typed API's source: the reflected types the API covers, as a
//! [`Catalogue`].
//!
//! A catalogue holds every covered type and every type it reaches, read
//! from a build's reflection and kept as a snapshot in the game data folder
//! [`DATA_VAR`](crate::DATA_VAR) names, outside this repository.
//! The generator writes the API crate's `typed` modules from it, so a new
//! build needs no hand change.

/// The table of typed API ids `typed/ids.rs` holds, sorted by id.
macro_rules! id_table {
    ($($id:literal => $name:literal,)*) => {
        pub(super) const IDS: &[(&str, &str)] = &[$(($id, $name)),*];
    };
}
use id_table;

mod diff;
mod ids;
#[doc(hidden)]
pub mod names;
mod rust;

pub use diff::{Change, Diff, Renames};
pub use rust::GenerateError;

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::reflect::{Kind, Reflection};

/// What the typed API covers, as data: the rule of the API coverage count.
const RELEVANT: &str = include_str!("typed/relevant.toml");

/// Bases deeper than this are taken for a loop in bad data.
const MAX_BASES: usize = 32;

/// The engine's name for the type a typed API id names, such as
/// `ecs::Health`; `None` for a string that is no such id.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::typed::engine_name;
///
/// assert_eq!(engine_name("not an id"), None);
/// ```
#[must_use]
pub fn engine_name(id: &str) -> Option<&'static str> {
    ids::IDS
        .binary_search_by(|(known, _)| (*known).cmp(id))
        .ok()
        .map(|at| ids::IDS[at].1)
}

/// The rename table, below the enshwick repository root: the type and
/// field renames between builds, which the generator keeps names by.
pub const RENAMES: &str = "crates/enshwick-gamedata/typed/renames.toml";

/// The folder of the game data folder that keeps each build's snapshot of
/// every reflected type ([`Catalogue::all`]).
pub const SNAPSHOTS: &str = "reflection";

/// The snapshot file of the build a table of `table_file` is for, in the
/// game data folder `dir`.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use enshwick_gamedata::typed::snapshot_in;
///
/// assert_eq!(
///     snapshot_in(Path::new("data"), "server-00-1.toml"),
///     Path::new("data/reflection/server-00-1.jsonl")
/// );
/// ```
#[must_use]
pub fn snapshot_in(dir: &std::path::Path, table_file: &str) -> std::path::PathBuf {
    let stem = table_file.strip_suffix(".toml").unwrap_or(table_file);
    dir.join(SNAPSHOTS).join(format!("{stem}.jsonl"))
}

/// The snapshot file of the build a table of `table_file` is for, in the
/// folder [`DATA_VAR`](crate::DATA_VAR) names; `None` without it.
///
/// # Examples
///
/// ```
/// let path = enshwick_gamedata::typed::snapshot_path("server-00-1.toml");
/// assert!(path.is_none_or(|p| p.ends_with("reflection/server-00-1.jsonl")));
/// ```
#[must_use]
pub fn snapshot_path(table_file: &str) -> Option<std::path::PathBuf> {
    crate::data_dir().map(|dir| snapshot_in(&dir, table_file))
}

/// The first line of a snapshot that records its build's Steam build.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    steam_build: crate::SteamBuild,
}

/// The Steam build the snapshot file `path` records in its first line, as
/// [`Catalogue::snapshot_for`] writes it; `None` for one that records none.
///
/// # Errors
///
/// When the file does not read.
///
/// # Examples
///
/// ```
/// let path = std::env::temp_dir().join(format!("steam-build-doc-{}.jsonl", std::process::id()));
/// let snapshot = enshwick_gamedata::typed::Catalogue::default()
///     .snapshot_for(Some(enshwick_gamedata::SteamBuild::new(7)));
/// std::fs::write(&path, snapshot)?;
/// let steam = enshwick_gamedata::typed::snapshot_steam_build(&path)?;
/// std::fs::remove_file(&path)?;
/// assert_eq!(steam, Some(enshwick_gamedata::SteamBuild::new(7)));
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn snapshot_steam_build(path: &std::path::Path) -> std::io::Result<Option<crate::SteamBuild>> {
    use std::io::BufRead as _;

    let mut first = String::new();
    std::io::BufReader::new(std::fs::File::open(path)?).read_line(&mut first)?;
    Ok(serde_json::from_str::<Header>(&first)
        .ok()
        .map(|h| h.steam_build))
}

/// The newest snapshot of a side in a game data folder, as
/// [`newest_snapshot`] picks it, with the snapshots it could not rank.
///
/// # Examples
///
/// See [`newest_snapshot`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewestSnapshot {
    path: std::path::PathBuf,
    steam: Option<crate::SteamBuild>,
    unranked: Vec<std::path::PathBuf>,
}

impl NewestSnapshot {
    /// The snapshot file.
    ///
    /// # Examples
    ///
    /// See [`newest_snapshot`].
    #[must_use]
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// The Steam build the snapshot records; `None` for a kept build's
    /// snapshot, whose table names it, or one that records none.
    ///
    /// # Examples
    ///
    /// See [`newest_snapshot`].
    #[must_use]
    pub const fn steam_build(&self) -> Option<crate::SteamBuild> {
        self.steam
    }

    /// The snapshots of builds without a kept table that record no Steam
    /// build, so they ranked below every one that does.
    ///
    /// # Examples
    ///
    /// See [`newest_snapshot`].
    #[must_use]
    pub fn unranked(&self) -> &[std::path::PathBuf] {
        &self.unranked
    }
}

/// The newest snapshot of `side` in the game data folder `dir`, ranked as
/// its build's kept table among `tables` ranks, or first without one.
///
/// A build no table is kept for is newer than every kept one, since the
/// tables are frozen, and such builds rank by the Steam build their
/// snapshot records; one that records none ranks below every one that does,
/// by file name among its like. The linker age in a file name ranks
/// nothing: it starts over with each new GUID.
///
/// # Errors
///
/// When the folder or a snapshot without a table does not read.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::{Side, typed::newest_snapshot};
///
/// let none = newest_snapshot(std::path::Path::new("no-such-folder"), &[], Side::Server)?;
/// assert!(none.is_none());
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn newest_snapshot(
    dir: &std::path::Path,
    tables: &[crate::Table],
    side: crate::Side,
) -> std::io::Result<Option<NewestSnapshot>> {
    let entries = match std::fs::read_dir(dir.join(SNAPSHOTS)) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut untabled = Vec::new();
    let mut kept = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str().filter(|n| snapshot_age(n, side).is_some()) else {
            continue;
        };
        let stem = name.strip_suffix(".jsonl");
        if let Some(table) = tables
            .iter()
            .find(|t| t.side() == side && t.file_name().strip_suffix(".toml") == stem)
        {
            kept.push(table);
            continue;
        }
        untabled.push((snapshot_steam_build(&entry.path())?, entry.path()));
    }
    let unranked = untabled
        .iter()
        .filter(|(steam, _)| steam.is_none())
        .map(|(_, path)| path.clone())
        .collect();
    let newest = untabled.into_iter().max().or_else(|| {
        kept.into_iter()
            .max_by_key(|t| crate::resolve::rank(t))
            .map(|t| (None, snapshot_in(dir, &t.file_name())))
    });
    Ok(newest.map(|(steam, path)| NewestSnapshot {
        path,
        steam,
        unranked,
    }))
}

/// The linker age the snapshot file `name` of `side` names its build by
/// (`<side>-<guid>-<age>.jsonl`); `None` for any other file.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::{Side, typed::snapshot_age};
///
/// let name = format!("server-{}-224.jsonl", "ab".repeat(16));
/// assert_eq!(snapshot_age(&name, Side::Server), Some(224));
/// assert_eq!(snapshot_age(&name, Side::Client), None);
/// ```
#[must_use]
pub fn snapshot_age(name: &str, side: crate::Side) -> Option<u32> {
    let rest = name.strip_suffix(".jsonl")?.strip_prefix(side.as_str())?;
    let (guid, age) = rest.strip_prefix('-')?.rsplit_once('-')?;
    let hex = guid.len() == 32 && guid.bytes().all(|b| b.is_ascii_hexdigit());
    hex.then(|| age.parse().ok()).flatten()
}

/// The words a generated file's header names the build of `table` by.
///
/// # Examples
///
/// ```
/// let table = enshwick_gamedata::Table::parse(
///     "name = \"t\"\nside = \"server\"\nguid = \"00000000000000000000000000000001\"\nage = 2\n",
/// )?;
/// assert_eq!(enshwick_gamedata::typed::build_name(&table), "server build 2");
/// # Ok::<(), enshwick_gamedata::TableError>(())
/// ```
#[must_use]
pub fn build_name(table: &crate::Table) -> String {
    let age = table.fingerprint().age();
    let side = table.side().as_str();
    table.steam_build().map_or_else(
        || format!("{side} build {age}"),
        |steam| format!("{side} build {age} (Steam {steam})"),
    )
}

/// The words a generated file's header names the build of the snapshot
/// `path` by.
///
/// They are its kept table's [`build_name`] among `tables`, else its side
/// and the linker age in its file name, with the Steam build `recorded` the
/// snapshot records.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::{SteamBuild, typed::snapshot_build};
///
/// let path = format!("data/reflection/server-{}-224.jsonl", "ab".repeat(16));
/// assert_eq!(snapshot_build(path.as_ref(), &[], None), "server build 224");
/// assert_eq!(
///     snapshot_build(path.as_ref(), &[], Some(SteamBuild::new(9))),
///     "server build 224 (Steam 9)"
/// );
/// ```
#[must_use]
pub fn snapshot_build(
    path: &std::path::Path,
    tables: &[crate::Table],
    recorded: Option<crate::SteamBuild>,
) -> String {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let stem = name.strip_suffix(".jsonl");
    if let Some(table) = tables
        .iter()
        .find(|t| t.file_name().strip_suffix(".toml") == stem)
    {
        return build_name(table);
    }
    let build = [crate::Side::Server, crate::Side::Client]
        .into_iter()
        .find_map(|side| {
            Some(format!(
                "{} build {}",
                side.as_str(),
                snapshot_age(name, side)?
            ))
        })
        .unwrap_or_else(|| name.to_owned());
    match recorded {
        Some(id) => format!("{build} (Steam {id})"),
        None => build,
    }
}

/// What a covered type is to a mod: a component an entity holds, an event
/// the game raises, or a resource of the game's content.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::typed::Role;
///
/// assert_eq!(Role::ALL.len(), 3);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Role {
    /// An ECS component.
    Component,
    /// An ECS game event.
    Event,
    /// A resource of the game's content data.
    Resource,
}

impl Role {
    /// Every role, in the order reports list them.
    pub const ALL: [Self; 3] = [Self::Component, Self::Event, Self::Resource];
}

/// The rule file of what the typed API covers, below the enshwick
/// repository root, which [`Relevance::parse`] reads.
pub const RELEVANCE: &str = "crates/enshwick-gamedata/src/typed/relevant.toml";

/// The rule of what the typed API covers, by role: what a tree's
/// `relevant.toml` ([`RELEVANCE`]) says, applied to a build's reflection by
/// [`Catalogue::covered`].
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::typed::Relevance;
///
/// let rule = "[roots]\ncomponent = \"demo::ecs::Component\"\n[suffixes]\n\
///     [left_out]\nnamespaces = []\nwords = []\nprefixes = []\n";
/// assert!(Relevance::parse(rule).is_ok());
/// assert!(Relevance::parse("roots = 1").is_err());
/// ```
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Relevance {
    roots: BTreeMap<Role, String>,
    suffixes: BTreeMap<Role, Vec<String>>,
    left_out: LeftOut,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LeftOut {
    namespaces: Vec<String>,
    words: Vec<String>,
    prefixes: Vec<String>,
}

/// Why a relevance rule does not read.
///
/// # Examples
///
/// ```
/// assert!(enshwick_gamedata::typed::Relevance::parse("[roots]\ncomponent = 1\n").is_err());
/// ```
#[derive(Debug, thiserror::Error)]
#[error("the relevance rule: {0}")]
pub struct ParseRelevanceError(#[from] toml::de::Error);

impl Relevance {
    /// The rule `text` holds, as `relevant.toml` writes it.
    ///
    /// # Errors
    ///
    /// [`ParseRelevanceError`] for a text that is no such rule.
    ///
    /// # Examples
    ///
    /// See [`Relevance`].
    pub fn parse(text: &str) -> Result<Self, ParseRelevanceError> {
        Ok(toml::from_str(text)?)
    }

    /// The rule of this crate's own `relevant.toml`, the one its committed
    /// typed API was generated by.
    ///
    /// # Panics
    ///
    /// Never: the embedded file reads, as a test checks.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_gamedata::typed::{Catalogue, Relevance, Role};
    ///
    /// let covered = Catalogue::default().covered(&Relevance::committed());
    /// assert_eq!(covered.count(Role::Component), 0);
    /// ```
    #[must_use]
    pub fn committed() -> Self {
        #[expect(clippy::expect_used, reason = "the embedded file is checked by a test")]
        Self::parse(RELEVANT).expect("relevant.toml reads")
    }

    /// Whether the type `name` is left out of every role.
    fn left_out(&self, name: &str) -> bool {
        let (_, short) = names::split(name);
        let prefixed = |prefix: &String| {
            short
                .strip_prefix(prefix.as_str())
                .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_uppercase()))
        };
        self.left_out
            .namespaces
            .iter()
            .any(|n| name.starts_with(n.as_str()))
            || self
                .left_out
                .words
                .iter()
                .any(|w| name.contains(w.as_str()))
            || self.left_out.prefixes.iter().any(prefixed)
    }
}

/// A reflected type, as the snapshot keeps it: names and layout only.
///
/// Unknown keys are ignored: every tree on a machine reads one snapshot, so
/// a key a newer tree writes must not refuse it to an older one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Ty {
    pub(crate) name: String,
    #[serde(with = "kind_byte")]
    pub(crate) kind: Kind,
    pub(crate) size: u32,
    pub(crate) align: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) base: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) inner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) count: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) fields: Vec<Field>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) values: Vec<(String, u64)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) attributes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) roles: Vec<Role>,
}

/// A field of a reflected struct; unknown keys are ignored, as in [`Ty`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Field {
    pub(crate) name: String,
    pub(crate) offset: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) ty: Option<String>,
}

mod kind_byte {
    use serde::{Deserialize as _, Deserializer, Serializer};

    use crate::reflect::Kind;

    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's serialize_with passes a reference"
    )]
    pub(super) fn serialize<S: Serializer>(kind: &Kind, out: S) -> Result<S::Ok, S::Error> {
        out.serialize_u8(kind.byte())
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(input: D) -> Result<Kind, D::Error> {
        u8::deserialize(input).map(Kind::from_byte)
    }
}

/// Why a snapshot does not read.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::typed::Catalogue;
///
/// assert!(Catalogue::from_snapshot("not json").is_err());
/// ```
#[derive(Debug, thiserror::Error)]
#[error("snapshot line {line}: {source}")]
pub struct SnapshotError {
    line: usize,
    #[source]
    source: serde_json::Error,
}

/// The types the typed API covers on one build and every type they reach
/// through bases, fields and inner types, by full name; one name more than
/// one type carries is left out, as lookups leave it.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::typed::{Catalogue, Role};
///
/// let snapshot = r#"{"name":"demo::ecs::Component","kind":18,"size":1,"align":1}
/// {"name":"demo::ecs::Fuel","kind":18,"size":4,"align":4,"base":"demo::ecs::Component","roles":["component"]}"#;
/// let catalogue = Catalogue::from_snapshot(snapshot)?;
/// assert_eq!(catalogue.count(Role::Component), 1);
/// assert_eq!(Catalogue::from_snapshot(&catalogue.snapshot())?, catalogue);
/// # Ok::<(), enshwick_gamedata::typed::SnapshotError>(())
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Catalogue {
    pub(crate) types: BTreeMap<String, Ty>,
}

impl Catalogue {
    /// The covered types of `reflection` by this crate's own rule
    /// ([`Relevance::committed`]) and every type they reach.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use enshwick_gamedata::reflect::{Image, Layout, Reflection};
    /// use enshwick_gamedata::typed::{Catalogue, Role};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let bytes = std::fs::read("enshrouded_server.exe")?;
    /// let image = Image::parse(&bytes, Layout::File, None)?;
    /// let catalogue = Catalogue::from_reflection(&Reflection::scan(&image));
    /// assert!(catalogue.count(Role::Event) > 0);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn from_reflection(reflection: &Reflection<'_>) -> Self {
        Self::all(reflection).covered(&Relevance::committed())
    }

    /// Every type of `reflection`, with no roles: what a build's snapshot
    /// keeps, so one snapshot serves every rule a tree's
    /// [`Catalogue::covered`] applies.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use enshwick_gamedata::reflect::{Image, Layout, Reflection};
    /// use enshwick_gamedata::typed::{Catalogue, Relevance, Role};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let bytes = std::fs::read("enshrouded_server.exe")?;
    /// let image = Image::parse(&bytes, Layout::File, None)?;
    /// let all = Catalogue::all(&Reflection::scan(&image));
    /// assert_eq!(all.count(Role::Event), 0);
    /// assert!(all.covered(&Relevance::committed()).count(Role::Event) > 0);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn all(reflection: &Reflection<'_>) -> Self {
        let types = reflection
            .type_names()
            .filter_map(|name| {
                let info = reflection.type_info(name).ok()?;
                let ty = Ty {
                    name: name.to_owned(),
                    kind: info.kind(),
                    size: info.size(),
                    align: info.align(),
                    base: info.base().map(str::to_owned),
                    inner: info.inner().map(str::to_owned),
                    count: info.element_count(),
                    fields: info
                        .fields()
                        .iter()
                        .map(|f| Field {
                            name: f.name().to_owned(),
                            offset: f.offset(),
                            ty: f.type_name().map(str::to_owned),
                        })
                        .collect(),
                    values: info
                        .enum_values()
                        .iter()
                        .map(|v| (v.name().to_owned(), v.value()))
                        .collect(),
                    attributes: info
                        .attributes()
                        .iter()
                        .map(|a| a.name().to_owned())
                        .collect(),
                    roles: Vec::new(),
                };
                Some((name.to_owned(), ty))
            })
            .collect();
        Self { types }
    }

    /// The types `relevance` gives a role among these, with those roles,
    /// and every type they reach; the roles these types held go.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_gamedata::typed::{Catalogue, Relevance, Role};
    ///
    /// let all = Catalogue::from_snapshot(
    ///     r#"{"name":"demo::ecs::Component","kind":18,"size":1,"align":1}
    /// {"name":"demo::ecs::Fuel","kind":18,"size":4,"align":4,"base":"demo::ecs::Component"}"#,
    /// )?;
    /// let rule = Relevance::parse(
    ///     "[roots]\ncomponent = \"demo::ecs::Component\"\n[suffixes]\n\
    ///      [left_out]\nnamespaces = []\nwords = []\nprefixes = []\n",
    /// )?;
    /// assert_eq!(all.covered(&rule).names(Role::Component).collect::<Vec<_>>(), ["demo::ecs::Fuel"]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn covered(mut self, relevance: &Relevance) -> Self {
        for ty in self.types.values_mut() {
            ty.roles.clear();
        }
        self.assign_roles(relevance);
        self.reached()
    }

    /// Gives each type the roles [`Relevance`] gives it.
    fn assign_roles(&mut self, relevance: &Relevance) {
        let roles: BTreeMap<String, Vec<Role>> = self
            .types
            .iter()
            .filter(|(name, _)| !relevance.left_out(name))
            .map(|(name, ty)| {
                let roles = Role::ALL
                    .into_iter()
                    .filter(|role| {
                        relevance
                            .roots
                            .get(role)
                            .is_some_and(|root| self.derives_from(ty, root))
                            || relevance.suffixes.get(role).is_some_and(|suffixes| {
                                suffixes
                                    .iter()
                                    .any(|suffix| name.ends_with(suffix.as_str()))
                            })
                    })
                    .collect();
                (name.clone(), roles)
            })
            .collect();
        for (name, roles) in roles {
            if let Some(ty) = self.types.get_mut(&name) {
                ty.roles = roles;
            }
        }
    }

    /// Whether the chain of bases of `ty` reaches `root`.
    fn derives_from(&self, ty: &Ty, root: &str) -> bool {
        let mut base = ty.base.as_deref();
        for _ in 0..MAX_BASES {
            match base {
                Some(name) if name == root => return true,
                Some(name) => base = self.types.get(name).and_then(|t| t.base.as_deref()),
                None => return false,
            }
        }
        false
    }

    /// The types with a role and every type they reach.
    fn reached(&self) -> Self {
        let mut todo: Vec<&str> = self
            .types
            .iter()
            .filter(|(_, ty)| !ty.roles.is_empty())
            .map(|(name, _)| name.as_str())
            .collect();
        let mut seen = BTreeSet::new();
        while let Some(name) = todo.pop() {
            let Some(ty) = self.types.get(name) else {
                continue;
            };
            if !seen.insert(name) {
                continue;
            }
            todo.extend(ty.base.as_deref());
            todo.extend(ty.inner.as_deref());
            todo.extend(ty.fields.iter().filter_map(|f| f.ty.as_deref()));
        }
        Self {
            types: seen
                .into_iter()
                .filter_map(|name| Some((name.to_owned(), self.types.get(name)?.clone())))
                .collect(),
        }
    }

    /// How many types have `role`.
    ///
    /// # Examples
    ///
    /// See [`Catalogue`].
    #[must_use]
    pub fn count(&self, role: Role) -> usize {
        self.types
            .values()
            .filter(|t| t.roles.contains(&role))
            .count()
    }

    /// The full names of the types with `role`, in name order.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_gamedata::typed::{Catalogue, Role};
    ///
    /// assert_eq!(Catalogue::default().names(Role::Resource).count(), 0);
    /// ```
    pub fn names(&self, role: Role) -> impl Iterator<Item = &str> {
        self.types
            .values()
            .filter(move |t| t.roles.contains(&role))
            .map(|t| t.name.as_str())
    }

    /// The catalogue as a snapshot: one JSON object per line, in name
    /// order, so a new build's snapshot diffs by type.
    ///
    /// # Examples
    ///
    /// See [`Catalogue`].
    #[must_use]
    pub fn snapshot(&self) -> String {
        let mut out = String::new();
        for ty in self.types.values() {
            // A struct of strings and numbers always serializes.
            out.push_str(&serde_json::to_string(ty).unwrap_or_default());
            out.push('\n');
        }
        out
    }

    /// The catalogue as the snapshot of a build, which records the build's
    /// Steam build in a first line, when known, so [`newest_snapshot`] can
    /// rank it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_gamedata::{SteamBuild, typed::Catalogue};
    ///
    /// let snapshot = Catalogue::default().snapshot_for(Some(SteamBuild::new(7)));
    /// assert_eq!(snapshot, "{\"steam_build\":7}\n");
    /// assert_eq!(Catalogue::from_snapshot(&snapshot)?, Catalogue::default());
    /// # Ok::<(), enshwick_gamedata::typed::SnapshotError>(())
    /// ```
    #[must_use]
    pub fn snapshot_for(&self, steam: Option<crate::SteamBuild>) -> String {
        let header = steam.map(|steam_build| {
            // A struct of one number always serializes.
            serde_json::to_string(&Header { steam_build }).unwrap_or_default() + "\n"
        });
        header.unwrap_or_default() + &self.snapshot()
    }

    /// The catalogue `snapshot` holds, as [`Catalogue::snapshot`] or
    /// [`Catalogue::snapshot_for`] writes it.
    ///
    /// # Errors
    ///
    /// [`SnapshotError`] for a line that is no type.
    ///
    /// # Examples
    ///
    /// See [`Catalogue`].
    pub fn from_snapshot(snapshot: &str) -> Result<Self, SnapshotError> {
        let mut types = BTreeMap::new();
        for (index, line) in snapshot.lines().enumerate() {
            if line.trim().is_empty()
                || (index == 0 && serde_json::from_str::<Header>(line).is_ok())
            {
                continue;
            }
            let ty: Ty = serde_json::from_str(line).map_err(|source| SnapshotError {
                line: index + 1,
                source,
            })?;
            types.insert(ty.name.clone(), ty);
        }
        Ok(Self { types })
    }

    /// The catalogue as the codec's type registry, as the running game's
    /// would hold these types.
    #[cfg(any(test, feature = "test"))]
    #[doc(hidden)]
    #[must_use]
    pub fn registry(&self) -> enshwick_base::resources::Registry<'_> {
        use enshwick_base::resources::{Constant, Field as CodecField, Registry, Type};

        Registry::new(self.types.values().map(|ty| {
            let mut out = Type::new(&ty.name, ty.kind, ty.size, ty.align);
            if let Some(inner) = ty.base.as_deref().or(ty.inner.as_deref()) {
                out = out.with_inner(inner);
            }
            match ty.kind {
                Kind::Struct => out.with_fields(
                    ty.fields
                        .iter()
                        .filter_map(|f| {
                            let offset = u32::try_from(f.offset).ok()?;
                            Some(match f.ty.as_deref() {
                                Some(of) => CodecField::new(&f.name, offset, of),
                                None => CodecField::new(&f.name, offset, "").untyped(),
                            })
                        })
                        .collect(),
                ),
                Kind::Enum => out.with_constants(
                    ty.values
                        .iter()
                        .map(|(name, value)| Constant::new(name, *value))
                        .collect(),
                ),
                _ => out.with_count(ty.count.unwrap_or(0)),
            }
        }))
    }
}

#[cfg(test)]
mod tests;
