//! What a new build changed in the covered types: the stability rules'
//! cases, decided from two catalogues by name, with renames taken only on an
//! exact layout match or from the rename table.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write as _};

use serde::{Deserialize, Serialize};

use super::{Catalogue, Field, Ty};

/// A change one type went through between two builds.
///
/// # Examples
///
/// See [`Diff`].
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Change {
    /// The new build adds the type.
    Added,
    /// The new build lacks the type: its item turns off there.
    Removed,
    /// The new build names the type `to`, with the same layout.
    Renamed {
        /// The new full name.
        to: String,
    },
    /// Removed types with the same layout as an added one: the rename is
    /// not decided, and a person names it in the rename table.
    Unsure {
        /// The added types it could be.
        candidates: Vec<String>,
    },
    /// The type changed its kind or size: its item's shape check decides on
    /// each build, and a person looks at it.
    Retyped {
        /// What changed, such as `size 16 -> 24`.
        what: String,
    },
    /// A field the new build adds.
    FieldAdded(String),
    /// A field the new build lacks.
    FieldRemoved(String),
    /// A field renamed at the same offset with the same type.
    FieldRenamed {
        /// The old name.
        from: String,
        /// The new name.
        to: String,
    },
    /// A field of another type now.
    FieldRetyped {
        /// The field.
        field: String,
        /// The old type.
        from: String,
        /// The new type.
        to: String,
    },
    /// A field at another offset; read by name, it needs no change.
    FieldMoved {
        /// The field.
        field: String,
        /// The old offset.
        from: u64,
        /// The new offset.
        to: u64,
    },
    /// An enum constant the new build adds.
    ConstantAdded(String),
    /// An enum constant the new build lacks.
    ConstantRemoved(String),
    /// An enum constant with another value; read by name, it needs no
    /// change.
    ConstantRenumbered {
        /// The constant.
        name: String,
        /// The old value.
        from: u64,
        /// The new value.
        to: u64,
    },
}

impl Change {
    /// Whether a person looks at it before the new build's API ships.
    #[must_use]
    pub const fn needs_person(&self) -> bool {
        matches!(self, Self::Unsure { .. } | Self::Retyped { .. })
    }
}

/// The renames a person or `cargo enshwick pin` recorded, one table for all
/// builds; the loader resolves a renamed type's old id under its new name.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::typed::Renames;
///
/// let renames = Renames::parse("[[types]]\nfrom = \"demo::A\"\nto = \"demo::B\"\n")?;
/// assert_eq!(renames.to_toml(), "[[types]]\nfrom = \"demo::A\"\nto = \"demo::B\"\n");
/// # Ok::<(), toml::de::Error>(())
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Renames {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    types: Vec<TypeRename>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    fields: Vec<FieldRename>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TypeRename {
    from: String,
    to: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldRename {
    #[serde(rename = "type")]
    ty: String,
    from: String,
    to: String,
}

impl Renames {
    /// The table `text` holds.
    ///
    /// # Errors
    ///
    /// When it is not the table's TOML.
    ///
    /// # Examples
    ///
    /// See [`Renames`].
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// The table as TOML, sorted.
    ///
    /// # Examples
    ///
    /// See [`Renames`].
    #[must_use]
    pub fn to_toml(&self) -> String {
        let mut sorted = self.clone();
        sorted.types.sort();
        sorted.fields.sort();
        toml::to_string(&sorted).unwrap_or_default()
    }

    /// The table with the renames `diff` found added.
    #[must_use]
    pub fn with(mut self, diff: &Diff) -> Self {
        for (name, changes) in &diff.changes {
            for change in changes {
                match change {
                    Change::Renamed { to } => self.types.push(TypeRename {
                        from: name.clone(),
                        to: to.clone(),
                    }),
                    Change::FieldRenamed { from, to } => self.fields.push(FieldRename {
                        ty: name.clone(),
                        from: from.clone(),
                        to: to.clone(),
                    }),
                    _ => {}
                }
            }
        }
        self.types.sort();
        self.types.dedup();
        self.fields.sort();
        self.fields.dedup();
        self
    }

    /// Every renamed type's old and new full names.
    pub(crate) fn types(&self) -> impl Iterator<Item = (&str, &str)> {
        self.types.iter().map(|r| (r.from.as_str(), r.to.as_str()))
    }

    fn field(&self, ty: &str, from: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|r| r.ty == ty && r.from == from)
            .map(|r| r.to.as_str())
    }
}

/// Every change between an old and a new build's catalogues, by the old
/// full name (the new one for an added type), for the report `cargo
/// enshwick pin` prints.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::typed::{Catalogue, Change, Renames};
///
/// let old = Catalogue::from_snapshot(r#"{"name":"demo::A","kind":18,"size":4,"align":4}"#)?;
/// let new = Catalogue::from_snapshot(r#"{"name":"demo::B","kind":18,"size":4,"align":4}"#)?;
/// let diff = old.diff(&new, &Renames::default());
/// assert_eq!(diff.changes["demo::A"], [Change::Renamed { to: "demo::B".into() }]);
/// assert!(!diff.needs_person());
/// # Ok::<(), enshwick_gamedata::typed::SnapshotError>(())
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Diff {
    /// The changes of each type that changed.
    pub changes: BTreeMap<String, Vec<Change>>,
}

impl Diff {
    /// Whether any change needs a person.
    ///
    /// # Examples
    ///
    /// See [`Diff`].
    #[must_use]
    pub fn needs_person(&self) -> bool {
        self.changes.values().flatten().any(Change::needs_person)
    }

    fn count(&self, pick: impl Fn(&Change) -> bool) -> usize {
        self.changes.values().flatten().filter(|c| pick(c)).count()
    }
}

/// Whether `old` and `new` are one layout under two names.
fn same_layout(old: &Ty, new: &Ty) -> bool {
    old.kind == new.kind
        && old.size == new.size
        && old.align == new.align
        && old.base == new.base
        && old.inner == new.inner
        && old.count == new.count
        && old.fields == new.fields
        && old.values == new.values
}

impl Catalogue {
    /// Every change from this build's catalogue to `new`'s, taking the
    /// renames `known` records and those an exact layout match decides.
    ///
    /// # Examples
    ///
    /// See [`Diff`].
    #[must_use]
    pub fn diff(&self, new: &Self, known: &Renames) -> Diff {
        let mut changes: BTreeMap<String, Vec<Change>> = BTreeMap::new();
        let removed: Vec<&Ty> = self
            .types
            .values()
            .filter(|t| !new.types.contains_key(&t.name))
            .collect();
        let added: BTreeSet<&str> = new
            .types
            .keys()
            .filter(|n| !self.types.contains_key(*n))
            .map(String::as_str)
            .collect();
        let mut renamed_to = BTreeSet::new();
        for old in &removed {
            let recorded = known
                .types()
                .find(|(from, _)| *from == old.name)
                .map(|(_, to)| to)
                .filter(|to| added.contains(to));
            let candidates: Vec<&str> = recorded.map_or_else(
                || {
                    added
                        .iter()
                        .copied()
                        .filter(|a| new.types.get(*a).is_some_and(|n| same_layout(old, n)))
                        .collect()
                },
                |to| vec![to],
            );
            let matched_back = |to: &str| {
                removed
                    .iter()
                    .filter(|r| new.types.get(to).is_some_and(|n| same_layout(r, n)))
                    .count()
                    == 1
            };
            let entry = changes.entry(old.name.clone()).or_default();
            match candidates.as_slice() {
                [] => entry.push(Change::Removed),
                [to] if recorded.is_some() || matched_back(to) => {
                    renamed_to.insert(*to);
                    entry.push(Change::Renamed {
                        to: (*to).to_owned(),
                    });
                    // A recorded rename may change the layout too.
                    if let Some(now) = new.types.get(*to) {
                        entry.extend(changed(old, now, known));
                    }
                }
                many => entry.push(Change::Unsure {
                    candidates: many.iter().map(|c| (*c).to_owned()).collect(),
                }),
            }
        }
        for name in added.iter().filter(|n| !renamed_to.contains(*n)) {
            changes
                .entry((*name).to_owned())
                .or_default()
                .push(Change::Added);
        }
        for (name, old) in &self.types {
            let Some(now) = new.types.get(name) else {
                continue;
            };
            let found = changed(old, now, known);
            if !found.is_empty() {
                changes.entry(name.clone()).or_default().extend(found);
            }
        }
        Diff { changes }
    }
}

/// The changes of one type both builds have.
fn changed(old: &Ty, new: &Ty, known: &Renames) -> Vec<Change> {
    let mut out = Vec::new();
    if old.kind != new.kind || old.size != new.size {
        out.push(Change::Retyped {
            what: format!(
                "kind {} -> {}, size {} -> {}",
                old.kind.byte(),
                new.kind.byte(),
                old.size,
                new.size
            ),
        });
    }
    let by_name = |fields: &[Field]| -> BTreeMap<String, Field> {
        fields.iter().map(|f| (f.name.clone(), f.clone())).collect()
    };
    let (before, after) = (by_name(&old.fields), by_name(&new.fields));
    let mut gone: Vec<&Field> = before
        .values()
        .filter(|f| !after.contains_key(&f.name))
        .collect();
    let mut fresh: Vec<&Field> = after
        .values()
        .filter(|f| !before.contains_key(&f.name))
        .collect();
    gone.retain(|g| {
        let recorded = known.field(&old.name, &g.name);
        let partners: Vec<usize> = fresh
            .iter()
            .enumerate()
            .filter(|(_, f)| {
                recorded.map_or(f.offset == g.offset && f.ty == g.ty, |to| f.name == to)
            })
            .map(|(i, _)| i)
            .collect();
        let unique_back = |f: &Field| {
            before
                .values()
                .filter(|b| !after.contains_key(&b.name) && b.offset == f.offset && b.ty == f.ty)
                .count()
                == 1
        };
        match partners.as_slice() {
            [i] if recorded.is_some() || unique_back(fresh[*i]) => {
                let to = fresh.remove(*i);
                out.push(Change::FieldRenamed {
                    from: g.name.clone(),
                    to: to.name.clone(),
                });
                false
            }
            _ => true,
        }
    });
    out.extend(gone.iter().map(|f| Change::FieldRemoved(f.name.clone())));
    out.extend(fresh.iter().map(|f| Change::FieldAdded(f.name.clone())));
    for (name, was) in &before {
        let Some(now) = after.get(name) else {
            continue;
        };
        if was.ty != now.ty {
            out.push(Change::FieldRetyped {
                field: name.clone(),
                from: was.ty.clone().unwrap_or_default(),
                to: now.ty.clone().unwrap_or_default(),
            });
        } else if was.offset != now.offset {
            out.push(Change::FieldMoved {
                field: name.clone(),
                from: was.offset,
                to: now.offset,
            });
        }
    }
    let (values, now): (BTreeMap<&str, u64>, BTreeMap<&str, u64>) = (
        old.values.iter().map(|(n, v)| (n.as_str(), *v)).collect(),
        new.values.iter().map(|(n, v)| (n.as_str(), *v)).collect(),
    );
    for (name, value) in &values {
        match now.get(name) {
            None => out.push(Change::ConstantRemoved((*name).to_owned())),
            Some(to) if to != value => out.push(Change::ConstantRenumbered {
                name: (*name).to_owned(),
                from: *value,
                to: *to,
            }),
            Some(_) => {}
        }
    }
    out.extend(
        now.keys()
            .filter(|n| !values.contains_key(*n))
            .map(|n| Change::ConstantAdded((*n).to_owned())),
    );
    out
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Added => f.write_str("added"),
            Self::Removed => f.write_str("removed: its item turns off on the new build"),
            Self::Renamed { to } => write!(f, "renamed to {to}"),
            Self::Unsure { candidates } => write!(
                f,
                "NEEDS A PERSON: removed, with the layout of {}; name the rename in the table",
                candidates.join(", ")
            ),
            Self::Retyped { what } => write!(f, "NEEDS A PERSON: {what}"),
            Self::FieldAdded(name) => write!(f, "field {name} added"),
            Self::FieldRemoved(name) => write!(f, "field {name} removed"),
            Self::FieldRenamed { from, to } => write!(f, "field {from} renamed to {to}"),
            Self::FieldRetyped { field, from, to } => {
                write!(f, "field {field} retyped from {from} to {to}")
            }
            Self::FieldMoved { field, from, to } => {
                write!(f, "field {field} moved from {from:#x} to {to:#x}")
            }
            Self::ConstantAdded(name) => write!(f, "constant {name} added"),
            Self::ConstantRemoved(name) => write!(f, "constant {name} removed"),
            Self::ConstantRenumbered { name, from, to } => {
                write!(f, "constant {name} renumbered from {from} to {to}")
            }
        }
    }
}

impl fmt::Display for Diff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let types = |pick: fn(&Change) -> bool| self.count(pick);
        let mut head = String::new();
        let _ = write!(
            head,
            "types: {} added, {} removed, {} renamed, {} retyped, {} unsure; \
             fields: {} added, {} removed, {} renamed, {} retyped, {} moved; \
             constants: {} added, {} removed, {} renumbered",
            types(|c| matches!(c, Change::Added)),
            types(|c| matches!(c, Change::Removed)),
            types(|c| matches!(c, Change::Renamed { .. })),
            types(|c| matches!(c, Change::Retyped { .. })),
            types(|c| matches!(c, Change::Unsure { .. })),
            types(|c| matches!(c, Change::FieldAdded(_))),
            types(|c| matches!(c, Change::FieldRemoved(_))),
            types(|c| matches!(c, Change::FieldRenamed { .. })),
            types(|c| matches!(c, Change::FieldRetyped { .. })),
            types(|c| matches!(c, Change::FieldMoved { .. })),
            types(|c| matches!(c, Change::ConstantAdded(_))),
            types(|c| matches!(c, Change::ConstantRemoved(_))),
            types(|c| matches!(c, Change::ConstantRenumbered { .. })),
        );
        writeln!(f, "{head}")?;
        for (name, changes) in &self.changes {
            for change in changes {
                writeln!(f, "{name}: {change}")?;
            }
        }
        Ok(())
    }
}
