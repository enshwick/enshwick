//! Changes of vanilla values that mods and data packs declare at init.
//!
//! The loader merges them per entry and field in load order, reports every
//! set another mod overrides, and works out each value from the changes of
//! the mods still on when the game first reads the resource.

use rustc_hash::FxHashMap;

use enshwick::content::Guid;
use enshwick::content::changes::{Change, Field, Value};

use crate::mod_index::ModIndex;

#[cfg(windows)]
mod game;
#[cfg(windows)]
pub(crate) use game::{declare, serve, wanted, world_event};

/// A change one mod declared, with where it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct Declared {
    /// The mod.
    pub owner: ModIndex,
    /// The change.
    pub change: Change,
    /// Its source, such as `items.toml:12`; may be empty.
    pub source: String,
}

/// A set that a later mod's set of the same field overrides.
#[derive(Clone, Debug, PartialEq)]
pub struct Conflict {
    /// The entry and the field.
    pub entry: Guid,
    /// The field.
    pub field: Field,
    /// The set in force.
    pub winner: Declared,
    /// The set it overrides.
    pub overridden: Declared,
    /// Whether the winner requires the overridden mod, so it overrides it on
    /// purpose.
    pub intended: bool,
}

impl Conflict {
    /// The line the logs and the conflict report carry, with mods named by
    /// `name`.
    #[must_use]
    pub fn describe(&self, name: impl Fn(ModIndex) -> String) -> String {
        let at = |d: &Declared| {
            if d.source.is_empty() {
                name(d.owner)
            } else {
                format!("{} {}", name(d.owner), d.source)
            }
        };
        format!(
            "{} of {} is {} from {}, which overrides {} from {}",
            self.field,
            self.entry,
            self.winner.change.value,
            at(&self.winner),
            self.overridden.change.value,
            at(&self.overridden),
        )
    }
}

/// Every declared change, by entry and field, in load order.
#[derive(Debug, Default)]
pub struct Merged {
    values: FxHashMap<(Guid, Field), Vec<(ModIndex, Value)>>,
}

impl Merged {
    /// The value of `field` of `entry` from `base` and the changes of the
    /// mods `on` lets through, as [`Field::resolve`] works it out; `None`
    /// when no such mod changes it.
    #[must_use]
    pub fn value(
        &self,
        entry: Guid,
        field: Field,
        base: f64,
        on: impl Fn(ModIndex) -> bool,
    ) -> Option<f64> {
        let values = self.values.get(&(entry, field))?;
        let mut live = values.iter().filter(|(owner, _)| on(*owner)).peekable();
        live.peek()?;
        Some(field.resolve(base, live.map(|(_, value)| *value)))
    }

    /// The mods that change `field` of `entry` and that `on` lets through.
    pub fn owners(
        &self,
        entry: Guid,
        field: Field,
        on: impl Fn(ModIndex) -> bool,
    ) -> Vec<ModIndex> {
        let mut owners: Vec<ModIndex> = self
            .values
            .get(&(entry, field))
            .into_iter()
            .flatten()
            .map(|(owner, _)| *owner)
            .filter(|owner| on(*owner))
            .collect();
        owners.dedup();
        owners
    }

    /// The entries whose `field` some mod changes, in no set order.
    pub fn entries(&self, field: Field) -> impl Iterator<Item = Guid> + '_ {
        self.values
            .keys()
            .filter(move |(_, f)| *f == field)
            .map(|(entry, _)| *entry)
    }

    /// The mods that change `field` of any entry.
    #[must_use]
    pub fn changers(&self, field: Field) -> Vec<ModIndex> {
        let mut owners: Vec<ModIndex> = self
            .values
            .iter()
            .filter(|((_, f), _)| *f == field)
            .flat_map(|(_, values)| values.iter().map(|(owner, _)| *owner))
            .collect();
        owners.sort_unstable();
        owners.dedup();
        owners
    }

    /// Whether any change was declared.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// Merges `declared`, in load order, and finds the sets later mods
/// override; `requires(a, b)` says whether mod `a` requires mod `b`.
#[must_use]
pub fn merge(
    declared: &[Declared],
    requires: impl Fn(ModIndex, ModIndex) -> bool,
) -> (Merged, Vec<Conflict>) {
    let mut merged = Merged::default();
    let mut sets: FxHashMap<(Guid, Field), Vec<&Declared>> = FxHashMap::default();
    for d in declared {
        let key = (d.change.entry, d.change.field);
        merged
            .values
            .entry(key)
            .or_default()
            .push((d.owner, d.change.value));
        if matches!(d.change.value, Value::Set(_)) {
            sets.entry(key).or_default().push(d);
        }
    }
    let mut conflicts = Vec::new();
    for ((entry, field), sets) in sets {
        let Some(&winner) = sets.last() else {
            continue;
        };
        for &overridden in sets.iter().filter(|d| d.owner != winner.owner) {
            conflicts.push(Conflict {
                entry,
                field,
                winner: winner.clone(),
                overridden: overridden.clone(),
                intended: requires(winner.owner, overridden.owner),
            });
        }
    }
    conflicts.sort_by(|a, b| {
        (a.winner.owner, &a.winner.source, a.entry.0).cmp(&(
            b.winner.owner,
            &b.winner.source,
            b.entry.0,
        ))
    });
    (merged, conflicts)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WOOD: Guid = Guid([1; 16]);
    const STONE: Guid = Guid([2; 16]);

    fn declared(owner: usize, entry: Guid, value: Value, source: &str) -> Declared {
        Declared {
            owner: ModIndex::new(owner),
            change: Change::new(entry, Field::MaxStack, value),
            source: source.to_owned(),
        }
    }

    fn no_requires(_: ModIndex, _: ModIndex) -> bool {
        false
    }

    #[test]
    fn a_later_set_wins_and_the_overridden_one_is_reported() {
        let all = [
            declared(0, WOOD, Value::Set(100.0), "items.toml:1"),
            declared(1, WOOD, Value::Set(500.0), "items.toml:4"),
        ];
        let (merged, conflicts) = merge(&all, no_requires);
        assert_eq!(
            merged.value(WOOD, Field::MaxStack, 20.0, |_| true),
            Some(500.0)
        );
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].winner.owner, ModIndex::new(1));
        assert_eq!(conflicts[0].overridden.owner, ModIndex::new(0));
        assert!(!conflicts[0].intended);
        let line = conflicts[0].describe(|m| format!("mod{}", m.get()));
        assert!(
            line.contains("500 from mod1 items.toml:4, which overrides 100 from mod0 items.toml:1"),
            "{line}"
        );
    }

    #[test]
    fn factors_multiply_without_a_conflict() {
        let all = [
            declared(0, WOOD, Value::Times(2.0), ""),
            declared(1, WOOD, Value::Times(3.0), ""),
        ];
        let (merged, conflicts) = merge(&all, no_requires);
        assert_eq!(
            merged.value(WOOD, Field::MaxStack, 20.0, |_| true),
            Some(120.0)
        );
        assert!(conflicts.is_empty());
    }

    #[test]
    fn a_mod_that_requires_the_other_overrides_it_on_purpose() {
        let all = [
            declared(0, WOOD, Value::Set(100.0), ""),
            declared(1, WOOD, Value::Set(500.0), ""),
        ];
        let (_, conflicts) = merge(&all, |a, b| a == ModIndex::new(1) && b == ModIndex::new(0));
        assert!(conflicts[0].intended);
    }

    #[test]
    fn a_mod_that_is_off_drops_out_of_the_value() {
        let all = [
            declared(0, WOOD, Value::Set(100.0), ""),
            declared(1, WOOD, Value::Set(500.0), ""),
            declared(2, WOOD, Value::Times(2.0), ""),
        ];
        let (merged, _) = merge(&all, no_requires);
        let on = |m: ModIndex| m != ModIndex::new(1);
        assert_eq!(merged.value(WOOD, Field::MaxStack, 20.0, on), Some(200.0));
        assert_eq!(
            merged.owners(WOOD, Field::MaxStack, on),
            [ModIndex::new(0), ModIndex::new(2)]
        );
        assert_eq!(merged.value(WOOD, Field::MaxStack, 20.0, |_| false), None);
    }

    #[test]
    fn an_entry_nobody_changes_has_no_value() {
        let (merged, _) = merge(&[declared(0, WOOD, Value::Set(5.0), "")], no_requires);
        assert_eq!(merged.value(STONE, Field::MaxStack, 20.0, |_| true), None);
        assert_eq!(merged.entries(Field::MaxStack).collect::<Vec<_>>(), [WOOD]);
    }

    #[test]
    fn one_mod_setting_twice_is_no_conflict() {
        let all = [
            declared(0, WOOD, Value::Set(100.0), ""),
            declared(0, WOOD, Value::Set(200.0), ""),
        ];
        let (merged, conflicts) = merge(&all, no_requires);
        assert!(conflicts.is_empty());
        assert_eq!(
            merged.value(WOOD, Field::MaxStack, 20.0, |_| true),
            Some(200.0)
        );
    }
}
