//! Buff types of a mod's own: a copy of a vanilla type in the game's one buff
//! collection, under the mod's id, with its own texts, icon and timing.

use core::time::Duration;

use crate::Off;
use crate::abi::{BuffTypeAdd, BuffTypeInfo, Status};
use crate::content::icons::Icon;
use crate::content::{Blob, Draft, EngineFacts, Guid, Id, ResourceType, sized};
use crate::engine_ids as ids;
use crate::resources::Grow;
use enshwick_content::content::LoadMethods as _;
use enshwick_content::content::icons::LoadMethods as _;

use super::{BuffSlotType, BuffTypeId};

/// The game's one buff collection, on the client and the server alike
/// (client 26430625, server 26430253).
pub const BUFF_COLLECTION: Guid = Guid::parse("02022e86-8efe-40cb-8f6f-d22071cc3be3");

/// The type of the buff collection.
pub const BUFF_COLLECTION_RESOURCE: ResourceType = ResourceType::new(ids::BUFF_COLLECTION);

/// The alignment of the collection's arrays of buff types, which hold a
/// 64-bit time; the engine facts give sizes and offsets, not alignments.
const BUFF_ALIGN: usize = 8;
/// The size of a hash map key, a [`BuffTypeId`], and of a value, a `u32`
/// index into the buff types.
const KEY: usize = size_of::<u32>();

/// A buff type of a mod's own, for [`Load::add_buff`](../struct.Load.html#method.add_buff):
/// a copy of the vanilla type `like` with what the mod changes.
///
/// Every `None` keeps the vanilla type's value. Give it stat modifiers with
/// [`Load::add_buff_modifiers`](../struct.Load.html#method.add_buff_modifiers)
/// and apply it with [`Buffs::apply`](super::Buffs::apply).
///
/// # Examples
///
/// ```
/// use core::time::Duration;
/// use enshwick::buffs::{BuffTypeId, NewBuff};
/// use enshwick::content::id;
///
/// let lucky = NewBuff::new(id!("lucky"), BuffTypeId(0xf9b3_64f6))
///     .default_life_time(Duration::from_mins(5))
///     .plain();
/// assert_eq!(lucky.slot, None);
/// ```
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct NewBuff {
    /// The new type's id; its game id is [`BuffTypeId::from_guid`] of its
    /// GUID.
    pub id: Id,
    /// The vanilla type it copies, from [`Buffs::types`](super::Buffs::types).
    pub like: BuffTypeId,
    /// The tag of its name, given text with [`Load::add_texts`](../struct.Load.html#method.add_texts).
    #[script(default)]
    pub name: Option<Guid>,
    /// The tag of its description.
    #[script(default)]
    pub description: Option<Guid>,
    /// An icon of its own for the HUD's buff row; it shows on a client whose
    /// loader knows the build's texture code.
    #[script(default)]
    pub icon: Option<Icon>,
    /// How long it lasts, in game time.
    #[script(default)]
    pub default_life_time: Option<Duration>,
    /// Where an entity holds it.
    #[script(default)]
    pub slot: Option<BuffSlotType>,
    /// Whether the game shows it nowhere.
    #[script(default)]
    pub hide_in_ui: Option<bool>,
    /// Whether the HUD hides how long it has left.
    #[script(default)]
    pub hide_duration_in_ui: Option<bool>,
    /// Whether the game saves it with a character; a world loaded without
    /// the mod drops a saved one.
    #[script(default)]
    pub is_saved: Option<bool>,
    /// Drops what the vanilla type runs: its impact program and values,
    /// tooltip lines, helpers, follow-up and effects, so of what it does to
    /// stats only the mod's modifiers act. It keeps how the vanilla type
    /// looks and whom it reaches: its colour, food poison mark, apply type,
    /// faction mask and whether it ends at death. The name is enshwick's.
    #[script(default)]
    pub plain: bool,
}

impl NewBuff {
    /// A copy of `like` with no change but its identity.
    ///
    /// # Examples
    ///
    /// See [`NewBuff`].
    #[must_use]
    pub const fn new(id: Id, like: BuffTypeId) -> Self {
        Self {
            id,
            like,
            name: None,
            description: None,
            icon: None,
            default_life_time: None,
            slot: None,
            hide_in_ui: None,
            hide_duration_in_ui: None,
            is_saved: None,
            plain: false,
        }
    }

    /// Sets the tag of its name.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::buffs::{BuffTypeId, NewBuff};
    /// use enshwick::content::id;
    ///
    /// let name = id!("lucky").part("name");
    /// let lucky = NewBuff::new(id!("lucky"), BuffTypeId(0xf9b3_64f6)).name(name.guid());
    /// assert_eq!(lucky.name, Some(name.guid()));
    /// ```
    #[must_use]
    pub const fn name(mut self, tag: Guid) -> Self {
        self.name = Some(tag);
        self
    }

    /// Sets the tag of its description.
    ///
    /// # Examples
    ///
    /// See [`NewBuff::name`].
    #[must_use]
    pub const fn description(mut self, tag: Guid) -> Self {
        self.description = Some(tag);
        self
    }

    /// Gives it an icon of its own.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::buffs::{BuffTypeId, NewBuff};
    /// use enshwick::content::icons::{Icon, Pixels, Tint};
    /// use enshwick::content::{Guid, id};
    ///
    /// let icon = Icon::new(id!("lucky").part("icon"), Guid::parse("02778393-2031-44cd-b7cd-3bc215acd7af"), Pixels::Tint(Tint::new([60, 200, 90], 70)));
    /// let lucky = NewBuff::new(id!("lucky"), BuffTypeId(0xf9b3_64f6)).icon(icon);
    /// assert!(lucky.icon.is_some());
    /// ```
    #[must_use]
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Sets how long it lasts.
    ///
    /// # Examples
    ///
    /// See [`NewBuff`].
    #[must_use]
    pub const fn default_life_time(mut self, life_time: Duration) -> Self {
        self.default_life_time = Some(life_time);
        self
    }

    /// Sets where an entity holds it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::buffs::{BuffSlotType, BuffTypeId, NewBuff};
    /// use enshwick::content::id;
    ///
    /// let lucky = NewBuff::new(id!("lucky"), BuffTypeId(0xf9b3_64f6)).slot(BuffSlotType::Buff);
    /// assert_eq!(lucky.slot, Some(BuffSlotType::Buff));
    /// ```
    #[must_use]
    pub const fn slot(mut self, slot: BuffSlotType) -> Self {
        self.slot = Some(slot);
        self
    }

    /// Sets whether the game shows it nowhere.
    ///
    /// # Examples
    ///
    /// See [`NewBuff::slot`].
    #[must_use]
    pub const fn hide_in_ui(mut self, hide: bool) -> Self {
        self.hide_in_ui = Some(hide);
        self
    }

    /// Sets whether the HUD hides how long it has left.
    ///
    /// # Examples
    ///
    /// See [`NewBuff::slot`].
    #[must_use]
    pub const fn hide_duration_in_ui(mut self, hide: bool) -> Self {
        self.hide_duration_in_ui = Some(hide);
        self
    }

    /// Sets whether the game saves it with a character.
    ///
    /// # Examples
    ///
    /// See [`NewBuff::slot`].
    #[must_use]
    #[expect(
        clippy::wrong_self_convention,
        reason = "a setter, named as its field after the game's isSaved"
    )]
    pub const fn is_saved(mut self, saved: bool) -> Self {
        self.is_saved = Some(saved);
        self
    }

    /// Drops what the vanilla type runs, as [`NewBuff::plain`](#structfield.plain) says.
    ///
    /// # Examples
    ///
    /// See [`NewBuff`].
    #[must_use]
    pub const fn plain(mut self) -> Self {
        self.plain = true;
        self
    }
}

impl BuffSlotType {
    /// The game's slot value; `None` for an [`BuffSlotType::Other`] past a
    /// byte.
    fn to_slot(self) -> Option<u8> {
        let slot = match self {
            Self::Food => BuffTypeInfo::FOOD,
            Self::Buff => BuffTypeInfo::BUFF,
            Self::Debuff => BuffTypeInfo::DEBUFF,
            Self::Rested => BuffTypeInfo::RESTED,
            Self::TerrainDebuff => BuffTypeInfo::TERRAIN,
            Self::Other(other) => other,
        };
        u8::try_from(slot).ok()
    }
}

/// Where the fields a new buff type touches sit on the running build,
/// resolved by the loader.
///
/// # Examples
///
/// ```
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let layout = enshwick::buffs::BuffLayout::resolve(load)?;
///     enshwick::info!("a buff type takes {} bytes", layout.size);
///     Ok(())
/// }
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct BuffLayout {
    /// The collection's buff types.
    pub buffs: usize,
    /// The hash map's buckets.
    pub entries: usize,
    /// The hash map's keys.
    pub keys: usize,
    /// The hash map's values.
    pub values: usize,
    /// One bucket's size.
    pub bucket: usize,
    /// A bucket's first key.
    pub bucket_start: usize,
    /// A bucket's key count.
    pub bucket_count: usize,
    /// One buff type's size.
    pub size: usize,
    /// Its id.
    pub id: usize,
    /// Its slot.
    pub slot: usize,
    /// How long it lasts.
    pub life_time: usize,
    /// The tag of its name.
    pub name: usize,
    /// The tag of its description.
    pub description: usize,
    /// Whether it shows nowhere.
    pub hide_in_ui: usize,
    /// Its icon's UI image.
    pub icon: usize,
    /// Whether the HUD hides its time.
    pub hide_duration_in_ui: usize,
    /// Whether it is saved.
    pub is_saved: usize,
    /// Its impact program.
    pub impact: usize,
    /// The impact program reference's size.
    pub impact_size: usize,
    /// The buff that follows when it runs out.
    pub on_timeout: usize,
    /// The follow-up's size.
    pub on_timeout_size: usize,
    /// Whether a buff follows.
    pub on_timeout_is_set: usize,
    /// The array headers inside a buff type: impact values simple and
    /// scaled, tooltip lines, helpers on create and while applied, and the
    /// effect meshes, models and joint collections.
    pub relative: [usize; 8],
}

impl BuffLayout {
    /// The layout `facts` give: the running game's through [`Load`](../struct.Load.html), a game
    /// image's in a tool.
    ///
    /// # Errors
    ///
    /// The first [`Off`] a fact returns, when the build lacks it.
    ///
    /// # Examples
    ///
    /// See [`BuffLayout`].
    pub fn resolve(facts: &(impl EngineFacts + ?Sized)) -> Result<Self, Off> {
        let at = |id: &str| facts.offset(id);
        Ok(Self {
            buffs: at(ids::BUFF_COLLECTION_BUFFS)?,
            entries: at(ids::BUFF_COLLECTION_ENTRIES)?,
            keys: at(ids::BUFF_COLLECTION_KEYS)?,
            values: at(ids::BUFF_COLLECTION_VALUES)?,
            bucket: facts.size(ids::BUFF_MAP_ENTRY)?,
            bucket_start: at(ids::BUFF_MAP_ENTRY_START)?,
            bucket_count: at(ids::BUFF_MAP_ENTRY_COUNT)?,
            size: facts.size(ids::BUFF_TYPE)?,
            id: at(ids::BUFF_TYPE_ID)?,
            slot: at(ids::BUFF_TYPE_SLOT)?,
            life_time: at(ids::BUFF_TYPE_LIFE_TIME)?,
            name: at(ids::BUFF_TYPE_NAME)?,
            description: at(ids::BUFF_TYPE_DESCRIPTION)?,
            hide_in_ui: at(ids::BUFF_TYPE_HIDE_IN_UI)?,
            icon: at(ids::BUFF_TYPE_ICON)?,
            hide_duration_in_ui: at(ids::BUFF_TYPE_HIDE_DURATION_IN_UI)?,
            is_saved: at(ids::BUFF_TYPE_IS_SAVED)?,
            impact: at(ids::BUFF_TYPE_IMPACT)?,
            impact_size: facts.size(ids::IMPACT_PROGRAM_REFERENCE)?,
            on_timeout: at(ids::BUFF_TYPE_ON_TIMEOUT)?,
            on_timeout_size: facts.size(ids::BUFF_ON_TIMEOUT)?,
            on_timeout_is_set: at(ids::BUFF_TYPE_ON_TIMEOUT_IS_SET)?,
            relative: [
                at(ids::BUFF_TYPE_IMPACT_SIMPLE)?,
                at(ids::BUFF_TYPE_IMPACT_SCALED)?,
                at(ids::BUFF_TYPE_UI_VALUES)?,
                at(ids::BUFF_TYPE_ON_CREATE)?,
                at(ids::BUFF_TYPE_WHILE_APPLIED)?,
                at(ids::BUFF_TYPE_VFX_MESHES)?,
                at(ids::BUFF_TYPE_VFX_MODELS)?,
                at(ids::BUFF_TYPE_VFX_JOINTS)?,
            ],
        })
    }
}

/// What a new type writes into its copy: a [`BuffTypeAdd`] as the loader
/// and the fake host take it.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edit {
    id: BuffTypeId,
    like: BuffTypeId,
    name: Option<Guid>,
    description: Option<Guid>,
    icon: Option<Guid>,
    life_time: Option<i64>,
    slot: Option<u8>,
    hide_in_ui: Option<bool>,
    hide_duration_in_ui: Option<bool>,
    is_saved: Option<bool>,
    plain: bool,
}

#[doc(hidden)]
impl Edit {
    /// The edit `request` asks for; `None` for a change or flag the ABI
    /// does not name, or a negative life time.
    #[must_use]
    pub fn of(request: &BuffTypeAdd) -> Option<Self> {
        if request.changes & !BuffTypeAdd::ALL_CHANGES != 0
            || request.flags & !BuffTypeAdd::ALL_FLAGS != 0
            || request.life_time_ns < 0
        {
            return None;
        }
        let set = |change| request.changes & change != 0;
        let flag = |bit| request.flags & bit != 0;
        Some(Self {
            id: BuffTypeId::from_guid(request.guid),
            like: request.like,
            name: set(BuffTypeAdd::NAME).then_some(request.name),
            description: set(BuffTypeAdd::DESCRIPTION).then_some(request.description),
            icon: set(BuffTypeAdd::ICON).then_some(request.icon),
            life_time: set(BuffTypeAdd::LIFE_TIME).then_some(request.life_time_ns),
            slot: set(BuffTypeAdd::SLOT).then_some(request.slot),
            hide_in_ui: set(BuffTypeAdd::SET_HIDE_IN_UI).then(|| flag(BuffTypeAdd::HIDE_IN_UI)),
            hide_duration_in_ui: set(BuffTypeAdd::SET_HIDE_DURATION_IN_UI)
                .then(|| flag(BuffTypeAdd::HIDE_DURATION_IN_UI)),
            is_saved: set(BuffTypeAdd::SET_SAVED).then(|| flag(BuffTypeAdd::SAVED)),
            plain: flag(BuffTypeAdd::PLAIN),
        })
    }

    /// The new type's game id.
    #[must_use]
    pub const fn id(&self) -> BuffTypeId {
        self.id
    }

    /// The UI image of its own icon, if it names one.
    #[must_use]
    pub const fn icon(&self) -> Option<Guid> {
        self.icon
    }

    /// The edit with the vanilla type's icon: an icon a client's UI does
    /// not know ends the game.
    #[must_use]
    pub const fn without_icon(self) -> Self {
        Self { icon: None, ..self }
    }

    /// Writes the edit into `copy`, one buff type.
    fn apply(&self, layout: &BuffLayout, copy: &mut [u8]) -> Option<()> {
        let mut blob = Blob::new(copy);
        let mut put = |at: usize, bytes: &[u8]| blob.write(at, bytes).then_some(());
        put(layout.id, &self.id.0.to_le_bytes())?;
        let tags = [
            (layout.name, self.name),
            (layout.description, self.description),
            (layout.icon, self.icon),
        ];
        for (at, guid) in tags {
            if let Some(guid) = guid {
                put(at, &guid.0)?;
            }
        }
        if let Some(life_time) = self.life_time {
            put(layout.life_time, &life_time.to_le_bytes())?;
        }
        if let Some(slot) = self.slot {
            put(layout.slot, &[slot])?;
        }
        let flags = [
            (layout.hide_in_ui, self.hide_in_ui),
            (layout.hide_duration_in_ui, self.hide_duration_in_ui),
            (layout.is_saved, self.is_saved),
        ];
        for (at, flag) in flags {
            if let Some(flag) = flag {
                put(at, &[u8::from(flag)])?;
            }
        }
        if self.plain {
            put(layout.impact, &vec![0; layout.impact_size])?;
            put(layout.on_timeout, &vec![0; layout.on_timeout_size])?;
            put(layout.on_timeout_is_set, &[0])?;
            for header in layout.relative {
                blob.set_array(header, header, 0).then_some(())?;
            }
        }
        copy.copy_from_slice(blob.bytes());
        Some(())
    }
}

/// Why the game's buff collection cannot take a new type.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error, crate::script::ErrorKind)]
#[non_exhaustive]
pub enum Refusal {
    /// The collection does not read with the build's layout.
    #[error("the game's buff collection does not read with this build's layout")]
    Unread,
    /// The type it copies is none of the game's.
    #[error("the game has no buff type {0:?} to copy")]
    NotVanilla(BuffTypeId),
    /// The game or a type added before has its id.
    #[error(
        "the game, this mod or a mod loaded before has a buff type with this id; it needs another id"
    )]
    Taken,
}

#[doc(hidden)]
impl Refusal {
    /// The loader's answer for it, which the fake host gives too.
    #[must_use]
    pub const fn status(self) -> Status {
        match self {
            Self::Unread => Status::UNAVAILABLE,
            Self::NotVanilla(_) => Status::BAD_ARGS,
            Self::Taken => Status::REFUSED,
        }
    }
}

/// Where the collection read through `blob` holds the buff type `id`.
fn find(layout: &BuffLayout, blob: &Blob<&[u8]>, id: BuffTypeId) -> Option<usize> {
    let (buffs, count) = blob.array(layout.buffs)?;
    (0..usize::try_from(count).ok()?)
        .map(|i| buffs + i * layout.size)
        .find(|&at| blob.read_u32(at + layout.id) == Some(id.0))
}

/// Whether the vanilla collection `vanilla` takes `edit` after the types
/// in `added`, every mod's before it.
///
/// # Errors
///
/// The [`Refusal`] that keeps it out.
#[doc(hidden)]
pub fn check(
    layout: &BuffLayout,
    vanilla: &[u8],
    edit: &Edit,
    added: &[Edit],
) -> Result<(), Refusal> {
    let blob = Blob::reading(vanilla);
    let pairs = map_pairs(layout, &blob).ok_or(Refusal::Unread)?;
    if find(layout, &blob, edit.like).is_none() {
        return Err(Refusal::NotVanilla(edit.like));
    }
    let taken = pairs.iter().any(|&(key, _)| key == edit.id.0)
        || added.iter().any(|other| other.id == edit.id);
    if taken {
        return Err(Refusal::Taken);
    }
    Ok(())
}

/// Builds in `out` the buff collection `base` with `types` after the others,
/// each a copy of its `like` with its edits, and their keys in the hash map;
/// the ids of the types it left out.
///
/// The types were checked against the vanilla collection; one whose `like`
/// a patch before this one removed or whose id it took is left out. `None`
/// when `base` does not read with `layout`; `out` then holds nothing of use.
#[doc(hidden)]
pub fn build(
    layout: &BuffLayout,
    base: &[u8],
    types: &[Edit],
    out: &mut impl Grow,
) -> Option<Vec<BuffTypeId>> {
    let reading = Blob::reading(base);
    let mut pairs = map_pairs(layout, &reading)?;
    let mut copies = Vec::with_capacity(types.len());
    let mut keys = Vec::with_capacity(types.len());
    let mut left = Vec::new();
    for edit in types {
        let taken = pairs.iter().any(|&(key, _)| key == edit.id.0) || keys.contains(&edit.id.0);
        let Some(like) = find(layout, &reading, edit.like).filter(|_| !taken) else {
            left.push(edit.id);
            continue;
        };
        let mut copy = base.get(like..like.checked_add(layout.size)?)?.to_vec();
        edit.apply(layout, &mut copy)?;
        copies.push((copy, Some(like)));
        keys.push(edit.id.0);
    }
    if copies.is_empty() {
        Blob::over(out, base);
        return Some(left);
    }
    let added = copies.len();
    let (_, first) = reading.array(layout.buffs)?;
    pairs.extend(keys.into_iter().zip(first..));
    by_bucket(&mut pairs, reading.array(layout.entries)?.1)?;
    sized(out, base, |blob| {
        blob.grow_array(
            layout.buffs,
            layout.size,
            BUFF_ALIGN,
            &layout.relative,
            &copies,
        )?;
        write_map(layout, blob, &pairs, added)
    })?;
    Some(left)
}

/// Every key of the collection's hash map with its index into the buff
/// types; `None` when a bucket runs past the keys or the bucket count is not
/// a power of two, which the game's lookup needs.
fn map_pairs(layout: &BuffLayout, blob: &Blob<&[u8]>) -> Option<Vec<(u32, u32)>> {
    let (entries, buckets) = blob.array(layout.entries)?;
    let (keys, count) = blob.array(layout.keys)?;
    let (values, values_count) = blob.array(layout.values)?;
    if count != values_count || !buckets.is_power_of_two() {
        return None;
    }
    let mut pairs = Vec::with_capacity(usize::try_from(count).ok()? + 1);
    for bucket in 0..usize::try_from(buckets).ok()? {
        let at = entries + bucket * layout.bucket;
        let first = blob.read_u32(at + layout.bucket_start)?;
        let len = blob.read_u32(at + layout.bucket_count)?;
        if first.checked_add(len)? > count {
            return None;
        }
        for i in first..first + len {
            let i = usize::try_from(i).ok()?;
            pairs.push((
                blob.read_u32(keys + i * KEY)?,
                blob.read_u32(values + i * KEY)?,
            ));
        }
    }
    (pairs.len() == usize::try_from(count).ok()?).then_some(pairs)
}

/// Orders `pairs` by the bucket the game looks a key up in among
/// `buckets`, `key & (buckets - 1)`, as [`write_map`] takes them; `None`
/// for no buckets.
fn by_bucket(pairs: &mut [(u32, u32)], buckets: u32) -> Option<()> {
    let mask = buckets.checked_sub(1)?;
    pairs.sort_by_key(|&(key, _)| key & mask);
    Some(())
}

/// Writes `pairs`, in [`by_bucket`] order, as the collection's hash map,
/// with `added` more keys and values than the map held. It edits only
/// `blob`, so a measure and the build of [`sized`] write the same.
fn write_map(
    layout: &BuffLayout,
    blob: &mut dyn Draft,
    pairs: &[(u32, u32)],
    added: usize,
) -> Option<()> {
    let (entries, buckets) = blob.array(layout.entries)?;
    let mask = buckets.checked_sub(1)?;
    let zeros = vec![(vec![0; KEY], None); added];
    blob.grow_array(layout.keys, KEY, KEY, &[], &zeros)?;
    blob.grow_array(layout.values, KEY, KEY, &[], &zeros)?;
    let (keys, _) = blob.array(layout.keys)?;
    let (values, _) = blob.array(layout.values)?;
    for (i, &(key, value)) in pairs.iter().enumerate() {
        let written = blob.write(keys + i * KEY, &key.to_le_bytes())
            && blob.write(values + i * KEY, &value.to_le_bytes());
        if !written {
            return None;
        }
    }
    let mut first = 0;
    for bucket in 0..buckets {
        let len = pairs
            .get(first..)?
            .iter()
            .take_while(|&&(key, _)| key & mask == bucket)
            .count();
        let at = entries + usize::try_from(bucket).ok()? * layout.bucket;
        let start = u32::try_from(first).ok()?.to_le_bytes();
        let count = u32::try_from(len).ok()?.to_le_bytes();
        if !(blob.write(at + layout.bucket_start, &start)
            && blob.write(at + layout.bucket_count, &count))
        {
            return None;
        }
        first += len;
    }
    Some(())
}

impl NewBuff {
    /// The request the loader takes, once the life time and slot convert;
    /// the reason they do not otherwise.
    fn request(&self) -> Result<BuffTypeAdd, &'static str> {
        let life_time = self
            .default_life_time
            .map(|life_time| i64::try_from(life_time.as_nanos()))
            .transpose()
            .map_err(|_| "the life time is too long")?;
        let slot = self
            .slot
            .map(|slot| slot.to_slot().ok_or("the slot is past a byte"))
            .transpose()?;
        let flag = |value: Option<bool>, change, bit| {
            (
                value.map_or(0, |_| change),
                if value == Some(true) { bit } else { 0 },
            )
        };
        let flags = [
            flag(
                self.hide_in_ui,
                BuffTypeAdd::SET_HIDE_IN_UI,
                BuffTypeAdd::HIDE_IN_UI,
            ),
            flag(
                self.hide_duration_in_ui,
                BuffTypeAdd::SET_HIDE_DURATION_IN_UI,
                BuffTypeAdd::HIDE_DURATION_IN_UI,
            ),
            flag(self.is_saved, BuffTypeAdd::SET_SAVED, BuffTypeAdd::SAVED),
        ];
        let tags = [
            (self.name, BuffTypeAdd::NAME),
            (self.description, BuffTypeAdd::DESCRIPTION),
            (
                self.icon.as_ref().map(|icon| icon.id.guid()),
                BuffTypeAdd::ICON,
            ),
        ];
        let mut changes = flags.iter().fold(0, |all, (change, _)| all | change);
        for (tag, change) in tags {
            changes |= tag.map_or(0, |_| change);
        }
        changes |= life_time.map_or(0, |_| BuffTypeAdd::LIFE_TIME);
        changes |= slot.map_or(0, |_| BuffTypeAdd::SLOT);
        let plain = if self.plain { BuffTypeAdd::PLAIN } else { 0 };
        Ok(enshwick_abi::headed!(BuffTypeAdd {
            guid: self.id.guid(),
            name: self.name.unwrap_or(Guid::NIL),
            description: self.description.unwrap_or(Guid::NIL),
            icon: self.icon.as_ref().map_or(Guid::NIL, |icon| icon.id.guid()),
            life_time_ns: life_time.unwrap_or(0),
            like: self.like,
            changes,
            slot: slot.unwrap_or(0),
            flags: flags.iter().fold(plain, |all, (_, bit)| all | bit),
        }))
    }
}

/// Why the loader refused a buff type, from its answer.
fn refused(status: Status, like: BuffTypeId) -> String {
    match status {
        Status::BAD_ARGS => Refusal::NotVanilla(like).to_string(),
        Status::REFUSED => Refusal::Taken.to_string(),
        Status::UNAVAILABLE => {
            "the game's buff collection does not read on this build; buff types are off".into()
        }
        Status::TOO_LATE => "a buff type is added during init only".into(),
        other => format!("the loader answered {other:?}"),
    }
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::add_buff`.
    #[doc(hidden)]
    fn add_buff(&mut self, new: NewBuff) -> Result<BuffTypeId, Off>;
}

impl LoadMethods for crate::BaseLoad {
    fn add_buff(&mut self, new: NewBuff) -> Result<BuffTypeId, Off> {
        let fail = |why: &str| Off::new(format!("buff {}: {why}", new.id));
        if !self.joins() {
            return Err(fail("a mod with buff types runs on both sides"));
        }
        let request = new.request().map_err(fail)?;
        self.declare(new.id)?;
        let host = self.host();
        let mut id = BuffTypeId(0);
        let status = (host.api().buff_type_add)(host.handle(), &raw const request, &raw mut id);
        if !status.is_ok() {
            return Err(fail(&refused(status, new.like)));
        }
        // Served only for a type the loader took; it names the icon only once
        // the client's UI knows the image.
        if let Some(icon) = new.icon {
            self.add_icon(icon)?;
        }
        Ok(id)
    }
}

#[cfg(test)]
mod tests;
