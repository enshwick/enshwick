//! Stats: named numbers on entities, such as extra health or a chance to
//! crit, that items, buffs and mods modify, worked out once per change and
//! read for free.
//!
//! A stat kind has a content [`Id`] and a [`StatDef`]: what it is
//! ([`StatType`]), its default and its range. The text that names it is the
//! derived part `<name>.name` in its owner's texts ([`name_key`]). A mod
//! defines its own kinds with [`Load::define_stat`], named with
//! [`id!`](crate::content::id), and opens other kinds with
//! [`Load::open_stat`]: the platform's built-in ones in [`builtin`], or a
//! dependency's through [`deps!`](crate::deps), which its owner exports.
//! Every mod that opens one id shares its values.
//!
//! [`Modifier`]s come from three sources: items an entity wears (their
//! [`ItemStats`]), buffs it has ([`Load::add_buff_modifiers`]) and modifiers a
//! mod keeps on it ([`Stats::set`]). The server sums them with [`aggregate`]
//! after each step in which a source changed, keeps the values, and raises
//! [`Load::on_stats_changed`] for each value that moved; [`Stats::value`]
//! reads a kept value without locks from any thread.
//!
//! The platform applies the built-in kinds to the game itself on the
//! server; a mod's own kinds mean what the mods that read them make of
//! them.
//!
//! # Examples
//!
//! ```
//! use core::time::Duration;
//! use enshwick::content::id;
//! use enshwick::events::PlayerJoined;
//! use enshwick::stats::{Lasting, Modifier, Op, StatDef, builtin};
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let luck = load.define_stat(id!("luck"), &StatDef::additive().range(0.0, 1.0))?;
//!     let health = load.open_stat(builtin::HEALTH_MAX)?;
//!     let stats = load.stats()?;
//!     load.on::<PlayerJoined>(move |joined| {
//!         let player = joined.entity;
//!         let minute = Lasting::For(Duration::from_secs(60));
//!         let _health = stats.set(player, "welcome", Modifier::new(health, Op::Add, 50.0), minute);
//!         let _luck = stats.set(player, "welcome", Modifier::new(luck, Op::Add, 0.1), minute);
//!     })
//! }
//! ```

use core::ffi::c_void;

use crate::abi::{
    Header, StatChange as StatChangeData, StatModifierData, StatModify, StatRef, StatSource,
    Status, Str,
};
use crate::buffs::BuffTypeId;
use crate::content::Id;
use crate::content::items::StatFormat;
use crate::inventory::ItemInstance;
use crate::item_data::ItemData;
pub use crate::item_data::SetError;
use crate::{Entity, ItemId};
use crate::{Host, Load, Off};

/// The item data kind that holds every item's [`ItemStats`].
pub const ITEM_KIND: &str = "enshwick:stats";

/// The key stored for a full stat id: the game id of the id's GUID, as
/// [`Guid::named`](crate::content::Guid::named) derives it.
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
/// use enshwick::stats::{StatKey, key_of};
///
/// assert_eq!(key_of("enshwick:health_max"), StatKey(Guid::named("enshwick:health_max").fnv1a()));
/// assert_ne!(key_of("a:b"), StatKey(0));
/// ```
#[must_use]
pub const fn key_of(id: &str) -> StatKey {
    StatKey(crate::content::Guid::hashed(id).fnv1a())
}

/// The text key that names the stat kind or affix of full id `id` in its
/// owner's texts, `<name>.name`.
///
/// It is the name side of the id [`Id::part`] derives for the part `name`;
/// `None` for an id that is not `<mod>:<name>`, or whose name with `.name`
/// is longer than a content id's name may be.
///
/// # Examples
///
/// ```
/// use enshwick::content::Id;
/// use enshwick::stats::{builtin, name_key};
///
/// assert_eq!(name_key("you.hello:luck").as_deref(), Some("luck.name"));
/// assert_eq!(name_key(&builtin::HEALTH_MAX.to_string()).as_deref(), Some("health_max.name"));
/// assert_eq!(Id::new("you.hello:luck").part("name").to_string(), "you.hello:luck.name");
/// assert_eq!(name_key("luck"), None);
/// assert_eq!(name_key(&format!("you.hello:{}", "a".repeat(124))), None);
/// ```
#[must_use]
pub fn name_key(id: &str) -> Option<String> {
    crate::content::part_name(id, crate::content::NAME_PART)
}

/// The platform's built-in stat kinds, which it applies to the game on the
/// server.
///
/// Open one with [`Load::open_stat`]. The maximum kinds add points to the
/// entity's maximum health, stamina and mana, as gear does. Damage kinds are factors on the hits
/// an entity deals, 1 without modifiers; resistances are the part of a
/// hit's damage the entity takes less, 0 without modifiers and 1 for none
/// at all. Both act on hits the entity deals or takes directly; the book's
/// chapter on stats lists what reaches the game.
///
/// # Examples
///
/// ```
/// use enshwick::stats::builtin;
///
/// assert_eq!(builtin::DAMAGE_FIRE.to_string(), "enshwick:damage_fire");
/// assert!(builtin::ALL.iter().any(|(id, _)| *id == builtin::RESISTANCE));
/// ```
pub mod builtin;

/// A stat whose value changed, for [`Load::on_stats_changed`].
///
/// # Examples
///
/// See [`Load::on_stats_changed`].
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct StatChange {
    /// The entity.
    pub entity: Entity,
    /// The stat.
    pub stat: Stat,
    /// The value before.
    pub old: f32,
    /// The value now.
    pub new: f32,
}

/// What a stat kind is called and how it reads, from [`Stats::info`].
///
/// # Examples
///
/// See [`Stats::info`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct StatInfo {
    /// The full id, `<mod>:<name>`.
    pub id: String,
    /// The text key that names it in its owner's texts, see [`name_key`].
    pub text: String,
    /// The definition.
    pub def: StatDef,
}

/// The FNV-1a 32 hash of a modifier's key.
const fn key_hash(key: &str) -> u32 {
    key_of(key).0
}

/// Stat values and modifiers; get it from [`Load::stats`]. Copy it anywhere.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug)]
pub struct Stats {
    host: Host,
    items: ItemData<ItemStats>,
}

impl Stats {
    /// The value of `stat` on `entity`: what the server worked out after the
    /// last step a source changed, or the default. Any thread, without
    /// locks; a client answers defaults.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn value(&self, entity: Entity, stat: Stat) -> f32 {
        let mut out = 0.0;
        let status =
            (self.host.api().stat_value)(self.host.handle(), entity, stat.index, &raw mut out);
        if status.is_ok() { out } else { 0.0 }
    }

    /// Whether `stat` is on for `entity`: its value is not 0.
    ///
    /// # Examples
    ///
    /// ```
    /// fn glows(stats: &enshwick::stats::Stats, glow: enshwick::stats::Stat, entity: enshwick::Entity) -> bool {
    ///     stats.is_on(entity, glow)
    /// }
    /// ```
    #[must_use]
    pub fn is_on(&self, entity: Entity, stat: Stat) -> bool {
        self.value(entity, stat) != 0.0
    }

    /// Sets the mod's modifier `key` on `entity` for `modifier.stat`,
    /// replacing the one the mod set before under the same key and stat. The
    /// value follows after the next step. On the server, any thread.
    ///
    /// # Errors
    ///
    /// [`ModifyError`] on a client alone, or for a saved modifier on an
    /// entity that is no player.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn set(
        &self,
        entity: Entity,
        key: &str,
        modifier: Modifier,
        lasting: Lasting,
    ) -> Result<(), ModifyError> {
        let (flags, duration) = match lasting {
            Lasting::UntilRemoved => (0, 0),
            Lasting::For(d) => (0, u64::try_from(d.as_millis()).unwrap_or(u64::MAX).max(1)),
            Lasting::Saved => (StatModify::SAVED, 0),
        };
        self.modify(entity, key, modifier.to_data(), flags, duration)
    }

    /// Removes the mod's modifier `key` on `entity` for `stat`.
    ///
    /// # Errors
    ///
    /// As for [`Stats::set`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn unbless(stats: &enshwick::stats::Stats, health: enshwick::stats::Stat, player: enshwick::Entity) {
    ///     let _removed = stats.remove(player, "bless", health);
    /// }
    /// ```
    pub fn remove(&self, entity: Entity, key: &str, stat: Stat) -> Result<(), ModifyError> {
        let data = StatModifierData {
            stat: stat.index,
            ..StatModifierData::default()
        };
        self.modify(entity, key, data, StatModify::REMOVE, 0)
    }

    fn modify(
        &self,
        entity: Entity,
        key: &str,
        modifier: StatModifierData,
        flags: u32,
        duration_ms: u64,
    ) -> Result<(), ModifyError> {
        let request = StatModify {
            header: Header::of::<StatModify>(),
            entity,
            key: key_hash(key),
            modifier,
            flags,
            duration_ms,
        };
        match (self.host.api().stat_modify)(self.host.handle(), &raw const request) {
            Status::OK => Ok(()),
            Status::REFUSED if flags & StatModify::SAVED != 0 => Err(ModifyError::NotPlayer),
            Status::REFUSED | Status::WRONG_THREAD => Err(ModifyError::NotServer),
            other => Err(ModifyError::Refused(other.0)),
        }
    }

    /// What `item` gives the entity that wears it; empty for an item without
    /// any. On a client, what the server sent.
    ///
    /// # Examples
    ///
    /// ```
    /// fn bonuses(stats: &enshwick::stats::Stats, item: enshwick::inventory::ItemInstance) -> usize {
    ///     stats.item(item).modifiers.len()
    /// }
    /// ```
    #[must_use]
    pub fn item(&self, item: ItemInstance) -> ItemStats {
        self.items.get(item).unwrap_or_default()
    }

    /// Sets what `item` gives the entity that wears it; an empty
    /// [`ItemStats`] removes it. Clients that run Enshwick get it, and the
    /// entities that wear the item follow after the next step.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    /// The value is kept as item data, which sets aside, after a restart, a
    /// value on an item no load or join finds in its slot again.
    ///
    /// # Errors
    ///
    /// [`SetError`] when the value is too large or the server refuses it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::stats::{ItemStats, Modifier, Op};
    ///
    /// fn enchant(stats: &enshwick::stats::Stats, fire: enshwick::stats::Stat, ring: enshwick::inventory::ItemInstance) {
    ///     let mut bonuses = stats.item(ring);
    ///     bonuses.modifiers.push(Modifier::new(fire, Op::Add, 0.1).to_item());
    ///     let _set = stats.set_item(ring, &bonuses);
    /// }
    /// ```
    #[cfg(feature = "unstable")]
    pub fn set_item(&self, item: ItemInstance, stats: &ItemStats) -> Result<(), SetError> {
        if *stats == ItemStats::default() {
            self.items.remove(item)
        } else {
            self.items.set(item, stats)
        }
    }

    /// Every modifier that counts toward `stat` on `entity`, or toward every
    /// stat for `None`, in the order they apply. On the server, any thread.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::stats::Origin;
    ///
    /// fn from_items(stats: &enshwick::stats::Stats, player: enshwick::Entity) -> usize {
    ///     stats.sources(player, None).iter().filter(|s| matches!(s.origin, Origin::Item(_))).count()
    /// }
    /// ```
    #[must_use]
    pub fn sources(&self, entity: Entity, stat: Option<Stat>) -> Vec<Source> {
        let stat = stat.map_or(u32::MAX, |s| s.index);
        let mut out = vec![StatSource::default(); 32];
        loop {
            let mut count = 0u32;
            let status = (self.host.api().stat_sources)(
                self.host.handle(),
                entity,
                stat,
                out.as_mut_ptr(),
                u32::try_from(out.len()).unwrap_or(u32::MAX),
                &raw mut count,
            );
            let count = count as usize;
            if !status.is_ok() {
                return Vec::new();
            }
            if count <= out.len() {
                out.truncate(count);
                break;
            }
            out = vec![StatSource::default(); count];
        }
        out.iter()
            .filter_map(|s| {
                let origin = match s.kind {
                    StatSource::ITEM => Origin::Item(ItemInstance {
                        pide: Entity(s.id),
                        item: ItemId::new(s.owner),
                    }),
                    StatSource::BUFF => Origin::Buff(BuffTypeId(s.id)),
                    StatSource::ENTITY => Origin::Mod {
                        index: crate::mods::ModIndex(s.owner),
                        key: s.id,
                    },
                    _ => return None,
                };
                Some(Source {
                    origin,
                    stat: s.stat,
                    op: Op::from_raw(s.op)?,
                    value: s.value,
                })
            })
            .collect()
    }

    /// The id and definition of `stat`; `None` for a kind opened but never
    /// defined. Any thread.
    ///
    /// # Examples
    ///
    /// ```rust,standalone_crate
    /// use std::sync::OnceLock;
    ///
    /// use enshwick::content::id;
    /// use enshwick::stats::{Stat, StatDef, Stats};
    /// use enshwick::{Load, Mod, Off};
    ///
    /// static LUCK: OnceLock<(Stats, Stat)> = OnceLock::new();
    ///
    /// struct Lucky;
    ///
    /// impl Mod for Lucky {
    ///     fn init(load: &mut Load<'_>) -> Result<Self, Off> {
    ///         let luck = load.define_stat(id!("luck"), &StatDef::additive())?;
    ///         let _first = LUCK.set((load.stats()?, luck));
    ///         Ok(Self)
    ///     }
    /// }
    ///
    /// enshwick::test::declare_mod!(Lucky, "id = \"you.hello\"\nside = \"server\"");
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// // SAFETY: both functions are the exports of `declare_mod!(Lucky)`.
    /// let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
    /// assert!(status.is_ok());
    /// let (stats, luck) = LUCK.get().ok_or("init ran")?;
    /// let info = stats.info(*luck).ok_or("luck is defined")?;
    /// assert_eq!(info.id, "you.hello:luck");
    /// assert_eq!(info.text, "luck.name");
    /// # Ok::<(), &str>(())
    /// ```
    #[must_use]
    pub fn info(&self, stat: Stat) -> Option<StatInfo> {
        let mut def = StatDef::additive().to_data();
        let mut id = Str::EMPTY;
        let status =
            (self.host.api().stat_info)(self.host.handle(), stat.index, &raw mut def, &raw mut id);
        if !status.is_ok() {
            return None;
        }
        // SAFETY: `stat_info` returned OK and pointed `def.text` at a text the
        // loader keeps alive and unchanged for the process; the text is copied.
        let text = unsafe { def.text.as_str() }?.to_owned();
        let def = StatDef::from_data(&def)?;
        // SAFETY: `stat_info` returned OK and pointed `id` at an id the loader
        // keeps alive and unchanged for the process; the id is copied.
        let id = unsafe { id.as_str() }?.to_owned();
        Some(StatInfo { id, text, def })
    }

    /// Watches `entity` for the items it wears and the buffs it has, or stops
    /// watching it. Players are always watched, and so is every entity a mod
    /// keeps a modifier on; watch other entities whose buffs should count,
    /// such as an enemy a mod curses. Any thread, on the process that runs
    /// the world; elsewhere it does nothing.
    ///
    /// # Examples
    ///
    /// ```
    /// fn curse(stats: &enshwick::stats::Stats, enemy: enshwick::Entity) {
    ///     stats.watch(enemy, true);
    /// }
    /// ```
    pub fn watch(&self, entity: Entity, watch: bool) {
        let _status = (self.host.api().stat_watch)(self.host.handle(), entity, u8::from(watch));
    }
}

/// Calls the stats handler `F` with one change the loader reports; a change
/// shorter than its first version reaches no handler.
///
/// # Safety
///
/// - `change` is null or points at a change the loader wrote, readable for
///   the size its header states; no alignment is needed.
/// - No other thread writes that change during the call.
/// - `user` is the `Box<F>` that [`Load::on_stats_changed`] handed over,
///   which the loader keeps for the process.
unsafe extern "system" fn run_changed<F>(change: *const StatChangeData, user: *mut c_void)
where
    F: Fn(&StatChange),
{
    // SAFETY: the caller guarantees `change` is null or readable for the size
    // its header states, with no concurrent writer.
    let Some(change) = (unsafe { crate::abi::read_headed(change, size_of::<StatChangeData>()) })
    else {
        return;
    };
    // SAFETY: the caller guarantees `user` is the boxed `F` that `hand_over`
    // leaked for the process, so it is aligned, alive and only read through
    // shared references.
    let handler = unsafe { &*user.cast::<F>() };
    let change = StatChange {
        entity: change.entity,
        stat: Stat::from_ref(change.stat),
        old: change.old,
        new: change.new,
    };
    crate::host::guard("panic in a stats handler", || (), || handler(&change));
}

impl Load<'_> {
    fn open_stat_with(&mut self, id: Id, def: Option<&StatDef>) -> Result<Stat, Off> {
        let host = self.host();
        let data = def.map(StatDef::to_data);
        let def_ptr = data.as_ref().map_or(core::ptr::null(), core::ptr::from_ref);
        let full = id.to_string();
        let mut out = StatRef::default();
        let status = (host.api().stat_open)(host.handle(), Str::new(&full), def_ptr, &raw mut out);
        crate::load::ok(status, Stat::from_ref(out), || format!("stat {full:?}"))
    }

    /// Defines the mod's stat kind `id`, named with
    /// [`id!`](crate::content::id). Its text is `<name>.name` in the mod's
    /// texts ([`name_key`]). Call it during `init`, on both sides.
    ///
    /// # Errors
    ///
    /// [`Off`] for an id of another mod, a malformed definition, a kind
    /// defined twice, or after init.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn define_stat(&mut self, id: Id, def: &StatDef) -> Result<Stat, Off> {
        def.check()
            .map_err(|why| Off::new(format!("stat {:?}: {why}", id.to_string())))?;
        self.open_stat_with(id, Some(def))
    }

    /// Opens stat kind `id`: a built-in one from [`builtin`], or another
    /// mod's through [`deps!`](crate::deps). A kind no mod defines keeps 0
    /// and reads nothing. Call it during `init`, on both sides.
    ///
    /// # Errors
    ///
    /// [`Off`] for a kind of a loaded mod that does not export it, or after
    /// init.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn open_stat(&mut self, id: Id) -> Result<Stat, Off> {
        self.open_stat_with(id, None)
    }

    /// The stat values and modifiers. Call it during `init`.
    ///
    /// # Errors
    ///
    /// [`Off`] after init.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn stats(&mut self) -> Result<Stats, Off> {
        let host = self.host();
        Ok(Stats {
            host,
            items: self.open_item_data(ITEM_KIND)?,
        })
    }

    /// Gives every entity that has buff `buff` the `modifiers`, while it has
    /// it: a mod's own buff type, or a vanilla one. Buffs count on watched
    /// entities (see [`Stats::watch`]). Call it during `init`.
    ///
    /// # Errors
    ///
    /// [`Off`] after init.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::buffs::BuffTypeId;
    /// use enshwick::stats::{Modifier, Op, builtin};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let fire = load.open_stat(builtin::RESISTANCE_FIRE)?;
    ///     load.add_buff_modifiers(BuffTypeId(0x89cb_0b34), &[Modifier::new(fire, Op::Add, 0.2)])
    /// }
    /// ```
    pub fn add_buff_modifiers(
        &mut self,
        buff: BuffTypeId,
        modifiers: &[Modifier],
    ) -> Result<(), Off> {
        let host = self.host();
        let data: Vec<StatModifierData> = modifiers.iter().map(|m| m.to_data()).collect();
        let status = (host.api().stat_buff)(
            host.handle(),
            buff.0,
            data.as_ptr(),
            u32::try_from(data.len()).unwrap_or(u32::MAX),
        );
        crate::load::ok(status, (), || format!("buff modifiers for {:#x}", buff.0))
    }

    /// Runs `handler` for every stat value that changed on a watched entity,
    /// on the server's game thread after the step that changed it.
    ///
    /// # Errors
    ///
    /// [`Off`] after init.
    ///
    /// # Examples
    ///
    /// ```
    /// enshwick::deps!();
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let luck = load.open_stat(deps::lockers::id("luck")?)?;
    ///     load.on_stats_changed(move |change| {
    ///         if change.stat == luck {
    ///             enshwick::info!("{:?} has luck {}", change.entity, change.new);
    ///         }
    ///     })
    /// }
    /// ```
    pub fn on_stats_changed<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: Fn(&StatChange) + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::hand_over(
            handler,
            |user| (host.api().stats_changed)(host.handle(), run_changed::<F>, user),
            "stats handler",
        )
    }
}

mod defs;
mod modifiers;
#[cfg(test)]
mod tests;

pub use defs::*;
pub use modifiers::*;
