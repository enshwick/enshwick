//! The client's crafting menu: which recipes each workshop lists, in which
//! group and set, baked into the UI bundle.
//!
//! The menu does not walk the recipe registry. It walks a tree per workshop
//! (groups of sets of recipe ids) in the UI bundle, and a recipe whose id no
//! tree names never shows, whatever the registry holds. `Load::add_recipe`,
//! `Load::derive_recipes` and [`Load::add_workshop`] list what they add
//! here on the client; the dedicated server has no UI bundle.

use crate::abi::Side;
use crate::engine_ids as ids;
use crate::{Load, Off};

use super::workshops::WorkshopId;
use super::{Blob, EngineFacts, Guid, types};

/// The client's UI bundle resource, which holds the crafting menu.
pub const UI_BUNDLE: Guid = Guid::parse("46e14df6-fb8f-4f66-9463-e23b9e19e48e");

/// The most recipes one group of the crafting menu shows, all its sets
/// together; the game drops the rest without a word.
pub const GROUP_ENTRIES: usize = 512;

/// Where the fields of the crafting menu sit on the running build, resolved
/// by the loader.
///
/// # Examples
///
/// See [`MenuLayout::resolve`] and [`list_recipes`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuLayout {
    /// The recipe collection inside the UI bundle.
    pub collection: usize,
    /// The collection's workshop of each tree.
    pub workshops: usize,
    /// The collection's trees, one per workshop.
    pub trees: usize,
    /// The collection's most groups in one tree.
    pub max_groups: usize,
    /// The collection's most sets in one group.
    pub max_sets: usize,
    /// One tree's size.
    pub tree: usize,
    /// A tree's groups.
    pub groups: usize,
    /// One group's size.
    pub group: usize,
    /// A group's id.
    pub group_id: usize,
    /// A group's sets.
    pub sets: usize,
    /// One set's size.
    pub set: usize,
    /// A set's entries, recipe ids.
    pub entries: usize,
}

impl MenuLayout {
    /// The layout `facts` give; [`Off`] on the dedicated server, which has
    /// no crafting menu.
    ///
    /// # Errors
    ///
    /// The first [`Off`] a fact returns, when the build lacks it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::menu::MenuLayout;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let layout = MenuLayout::resolve(load)?;
    ///     enshwick::info!("crafting menu at {:#x}", layout.collection);
    ///     Ok(())
    /// }
    /// ```
    pub fn resolve(facts: &(impl EngineFacts + ?Sized)) -> Result<Self, Off> {
        let at = |id: &str| facts.offset(id);
        Ok(Self {
            collection: at(ids::UI_BUNDLE_CRAFTING_MENU)?,
            workshops: at(ids::CRAFTING_MENU_WORKSHOPS)?,
            trees: at(ids::CRAFTING_MENU_TREES)?,
            max_groups: at(ids::CRAFTING_MENU_MAX_GROUPS)?,
            max_sets: at(ids::CRAFTING_MENU_MAX_SETS)?,
            tree: facts.size(ids::CRAFTING_MENU_TREE)?,
            groups: at(ids::CRAFTING_MENU_TREE_GROUPS)?,
            group: facts.size(ids::CRAFTING_MENU_GROUP)?,
            group_id: at(ids::CRAFTING_MENU_GROUP_ID)?,
            sets: at(ids::CRAFTING_MENU_GROUP_SETS)?,
            set: facts.size(ids::CRAFTING_MENU_SET)?,
            entries: at(ids::CRAFTING_MENU_SET_ENTRIES)?,
        })
    }
}

/// A recipe to list in the crafting menu, for [`list_recipes`].
///
/// # Examples
///
/// See [`list_recipes`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuEntry {
    /// The recipe; its id is [`Guid::fnv1a`].
    pub recipe: Guid,
    /// The recipe it follows in the menu, such as the one it copies.
    pub after: Option<Guid>,
    /// The workshop whose menu lists it; `None` is the workshop that lists
    /// `after`. Without `after` in that menu, the recipe goes last in the
    /// workshop's first set.
    pub workshop: Option<WorkshopId>,
}

/// The UI bundle with each of `entries` listed in the crafting menu, right
/// after the recipe it follows.
///
/// A recipe with no place to go, or whose group already shows [`GROUP_ENTRIES`]
/// recipes, is left out with a warning; `None` only when the bundle does not
/// read with `layout`.
///
/// # Examples
///
/// ```
/// use enshwick::content::menu::{MenuEntry, MenuLayout, UI_BUNDLE, list_recipes};
/// use enshwick::content::{Guid, Id, id, types};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let layout = MenuLayout::resolve(load)?;
///     let entry = MenuEntry {
///         recipe: id!("locker.recipe").guid(),
///         after: Some(Guid::parse("45390726-8a83-4538-91a2-db2a633c8eac")),
///         workshop: None,
///     };
///     let bundle = UI_BUNDLE.key(types::FB_UI_BUNDLE)?;
///     load.serve(bundle, bundle, move |base| list_recipes(&layout, base, &[entry]))
/// }
/// ```
#[must_use]
pub fn list_recipes(layout: &MenuLayout, bundle: &[u8], entries: &[MenuEntry]) -> Option<Vec<u8>> {
    let mut blob = Blob::new(bundle);
    let (mut listed, mut unplaced, mut full) = (0usize, 0usize, 0usize);
    for entry in entries {
        let Place::In { group, set, after } = find_place(layout, &blob, entry)? else {
            unplaced += 1;
            continue;
        };
        if group_entries(layout, &blob, group)? >= GROUP_ENTRIES {
            full += 1;
            continue;
        }
        let field = set + layout.entries;
        let (start, count) = blob.array(field)?;
        let mut ids = blob
            .bytes()
            .get(start..start + usize::try_from(count).ok()? * 4)?
            .to_vec();
        let at = after.map_or(ids.len(), |index| (index + 1) * 4);
        ids.splice(at..at, entry.recipe.fnv1a().to_le_bytes());
        let moved = blob.append(&ids, 4);
        if !blob.set_array(field, moved, count + 1) {
            return None;
        }
        listed += 1;
    }
    if listed > 0 {
        crate::info!("{listed} recipes listed in the crafting menu");
    }
    if unplaced > 0 {
        crate::warn!(
            "{unplaced} recipes left out of the crafting menu: neither the recipe they follow nor their workshop is in it"
        );
    }
    if full > 0 {
        crate::warn!(
            "{full} recipes left out of the crafting menu: their group shows {GROUP_ENTRIES} recipes, the game's most"
        );
    }
    Some(blob.into_bytes())
}

/// The UI bundle with each recipe of `moves` taken out of every workshop's
/// crafting menu but its new workshop's, where it goes last in the first
/// set unless it is listed there already.
///
/// A recipe whose new workshop has no menu is only taken out; `None` when
/// the bundle does not read with `layout`.
///
/// # Examples
///
/// ```
/// use enshwick::content::menu::{MenuLayout, UI_BUNDLE, move_recipes};
/// use enshwick::content::workshops::WorkshopId;
/// use enshwick::content::{Guid, types};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let layout = MenuLayout::resolve(load)?;
///     let planks = Guid::parse("45390726-8a83-4538-91a2-db2a633c8eac");
///     let kiln = WorkshopId::new(Guid::parse("5b828914-9b48-40f8-b541-72e9b942cee6"));
///     let bundle = UI_BUNDLE.key(types::FB_UI_BUNDLE)?;
///     load.serve(bundle, bundle, move |base| move_recipes(&layout, base, &[(planks, kiln)]))
/// }
/// ```
#[must_use]
pub fn move_recipes(
    layout: &MenuLayout,
    bundle: &[u8],
    moves: &[(Guid, WorkshopId)],
) -> Option<Vec<u8>> {
    let mut blob = Blob::new(bundle);
    let (workshops, _) = blob.array(layout.collection + layout.workshops)?;
    let mut entries = Vec::new();
    for &(recipe, workshop) in moves {
        let id = recipe.fnv1a();
        let mut listed = false;
        for tree in 0..tree_count(layout, &blob)? {
            let home = blob.read_u32(workshops + tree * 4)? == workshop.id();
            for group in groups(layout, &blob, tree)? {
                for set in elements(&blob, group + layout.sets, layout.set)? {
                    let field = set + layout.entries;
                    let (start, count) = blob.array(field)?;
                    let ids: Vec<u32> = (0..usize::try_from(count).ok()?)
                        .map(|i| blob.read_u32(start + i * 4))
                        .collect::<Option<_>>()?;
                    if home {
                        listed |= ids.contains(&id);
                        continue;
                    }
                    if !ids.contains(&id) {
                        continue;
                    }
                    let kept: Vec<u8> = ids
                        .iter()
                        .filter(|&&other| other != id)
                        .flat_map(|other| other.to_le_bytes())
                        .collect();
                    let at = blob.append(&kept, 4);
                    if !blob.set_array(field, at, u32::try_from(kept.len() / 4).ok()?) {
                        return None;
                    }
                }
            }
        }
        if !listed {
            entries.push(MenuEntry {
                recipe,
                after: None,
                workshop: Some(workshop),
            });
        }
    }
    list_recipes(layout, &blob.into_bytes(), &entries)
}

/// Where a recipe goes in the crafting menu.
enum Place {
    /// In the set at `set` of the group at `group`, by offset in the blob,
    /// after the entry at index `after`, or last.
    In {
        group: usize,
        set: usize,
        after: Option<usize>,
    },
    /// Nowhere: neither the recipe it follows nor its workshop is listed.
    Nowhere,
}

/// The place for `entry`; `None` when the bundle does not read.
fn find_place(layout: &MenuLayout, blob: &Blob, entry: &MenuEntry) -> Option<Place> {
    let tree_count = tree_count(layout, blob)?;
    let trees: Vec<usize> = match entry.workshop {
        Some(workshop) => match tree_of(layout, blob, workshop) {
            Some(index) => vec![index],
            None => return Some(Place::Nowhere),
        },
        None => (0..tree_count).collect(),
    };
    if let Some(after) = entry.after.map(Guid::fnv1a) {
        for &tree in &trees {
            for group in groups(layout, blob, tree)? {
                for set in elements(blob, group + layout.sets, layout.set)? {
                    let (start, count) = blob.array(set + layout.entries)?;
                    for index in 0..usize::try_from(count).ok()? {
                        if blob.read_u32(start + index * 4)? == after {
                            return Some(Place::In {
                                group,
                                set,
                                after: Some(index),
                            });
                        }
                    }
                }
            }
        }
    }
    let (Some(_), [tree]) = (entry.workshop, trees.as_slice()) else {
        return Some(Place::Nowhere);
    };
    let Some(&group) = groups(layout, blob, *tree)?.first() else {
        return Some(Place::Nowhere);
    };
    let Some(&set) = elements(blob, group + layout.sets, layout.set)?.first() else {
        return Some(Place::Nowhere);
    };
    Some(Place::In {
        group,
        set,
        after: None,
    })
}

/// How many trees the menu has, one per workshop; `None` unless the
/// workshops and the trees are as many and more than none, as in every
/// menu the game bakes.
fn tree_count(layout: &MenuLayout, blob: &Blob) -> Option<usize> {
    let (_, workshops) = blob.array(layout.collection + layout.workshops)?;
    let (_, trees) = blob.array(layout.collection + layout.trees)?;
    (workshops == trees && trees > 0)
        .then(|| usize::try_from(trees).ok())
        .flatten()
}

/// The index of `workshop`'s tree; `None` for a workshop the menu lacks or
/// a bundle that does not read.
fn tree_of(layout: &MenuLayout, blob: &Blob, workshop: WorkshopId) -> Option<usize> {
    let (start, _) = blob.array(layout.collection + layout.workshops)?;
    (0..tree_count(layout, blob)?)
        .find(|&index| blob.read_u32(start + index * 4) == Some(workshop.id()))
}

/// The offsets of the groups of tree `tree`.
fn groups(layout: &MenuLayout, blob: &Blob, tree: usize) -> Option<Vec<usize>> {
    let (trees, _) = blob.array(layout.collection + layout.trees)?;
    elements(
        blob,
        trees + tree * layout.tree + layout.groups,
        layout.group,
    )
}

/// The offsets of the `size`-byte elements of the array whose header is at
/// `field`.
fn elements(blob: &Blob, field: usize, size: usize) -> Option<Vec<usize>> {
    let (start, count) = blob.array(field)?;
    Some(
        (0..usize::try_from(count).ok()?)
            .map(|i| start + i * size)
            .collect(),
    )
}

/// How many recipes the group at `group` lists, all its sets together.
fn group_entries(layout: &MenuLayout, blob: &Blob, group: usize) -> Option<usize> {
    let mut total = 0usize;
    for set in elements(blob, group + layout.sets, layout.set)? {
        let (_, count) = blob.array(set + layout.entries)?;
        total += usize::try_from(count).ok()?;
    }
    Some(total)
}

/// The UI bundle with a crafting menu for `workshop`: one group with one empty
/// set, where recipes that name the workshop go.
///
/// A workshop the menu already has keeps its tree; `None` when the bundle does
/// not read with `layout`.
///
/// # Examples
///
/// ```
/// use enshwick::content::menu::{MenuLayout, UI_BUNDLE, list_workshop};
/// use enshwick::content::workshops::WorkshopId;
/// use enshwick::content::{Id, id, types};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let layout = MenuLayout::resolve(load)?;
///     let bench = WorkshopId::new(id!("bench.workshop").guid());
///     let bundle = UI_BUNDLE.key(types::FB_UI_BUNDLE)?;
///     load.serve(bundle, bundle, move |base| list_workshop(&layout, base, bench))
/// }
/// ```
#[must_use]
pub fn list_workshop(layout: &MenuLayout, bundle: &[u8], workshop: WorkshopId) -> Option<Vec<u8>> {
    let mut blob = Blob::new(bundle);
    let workshops = layout.collection + layout.workshops;
    let trees = layout.collection + layout.trees;
    tree_count(layout, &blob)?;
    if tree_of(layout, &blob, workshop).is_some() {
        return Some(blob.into_bytes());
    }
    let set = blob.append(&vec![0; layout.set], 8);
    let mut group = vec![0u8; layout.group];
    group
        .get_mut(layout.group_id..layout.group_id + 16)?
        .copy_from_slice(&workshop.guid().0);
    let group = blob.append(&group, 8);
    if !blob.set_array(group + layout.sets, set, 1) {
        return None;
    }
    blob.grow_array(
        workshops,
        4,
        4,
        &[],
        &[(workshop.id().to_le_bytes().to_vec(), None)],
    )?;
    let index = blob.grow_array(
        trees,
        layout.tree,
        8,
        &[layout.groups],
        &[(vec![0; layout.tree], None)],
    )?;
    let (start, _) = blob.array(trees)?;
    let tree = start + usize::try_from(index).ok()? * layout.tree;
    if !blob.set_array(tree + layout.groups, group, 1) {
        return None;
    }
    for max in [layout.max_groups, layout.max_sets] {
        let at = layout.collection + max;
        if blob.read_u32(at)? < 1 && !blob.write(at, &1u32.to_le_bytes()) {
            return None;
        }
    }
    Some(blob.into_bytes())
}

/// Serves the UI bundle with `edit` on it, on the client only. A layout
/// that does not resolve, or a bundle that does not read, leaves the menu
/// as it is with a warning: the recipes themselves still work.
pub(crate) fn serve(
    load: &mut Load<'_>,
    edit: impl Fn(&MenuLayout, &[u8]) -> Option<Vec<u8>> + Send + Sync + 'static,
) -> Result<(), Off> {
    if load.host().build().side != Side::CLIENT {
        return Ok(());
    }
    let resolved = MenuLayout::resolve(load)
        .and_then(|layout| Ok((layout, UI_BUNDLE.key(types::FB_UI_BUNDLE)?)));
    let (layout, bundle) = match resolved {
        Ok(resolved) => resolved,
        Err(off) => {
            crate::warn_once!("new recipes stay out of the crafting menu: {off}");
            return Ok(());
        }
    };
    load.serve(bundle, bundle, move |base| {
        edit(&layout, base).or_else(|| {
            crate::warn!("new recipes stay out of the crafting menu: the UI bundle does not read");
            Some(base.to_vec())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT: MenuLayout = MenuLayout {
        collection: 0x40,
        workshops: 0,
        trees: 8,
        max_groups: 0x10,
        max_sets: 0x14,
        tree: 8,
        groups: 0,
        group: 0x38,
        group_id: 0,
        sets: 0x30,
        set: 0x28,
        entries: 0x20,
    };

    type Tree<'a> = &'a [&'a [&'a [u32]]];

    /// A bundle whose menu has a tree per workshop: groups of sets of ids.
    fn bundle(trees: &[(u32, Tree<'_>)]) -> Vec<u8> {
        let mut blob = Blob::new(&[0u8; 0x60]);
        let mut tree_bytes = Vec::new();
        for (_, groups) in trees {
            let first = blob.append(&vec![0u8; groups.len() * 0x38], 8);
            for (g, sets) in groups.iter().enumerate() {
                let set_start = blob.append(&vec![0u8; sets.len() * 0x28], 8);
                for (s, ids) in sets.iter().enumerate() {
                    let ids: Vec<u8> = ids.iter().flat_map(|id| id.to_le_bytes()).collect();
                    let at = blob.append(&ids, 4);
                    let set = set_start + s * 0x28;
                    assert!(blob.set_array(set + 0x20, at, u32::try_from(ids.len() / 4).unwrap()));
                }
                let group = first + g * 0x38;
                assert!(blob.write(group, &[u8::try_from(g).unwrap() + 1; 16]));
                assert!(blob.set_array(
                    group + 0x30,
                    set_start,
                    u32::try_from(sets.len()).unwrap()
                ));
            }
            tree_bytes.push((first, groups.len()));
        }
        let trees_at = blob.append(&vec![0u8; trees.len() * 8], 8);
        for (i, (first, count)) in tree_bytes.iter().enumerate() {
            assert!(blob.set_array(trees_at + i * 8, *first, u32::try_from(*count).unwrap()));
        }
        let ids: Vec<u8> = trees.iter().flat_map(|(w, _)| w.to_le_bytes()).collect();
        let workshops_at = blob.append(&ids, 4);
        let count = u32::try_from(trees.len()).unwrap();
        assert!(blob.set_array(0x40, workshops_at, count));
        assert!(blob.set_array(0x48, trees_at, count));
        assert!(blob.write(0x50, &1u32.to_le_bytes()));
        assert!(blob.write(0x54, &2u32.to_le_bytes()));
        blob.into_bytes()
    }

    /// The menu as workshop ids and their trees of ids.
    fn read(bytes: &[u8]) -> Vec<(u32, Vec<Vec<Vec<u32>>>)> {
        let blob = Blob::new(bytes);
        let (workshops, count) = blob.array(0x40).unwrap();
        (0..count as usize)
            .map(|t| {
                let tree = groups(&LAYOUT, &blob, t)
                    .unwrap()
                    .into_iter()
                    .map(|group| {
                        elements(&blob, group + 0x30, 0x28)
                            .unwrap()
                            .into_iter()
                            .map(|set| {
                                let (start, n) = blob.array(set + 0x20).unwrap();
                                (0..n as usize)
                                    .map(|i| blob.read_u32(start + i * 4).unwrap())
                                    .collect()
                            })
                            .collect()
                    })
                    .collect();
                (blob.read_u32(workshops + t * 4).unwrap(), tree)
            })
            .collect()
    }

    const CHEST: Guid = Guid::parse("45390726-8a83-4538-91a2-db2a633c8eac");
    const LOCKER: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
    const VAULT: Guid = Guid::parse("0b6c5a52-1df4-4a3b-bd0e-6c1f7e2a9c44");

    fn bench() -> WorkshopId {
        WorkshopId::new(Guid::parse("5b828914-9b48-40f8-b541-72e9b942cee6"))
    }

    #[test]
    fn a_moved_recipe_leaves_every_other_menu_and_joins_its_station_s() {
        let kiln = bench().id();
        let base = bundle(&[
            (7, &[&[&[1, CHEST.fnv1a(), 2]], &[&[CHEST.fnv1a()]]]),
            (kiln, &[&[&[3]]]),
            (9, &[&[&[4]]]),
        ]);
        let built = move_recipes(&LAYOUT, &base, &[(CHEST, bench())]).unwrap();
        assert_eq!(
            read(&built),
            [
                (7, vec![vec![vec![1, 2]], vec![vec![]]]),
                (kiln, vec![vec![vec![3, CHEST.fnv1a()]]]),
                (9, vec![vec![vec![4]]]),
            ]
        );
        let again = move_recipes(&LAYOUT, &built, &[(CHEST, bench())]).unwrap();
        assert_eq!(read(&again), read(&built));
    }

    #[test]
    fn a_new_recipe_goes_right_after_the_one_it_copies() {
        let base = bundle(&[(7, &[&[&[1, 2]]]), (9, &[&[&[3], &[4, CHEST.fnv1a(), 5]]])]);
        let entry = MenuEntry {
            recipe: LOCKER,
            after: Some(CHEST),
            workshop: None,
        };
        let built = list_recipes(&LAYOUT, &base, &[entry]).unwrap();
        assert_eq!(
            read(&built),
            [
                (7, vec![vec![vec![1, 2]]]),
                (
                    9,
                    vec![vec![vec![3], vec![4, CHEST.fnv1a(), LOCKER.fnv1a(), 5]]]
                ),
            ]
        );
    }

    #[test]
    fn recipes_of_several_mods_compose_in_order() {
        let base = bundle(&[(9, &[&[&[CHEST.fnv1a(), 5]]])]);
        let locker = MenuEntry {
            recipe: LOCKER,
            after: Some(CHEST),
            workshop: None,
        };
        let first = list_recipes(&LAYOUT, &base, &[locker]).unwrap();
        let vault = MenuEntry {
            recipe: VAULT,
            after: Some(LOCKER),
            workshop: None,
        };
        let second = list_recipes(&LAYOUT, &first, &[vault]).unwrap();
        assert_eq!(
            read(&second),
            [(
                9,
                vec![vec![vec![CHEST.fnv1a(), LOCKER.fnv1a(), VAULT.fnv1a(), 5]]]
            )]
        );
    }

    #[test]
    fn of_two_mods_copying_one_recipe_the_later_one_comes_first() {
        let base = bundle(&[(9, &[&[&[CHEST.fnv1a(), 5]]])]);
        let after_chest = |recipe| MenuEntry {
            recipe,
            after: Some(CHEST),
            workshop: None,
        };
        let first = list_recipes(&LAYOUT, &base, &[after_chest(LOCKER)]).unwrap();
        let second = list_recipes(&LAYOUT, &first, &[after_chest(VAULT)]).unwrap();
        assert_eq!(
            read(&second),
            [(
                9,
                vec![vec![vec![CHEST.fnv1a(), VAULT.fnv1a(), LOCKER.fnv1a(), 5]]]
            )]
        );
    }

    #[test]
    fn a_recipe_moved_to_a_workshop_goes_last_in_its_first_set() {
        let base = bundle(&[
            (9, &[&[&[CHEST.fnv1a()]]]),
            (bench().id(), &[&[&[1, 2], &[3]], &[&[4]]]),
        ]);
        let entry = MenuEntry {
            recipe: LOCKER,
            after: Some(CHEST),
            workshop: Some(bench()),
        };
        let built = list_recipes(&LAYOUT, &base, &[entry]).unwrap();
        assert_eq!(
            read(&built)[1],
            (
                bench().id(),
                vec![vec![vec![1, 2, LOCKER.fnv1a()], vec![3]], vec![vec![4]]]
            )
        );
        assert_eq!(read(&built)[0], (9, vec![vec![vec![CHEST.fnv1a()]]]));
    }

    #[test]
    fn a_recipe_with_no_place_leaves_the_menu_as_it_was() {
        let base = bundle(&[(9, &[&[&[1]]])]);
        let entries = [
            MenuEntry {
                recipe: LOCKER,
                after: Some(CHEST),
                workshop: None,
            },
            MenuEntry {
                recipe: VAULT,
                after: None,
                workshop: Some(bench()),
            },
        ];
        let built = list_recipes(&LAYOUT, &base, &entries).unwrap();
        assert_eq!(built, base);
    }

    #[test]
    fn a_full_group_takes_no_more_recipes() {
        let full: Vec<u32> = (1..=u32::try_from(GROUP_ENTRIES).unwrap()).collect();
        let (left, right) = full.split_at(300);
        let base = bundle(&[(bench().id(), &[&[left, right]])]);
        let entry = MenuEntry {
            recipe: LOCKER,
            after: Some(CHEST),
            workshop: Some(bench()),
        };
        assert_eq!(list_recipes(&LAYOUT, &base, &[entry]).unwrap(), base);

        let room = bundle(&[(bench().id(), &[&[left, &right[1..]]])]);
        let built = list_recipes(&LAYOUT, &room, &[entry]).unwrap();
        assert_eq!(read(&built)[0].1[0][0].len(), 301);
        assert_eq!(read(&built)[0].1[0][0][300], LOCKER.fnv1a());
    }

    #[test]
    fn a_new_workshop_gets_a_menu_its_recipes_go_to() {
        let base = bundle(&[(9, &[&[&[CHEST.fnv1a()]]])]);
        let built = list_workshop(&LAYOUT, &base, bench()).unwrap();
        assert_eq!(
            read(&built),
            [
                (9, vec![vec![vec![CHEST.fnv1a()]]]),
                (bench().id(), vec![vec![vec![]]])
            ]
        );
        let blob = Blob::new(&built);
        let (groups, _) = blob.array(blob.array(0x48).unwrap().0 + 8).unwrap();
        assert_eq!(&blob.bytes()[groups..groups + 16], &bench().guid().0);
        assert_eq!(list_workshop(&LAYOUT, &built, bench()).unwrap(), built);

        let entry = MenuEntry {
            recipe: LOCKER,
            after: Some(CHEST),
            workshop: Some(bench()),
        };
        let built = list_recipes(&LAYOUT, &built, &[entry]).unwrap();
        assert_eq!(
            read(&built)[1],
            (bench().id(), vec![vec![vec![LOCKER.fnv1a()]]])
        );
        assert_eq!(read(&built)[0], (9, vec![vec![vec![CHEST.fnv1a()]]]));
    }

    #[test]
    fn the_largest_counts_cover_a_new_workshop_s_menu() {
        let mut blob = Blob::new(&bundle(&[(9, &[&[&[1]]])]));
        assert!(blob.write(0x50, &0u32.to_le_bytes()));
        assert!(blob.write(0x54, &0u32.to_le_bytes()));
        let built = Blob::new(&list_workshop(&LAYOUT, blob.bytes(), bench()).unwrap());
        assert_eq!(built.read_u32(0x50), Some(1));
        assert_eq!(built.read_u32(0x54), Some(1));
        let kept =
            Blob::new(&list_workshop(&LAYOUT, &bundle(&[(9, &[&[&[1]]])]), bench()).unwrap());
        assert_eq!(kept.read_u32(0x54), Some(2));
    }

    #[test]
    fn a_bundle_that_does_not_read_builds_nothing() {
        let entry = MenuEntry {
            recipe: LOCKER,
            after: Some(CHEST),
            workshop: None,
        };
        assert!(list_recipes(&LAYOUT, &[0; 8], &[entry]).is_none());
        assert!(list_workshop(&LAYOUT, &[0; 8], bench()).is_none());

        let mut uneven = Blob::new(&bundle(&[(9, &[&[&[CHEST.fnv1a()]]])]));
        let (workshops, _) = uneven.array(0x40).unwrap();
        assert!(uneven.set_array(0x40, workshops, 2));
        assert!(list_recipes(&LAYOUT, uneven.bytes(), &[entry]).is_none());
        assert!(list_workshop(&LAYOUT, uneven.bytes(), bench()).is_none());
        let empty = bundle(&[]);
        assert!(list_recipes(&LAYOUT, &empty, &[entry]).is_none());
    }
}
