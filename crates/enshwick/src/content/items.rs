//! New items: an item cloned from a vanilla one, listed in the item registry,
//! with its own name, tooltip lines and a meter each item keeps.

use std::sync::Mutex;

use crate::abi::ItemData;
use crate::engine_ids as ids;
use crate::locks::Relock;
use crate::{Load, Off};

use super::{Blob, EngineFacts, Guid, Id, types};

/// The meter templates the mod serves.
static METERS: crate::fresh::Fresh<Mutex<Vec<FuelSetup>>> = crate::fresh::Fresh::new();

/// The game's item registry resource.
pub const ITEM_REGISTRY: Guid = Guid::parse("3064d35d-7342-40ca-bdc9-aad58f83bf45");

/// The template of the water pouch's own entity, `Pide_Fuel_Water_Pouch`:
/// the base every [`FuelSetup`] template is cloned from.
pub const METER_TEMPLATE_BASE: Guid = Guid::parse("39246eb2-1b9f-4881-8c60-e0e4f5d956f8");

/// The tag of a vanilla item's name, read from the game's archive with the
/// item's name field; `None` for an item the archive lacks or a loader
/// without the archive or the field.
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
/// use enshwick::content::items::vanilla_name_tag;
///
/// let chest = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
/// if let Some(tag) = vanilla_name_tag(chest) {
///     enshwick::info!("the chest's name is text {:#010x}", tag.fnv1a());
/// }
/// ```
#[must_use]
pub fn vanilla_name_tag(item: Guid) -> Option<Guid> {
    let host = crate::host()?;
    let at = host.offset(ids::ITEM_INFO_NAME).ok()?;
    let info = host.game_resource(item.key_by_hash(host.hash(ids::ITEM_INFO).ok()?))?;
    Some(Guid(info.get(at..at.checked_add(16)?)?.try_into().ok()?))
}

/// The name a vanilla item has in the game in `language`, as the player
/// sees it: from the game's own data and texts, on a client (see
/// [`crate::i18n::game_text_in`]).
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
/// use enshwick::content::items::vanilla_name;
/// use enshwick::i18n::Language;
///
/// let chest = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
/// let name = vanilla_name(chest, Language::UK_UA).unwrap_or("Chest");
/// assert!(!name.is_empty());
/// ```
#[must_use]
pub fn vanilla_name(item: Guid, language: crate::i18n::Language) -> Option<&'static str> {
    // Only the client's archive holds texts; skip reading the item there.
    if crate::host()?.build().side == crate::abi::Side::SERVER {
        return None;
    }
    crate::i18n::game_text_in(vanilla_name_tag(item)?.fnv1a(), language)
}

/// A new item for [`Load::add_item`].
///
/// # Examples
///
/// ```
/// use enshwick::content::{Guid, Id};
/// use enshwick::content::items::NewItem;
///
/// let item = NewItem::new(
///     Id::new("you.lockers:locker"),
///     Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d"),
/// );
/// assert_eq!(item.debug_name, "you.lockers:locker");
/// assert!(item.meter.is_none() && item.stats.is_empty());
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewItem {
    /// The new item's id; its game id is [`Id::id`].
    pub id: Id,
    /// The vanilla item it is cloned from: what it looks like and does.
    pub like: Guid,
    /// The tag of its name, given text with [`Load::add_texts`]; `None` keeps
    /// the vanilla item's name.
    pub name: Option<Guid>,
    /// The tag of its description; `None` keeps the vanilla one.
    pub description: Option<Guid>,
    /// The name in the registry's debug list; the id unless set.
    pub debug_name: String,
    /// A count each item keeps, such as charges, which the game saves,
    /// sends to players and shows as a bar on the item.
    pub meter: Option<FuelSetup>,
    /// Lines the item's tooltip shows after the vanilla item's.
    pub stats: Vec<ImpactUiValue>,
    /// An icon of its own; `None` keeps the vanilla item's. It shows on a
    /// client whose loader knows the build's texture code.
    pub icon: Option<super::icons::Icon>,
}

impl NewItem {
    /// An item cloned from `like` with no edits but its identity.
    ///
    /// # Examples
    ///
    /// See [`NewItem`].
    #[must_use]
    pub fn new(id: Id, like: Guid) -> Self {
        Self {
            id,
            like,
            name: None,
            description: None,
            debug_name: id.name().to_owned(),
            meter: None,
            stats: Vec::new(),
            icon: None,
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
/// The item gets an entity template, cloned from the water pouch's:
/// `template` is that template's id, the mod's own and fixed forever, as
/// saves hold its GUID. Items with the same meter may share one template.
///
/// # Examples
///
/// ```
/// use enshwick::content::Id;
/// use enshwick::content::items::FuelSetup;
///
/// let charges = FuelSetup { template: Id::new("my-mod:charges"), max_fuel_charges: 5, start: 3 };
/// assert!(charges.start <= charges.max_fuel_charges);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FuelSetup {
    /// The id of the item's entity template; the name is enshwick's.
    pub template: Id,
    /// The most it holds, 1 or more.
    pub max_fuel_charges: u16,
    /// What a new item holds when nothing sets its fill, a count where the
    /// game keeps a percentage; the name is enshwick's.
    pub start: u16,
}

impl FuelSetup {
    /// The fill `start` stands for, as the game's template keeps it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Id;
    /// use enshwick::content::items::FuelSetup;
    ///
    /// let meter = FuelSetup { template: Id::new("my-mod:charges"), max_fuel_charges: 4, start: 1 };
    /// assert!((meter.start_fill() - 0.25).abs() < f32::EPSILON);
    /// ```
    #[must_use]
    pub fn start_fill(self) -> f32 {
        fill(self.start, self.max_fuel_charges)
    }
}

/// The fill that makes the game give an item `count` of `max`: the game
/// rounds `max` times the fill to the nearest whole count.
///
/// # Examples
///
/// ```
/// use enshwick::content::items::fill;
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
/// use enshwick::content::Guid;
/// use enshwick::content::items::{ImpactUiValue, StatFormat};
///
/// let charges = ImpactUiValue {
///     loca_id: Guid::parse("c8d2e3f4-5061-4b72-9c83-d4e5f6071829"),
///     value: 3,
///     value_format: StatFormat::Number,
/// };
/// assert_eq!(charges.value, 3);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImpactUiValue {
    /// The tag of the line's text, given text with [`Load::add_texts`].
    pub loca_id: Guid,
    /// The number.
    pub value: i32,
    /// How the number reads.
    pub value_format: StatFormat,
}

/// How a [`ImpactUiValue`]'s number reads.
///
/// The name is enshwick's: the game's number format together with its signed
/// flag.
///
/// # Examples
///
/// See [`ImpactUiValue`].
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

/// Where the fields an item edit touches sit on the running build, resolved
/// by the loader.
///
/// # Examples
///
/// See [`ItemLayout::resolve`] and [`clone_item`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemLayout {
    /// The item's game id.
    pub id: usize,
    /// The item's GUID.
    pub object: usize,
    /// The tag of its name.
    pub name: usize,
    /// The tag of its description.
    pub description: usize,
    /// Its flags, such as [`ItemData::HAS_PIDE`].
    pub flags: usize,
    /// The template of each item's own entity.
    pub pid_entity: usize,
    /// The tooltip's lines.
    pub ui_values: usize,
    /// The UI image of its icon.
    pub icon_image: usize,
    /// One tooltip line: its size, then its label, number format, value and
    /// "signed" flag.
    pub ui_value: [usize; 5],
    /// The item registry's references to every item.
    pub refs: usize,
    /// The item registry's debug names, beside the references.
    pub names: usize,
}

impl ItemLayout {
    /// The layout `facts` give: the running game's through [`Load`], a game
    /// image's in a tool.
    ///
    /// # Errors
    ///
    /// The first [`Off`] a fact returns, when the build lacks it.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let layout = enshwick::content::items::ItemLayout::resolve(load)?;
    ///     enshwick::info!("an item's id at +{:#x}", layout.id);
    ///     Ok(())
    /// }
    /// ```
    pub fn resolve(facts: &(impl EngineFacts + ?Sized)) -> Result<Self, Off> {
        let at = |id: &str| facts.offset(id);
        Ok(Self {
            id: at(ids::ITEM_INFO_ID)?,
            object: at(ids::ITEM_INFO_OBJECT)?,
            name: at(ids::ITEM_INFO_NAME)?,
            description: at(ids::ITEM_INFO_DESCRIPTION)?,
            flags: at(ids::ITEM_INFO_FLAGS)?,
            pid_entity: at(ids::ITEM_INFO_PID_ENTITY)?,
            ui_values: at(ids::ITEM_INFO_UI_VALUES)?,
            icon_image: at(ids::ITEM_INFO_ICON_IMAGE)?,
            ui_value: [
                facts.size(ids::ITEM_STAT)?,
                at(ids::ITEM_STAT_LABEL)?,
                at(ids::ITEM_STAT_FORMAT)?,
                at(ids::ITEM_STAT_VALUE)?,
                at(ids::ITEM_STAT_SIGNED)?,
            ],
            refs: at(ids::ITEM_REGISTRY_REFS)?,
            names: at(ids::ITEM_REGISTRY_NAMES)?,
        })
    }
}

/// Where the fields a [`FuelSetup`] template edit touches sit on the running
/// build, resolved by the loader.
///
/// # Examples
///
/// See [`clone_meter_template`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeterLayout {
    /// A template's components.
    pub components: usize,
    /// The component that holds a meter's maximum and starting fill: its
    /// size, then where the maximum and the fill sit.
    pub fuel_setup: [usize; 3],
    /// The hash of that component's type.
    pub fuel_setup_type: u32,
    /// The template collection's list of templates.
    pub templates: usize,
}

impl MeterLayout {
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
    ///     let layout = enshwick::content::items::MeterLayout::resolve(load)?;
    ///     assert!(layout.fuel_setup[0] > 0);
    ///     Ok(())
    /// }
    /// ```
    pub fn resolve(facts: &(impl EngineFacts + ?Sized)) -> Result<Self, Off> {
        Ok(Self {
            components: facts.offset(ids::TEMPLATE_COMPONENTS)?,
            fuel_setup: [
                facts.size(ids::FUEL_SETUP)?,
                facts.offset(ids::FUEL_SETUP_MAX)?,
                facts.offset(ids::FUEL_SETUP_START)?,
            ],
            fuel_setup_type: facts.hash(ids::FUEL_SETUP)?,
            templates: facts.offset(ids::TEMPLATE_COLLECTION_TEMPLATES)?,
        })
    }
}

/// A GUID reference in a blob.
const REFERENCE: usize = 16;
/// A blob string header.
const BLOB_STRING: usize = 8;
/// A `BlobVariant`: type hash, offset from its second word, size.
const VARIANT: usize = 12;

/// `item`'s item data under `guid`, its own or a former one, made from the
/// vanilla one in `like` with its id, object GUID, tags, meter and lines.
///
/// `None` when `like` is too short for the layout, or when the item has a meter
/// and `like` already has an entity of its own (a weapon's durability), which
/// the meter's would replace. [`Load::add_item`] serves it; call it from a
/// [`Load::serve`] builder of your own for an edit `add_item` does not make.
///
/// # Examples
///
/// ```
/// use enshwick::content::{Guid, Id};
/// use enshwick::content::items::{ItemLayout, NewItem, clone_item};
///
/// let layout = ItemLayout {
///     id: 0, object: 4, name: 0x48, description: 0x68, flags: 0x70, pid_entity: 0x78,
///     ui_values: 0x88, icon_image: 0x58, ui_value: [0x28, 0, 4, 8, 0x24], refs: 0, names: 0x38,
/// };
/// let item = NewItem::new(
///     Id::new("you.lockers:locker"),
///     Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d"),
/// );
/// let vanilla = [0u8; 0x90];
/// let clone = clone_item(&layout, &vanilla, &item, item.id.guid()).ok_or("too short")?;
/// assert_eq!(clone[..4], enshwick::ItemId::from_guid(item.id.guid()).get().to_le_bytes());
/// assert_eq!(clone[4..20], item.id.guid().0);
/// # Ok::<(), &str>(())
/// ```
#[must_use]
pub fn clone_item(layout: &ItemLayout, like: &[u8], item: &NewItem, guid: Guid) -> Option<Vec<u8>> {
    let mut info = Blob::new(like);
    let mut written =
        info.write(layout.id, &guid.fnv1a().to_le_bytes()) && info.write(layout.object, &guid.0);
    if let Some(name) = item.name {
        written &= info.write(layout.name, &name.0);
    }
    if let Some(description) = item.description {
        written &= info.write(layout.description, &description.0);
    }
    if let Some(meter) = item.meter {
        let flags = info.read_u32(layout.flags)?;
        if flags & ItemData::HAS_PIDE != 0 || meter.max_fuel_charges == 0 {
            return None;
        }
        let flags = flags | ItemData::HAS_PIDE | ItemData::USES_FUEL;
        written &= info.write(layout.flags, &flags.to_le_bytes())
            && info.write(layout.pid_entity, &meter.template.guid().0);
    }
    if let Some(icon) = item.icon {
        written &= info.write(layout.icon_image, &icon.id.guid().0);
    }
    if !item.stats.is_empty() {
        let [size, label, format, value, signed] = layout.ui_value;
        let mut rows = Vec::with_capacity(item.stats.len());
        for stat in &item.stats {
            let mut row = vec![0u8; size];
            let (number_format, is_signed) = stat.value_format.game();
            let mut put = |at: usize, bytes: &[u8]| {
                row.get_mut(at..at + bytes.len())
                    .map(|target| target.copy_from_slice(bytes))
            };
            put(label, &stat.loca_id.fnv1a().to_le_bytes())?;
            put(format, &[number_format])?;
            put(value, &stat.value.to_le_bytes())?;
            put(signed, &[u8::from(is_signed)])?;
            rows.push((row, None));
        }
        info.grow_array(layout.ui_values, size, 4, &[], &rows)?;
    }
    written.then(|| info.into_bytes())
}

/// A [`FuelSetup`]'s entity template, cloned from the water pouch's in
/// `template`: its fuel setup holds the meter's maximum and starting fill.
/// `None` when the template has no fuel setup of the layout's size.
///
/// # Examples
///
/// ```
/// use enshwick::content::{Blob, Guid};
/// use enshwick::content::items::{FuelSetup, MeterLayout, clone_meter_template};
///
/// let layout = MeterLayout {
///     components: 0, fuel_setup: [0x28, 0x20, 0x24], fuel_setup_type: 7, templates: 0,
/// };
/// // One component: a fuel setup of 0x28 bytes right after its variant.
/// let mut template = vec![8, 0, 0, 0, 1, 0, 0, 0];
/// template.extend_from_slice(&7u32.to_le_bytes());
/// template.extend_from_slice(&8u32.to_le_bytes());
/// template.extend_from_slice(&0x28u32.to_le_bytes());
/// template.extend_from_slice(&[0; 0x28]);
/// let meter = FuelSetup { template: enshwick::content::Id::new("my-mod:charges"), max_fuel_charges: 3, start: 3 };
/// let clone = Blob::new(&clone_meter_template(&layout, &template, &meter).ok_or("no fuel setup")?);
/// assert_eq!(clone.read_u32(20 + 0x20), Some(3));
/// # Ok::<(), &str>(())
/// ```
#[must_use]
pub fn clone_meter_template(
    layout: &MeterLayout,
    template: &[u8],
    meter: &FuelSetup,
) -> Option<Vec<u8>> {
    let mut blob = Blob::new(template);
    let (start, count) = blob.array(layout.components)?;
    let [expected, max_at, fill_at] = layout.fuel_setup;
    for variant in (0..usize::try_from(count).ok()?).map(|i| start + i * VARIANT) {
        if blob.read_u32(variant)? != layout.fuel_setup_type {
            continue;
        }
        let data = (variant + 4).checked_add(usize::try_from(blob.read_u32(variant + 4)?).ok()?)?;
        let size = usize::try_from(blob.read_u32(variant + 8)?).ok()?;
        if size != expected {
            return None;
        }
        let max = i32::from(meter.max_fuel_charges);
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
/// use enshwick::content::{Blob, Guid};
/// use enshwick::content::items::{MeterLayout, list_meter_template};
///
/// let layout = MeterLayout {
///     components: 0, fuel_setup: [0x28, 0x20, 0x24], fuel_setup_type: 7, templates: 0,
/// };
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
/// beside it.
///
/// `None` when the registry does not have the layout's arrays or lists another
/// item with `guid`'s game id, which would stand for both.
///
/// # Examples
///
/// ```
/// use enshwick::content::{Guid, types};
/// use enshwick::content::items::{ITEM_REGISTRY, ItemLayout, list_item};
///
/// /// Lists an item whose item data the mod serves itself.
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let layout = ItemLayout::resolve(load)?;
///     let staff = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
///     let registry = ITEM_REGISTRY.key(types::ITEM_REGISTRY_RESOURCE)?;
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
    let (first, refs) = registry.array(layout.refs)?;
    let (_, names) = registry.array(layout.names)?;
    let listed = registry
        .bytes()
        .get(first..first.checked_add(usize::try_from(refs).ok()?.checked_mul(REFERENCE)?)?)?;
    if let Some(other) = listed
        .as_chunks::<REFERENCE>()
        .0
        .iter()
        .map(|r| Guid(*r))
        .find(|other| other.fnv1a() == guid.fnv1a() && *other != guid)
    {
        crate::warn!(
            "item {debug_name} ({guid}) has the game id {:#010x} of item {other}; it needs another id",
            guid.fnv1a()
        );
        return None;
    }
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
    /// Adds an item to the game, on the server and the client alike: the
    /// item data cloned from a vanilla item, listed in the item registry
    /// beside items other mods add, and with a [`FuelSetup`] its entity
    /// template, listed in the template collection. Players see it once a
    /// recipe, loot or an action gives it to them.
    ///
    /// # Errors
    ///
    /// [`Off`] when the item layout is not reflected,
    /// the meter's maximum is 0, or the loader refuses one of its content ids
    /// (see [`Load::declare`]).
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::{Guid, Id};
    /// use enshwick::content::items::{FuelSetup, NewItem};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.add_item(NewItem {
    ///         meter: Some(FuelSetup { template: Id::new("my-mod:charges"), max_fuel_charges: 3, start: 3 }),
    ///         ..NewItem::new(
    ///             Id::new("my-mod:charged-locker"),
    ///             Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d"),
    ///         )
    ///     })
    /// }
    /// ```
    pub fn add_item(&mut self, item: NewItem) -> Result<(), Off> {
        self.declare(item.id)?;
        if let Some(meter) = item.meter {
            self.declare(meter.template)?;
        }
        for old in crate::migrations::old_guids(item.id.guid()) {
            let debug_name = format!("{} (renamed)", item.debug_name);
            self.add_one_item(item.clone(), old, debug_name)?;
        }
        let (guid, debug_name) = (item.id.guid(), item.debug_name.clone());
        self.add_one_item(item, guid, debug_name)
    }

    fn add_one_item(&mut self, item: NewItem, guid: Guid, debug_name: String) -> Result<(), Off> {
        let layout = ItemLayout::resolve(self)?;
        let meter = item.meter;
        if meter.is_some_and(|m| m.max_fuel_charges == 0) {
            return Err(Off::new(format!(
                "item {}: a meter holds at least 1",
                item.debug_name
            )));
        }
        let like = item.like;
        if let Some(icon) = item.icon {
            self.add_icon(icon)?;
        }
        self.serve(
            guid.key(types::ITEM_INFO)?,
            like.key(types::ITEM_INFO)?,
            move |base| {
                // Naming an icon the client's UI does not know ends the
                // game: only once the loader says it shows served ones.
                if item.icon.is_some() && !super::icons::ready() {
                    let vanilla = NewItem {
                        icon: None,
                        ..item.clone()
                    };
                    return clone_item(&layout, base, &vanilla, guid);
                }
                clone_item(&layout, base, &item, guid)
            },
        )?;
        let registry = ITEM_REGISTRY.key(types::ITEM_REGISTRY_RESOURCE)?;
        self.serve(registry, registry, move |base| {
            list_item(&layout, base, guid, &debug_name)
        })?;
        if let Some(meter) = meter {
            // Items of one kind may share their template: serve it once.
            let mut served = METERS.get_or_init(Mutex::default).relock();
            match served.iter().find(|m| m.template == meter.template) {
                Some(same) if *same == meter => return Ok(()),
                Some(_) => {
                    return Err(Off::new(format!(
                        "meter template {}: served already with another maximum or start",
                        meter.template
                    )));
                }
                None => served.push(meter),
            }
            drop(served);
            let layout = MeterLayout::resolve(self)?;
            self.serve(
                meter.template.key(types::TEMPLATE_RESOURCE)?,
                METER_TEMPLATE_BASE.key(types::TEMPLATE_RESOURCE)?,
                move |base| clone_meter_template(&layout, base, &meter),
            )?;
            let collection =
                super::workshops::TEMPLATE_COLLECTION.key(types::TEMPLATE_COLLECTION_RESOURCE)?;
            self.serve(collection, collection, move |base| {
                list_meter_template(&layout, base, meter.template.guid())
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
        icon_image: 0x58,
        ui_value: [0x28, 0, 4, 8, 0x24],
        refs: 0,
        names: 0x38,
    };
    const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    const NEW_ID: Id = Id::new("test:new");
    const NEW: Guid = NEW_ID.guid();
    const NAME: Guid = Guid::parse("b7c1d2e3-4f50-4a61-8b72-c3d4e5f60718");
    const TEMPLATE_ID: Id = Id::new("test:charges");
    const TEMPLATE: Guid = TEMPLATE_ID.guid();

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
            ..NewItem::new(NEW_ID, CHEST)
        };
        let clone = clone_item(&LAYOUT, &chest, &item, NEW).unwrap();
        assert_eq!(&clone[..4], &NEW.fnv1a().to_le_bytes());
        assert_eq!(&clone[4..20], &NEW.0);
        assert_eq!(&clone[0x48..0x58], &NAME.0);
        assert_eq!(clone[0x70], 9);
        assert_eq!(clone.len(), chest.len(), "nothing grows without stats");
    }

    #[test]
    fn a_meter_gives_the_item_an_entity_that_keeps_fuel() {
        let mut staff = vec![0u8; 0x98];
        staff[0x78..0x7c].copy_from_slice(&(1u32 << 8).to_le_bytes());
        let meter = FuelSetup {
            template: TEMPLATE_ID,
            max_fuel_charges: 3,
            start: 3,
        };
        let item = NewItem {
            meter: Some(meter),
            ..NewItem::new(NEW_ID, CHEST)
        };
        let clone = Blob::new(&clone_item(&LAYOUT, &staff, &item, NEW).unwrap());
        assert_eq!(
            clone.read_u32(0x78),
            Some(1 << 8 | ItemData::HAS_PIDE | ItemData::USES_FUEL)
        );
        assert_eq!(&clone.bytes()[0x80..0x90], &TEMPLATE.0);

        let mut sword = staff.clone();
        sword[0x78..0x7c].copy_from_slice(&ItemData::HAS_PIDE.to_le_bytes());
        assert!(
            clone_item(&LAYOUT, &sword, &item, NEW).is_none(),
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
                ImpactUiValue {
                    loca_id: NAME,
                    value: 3,
                    value_format: StatFormat::Number,
                },
                ImpactUiValue {
                    loca_id: CHEST,
                    value: -2,
                    value_format: StatFormat::Signed,
                },
            ],
            ..NewItem::new(NEW_ID, CHEST)
        };
        let clone = Blob::new(&clone_item(&LAYOUT, armour.bytes(), &item, NEW).unwrap());
        let (rows, count) = clone.array(0x90).unwrap();
        assert_eq!(count, 3);
        assert_eq!(clone.read_u32(rows), Some(7));
        assert_eq!(clone.read_u32(rows + 0x28), Some(NAME.fnv1a()));
        assert_eq!(clone.read_u32(rows + 0x28 + 8), Some(3));
        assert_eq!(clone.bytes()[rows + 0x28 + 0x24], 0);
        assert_eq!(clone.read_u32(rows + 0x50), Some(CHEST.fnv1a()));
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
            fuel_setup_type: 7,
            templates: 0,
        };
        let mut template = vec![8, 0, 0, 0, 2, 0, 0, 0];
        // A first component of another type, then the fuel setup.
        for (hash, offset, size) in [(0x1234_u32, 20u32, 4u32), (7, 12, 0x28)] {
            template.extend_from_slice(&hash.to_le_bytes());
            template.extend_from_slice(&offset.to_le_bytes());
            template.extend_from_slice(&size.to_le_bytes());
        }
        template.extend_from_slice(&[0; 4 + 0x28]);
        let meter = FuelSetup {
            template: TEMPLATE_ID,
            max_fuel_charges: 5,
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
