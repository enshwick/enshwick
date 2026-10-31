//! The game's loot tables, read and changed: the random loot table's stack
//! sizes per item, and the counts, chances and weights of the fill tables
//! containers roll.

use crate::engine_ids as ids;
use crate::{ItemId, Off};

use super::changes::Field;
use super::{Blob, EngineFacts, Guid};

/// Where the fields a loot table edit touches sit on the running build,
/// resolved by the loader.
///
/// # Examples
///
/// See [`LootLayout::resolve`] and [`change_random_loot`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LootLayout {
    /// The random loot table's list of items.
    pub items: usize,
    /// One random loot item's size.
    pub item: usize,
    /// Its item id, a `u32`.
    pub item_id: usize,
    /// Its least stack size, a `u16`.
    pub stack_min: usize,
    /// Its largest stack size, a `u16`.
    pub stack_max: usize,
    /// Its largest stack size when the roll scales it, a `u16`.
    pub stack_max_scaled: usize,
    /// Where a fill table's root group sits in it.
    pub root: usize,
    /// One group's size.
    pub group: usize,
    /// A group's stacks.
    pub stacks: usize,
    /// A group's groups.
    pub groups: usize,
    /// One stack's size.
    pub stack: usize,
    /// A stack's item id, a `u32`.
    pub stack_item: usize,
    /// A stack's least count, a `u32`.
    pub count_min: usize,
    /// A stack's largest count, a `u32`.
    pub count_max: usize,
    /// A stack's chance, an `f32`.
    pub probability: usize,
    /// A stack's weight, an `f32`.
    pub weight: usize,
}

impl LootLayout {
    /// The layout `facts` give: the running game's through
    /// [`Load`](crate::Load), a game image's in the loader and in tools.
    ///
    /// # Errors
    ///
    /// The first [`Off`] a fact returns, when the build lacks it.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let layout = enshwick::content::loot_tables::LootLayout::resolve(load)?;
    ///     enshwick::info!("a fill stack is {} bytes", layout.stack);
    ///     Ok(())
    /// }
    /// ```
    pub fn resolve(facts: &(impl EngineFacts + ?Sized)) -> Result<Self, Off> {
        let at = |id: &str| facts.offset(id);
        Ok(Self {
            items: at(ids::LOOTABLE_ITEMS_ITEMS)?,
            item: facts.size(ids::LOOTABLE_ITEM)?,
            item_id: at(ids::LOOTABLE_ITEM_ITEM)?,
            stack_min: at(ids::LOOTABLE_ITEM_STACK_MIN)?,
            stack_max: at(ids::LOOTABLE_ITEM_STACK_MAX)?,
            stack_max_scaled: at(ids::LOOTABLE_ITEM_STACK_MAX_SCALED)?,
            root: at(ids::LOOT_FILL_TABLE_ROOT)?,
            group: facts.size(ids::LOOT_FILL_GROUP)?,
            stacks: at(ids::LOOT_FILL_GROUP_STACKS)?,
            groups: at(ids::LOOT_FILL_GROUP_GROUPS)?,
            stack: facts.size(ids::LOOT_FILL_STACK)?,
            stack_item: at(ids::LOOT_FILL_STACK_ITEM)?,
            count_min: at(ids::LOOT_FILL_STACK_COUNT_MIN)?,
            count_max: at(ids::LOOT_FILL_STACK_COUNT_MAX)?,
            probability: at(ids::LOOT_FILL_STACK_PROBABILITY)?,
            weight: at(ids::LOOT_FILL_STACK_WEIGHT)?,
        })
    }
}

/// Where a container's template names itself and the fill table it rolls,
/// resolved by the loader or a tool.
///
/// # Examples
///
/// See [`FillTableLayout::resolve`] and [`template_table`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FillTableLayout {
    /// A template's name.
    pub name: usize,
    /// A template's components.
    pub components: usize,
    /// The type hash of the component that names a fill table.
    pub fill: u32,
    /// The fill table's GUID inside that component.
    pub table: usize,
}

impl FillTableLayout {
    /// The layout `facts` give.
    ///
    /// # Errors
    ///
    /// The first [`Off`] a fact returns, when the build lacks it.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let layout = enshwick::content::loot_tables::FillTableLayout::resolve(load)?;
    ///     enshwick::info!("a template's fill table at +{:#x}", layout.table);
    ///     Ok(())
    /// }
    /// ```
    pub fn resolve(facts: &(impl EngineFacts + ?Sized)) -> Result<Self, Off> {
        Ok(Self {
            name: facts.offset(ids::TEMPLATE_NAME)?,
            components: facts.offset(ids::TEMPLATE_COMPONENTS)?,
            fill: facts.hash(ids::LOOT_FILL)?,
            table: facts.offset(ids::LOOT_FILL_DEFAULT_INVENTORY)?,
        })
    }
}

/// The name of the container `template` and the GUID of the fill table it
/// rolls; `None` for a template that rolls none or does not read.
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
/// use enshwick::content::loot_tables::{FillTableLayout, template_table};
///
/// fn table_of(layout: &FillTableLayout, template: &[u8]) -> Option<Guid> {
///     template_table(layout, template).map(|(_name, table)| table)
/// }
/// ```
#[must_use]
pub fn template_table(layout: &FillTableLayout, template: &[u8]) -> Option<(String, Guid)> {
    let blob = Blob::new(template);
    let fill =
        *super::containers::template_components(&blob, layout.components, layout.fill)?.first()?;
    let table: [u8; 16] = template
        .get(fill + layout.table..fill + layout.table + 16)?
        .try_into()
        .ok()?;
    if table == [0; 16] {
        return None;
    }
    let (at, len) = blob.array(layout.name)?;
    let name = template.get(at..at + usize::try_from(len).ok()?)?;
    let name = String::from_utf8_lossy(name)
        .trim_end_matches('\0')
        .to_owned();
    Some((name, Guid(table)))
}

/// The value in force of `field` for the item with game id `item`, from
/// `base`; `None` when nothing changes it.
///
/// # Examples
///
/// See [`change_random_loot`], which takes one.
pub type LootValue<'a> = &'a dyn Fn(ItemId, Field, f64) -> Option<f64>;

/// A whole value in force as the integer type `T`, which the field's range
/// keeps it inside.
fn whole<T: TryFrom<i64>>(value: f64) -> Option<T> {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Field::resolve keeps whole values inside their field's range"
    )]
    let value = value.round() as i64;
    T::try_from(value).ok()
}

/// The random loot table `table` with each item's stack sizes as `value`
/// gives them, and how many items changed.
///
/// The least and the largest change from their own values, and the ceiling
/// of a scaled roll follows by [`scaled_ceiling`].
///
/// # Examples
///
/// ```
/// use enshwick::content::changes::{Field, Value};
/// use enshwick::content::loot_tables::{LootLayout, change_random_loot};
///
/// fn doubled(layout: &LootLayout, table: &[u8]) -> Option<Vec<u8>> {
///     let double = |_item: enshwick::ItemId, field: Field, base: f64| Some(field.resolve(base, [Value::Times(2.0)]));
///     change_random_loot(layout, table, &double).map(|(bytes, _)| bytes)
/// }
/// ```
#[must_use]
pub fn change_random_loot(
    layout: &LootLayout,
    table: &[u8],
    value: LootValue<'_>,
) -> Option<(Vec<u8>, usize)> {
    let mut blob = Blob::new(table);
    let (start, count) = blob.array(layout.items)?;
    let mut changed = 0;
    for at in (0..usize::try_from(count).ok()?).map(|i| start + i * layout.item) {
        let item = ItemId::new(blob.read_u32(at + layout.item_id)?);
        let read = |blob: &Blob, at: usize| -> Option<u16> {
            Some(u16::from_le_bytes(
                blob.bytes().get(at..at + 2)?.try_into().ok()?,
            ))
        };
        let (min, max, scaled) = (
            read(&blob, at + layout.stack_min)?,
            read(&blob, at + layout.stack_max)?,
            read(&blob, at + layout.stack_max_scaled)?,
        );
        let new_min = value(item, Field::StackSize, f64::from(min)).and_then(whole::<u16>);
        let new_max = value(item, Field::StackSize, f64::from(max)).and_then(whole::<u16>);
        let (Some(new_min), Some(new_max)) = (new_min, new_max) else {
            continue;
        };
        let new_scaled = scaled_ceiling(scaled, max, new_max);
        if (new_min, new_max, new_scaled) == (min, max, scaled) {
            continue;
        }
        let written = blob.write(at + layout.stack_min, &new_min.to_le_bytes())
            && blob.write(at + layout.stack_max, &new_max.to_le_bytes())
            && blob.write(at + layout.stack_max_scaled, &new_scaled.to_le_bytes());
        changed += usize::from(written);
    }
    Some((blob.into_bytes(), changed))
}

/// The ceiling the game holds a scaled roll to, after an item's largest
/// stack went from `max` to `new_max`.
///
/// It keeps the proportion to the largest stack it had and never falls
/// below the new largest, so the game's own drop factor keeps its room.
///
/// # Examples
///
/// ```
/// use enshwick::content::loot_tables::scaled_ceiling;
///
/// // A ceiling of 20 for a largest stack of 10: doubled to 20, the ceiling becomes 40.
/// assert_eq!(scaled_ceiling(20, 10, 20), 40);
/// // The ceiling never falls below the new largest stack.
/// assert_eq!(scaled_ceiling(5, 10, 40), 40);
/// ```
#[must_use]
pub fn scaled_ceiling(scaled: u16, max: u16, new_max: u16) -> u16 {
    if max == 0 {
        return scaled.max(new_max);
    }
    let ratio = f64::from(scaled) / f64::from(max);
    whole::<u16>((f64::from(new_max) * ratio).min(65535.0))
        .unwrap_or(u16::MAX)
        .max(new_max)
}

/// The item ids every stack of the fill table `table` names, its groups'
/// stacks included.
///
/// # Examples
///
/// See [`change_fill_table`].
#[must_use]
pub fn fill_items(layout: &LootLayout, table: &[u8]) -> Vec<ItemId> {
    let blob = Blob::new(table);
    let mut items = Vec::new();
    let _whole = walk(layout, &blob, layout.root, 0, &mut |at| {
        if let Some(item) = blob.read_u32(at + layout.stack_item) {
            items.push(ItemId::new(item));
        }
        Some(())
    });
    items
}

/// Calls `visit` with the offset of every stack of the group at `group`
/// and of its groups, depth first; `None` when the table does not read.
fn walk(
    layout: &LootLayout,
    blob: &Blob,
    group: usize,
    depth: usize,
    visit: &mut dyn FnMut(usize) -> Option<()>,
) -> Option<()> {
    // Vanilla tables nest a few groups deep; a deeper one is malformed.
    if depth > 16 {
        return None;
    }
    let (stacks, count) = blob.array(group + layout.stacks)?;
    for i in 0..usize::try_from(count).ok()? {
        visit(stacks + i * layout.stack)?;
    }
    let (groups, count) = blob.array(group + layout.groups)?;
    for i in 0..usize::try_from(count).ok()? {
        walk(layout, blob, groups + i * layout.group, depth + 1, visit)?;
    }
    Some(())
}

/// The fill table `table` with each stack's count, chance and weight as
/// `value` gives them, and how many stacks changed; a stack of no item is
/// left.
///
/// # Examples
///
/// ```
/// use enshwick::content::changes::{Field, Value};
/// use enshwick::content::loot_tables::{LootLayout, change_fill_table, fill_items};
///
/// fn more_of(layout: &LootLayout, table: &[u8], wanted: enshwick::ItemId) -> Option<Vec<u8>> {
///     if !fill_items(layout, table).contains(&wanted) {
///         return None;
///     }
///     let more = |item: enshwick::ItemId, field: Field, base: f64| {
///         (item == wanted && field == Field::Count).then(|| field.resolve(base, [Value::Times(2.0)]))
///     };
///     change_fill_table(layout, table, &more).map(|(bytes, _)| bytes)
/// }
/// ```
#[must_use]
pub fn change_fill_table(
    layout: &LootLayout,
    table: &[u8],
    value: LootValue<'_>,
) -> Option<(Vec<u8>, usize)> {
    let read = Blob::new(table);
    let mut stacks = Vec::new();
    walk(layout, &read, layout.root, 0, &mut |at| {
        stacks.push(at);
        Some(())
    })?;
    let mut blob = Blob::new(table);
    let mut changed = 0;
    for at in stacks {
        let item = ItemId::new(blob.read_u32(at + layout.stack_item)?);
        if item.get() == 0 {
            continue;
        }
        let mut touched = false;
        for field_at in [at + layout.count_min, at + layout.count_max] {
            let old = blob.read_u32(field_at)?;
            if let Some(new) = value(item, Field::Count, f64::from(old)).and_then(whole::<u32>)
                && new != old
            {
                touched |= blob.write(field_at, &new.to_le_bytes());
            }
        }
        for (field, field_at) in [
            (Field::SpawnProbability, at + layout.probability),
            (Field::Weight, at + layout.weight),
        ] {
            let bytes = blob.bytes().get(field_at..field_at + 4)?;
            let old = f32::from_le_bytes(bytes.try_into().ok()?);
            if let Some(new) = value(item, field, f64::from(old)) {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "the game keeps chances and weights as f32"
                )]
                let new = new as f32;
                if new.to_bits() != old.to_bits() {
                    touched |= blob.write(field_at, &new.to_le_bytes());
                }
            }
        }
        changed += usize::from(touched);
    }
    Some((blob.into_bytes(), changed))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::content::changes::Value;

    const LAYOUT: LootLayout = LootLayout {
        items: 0x14,
        item: 0x24,
        item_id: 0,
        stack_min: 0x18,
        stack_max: 0x1a,
        stack_max_scaled: 0x1c,
        root: 0,
        group: 0x24,
        stacks: 0x4,
        groups: 0xc,
        stack: 0x28,
        stack_item: 0x10,
        count_min: 0x14,
        count_max: 0x18,
        probability: 0x1c,
        weight: 0x20,
    };
    const WOOD: u32 = 722_268_629;
    const STONE: u32 = 631_520_303;

    pub(crate) const FILL: FillTableLayout = FillTableLayout {
        name: 0,
        components: 0x10,
        fill: 0x33f6_8834,
        table: 0,
    };

    /// A container template named `name` whose fill table is `table`.
    pub(crate) fn container(name: &str, table: Guid) -> Vec<u8> {
        let mut blob = Blob::new(&[0u8; 0x18]);
        let name_at = blob.append(name.as_bytes(), 1);
        assert!(blob.set_array(0, name_at, u32::try_from(name.len()).unwrap()));
        let list_at = blob.append(&[0u8; 12], 4);
        let data_at = blob.append(&[table.0.to_vec(), vec![0; 4]].concat(), 4);
        let offset = u32::try_from(data_at - (list_at + 4)).unwrap();
        let variant = [
            FILL.fill.to_le_bytes(),
            offset.to_le_bytes(),
            20u32.to_le_bytes(),
        ]
        .concat();
        assert!(blob.write(list_at, &variant));
        assert!(blob.set_array(0x10, list_at, 1));
        blob.into_bytes()
    }

    #[test]
    fn a_container_names_itself_and_its_fill_table() {
        let table = Guid([4; 16]);
        assert_eq!(
            template_table(&FILL, &container("LootContainer_Enemy_Corpse_Vukah", table)),
            Some(("LootContainer_Enemy_Corpse_Vukah".to_owned(), table))
        );
        assert_eq!(
            template_table(&FILL, &container("Prop_Storage_16", Guid([0; 16]))),
            None
        );
    }

    fn double(field: Field) -> impl Fn(ItemId, Field, f64) -> Option<f64> {
        move |item, asked, base| {
            (item == ItemId::new(WOOD) && asked == field)
                .then(|| asked.resolve(base, [Value::Times(2.0)]))
        }
    }

    fn random_table() -> Vec<u8> {
        let mut blob = Blob::new(&[0u8; 0x50]);
        let mut items = Vec::new();
        for (item, min, max, scaled) in [(WOOD, 2u16, 5u16, 20u16), (STONE, 1, 3, 3)] {
            let mut entry = vec![0u8; 0x24];
            entry[0..4].copy_from_slice(&item.to_le_bytes());
            entry[0x18..0x1a].copy_from_slice(&min.to_le_bytes());
            entry[0x1a..0x1c].copy_from_slice(&max.to_le_bytes());
            entry[0x1c..0x1e].copy_from_slice(&scaled.to_le_bytes());
            items.extend_from_slice(&entry);
        }
        let at = blob.append(&items, 4);
        assert!(blob.set_array(0x14, at, 2));
        blob.into_bytes()
    }

    fn stack(item: u32, min: u32, max: u32, chance: f32) -> Vec<u8> {
        let mut bytes = vec![0u8; 0x28];
        bytes[0x10..0x14].copy_from_slice(&item.to_le_bytes());
        bytes[0x14..0x18].copy_from_slice(&min.to_le_bytes());
        bytes[0x18..0x1c].copy_from_slice(&max.to_le_bytes());
        bytes[0x1c..0x20].copy_from_slice(&chance.to_le_bytes());
        bytes[0x20..0x24].copy_from_slice(&1.0f32.to_le_bytes());
        bytes
    }

    /// A fill table: the root holds a stone stack and one group, which holds
    /// a wood stack and a stack of no item.
    fn fill_table() -> Vec<u8> {
        let mut blob = Blob::new(&[0u8; 0x24]);
        let inner = blob.append(&[0u8; 0x24], 4);
        let root_stacks = blob.append(&stack(STONE, 1, 2, 0.5), 4);
        let inner_stacks = blob.append(&[stack(WOOD, 3, 6, 0.25), stack(0, 0, 0, 1.0)].concat(), 4);
        assert!(blob.set_array(0x4, root_stacks, 1));
        assert!(blob.set_array(0xc, inner, 1));
        assert!(blob.set_array(inner + 0x4, inner_stacks, 2));
        blob.into_bytes()
    }

    #[test]
    fn random_loot_stack_sizes_change_for_the_named_item_only() {
        let (bytes, changed) =
            change_random_loot(&LAYOUT, &random_table(), &double(Field::StackSize)).unwrap();
        assert_eq!(changed, 1);
        let blob = Blob::new(&bytes);
        let (at, _) = blob.array(0x14).unwrap();
        assert_eq!(bytes[at + 0x18..at + 0x1e], [4, 0, 10, 0, 40, 0]);
        assert_eq!(
            bytes[at + 0x24 + 0x18..at + 0x24 + 0x1e],
            [1, 0, 3, 0, 3, 0]
        );
    }

    #[test]
    fn a_set_stack_keeps_the_room_the_game_s_factor_had() {
        assert_eq!(scaled_ceiling(20, 5, 10), 40);
        assert_eq!(scaled_ceiling(3, 3, 999), 999);
        assert_eq!(scaled_ceiling(250, 1, 999), 65535);
        assert_eq!(scaled_ceiling(7, 0, 4), 7);
        let set = |item: ItemId, field: Field, base: f64| {
            (item == ItemId::new(WOOD)).then(|| field.resolve(base, [Value::Set(10.0)]))
        };
        let (bytes, _) = change_random_loot(&LAYOUT, &random_table(), &set).unwrap();
        let blob = Blob::new(&bytes);
        let (at, _) = blob.array(0x14).unwrap();
        assert_eq!(bytes[at + 0x18..at + 0x1e], [10, 0, 10, 0, 40, 0]);
    }

    #[test]
    fn a_fill_table_lists_the_items_of_every_group() {
        let mut items = fill_items(&LAYOUT, &fill_table());
        items.sort_unstable();
        assert_eq!(
            items,
            [ItemId::new(0), ItemId::new(STONE), ItemId::new(WOOD)]
        );
    }

    #[test]
    fn a_fill_stack_changes_in_a_nested_group_and_one_of_no_item_stays() {
        let table = fill_table();
        let (bytes, changed) = change_fill_table(&LAYOUT, &table, &double(Field::Count)).unwrap();
        assert_eq!(changed, 1);
        let blob = Blob::new(&bytes);
        let (inner, _) = blob.array(0xc).unwrap();
        let (stacks, _) = blob.array(inner + 0x4).unwrap();
        assert_eq!(blob.read_u32(stacks + 0x14), Some(6));
        assert_eq!(blob.read_u32(stacks + 0x18), Some(12));
        let (bytes, _) =
            change_fill_table(&LAYOUT, &table, &double(Field::SpawnProbability)).unwrap();
        let chance = f32::from_le_bytes(bytes[stacks + 0x1c..stacks + 0x20].try_into().unwrap());
        assert_eq!(chance, 0.5);
        let every =
            |_: ItemId, field: Field, base: f64| Some(field.resolve(base, [Value::Set(9.0)]));
        let (bytes, _) = change_fill_table(&LAYOUT, &table, &every).unwrap();
        assert_eq!(
            bytes[stacks + 0x28..stacks + 0x50],
            table[stacks + 0x28..stacks + 0x50]
        );
    }
}
