//! Changes of vanilla values that mods and data packs declare at init.
//!
//! The loader merges them per entry and field in load order, reports every
//! set another mod overrides, and works out each value from the changes of
//! the mods still on when the game first reads the resource.

use std::collections::BTreeSet;

use rustc_hash::FxHashMap;

use enshwick::content::Guid;
use enshwick::content::changes::{Change, Field, Value};

use crate::mod_index::ModIndex;

#[cfg(windows)]
mod game;
#[cfg(windows)]
pub(crate) use game::{declare, reachable, serve, wanted, world_event};

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

/// What one change changes: the entry, the field and the item inside the
/// entry where the field names one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    /// The entry.
    pub entry: Guid,
    /// The field.
    pub field: Field,
    /// The item inside the entry, such as a recipe's ingredient.
    pub item: Option<Guid>,
}

impl Key {
    /// The key of `field` of `entry`, with no item.
    #[must_use]
    pub const fn new(entry: Guid, field: Field) -> Self {
        Self {
            entry,
            field,
            item: None,
        }
    }

    /// The key `change` changes.
    #[must_use]
    pub const fn of(change: &Change) -> Self {
        Self {
            entry: change.entry,
            field: change.field,
            item: change.item,
        }
    }
}

/// A set that a later mod's set of the same field overrides.
#[derive(Clone, Debug, PartialEq)]
pub struct Conflict {
    /// What both sets change.
    pub key: Key,
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
        let what = match self.key.item {
            Some(item) => format!("{} {item} of {}", self.key.field, self.key.entry),
            None => format!("{} of {}", self.key.field, self.key.entry),
        };
        format!(
            "{what} is {} from {}, which overrides {} from {}",
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
    values: FxHashMap<Key, Vec<(ModIndex, Value)>>,
}

impl Merged {
    /// The value `key` names, from `base` and the changes of the mods `on`
    /// lets through, as [`Field::resolve`] works it out; `None` when no such
    /// mod changes it.
    #[must_use]
    pub fn value(&self, key: Key, base: f64, on: impl Fn(ModIndex) -> bool) -> Option<f64> {
        let values = self.values.get(&key)?;
        let mut live = values.iter().filter(|(owner, _)| on(*owner)).peekable();
        live.peek()?;
        Some(key.field.resolve(base, live.map(|(_, value)| *value)))
    }

    /// The mods that change what `key` names and that `on` lets through.
    pub fn owners(&self, key: Key, on: impl Fn(ModIndex) -> bool) -> Vec<ModIndex> {
        let mut owners: Vec<ModIndex> = self
            .values
            .get(&key)
            .into_iter()
            .flatten()
            .map(|(owner, _)| *owner)
            .filter(|owner| on(*owner))
            .collect();
        owners.dedup();
        owners
    }

    /// The entries some mod changes with a field of kind `kind`, each
    /// once, in no set order.
    #[must_use]
    pub fn entries(&self, kind: enshwick::content::changes::Entry) -> Vec<Guid> {
        let mut entries: Vec<Guid> = self
            .values
            .keys()
            .filter(|key| key.field.entry() == kind)
            .map(|key| key.entry)
            .collect();
        entries.sort_unstable_by_key(|guid| guid.0);
        entries.dedup();
        entries
    }

    /// Every key of `entry` some mod changes, in no set order.
    #[must_use]
    pub fn keys(&self, entry: Guid) -> Vec<Key> {
        self.values
            .keys()
            .filter(|key| key.entry == entry)
            .copied()
            .collect()
    }

    /// The mods that change `field` of any entry.
    #[must_use]
    pub fn changers(&self, field: Field) -> Vec<ModIndex> {
        let mut owners: Vec<ModIndex> = self
            .values
            .iter()
            .filter(|(key, _)| key.field == field)
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
    let mut sets: FxHashMap<Key, Vec<&Declared>> = FxHashMap::default();
    for d in declared {
        let key = Key::of(&d.change);
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
    for (key, sets) in sets {
        let Some(&winner) = sets.last() else {
            continue;
        };
        for &overridden in sets.iter().filter(|d| d.owner != winner.owner) {
            conflicts.push(Conflict {
                key,
                winner: winner.clone(),
                overridden: overridden.clone(),
                intended: requires(winner.owner, overridden.owner),
            });
        }
    }
    conflicts.sort_by(|a, b| {
        (a.winner.owner, &a.winner.source, a.key.entry.0).cmp(&(
            b.winner.owner,
            &b.winner.source,
            b.key.entry.0,
        ))
    });
    (merged, conflicts)
}

/// The stack sizes one pack raised past the game's: the items, and the
/// first one's label with the game's and the new size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Raised {
    items: BTreeSet<[u8; 16]>,
    first: (String, u16, u16),
}

impl Raised {
    /// The first raise: `item`, named `label`, from `old` to `new`.
    #[must_use]
    pub fn new(item: Guid, label: String, old: u16, new: u16) -> Self {
        Self {
            items: BTreeSet::from([item.0]),
            first: (label, old, new),
        }
    }

    /// Notes a raise of `item`; one item counts once however often the game
    /// builds it.
    pub fn note(&mut self, item: Guid) {
        self.items.insert(item.0);
    }

    /// The hold's reason: one line naming the first item and how many more.
    #[must_use]
    pub fn reason(&self) -> String {
        let (label, old, new) = &self.first;
        let more = match self.items.len() {
            0 | 1 => String::new(),
            n => format!(" and {} more items", n - 1),
        };
        format!(
            "it lets {label}{more} stack past the game's own size ({old} to {new}); without it the game deletes the excess of every larger stack in this world, so split such stacks to the game's size before you remove it"
        )
    }
}

/// The owners in `owners` whose changes raise a value from `old` to `new`.
///
/// Those are the owners without whom it would come out lower, as `without`
/// works it out; when none is, every owner whose changes alone raise it, as
/// `alone` works it out, so two packs that raise one value both hold.
#[must_use]
pub fn credited(
    owners: &[ModIndex],
    old: f64,
    new: f64,
    without: impl Fn(ModIndex) -> f64,
    alone: impl Fn(ModIndex) -> f64,
) -> Vec<ModIndex> {
    let needed: Vec<ModIndex> = owners
        .iter()
        .copied()
        .filter(|&owner| without(owner) < new)
        .collect();
    if !needed.is_empty() || new <= old {
        return needed;
    }
    owners
        .iter()
        .copied()
        .filter(|&owner| alone(owner) > old)
        .collect()
}

/// What happens to a mod's hold when a world loads on the side that keeps
/// it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HoldStep {
    /// Set it with this reason.
    Set(String),
    /// Release it.
    Release,
    /// Leave it: a mod with code keeps its own hold.
    Keep,
}

/// The step for a mod that is a pack or not and raised `raised`: the loader
/// holds and releases for packs alone, which have no code to keep a hold.
#[must_use]
pub fn hold_step(is_pack: bool, raised: Option<&Raised>) -> HoldStep {
    match (is_pack, raised) {
        (false, _) => HoldStep::Keep,
        (true, Some(raised)) => HoldStep::Set(raised.reason()),
        (true, None) => HoldStep::Release,
    }
}

/// Whether the loader's patch of a changed value runs before the mods'
/// patches of `key`, served from `base`.
///
/// It goes first on a vanilla resource, so mods patch the changed value, and
/// last on a mod's new resource, after the mod built it.
#[must_use]
pub fn goes_first(key: &enshwick::abi::ResourceKey, base: &enshwick::abi::ResourceKey) -> bool {
    key == base
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
            merged.value(Key::new(WOOD, Field::MaxStack), 20.0, |_| true),
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
            merged.value(Key::new(WOOD, Field::MaxStack), 20.0, |_| true),
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
        assert_eq!(
            merged.value(Key::new(WOOD, Field::MaxStack), 20.0, on),
            Some(200.0)
        );
        assert_eq!(
            merged.owners(Key::new(WOOD, Field::MaxStack), on),
            [ModIndex::new(0), ModIndex::new(2)]
        );
        assert_eq!(
            merged.value(Key::new(WOOD, Field::MaxStack), 20.0, |_| false),
            None
        );
    }

    #[test]
    fn an_entry_nobody_changes_has_no_value() {
        let (merged, _) = merge(&[declared(0, WOOD, Value::Set(5.0), "")], no_requires);
        assert_eq!(
            merged.value(Key::new(STONE, Field::MaxStack), 20.0, |_| true),
            None
        );
        assert_eq!(
            merged.entries(enshwick::content::changes::Entry::Item),
            [WOOD]
        );
    }

    #[test]
    fn a_pack_that_only_lowers_a_raised_value_is_not_credited() {
        let all = [
            declared(0, WOOD, Value::Set(999.0), ""),
            declared(1, WOOD, Value::Times(0.5), ""),
        ];
        let (merged, _) = merge(&all, no_requires);
        let new = merged
            .value(Key::new(WOOD, Field::MaxStack), 20.0, |_| true)
            .unwrap();
        let owners = merged.owners(Key::new(WOOD, Field::MaxStack), |_| true);
        let (without, alone) = sides(&merged);
        assert_eq!(
            credited(&owners, 20.0, new, without, alone),
            [ModIndex::new(0)]
        );
    }

    fn sides(merged: &Merged) -> (impl Fn(ModIndex) -> f64 + '_, impl Fn(ModIndex) -> f64 + '_) {
        let without = |m: ModIndex| {
            merged
                .value(Key::new(WOOD, Field::MaxStack), 20.0, |o| o != m)
                .unwrap_or(20.0)
        };
        let alone = |m: ModIndex| {
            merged
                .value(Key::new(WOOD, Field::MaxStack), 20.0, |o| o == m)
                .unwrap_or(20.0)
        };
        (without, alone)
    }

    #[test]
    fn two_packs_that_each_raise_a_value_are_both_credited() {
        for later in [999.0, 500.0] {
            let all = [
                declared(0, WOOD, Value::Set(999.0), ""),
                declared(1, WOOD, Value::Set(later), ""),
            ];
            let (merged, _) = merge(&all, no_requires);
            let new = merged
                .value(Key::new(WOOD, Field::MaxStack), 20.0, |_| true)
                .unwrap();
            let owners = merged.owners(Key::new(WOOD, Field::MaxStack), |_| true);
            let (without, alone) = sides(&merged);
            assert_eq!(
                credited(&owners, 20.0, new, without, alone),
                [ModIndex::new(0), ModIndex::new(1)],
                "later set {later}"
            );
        }
    }

    #[test]
    fn an_item_built_again_counts_once_in_the_reason() {
        let mut raised = Raised::new(WOOD, "Wood_Log".to_owned(), 20, 999);
        raised.note(WOOD);
        raised.note(STONE);
        raised.note(STONE);
        let reason = raised.reason();
        assert!(
            reason.starts_with(
                "it lets Wood_Log and 1 more items stack past the game's own size (20 to 999)"
            ),
            "{reason}"
        );
        assert!(!reason.contains('\n'));
    }

    #[test]
    fn a_pack_without_raises_releases_its_hold_and_a_mod_keeps_its_own() {
        let raised = Raised::new(WOOD, "Wood_Log".to_owned(), 20, 999);
        assert!(matches!(hold_step(true, Some(&raised)), HoldStep::Set(_)));
        assert_eq!(hold_step(true, None), HoldStep::Release);
        assert_eq!(hold_step(false, Some(&raised)), HoldStep::Keep);
        assert_eq!(hold_step(false, None), HoldStep::Keep);
    }

    #[test]
    fn the_loader_s_patch_goes_first_on_a_vanilla_key_and_last_on_a_new_one() {
        let key = |b: u8| enshwick::abi::ResourceKey {
            guid: [b; 16],
            type_hash: 1,
            part: 0,
        };
        assert!(goes_first(&key(1), &key(1)));
        assert!(!goes_first(&key(2), &key(1)));
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
            merged.value(Key::new(WOOD, Field::MaxStack), 20.0, |_| true),
            Some(200.0)
        );
    }
}
