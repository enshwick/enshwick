//! New items: an item cloned from a vanilla one, listed in the item registry,
//! with its own name, tooltip lines and a meter each item keeps.

use super::{Blob, Guid, types};
use crate::abi::ItemData;
use crate::{Load, Off};

/// The game's item registry resource.
pub const ITEM_REGISTRY: Guid = Guid::parse("3064d35d-7342-40ca-bdc9-aad58f83bf45");

/// The template of the water pouch's own entity, `Pide_Fuel_Water_Pouch`:
/// the base every [`Meter`] template is cloned from.
pub const METER_TEMPLATE_BASE: Guid = Guid::parse("39246eb2-1b9f-4881-8c60-e0e4f5d956f8");

/// The type hash of `ecs::FuelSetup`, the component that holds a meter's
/// maximum and starting fill in a template.
const FUEL_SETUP: u32 = 0x139a_fbec;

/// A new item for [`Load::add_item`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::Guid;
/// use enshwick_loader_api::content::items::NewItem;
///
/// let item = NewItem::new(
///     Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31"),
///     Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d"),
///     "Safe",
/// );
/// assert!(item.meter.is_none() && item.stats.is_empty());
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewItem {
    /// The new item's GUID; its game id is [`Guid::id`].
    pub guid: Guid,
    /// The vanilla item it is cloned from: what it looks like and does.
    pub like: Guid,
    /// The tag of its name, given text with [`Load::add_texts`]; `None` keeps
    /// the vanilla item's name.
    pub name: Option<Guid>,
    /// The tag of its description; `None` keeps the vanilla one.
    pub description: Option<Guid>,
    /// The name in the registry's debug list.
    pub debug_name: String,
    /// A count each item keeps, such as charges, which the game saves,
    /// sends to players and shows as a bar on the item.
    pub meter: Option<Meter>,
    /// Lines the item's tooltip shows after the vanilla item's.
    pub stats: Vec<Stat>,
}

impl NewItem {
    /// An item cloned from `like` with no edits but its identity.
    ///
    /// # Examples
    ///
    /// See [`NewItem`].
    #[must_use]
    pub fn new(guid: Guid, like: Guid, debug_name: impl Into<String>) -> Self {
        Self {
            guid,
            like,
            name: None,
            description: None,
            debug_name: debug_name.into(),
            meter: None,
            stats: Vec::new(),
        }
    }
}

/// A count each item keeps, from 0 to `max`, for [`NewItem::meter`].
///
/// The game keeps it on the item's own entity as the water pouch keeps its
/// water: it saves it with the world, sends it to players and draws it as a
/// bar on the item's slot. Read and change it with
/// [`World::item_meter`](crate::world::World::item_meter); set it for each
/// new item with [`Op::Give`](crate::inventory::Op::Give)'s `fill` or a
/// [`Drop`](crate::loot::Drop)'s fill.
///
/// The item gets an entity template of its own, cloned from the water
/// pouch's: `template` is that template's GUID, the mod's own and fixed
/// forever, as saves hold it.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::Guid;
/// use enshwick_loader_api::content::items::Meter;
///
/// let charges = Meter { template: Guid::parse("0b6f3c1e-2d4a-4e5f-8a9b-1c2d3e4f5a6b"), max: 5, start: 3 };
/// assert!(charges.start <= charges.max);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Meter {
    /// The GUID of the item's entity template.
    pub template: Guid,
    /// The most it holds, 1 or more.
    pub max: u16,
    /// What a new item holds when nothing sets its fill.
    pub start: u16,
}

impl Meter {
    /// The fill `start` stands for, as the game's template keeps it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::content::Guid;
    /// use enshwick_loader_api::content::items::Meter;
    ///
    /// let meter = Meter { template: Guid([0; 16]), max: 4, start: 1 };
    /// assert!((meter.start_fill() - 0.25).abs() < f32::EPSILON);
    /// ```
    #[must_use]
    pub fn start_fill(self) -> f32 {
        fill(self.start, self.max)
    }
}

/// The fill that makes the game give an item `count` of `max`: the game
/// rounds `max` times the fill to the nearest whole count.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::items::fill;
///
/// assert!((fill(3, 5) - 0.6).abs() < f32::EPSILON);
/// assert_eq!(fill(9, 5), 1.0);
/// assert_eq!(fill(1, 0), 0.0);
/// ```
#[must_use]
pub fn fill(count: u16, max: u16) -> f32 {
    if max == 0 {
        return 0.0;
    }
    f32::from(count.min(max)) / f32::from(max)
}

/// A line of the item's tooltip, for [`NewItem::stats`]: the text of a tag
/// and a number, as the game shows an armour's bonuses.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::Guid;
/// use enshwick_loader_api::content::items::{Stat, StatFormat};
///
/// let charges = Stat {
///     label: Guid::parse("c8d2e3f4-5061-4b72-9c83-d4e5f6071829"),
///     value: 3,
///     format: StatFormat::Number,
/// };
/// assert_eq!(charges.value, 3);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Stat {
    /// The tag of the line's text, given text with [`Load::add_texts`].
    pub label: Guid,
    /// The number.
    pub value: i32,
    /// How the number reads.
    pub format: StatFormat,
}

/// How a [`Stat`]'s number reads.
///
/// # Examples
///
/// See [`Stat`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StatFormat {
    /// A plain number: `3`.
    Number,
    /// A number with its sign: `+3`.
    Signed,
    /// A percentage: `3%`.
    Percent,
}

impl StatFormat {
    /// The game's number format for a tooltip value and its "signed" flag.
    const fn game(self) -> (u8, bool) {
        match self {
            Self::Number => (0, false),
            Self::Signed => (0, true),
            Self::Percent => (1, false),
        }
    }
}

/// Where the fields an item edit touches sit, from the game's reflection.
///
/// # Examples
///
/// See [`ItemLayout::resolve`] and [`clone_item`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemLayout {
    /// `ItemInfo.itemId`.
    pub id: usize,
    /// `ItemInfo.objectId`.
    pub object: usize,
    /// `ItemInfo.name`.
    pub name: usize,
    /// `ItemInfo.description`.
    pub description: usize,
    /// `ItemInfo.flags`.
    pub flags: usize,
    /// `ItemInfo.pidEntity`, the template of each item's own entity.
    pub pid_entity: usize,
    /// `ItemInfo.uiValues`, the tooltip's lines.
    pub ui_values: usize,
    /// `ImpactUiValue`: its size, then `locaId`, `valueFormat`, `value` and
    /// `isSigned`.
    pub ui_value: [usize; 5],
    /// `ItemRegistryResource.itemRefs`.
    pub refs: usize,
    /// `ItemRegistryResource.dbgNames`.
    pub names: usize,
}

impl ItemLayout {
    /// The layout on the running build.
    ///
    /// # Errors
    ///
    /// [`Off`] when a field is not reflected.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let layout = enshwick_loader_api::content::items::ItemLayout::resolve(load)?;
    ///     enshwick_loader_api::info!("ItemInfo.itemId at +{:#x}", layout.id);
    ///     Ok(())
    /// }
    /// ```
    pub fn resolve(load: &Load<'_>) -> Result<Self, Off> {
        let field = |ty: &str, name: &str| {
            load.field_offset(ty, name)
                .map(|o| usize::try_from(o).unwrap_or(usize::MAX))
        };
        let item = |name: &str| field("keen::ItemInfo", name);
        let value = |name: &str| field("keen::ImpactUiValue", name);
        Ok(Self {
            id: item("itemId")?,
            object: item("objectId")?,
            name: item("name")?,
            description: item("description")?,
            flags: item("flags")?,
            pid_entity: item("pidEntity")?,
            ui_values: item("uiValues")?,
            ui_value: [
                load.type_size("keen::ImpactUiValue")? as usize,
                value("locaId")?,
                value("valueFormat")?,
                value("value")?,
                value("isSigned")?,
            ],
            refs: field("keen::ItemRegistryResource", "itemRefs")?,
            names: field("keen::ItemRegistryResource", "dbgNames")?,
        })
    }
}

/// Where the fields a [`Meter`] template edit touches sit.
///
/// # Examples
///
/// See [`clone_meter_template`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeterLayout {
    /// `ecs::TemplateResource.components`.
    pub components: usize,
    /// `ecs::FuelSetup`: its size, then `maxFuelCharges` and
    /// `initialFuelPercentage`.
    pub fuel_setup: [usize; 3],
    /// `ecs::TemplateCollectionResource.templates`.
    pub templates: usize,
}

impl MeterLayout {
    /// The layout on the running build.
    ///
    /// # Errors
    ///
    /// [`Off`] when a field is not reflected.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let layout = enshwick_loader_api::content::items::MeterLayout::resolve(load)?;
    ///     assert!(layout.fuel_setup[0] > 0);
    ///     Ok(())
    /// }
    /// ```
    pub fn resolve(load: &Load<'_>) -> Result<Self, Off> {
        let field = |ty: &str, name: &str| {
            load.field_offset(ty, name)
                .map(|o| usize::try_from(o).unwrap_or(usize::MAX))
        };
        Ok(Self {
            components: field("keen::ecs::TemplateResource", "components")?,
            fuel_setup: [
                load.type_size("keen::ecs::FuelSetup")? as usize,
                field("keen::ecs::FuelSetup", "maxFuelCharges")?,
                field("keen::ecs::FuelSetup", "initialFuelPercentage")?,
            ],
            templates: field("keen::ecs::TemplateCollectionResource", "templates")?,
        })
    }
}

/// A GUID reference in a blob.
const REFERENCE: usize = 16;
/// A blob string header.
const BLOB_STRING: usize = 8;
/// A `BlobVariant`: type hash, offset from its second word, size.
const VARIANT: usize = 12;

/// `item`'s `ItemInfo`, made from the vanilla one in `like`: its id, object
/// GUID, the tags it names, its meter and tooltip lines written in, the rest
/// kept. `None` when `like` is too short for the layout, or when the item
/// has a meter and `like` already has an entity of its own (a weapon's
/// durability), which the meter's would replace. [`Load::add_item`] serves
/// it; call it from a [`Load::serve`] builder of your own for an edit
/// `add_item` does not make.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::Guid;
/// use enshwick_loader_api::content::items::{ItemLayout, NewItem, clone_item};
///
/// let layout = ItemLayout {
///     id: 0, object: 4, name: 0x48, description: 0x68, flags: 0x70, pid_entity: 0x78,
///     ui_values: 0x88, ui_value: [0x28, 0, 4, 8, 0x24], refs: 0, names: 0x38,
/// };
/// let item = NewItem::new(
///     Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31"),
///     Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d"),
///     "Safe",
/// );
/// let vanilla = [0u8; 0x90];
/// let clone = clone_item(&layout, &vanilla, &item).ok_or("too short")?;
/// assert_eq!(clone[..4], item.guid.id().to_le_bytes());
/// assert_eq!(clone[4..20], item.guid.0);
/// # Ok::<(), &str>(())
/// ```
#[must_use]
pub fn clone_item(layout: &ItemLayout, like: &[u8], item: &NewItem) -> Option<Vec<u8>> {
    let mut info = Blob::new(like);
    let mut written = info.write(layout.id, &item.guid.id().to_le_bytes())
        && info.write(layout.object, &item.guid.0);
    if let Some(name) = item.name {
        written &= info.write(layout.name, &name.0);
    }
    if let Some(description) = item.description {
        written &= info.write(layout.description, &description.0);
    }
    if let Some(meter) = item.meter {
        let flags = info.read_u32(layout.flags)?;
        if flags & ItemData::HAS_PIDE != 0 || meter.max == 0 {
            return None;
        }
        let flags = flags | ItemData::HAS_PIDE | ItemData::USES_FUEL;
        written &= info.write(layout.flags, &flags.to_le_bytes())
            && info.write(layout.pid_entity, &meter.template.0);
    }
    if !item.stats.is_empty() {
        let [size, label, format, value, signed] = layout.ui_value;
        let mut rows = Vec::with_capacity(item.stats.len());
        for stat in &item.stats {
            let mut row = vec![0u8; size];
            let (number_format, is_signed) = stat.format.game();
            let mut put = |at: usize, bytes: &[u8]| {
                row.get_mut(at..at + bytes.len())
                    .map(|target| target.copy_from_slice(bytes))
            };
            put(label, &stat.label.id().to_le_bytes())?;
            put(format, &[number_format])?;
            put(value, &stat.value.to_le_bytes())?;
            put(signed, &[u8::from(is_signed)])?;
            rows.push((row, None));
        }
        info.grow_array(layout.ui_values, size, 4, &[], &rows)?;
    }
    written.then(|| info.into_bytes())
}

/// A [`Meter`]'s entity template, cloned from the water pouch's in
/// `template`: its `FuelSetup` holds the meter's maximum and starting fill.
/// `None` when the template has no `FuelSetup` of the reflected size.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::{Blob, Guid};
/// use enshwick_loader_api::content::items::{Meter, MeterLayout, clone_meter_template};
///
/// let layout = MeterLayout { components: 0, fuel_setup: [0x28, 0x20, 0x24], templates: 0 };
/// // One component: a FuelSetup of 0x28 bytes right after its variant.
/// let mut template = vec![8, 0, 0, 0, 1, 0, 0, 0];
/// template.extend_from_slice(&0x139a_fbec_u32.to_le_bytes());
/// template.extend_from_slice(&8u32.to_le_bytes());
/// template.extend_from_slice(&0x28u32.to_le_bytes());
/// template.extend_from_slice(&[0; 0x28]);
/// let meter = Meter { template: Guid([1; 16]), max: 3, start: 3 };
/// let clone = Blob::new(&clone_meter_template(&layout, &template, &meter).ok_or("no FuelSetup")?);
/// assert_eq!(clone.read_u32(20 + 0x20), Some(3));
/// # Ok::<(), &str>(())
/// ```
#[must_use]
pub fn clone_meter_template(
    layout: &MeterLayout,
    template: &[u8],
    meter: &Meter,
) -> Option<Vec<u8>> {
    let mut blob = Blob::new(template);
    let (start, count) = blob.array(layout.components)?;
    let [expected, max_at, fill_at] = layout.fuel_setup;
    for variant in (0..usize::try_from(count).ok()?).map(|i| start + i * VARIANT) {
        if blob.read_u32(variant)? != FUEL_SETUP {
            continue;
        }
        let data = (variant + 4).checked_add(usize::try_from(blob.read_u32(variant + 4)?).ok()?)?;
        let size = usize::try_from(blob.read_u32(variant + 8)?).ok()?;
        if size != expected {
            return None;
        }
        let max = i32::from(meter.max);
        let written = blob.write(data + max_at, &max.to_le_bytes())
            && blob.write(data + fill_at, &meter.start_fill().to_le_bytes());
        return written.then(|| blob.into_bytes());
    }
    None
}

/// The template collection with `template` listed after the others.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::{Blob, Guid};
/// use enshwick_loader_api::content::items::{MeterLayout, list_meter_template};
///
/// let layout = MeterLayout { components: 0, fuel_setup: [0x28, 0x20, 0x24], templates: 0 };
/// let collection = list_meter_template(&layout, &[0; 8], Guid([7; 16])).ok_or("no list")?;
/// assert_eq!(Blob::new(&collection).array(0).map(|(_, n)| n), Some(1));
/// # Ok::<(), &str>(())
/// ```
#[must_use]
pub fn list_meter_template(
    layout: &MeterLayout,
    collection: &[u8],
    template: Guid,
) -> Option<Vec<u8>> {
    let mut blob = Blob::new(collection);
    blob.grow_array(
        layout.templates,
        REFERENCE,
        4,
        &[],
        &[(template.0.to_vec(), None)],
    )?;
    Some(blob.into_bytes())
}

/// The item registry with `guid` listed after the others, and its debug name
/// beside it; `None` when the registry does not have the layout's arrays.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::{Guid, types};
/// use enshwick_loader_api::content::items::{ITEM_REGISTRY, ItemLayout, list_item};
///
/// /// Lists an item whose `ItemInfo` the mod serves itself.
/// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
///     let layout = ItemLayout::resolve(load)?;
///     let staff = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
///     let registry = ITEM_REGISTRY.key(types::ITEM_REGISTRY);
///     load.serve(registry, registry, move |base| list_item(&layout, base, staff, "Staff"))
/// }
/// ```
#[must_use]
pub fn list_item(
    layout: &ItemLayout,
    registry: &[u8],
    guid: Guid,
    debug_name: &str,
) -> Option<Vec<u8>> {
    let mut registry = Blob::new(registry);
    let (_, refs) = registry.array(layout.refs)?;
    let (_, names) = registry.array(layout.names)?;
    registry.grow_array(layout.refs, REFERENCE, 4, &[], &[(guid.0.to_vec(), None)])?;
    // The debug names run beside the references; keep them in step.
    if names == refs {
        let mut text = debug_name.as_bytes().to_vec();
        text.push(0);
        let at = registry.append(&text, 1);
        let first = registry.grow_array(
            layout.names,
            BLOB_STRING,
            4,
            &[0],
            &[(vec![0; BLOB_STRING], None)],
        )?;
        let (start, _) = registry.array(layout.names)?;
        let element = start + usize::try_from(first).ok()? * BLOB_STRING;
        if !registry.set_array(element, at, u32::try_from(debug_name.len()).ok()?) {
            return None;
        }
    }
    Some(registry.into_bytes())
}

impl Load<'_> {
    /// Adds an item to the game, on the server and the client alike: an
    /// `ItemInfo` cloned from a vanilla item, listed in the item registry
    /// beside items other mods add, and with a [`Meter`] its entity
    /// template, listed in the template collection. Players see it once a
    /// recipe, loot or a command gives it to them.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader is too old, the item layout is not reflected,
    /// or the meter's maximum is 0.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::content::Guid;
    /// use enshwick_loader_api::content::items::{Meter, NewItem};
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     load.add_item(NewItem {
    ///         meter: Some(Meter {
    ///             template: Guid::parse("0b6f3c1e-2d4a-4e5f-8a9b-1c2d3e4f5a6b"),
    ///             max: 3,
    ///             start: 3,
    ///         }),
    ///         ..NewItem::new(
    ///             Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31"),
    ///             Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d"),
    ///             "Charged safe",
    ///         )
    ///     })
    /// }
    /// ```
    pub fn add_item(&mut self, item: NewItem) -> Result<(), Off> {
        let layout = ItemLayout::resolve(self)?;
        let meter = item.meter;
        if meter.is_some_and(|m| m.max == 0) {
            return Err(Off::new(format!(
                "item {}: a meter holds at least 1",
                item.debug_name
            )));
        }
        let (guid, like, debug_name) = (item.guid, item.like, item.debug_name.clone());
        self.serve(
            guid.key(types::ITEM_INFO),
            like.key(types::ITEM_INFO),
            move |base| clone_item(&layout, base, &item),
        )?;
        let registry = ITEM_REGISTRY.key(types::ITEM_REGISTRY);
        self.serve(registry, registry, move |base| {
            list_item(&layout, base, guid, &debug_name)
        })?;
        if let Some(meter) = meter {
            let layout = MeterLayout::resolve(self)?;
            self.serve(
                meter.template.key(types::TEMPLATE),
                METER_TEMPLATE_BASE.key(types::TEMPLATE),
                move |base| clone_meter_template(&layout, base, &meter),
            )?;
            let collection = super::workshops::TEMPLATE_COLLECTION.key(types::TEMPLATE_COLLECTION);
            self.serve(collection, collection, move |base| {
                list_meter_template(&layout, base, meter.template)
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT: ItemLayout = ItemLayout {
        id: 0,
        object: 4,
        name: 0x48,
        description: 0x68,
        flags: 0x78,
        pid_entity: 0x80,
        ui_values: 0x90,
        ui_value: [0x28, 0, 4, 8, 0x24],
        refs: 0,
        names: 0x38,
    };
    const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    const NEW: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
    const NAME: Guid = Guid::parse("b7c1d2e3-4f50-4a61-8b72-c3d4e5f60718");
    const TEMPLATE: Guid = Guid::parse("0b6f3c1e-2d4a-4e5f-8a9b-1c2d3e4f5a6b");

    fn string(blob: &Blob, header: usize) -> Vec<u8> {
        let (at, len) = blob.array(header).unwrap();
        blob.bytes()[at..at + len as usize].to_vec()
    }

    #[test]
    fn a_clone_takes_the_new_identity_and_keeps_the_rest() {
        let mut chest = vec![0u8; 0x98];
        chest[4..20].copy_from_slice(&CHEST.0);
        chest[0x70] = 9;
        let item = NewItem {
            name: Some(NAME),
            ..NewItem::new(NEW, CHEST, "New")
        };
        let clone = clone_item(&LAYOUT, &chest, &item).unwrap();
        assert_eq!(&clone[..4], &NEW.id().to_le_bytes());
        assert_eq!(&clone[4..20], &NEW.0);
        assert_eq!(&clone[0x48..0x58], &NAME.0);
        assert_eq!(clone[0x70], 9);
        assert_eq!(clone.len(), chest.len(), "nothing grows without stats");
    }

    #[test]
    fn a_meter_gives_the_item_an_entity_that_keeps_fuel() {
        let mut staff = vec![0u8; 0x98];
        staff[0x78..0x7c].copy_from_slice(&(1u32 << 8).to_le_bytes());
        let meter = Meter {
            template: TEMPLATE,
            max: 3,
            start: 3,
        };
        let item = NewItem {
            meter: Some(meter),
            ..NewItem::new(NEW, CHEST, "Staff")
        };
        let clone = Blob::new(&clone_item(&LAYOUT, &staff, &item).unwrap());
        assert_eq!(
            clone.read_u32(0x78),
            Some(1 << 8 | ItemData::HAS_PIDE | ItemData::USES_FUEL)
        );
        assert_eq!(&clone.bytes()[0x80..0x90], &TEMPLATE.0);

        let mut sword = staff.clone();
        sword[0x78..0x7c].copy_from_slice(&ItemData::HAS_PIDE.to_le_bytes());
        assert!(
            clone_item(&LAYOUT, &sword, &item).is_none(),
            "a durability entity stays"
        );
    }

    #[test]
    fn stats_follow_the_vanilla_lines() {
        let mut armour = Blob::new(&[0u8; 0x98]);
        let mut vanilla = vec![0u8; 0x28];
        vanilla[0..4].copy_from_slice(&7u32.to_le_bytes());
        let first = armour.append(&vanilla, 4);
        assert!(armour.set_array(0x90, first, 1));
        let item = NewItem {
            stats: vec![
                Stat {
                    label: NAME,
                    value: 3,
                    format: StatFormat::Number,
                },
                Stat {
                    label: CHEST,
                    value: -2,
                    format: StatFormat::Signed,
                },
            ],
            ..NewItem::new(NEW, CHEST, "Armour")
        };
        let clone = Blob::new(&clone_item(&LAYOUT, armour.bytes(), &item).unwrap());
        let (rows, count) = clone.array(0x90).unwrap();
        assert_eq!(count, 3);
        assert_eq!(clone.read_u32(rows), Some(7));
        assert_eq!(clone.read_u32(rows + 0x28), Some(NAME.id()));
        assert_eq!(clone.read_u32(rows + 0x28 + 8), Some(3));
        assert_eq!(clone.bytes()[rows + 0x28 + 0x24], 0);
        assert_eq!(clone.read_u32(rows + 0x50), Some(CHEST.id()));
        assert_eq!(
            clone.read_u32(rows + 0x50 + 8),
            Some((-2i32).cast_unsigned())
        );
        assert_eq!(clone.bytes()[rows + 0x50 + 0x24], 1);
    }

    #[test]
    fn fills_give_back_the_count() {
        for max in 1..=20u16 {
            for count in 0..=max {
                let back = (f32::from(max) * fill(count, max) + 0.5).floor();
                assert!(
                    (back - f32::from(count)).abs() < f32::EPSILON,
                    "{count}/{max}"
                );
            }
        }
    }

    #[test]
    fn a_meter_template_holds_the_maximum_and_the_start() {
        let layout = MeterLayout {
            components: 0,
            fuel_setup: [0x28, 0x20, 0x24],
            templates: 0,
        };
        let mut template = vec![8, 0, 0, 0, 2, 0, 0, 0];
        // A first component of another type, then the FuelSetup.
        for (hash, offset, size) in [(0x1234_u32, 20u32, 4u32), (FUEL_SETUP, 12, 0x28)] {
            template.extend_from_slice(&hash.to_le_bytes());
            template.extend_from_slice(&offset.to_le_bytes());
            template.extend_from_slice(&size.to_le_bytes());
        }
        template.extend_from_slice(&[0; 4 + 0x28]);
        let meter = Meter {
            template: TEMPLATE,
            max: 5,
            start: 2,
        };
        let clone = Blob::new(&clone_meter_template(&layout, &template, &meter).unwrap());
        let setup = 20 + 4 + 12;
        assert_eq!(clone.read_u32(setup + 0x20), Some(5));
        assert_eq!(clone.read_u32(setup + 0x24), Some(0.4f32.to_bits()));

        let wrong = MeterLayout {
            fuel_setup: [0x30, 0x20, 0x24],
            ..layout
        };
        assert!(clone_meter_template(&wrong, &template, &meter).is_none());
    }

    #[test]
    fn two_mods_can_list_items_one_after_the_other() {
        let mut blob = Blob::new(&[0u8; 0x48]);
        let reference = blob.append(&CHEST.0, 4);
        let text = blob.append(b"Chest\0", 1);
        let name = blob.append(&[0; 8], 4);
        assert!(blob.set_array(name, text, 5));
        assert!(blob.set_array(0, reference, 1));
        assert!(blob.set_array(0x38, name, 1));
        let once = list_item(&LAYOUT, blob.bytes(), NEW, "New").unwrap();
        let twice = Blob::new(&list_item(&LAYOUT, &once, NAME, "Other").unwrap());
        let (refs, count) = twice.array(0).unwrap();
        assert_eq!(count, 3);
        assert_eq!(&twice.bytes()[refs + 32..refs + 48], &NAME.0);
        let (names, _) = twice.array(0x38).unwrap();
        assert_eq!(string(&twice, names), b"Chest");
        assert_eq!(string(&twice, names + 8), b"New");
        assert_eq!(string(&twice, names + 16), b"Other");
    }
}
