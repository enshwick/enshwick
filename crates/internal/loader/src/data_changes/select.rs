//! Changes of every vanilla item a selection picks, turned into one change
//! per item from the game's data when the loader seals the changes, and
//! the summary of the conflicts many items share.

use enshwick::abi::ChangeId;
use enshwick::content::Guid;
use enshwick::content::changes::{Field, ItemTraits, ItemsChange, RecipeTraits, RecipesChange};
use rustc_hash::FxHashSet;

use super::{Conflict, Declared};
use crate::mod_index::ModIndex;

/// A change as a mod declared it: of one entry, or of every item a
/// selection picks.
#[derive(Clone, Debug, PartialEq)]
pub enum Pending {
    /// A change of one entry.
    One(Declared),
    /// A change of every vanilla item a selection picks.
    Each {
        /// The mod.
        owner: ModIndex,
        /// The change.
        change: ItemsChange,
        /// Its source; may be empty.
        source: String,
        /// The change a mod sets later it is; `None` for one whose value
        /// was given at init.
        live: Option<ChangeId>,
    },
    /// A change of every vanilla recipe a selection picks.
    Recipes {
        /// The mod.
        owner: ModIndex,
        /// The change.
        change: RecipesChange,
        /// Its source; may be empty.
        source: String,
    },
}

/// The vanilla items and recipes with what selections read of each, from
/// the game's data.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selectable {
    /// Every item.
    pub items: Vec<(Guid, ItemTraits)>,
    /// Every recipe.
    pub recipes: Vec<(Guid, RecipeTraits)>,
}

/// How many items one selection picked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picked {
    /// The mod.
    pub owner: ModIndex,
    /// The field it changes.
    pub field: Field,
    /// Its source.
    pub source: String,
    /// How many vanilla entries it reached.
    pub count: usize,
}

/// Every change of `pending` as changes of one entry each, in load order,
/// with how many entries each selection picked.
///
/// A selection becomes a change of each item or recipe of `vanilla` it
/// picks, in its own place, but for an entry the same mod changes by name,
/// which keeps that change of the field alone.
#[must_use]
pub fn expand(pending: Vec<Pending>, vanilla: &Selectable) -> (Vec<Declared>, Vec<Picked>) {
    let named: FxHashSet<(ModIndex, Guid, Field)> = pending
        .iter()
        .filter_map(|p| match p {
            Pending::One(one) => Some((one.owner, one.change.entry, one.change.field)),
            Pending::Each { .. } | Pending::Recipes { .. } => None,
        })
        .collect();
    let mut declared = Vec::new();
    let mut picked = Vec::new();
    for change in pending {
        let before = declared.len();
        let (owner, field, source) = match change {
            Pending::One(one) => {
                declared.push(one);
                continue;
            }
            Pending::Each {
                owner,
                change,
                source,
                live,
            } => {
                declared.extend(
                    vanilla
                        .items
                        .iter()
                        .filter(|(item, traits)| {
                            change.items.picks(change.field, traits)
                                && !named.contains(&(owner, *item, change.field))
                        })
                        .map(|(item, _)| Declared {
                            owner,
                            change: change.of(*item),
                            source: source.clone(),
                            live,
                        }),
                );
                (owner, change.field, source)
            }
            Pending::Recipes {
                owner,
                change,
                source,
            } => {
                declared.extend(
                    vanilla
                        .recipes
                        .iter()
                        .filter(|(recipe, traits)| {
                            change.recipes.picks(*traits)
                                && !named.contains(&(owner, *recipe, change.field))
                        })
                        .map(|(recipe, _)| Declared {
                            owner,
                            change: change.of(*recipe),
                            source: source.clone(),
                            live: None,
                        }),
                );
                (owner, change.field, source)
            }
        };
        picked.push(Picked {
            owner,
            field,
            source,
            count: declared.len() - before,
        });
    }
    (declared, picked)
}

/// Where the values a selection reads sit in an item's static data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraitOffsets {
    /// The stack size, a `u16`.
    pub max_stack: usize,
    /// The `ItemCategory`, a `u8`.
    pub category: usize,
    /// The flags, a `u32`.
    pub flags: usize,
    /// The GUID of the template the item places.
    pub placed: usize,
    /// The GUID of the template of the item's own entity, and where its
    /// equipment slot sits with the slot's `Backpack` value, when the build
    /// reflects them.
    pub backpack: Option<(usize, usize, u8)>,
}

/// What a selection reads of the item whose static data is `bytes`, with
/// `slots` answering how many slots a template's container holds; `None`
/// when the data is too short.
#[must_use]
pub fn traits_of(
    bytes: &[u8],
    at: TraitOffsets,
    slots: impl Fn(Guid) -> Option<u8>,
) -> Option<ItemTraits> {
    let guid = |offset: usize| -> Option<Guid> {
        let raw: [u8; 16] = bytes.get(offset..offset + 16)?.try_into().ok()?;
        (raw != [0; 16]).then_some(Guid(raw))
    };
    let max_stack = u16::from_le_bytes(bytes.get(at.max_stack..at.max_stack + 2)?.try_into().ok()?);
    let category = *bytes.get(at.category)?;
    let flags = u32::from_le_bytes(bytes.get(at.flags..at.flags + 4)?.try_into().ok()?);
    bytes.get(at.placed..at.placed + 16)?;
    let placed = guid(at.placed).and_then(&slots);
    let backpack_slots = at.backpack.and_then(|(pid, slot, backpack)| {
        (*bytes.get(slot)? == backpack)
            .then(|| guid(pid))
            .flatten()
            .and_then(&slots)
    });
    Some(ItemTraits {
        max_stack,
        category,
        flags,
        slots: placed,
        backpack_slots,
    })
}

/// One line for each group of conflicts that differ only in their entry,
/// as two selections that overlap make: the first conflict of the group and
/// how many more the group holds, in the order of `conflicts`.
#[must_use]
pub fn grouped(conflicts: &[Conflict]) -> Vec<(&Conflict, usize)> {
    let mut groups: Vec<(&Conflict, usize)> = Vec::new();
    for conflict in conflicts {
        let same = |first: &Conflict| {
            first.key.field == conflict.key.field
                && first.key.item == conflict.key.item
                && first.intended == conflict.intended
                && same_source(&first.winner, &conflict.winner)
                && same_source(&first.overridden, &conflict.overridden)
        };
        match groups.iter_mut().find(|(first, _)| same(first)) {
            Some((_, more)) => *more += 1,
            None => groups.push((conflict, 0)),
        }
    }
    groups
}

/// Whether two changes come from one mod's one line with one value.
fn same_source(a: &Declared, b: &Declared) -> bool {
    a.owner == b.owner && a.source == b.source && a.change.value == b.change.value
}

#[cfg(test)]
mod tests {
    use enshwick::abi::ItemData;
    use enshwick::content::categories::ItemCategoryMask;
    use enshwick::content::changes::{Change, Items, Recipes, Value};
    use enshwick::content::recipes::RecipeKind;
    use enshwick::content::workshops::WorkshopId;

    use super::*;
    use crate::data_changes::merge;

    fn guid(n: u8) -> Guid {
        Guid([n; 16])
    }

    fn items() -> Vec<(Guid, ItemTraits)> {
        let stack = |max_stack, category| ItemTraits {
            max_stack,
            category,
            flags: 0,
            slots: None,
            backpack_slots: None,
        };
        vec![
            (guid(1), stack(20, 9)),
            (guid(2), stack(50, 8)),
            (
                guid(3),
                ItemTraits {
                    flags: ItemData::HAS_PIDE,
                    ..stack(1, 2)
                },
            ),
            (
                guid(4),
                ItemTraits {
                    slots: Some(24),
                    ..stack(1, 6)
                },
            ),
        ]
    }

    fn selectable() -> Selectable {
        Selectable {
            items: items(),
            recipes: vec![
                (
                    guid(10),
                    RecipeTraits {
                        kind: RecipeKind::Roof,
                        workshop: 0,
                    },
                ),
                (
                    guid(11),
                    RecipeTraits {
                        kind: RecipeKind::Other,
                        workshop: KILN.id(),
                    },
                ),
                (
                    guid(12),
                    RecipeTraits {
                        kind: RecipeKind::Roof,
                        workshop: KILN.id(),
                    },
                ),
            ],
        }
    }

    const KILN: WorkshopId = WorkshopId::new(Guid([7; 16]));

    fn recipes(owner: usize, recipes: Recipes, value: Value) -> Pending {
        Pending::Recipes {
            owner: ModIndex::new(owner),
            change: RecipesChange::new(recipes, Field::Cost, value),
            source: format!("mod {owner}"),
        }
    }

    #[test]
    fn a_recipe_selection_picks_by_kind_or_workshop_and_keeps_named_ones() {
        let named = Pending::One(Declared {
            owner: ModIndex::new(0),
            change: Change::new(guid(12), Field::Cost, Value::Set(9.0)),
            source: "recipes.toml:4".to_owned(),
            live: None,
        });
        let pending = vec![
            recipes(0, Recipes::Kind(RecipeKind::Roof), Value::Times(0.5)),
            named,
            recipes(1, Recipes::Workshop(KILN), Value::Times(2.0)),
        ];
        let (declared, picked) = expand(pending, &selectable());
        let order: Vec<(usize, Guid)> = declared
            .iter()
            .map(|d| (d.owner.get(), d.change.entry))
            .collect();
        assert_eq!(
            order,
            vec![(0, guid(10)), (0, guid(12)), (1, guid(11)), (1, guid(12))]
        );
        assert_eq!(declared[1].change.value, Value::Set(9.0));
        assert_eq!(picked.iter().map(|p| p.count).collect::<Vec<_>>(), [1, 2]);
    }

    fn one(owner: usize, entry: Guid, value: Value) -> Pending {
        Pending::One(Declared {
            owner: ModIndex::new(owner),
            change: Change::new(entry, Field::MaxStackSize, value),
            source: "items.toml:2".to_owned(),
            live: None,
        })
    }

    fn each(owner: usize, items: Items, field: Field, value: Value) -> Pending {
        Pending::Each {
            owner: ModIndex::new(owner),
            change: ItemsChange::new(items, field, value),
            source: format!("mod {owner}"),
            live: None,
        }
    }

    #[test]
    fn a_selection_becomes_one_change_per_item_in_its_place() {
        let pending = vec![
            one(0, guid(1), Value::Set(30.0)),
            each(1, Items::Stackable, Field::MaxStackSize, Value::Times(2.0)),
            one(2, guid(2), Value::Set(10.0)),
        ];
        let (declared, picked) = expand(pending, &selectable());
        let order: Vec<(usize, Guid)> = declared
            .iter()
            .map(|d| (d.owner.get(), d.change.entry))
            .collect();
        assert_eq!(
            order,
            vec![(0, guid(1)), (1, guid(1)), (1, guid(2)), (2, guid(2))]
        );
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].count, 2);
    }

    #[test]
    fn an_item_a_mod_names_itself_keeps_that_change_over_its_selection() {
        let pending = vec![
            each(0, Items::Stackable, Field::MaxStackSize, Value::Times(4.0)),
            one(0, guid(1), Value::Set(999.0)),
            one(1, guid(2), Value::Set(10.0)),
        ];
        let (declared, picked) = expand(pending, &selectable());
        let order: Vec<(usize, Guid, Value)> = declared
            .iter()
            .map(|d| (d.owner.get(), d.change.entry, d.change.value))
            .collect();
        assert_eq!(
            order,
            vec![
                (0, guid(2), Value::Times(4.0)),
                (0, guid(1), Value::Set(999.0)),
                (1, guid(2), Value::Set(10.0)),
            ]
        );
        assert_eq!(picked[0].count, 1);
    }

    #[test]
    fn categories_and_containers_pick_from_the_game_s_data() {
        let pending = vec![
            each(
                0,
                Items::Category(ItemCategoryMask::MATERIALS),
                Field::MaxStackSize,
                Value::Set(999.0),
            ),
            each(1, Items::Containers, Field::Slots, Value::Set(48.0)),
        ];
        let (declared, picked) = expand(pending, &selectable());
        assert_eq!(declared.len(), 2);
        assert_eq!(declared[0].change.entry, guid(1));
        assert_eq!(declared[1].change.entry, guid(4));
        assert!(picked.iter().all(|p| p.count == 1));
    }

    #[test]
    fn a_selection_that_picks_nothing_is_counted_as_none() {
        let pending = vec![each(
            0,
            Items::Category(ItemCategoryMask::CURRENCY),
            Field::MaxStackSize,
            Value::Set(999.0),
        )];
        let (declared, picked) = expand(pending, &selectable());
        assert!(declared.is_empty());
        assert_eq!(picked[0].count, 0);
    }

    #[test]
    fn an_item_s_traits_read_from_its_data() {
        let at = TraitOffsets {
            max_stack: 0x14,
            category: 0xcc,
            flags: 0x5d8,
            placed: 0x600,
            backpack: Some((0x540, 0x3d0, 23)),
        };
        let mut bytes = vec![0u8; 0x620];
        bytes[0x14..0x16].copy_from_slice(&999u16.to_le_bytes());
        bytes[0xcc] = 9;
        bytes[0x5d8..0x5dc].copy_from_slice(&ItemData::HAS_PIDE.to_le_bytes());
        let traits = traits_of(&bytes, at, |_| Some(24)).unwrap();
        assert_eq!(
            (
                traits.max_stack,
                traits.category,
                traits.flags,
                traits.slots
            ),
            (999, 9, ItemData::HAS_PIDE, None)
        );
        bytes[0x600..0x610].copy_from_slice(&[7; 16]);
        let chest = traits_of(&bytes, at, |t| (t == guid(7)).then_some(24)).unwrap();
        assert_eq!(chest.slots, Some(24));
        assert_eq!(chest.backpack_slots, None);
        assert_eq!(traits_of(&bytes[..0x20], at, |_| None), None);
    }

    #[test]
    fn a_backpack_s_slots_come_from_its_own_entity() {
        let at = TraitOffsets {
            max_stack: 0x14,
            category: 0xcc,
            flags: 0x5d8,
            placed: 0x600,
            backpack: Some((0x540, 0x3d0, 23)),
        };
        let mut bytes = vec![0u8; 0x620];
        bytes[0x540..0x550].copy_from_slice(&[8; 16]);
        let slots = |t: Guid| (t == guid(8)).then_some(16);
        assert_eq!(traits_of(&bytes, at, slots).unwrap().backpack_slots, None);
        bytes[0x3d0] = 23;
        let backpack = traits_of(&bytes, at, slots).unwrap();
        assert_eq!((backpack.slots, backpack.backpack_slots), (None, Some(16)));
        let unreflected = TraitOffsets {
            backpack: None,
            ..at
        };
        assert_eq!(
            traits_of(&bytes, unreflected, slots)
                .unwrap()
                .backpack_slots,
            None
        );
    }

    #[test]
    fn overlapping_selections_report_one_line_per_pair_of_sources() {
        let counts = |pending: Vec<Pending>| {
            let (declared, _) = expand(pending, &selectable());
            let (_, conflicts) = merge(&declared, |_, _| false);
            grouped(&conflicts)
                .iter()
                .map(|(c, more)| (c.winner.owner.get(), c.overridden.owner.get(), *more))
                .collect::<Vec<_>>()
        };
        let both = vec![
            each(0, Items::Stackable, Field::MaxStackSize, Value::Set(100.0)),
            each(1, Items::Stackable, Field::MaxStackSize, Value::Set(999.0)),
        ];
        assert_eq!(counts(both.clone()), vec![(1, 0, 1)]);
        let mut three = both;
        three.push(one(2, guid(1), Value::Set(5.0)));
        assert_eq!(counts(three), vec![(1, 0, 0), (2, 0, 0), (2, 1, 0)]);
    }
}
