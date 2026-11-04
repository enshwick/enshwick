//! The typed API's source: the reflected types the API covers and every
//! type they reach, as a [`Catalogue`] read from a build's reflection and
//! kept as a snapshot beside the build tables. The generator writes the API
//! crate's `typed` modules from it, so a new build needs no hand change.

/// The table of typed API ids `typed/ids.rs` holds, sorted by id.
macro_rules! id_table {
    ($($id:literal => $name:literal,)*) => {
        pub(super) const IDS: &[(&str, &str)] = &[$(($id, $name)),*];
    };
}
use id_table;

mod ids;
#[doc(hidden)]
pub mod names;
mod rust;

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

/// The rule of [`RELEVANT`].
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Relevance {
    roots: BTreeMap<Role, String>,
    suffixes: BTreeMap<Role, String>,
    left_out: LeftOut,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LeftOut {
    namespaces: Vec<String>,
    words: Vec<String>,
    prefixes: Vec<String>,
}

impl Relevance {
    fn read() -> Self {
        #[expect(clippy::expect_used, reason = "the embedded file is checked by a test")]
        toml::from_str(RELEVANT).expect("relevant.toml reads")
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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

/// A field of a reflected struct.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Field {
    pub(crate) name: String,
    pub(crate) offset: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) ty: Option<String>,
}

mod kind_byte {
    use serde::{Deserialize as _, Deserializer, Serializer};

    use crate::reflect::Kind;

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
    /// The covered types of `reflection` and every type they reach.
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
        let mut all = Self { types };
        all.assign_roles(&Relevance::read());
        all.reached()
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
                            || relevance
                                .suffixes
                                .get(role)
                                .is_some_and(|suffix| name.ends_with(suffix.as_str()))
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
            #[expect(
                clippy::expect_used,
                reason = "a struct of strings and numbers serializes"
            )]
            out.push_str(&serde_json::to_string(ty).expect("a type serializes"));
            out.push('\n');
        }
        out
    }

    /// The catalogue `snapshot` holds, as [`Catalogue::snapshot`] writes it.
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
            if line.trim().is_empty() {
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
}

#[cfg(test)]
mod tests;
