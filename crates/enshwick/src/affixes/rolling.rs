//! Rolling affixes onto items: by hand, on random loot and on crafted
//! items. Unstable, since what an item rolled is kept as item data.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::Entity;
use crate::abi::{AffixRoll, Header, Status};
use crate::game_events::CraftRecipeEvent;
use crate::inventory::{ItemInstance, Slot};
use crate::locks::Relock;
use crate::loot::Source;
use crate::{Load, Off};

use super::{Affixes, ItemRarity, RollError, RollRules};

/// A seed from the item and the time, for rolls that need no repeating.
fn fresh_seed(item: ItemInstance) -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    #[expect(clippy::cast_possible_truncation, reason = "the low bits seed")]
    let now = now as u64;
    now ^ (u64::from(item.pide.0) << 32 | u64::from(item.item.get()))
}

impl Affixes {
    /// Rolls `item` from the pool of every mod's affixes that fit its
    /// category, replacing the affixes it had, from `seed` or a fresh one;
    /// `once` rolls only an item that never rolled. How many it rolled. On
    /// the server's game thread.
    ///
    /// # Errors
    ///
    /// [`RollError`] for an item that rolled before under `once`, off the
    /// server's game thread, or for an item without an entity.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::affixes::RollRules;
    ///
    /// fn reroll(affixes: &enshwick::affixes::Affixes, item: enshwick::inventory::ItemInstance) {
    ///     let _rolled = affixes.roll(item, RollRules::new(1, 3), None, false);
    /// }
    /// ```
    pub fn roll(
        &self,
        item: ItemInstance,
        rules: RollRules,
        seed: Option<u64>,
        once: bool,
    ) -> Result<u32, RollError> {
        let request = AffixRoll {
            header: Header::of::<AffixRoll>(),
            pide: item.pide,
            item: item.item,
            min: rules.min,
            max: rules.max,
            max_rarity: rules.max_rarity.to_raw(),
            flags: if once { AffixRoll::ONCE } else { 0 },
            seed: seed.unwrap_or_else(|| fresh_seed(item)),
        };
        let mut count = 0;
        match (self.host.api().affix_roll)(self.host.handle(), &raw const request, &raw mut count) {
            Status::OK => Ok(count),
            Status::REFUSED => Err(RollError::Rolled),
            Status::WRONG_THREAD => Err(RollError::WrongThread),
            other => Err(RollError::Refused(other.0)),
        }
    }
}

/// Rolls on random loot, for [`Load::add_loot_affixes`].
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct AffixLoot {
    /// The containers whose rolled loot rolls affixes.
    pub from: Source,
    /// The chance each fitting item rolls at all, from 0 to 1.
    pub chance: f64,
    /// What a roll may give.
    pub rules: RollRules,
}

impl AffixLoot {
    /// One affix at `chance` per fitting item in loot rolled `from`.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn new(from: Source, chance: f64) -> Self {
        Self {
            from,
            chance,
            rules: RollRules::new(1, 1),
        }
    }

    /// The same with `min` to `max` affixes.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn count(mut self, min: u8, max: u8) -> Self {
        self.rules.min = min;
        self.rules.max = max;
        self
    }

    /// The same, no rarer than `rarity`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::affixes::{AffixLoot, ItemRarity};
    /// use enshwick::loot::Source;
    ///
    /// let rule = AffixLoot::new(Source::bosses(), 1.0).max_rarity(ItemRarity::Epic);
    /// assert_eq!(rule.rules.max_rarity, ItemRarity::Epic);
    /// ```
    #[must_use]
    pub const fn max_rarity(mut self, rarity: ItemRarity) -> Self {
        self.rules.max_rarity = rarity;
        self
    }
}

/// Rolls on crafted items, for [`Load::add_craft_affixes`].
///
/// # Examples
///
/// ```
/// use enshwick::affixes::{AffixCraft, RollRules};
///
/// let rule = AffixCraft::new(0.25, RollRules::new(1, 2));
/// assert_eq!(rule.chance, 0.25);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct AffixCraft {
    /// The chance each fitting crafted item rolls at all, from 0 to 1.
    pub chance: f64,
    /// What a roll may give.
    pub rules: RollRules,
}

impl AffixCraft {
    /// Rolls under `rules` at `chance` per fitting crafted item.
    ///
    /// # Examples
    ///
    /// See [`AffixCraft`].
    #[must_use]
    pub const fn new(chance: f64, rules: RollRules) -> Self {
        Self { chance, rules }
    }
}

/// The crafters waiting for their items: when they asked, and the items
/// they held then.
type Crafts = Mutex<HashMap<Entity, (Instant, HashSet<ItemInstance>)>>;

/// How long after a craft request the crafter's new items count as crafted.
const CRAFT_WINDOW: Duration = Duration::from_secs(1);
/// How often the loot and craft rules look for items to roll.
const SWEEP: Duration = Duration::from_millis(250);

/// The items with an entity in `slots`.
fn instances(slots: &[Slot]) -> impl Iterator<Item = ItemInstance> + '_ {
    slots.iter().filter_map(ItemInstance::of)
}

/// Rolls `item` under `rules` at `chance`, unless it rolled before; items
/// whose category no affix fits stay as they are.
fn try_roll(affixes: Affixes, item: ItemInstance, chance: f64, rules: RollRules) {
    let hit = fastrand::f64() < chance;
    let rules = if hit { rules } else { RollRules::new(0, 0) };
    let _rolled = affixes.roll(item, rules, None, true);
}

impl Load<'_> {
    /// Rolls affixes onto the items with an entity that random loot puts
    /// into containers `rule.from` matches, each at `rule.chance`, as the
    /// container rolls its loot. Each item rolls once: an item rolled
    /// before, by this rule or another, stays as it is. On the server.
    ///
    /// # Errors
    ///
    /// [`Off`] for a chance outside 0 to 1, or when the build lacks the
    /// loot or inventory pieces.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn add_loot_affixes(&mut self, rule: AffixLoot) -> Result<(), Off> {
        if !(0.0..=1.0).contains(&rule.chance) {
            return Err(Off::new(format!(
                "affix loot: chance {} is not between 0 and 1",
                rule.chance
            )));
        }
        let affixes = self.affixes();
        let inventories = self.inventories()?;
        let rolled = crate::loot::watch_rolls(self)?;
        let scheduler = self.scheduler()?;
        scheduler.every(SWEEP, move || {
            let containers: Vec<_> = core::mem::take(&mut *rolled.relock());
            for roll in containers.iter().filter(|r| rule.from.matches(r)) {
                let Some(container) = roll.entity else {
                    continue;
                };
                for item in instances(&inventories.read().slots(container)) {
                    try_roll(affixes, item, rule.chance, rule.rules);
                }
            }
            core::ops::ControlFlow::Continue(())
        });
        Ok(())
    }

    /// Rolls affixes onto the items with an entity a player crafts, each at
    /// `rule.chance`: the items new in the crafter's inventories within a
    /// second of the craft request. Each item rolls once. On the server.
    ///
    /// # Errors
    ///
    /// [`Off`] for a chance outside 0 to 1, or when the build lacks the
    /// event or inventory pieces.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::affixes::{AffixCraft, RollRules};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.add_craft_affixes(AffixCraft::new(0.2, RollRules::new(1, 1)))
    /// }
    /// ```
    pub fn add_craft_affixes(&mut self, rule: AffixCraft) -> Result<(), Off> {
        if !(0.0..=1.0).contains(&rule.chance) {
            return Err(Off::new(format!(
                "affix craft: chance {} is not between 0 and 1",
                rule.chance
            )));
        }
        let affixes = self.affixes();
        let inventories = self.inventories()?;
        let scheduler = self.scheduler()?;
        let pending: Arc<Crafts> = Arc::default();
        let requests = Arc::clone(&pending);
        self.on::<CraftRecipeEvent>(move |craft| {
            let held: HashSet<ItemInstance> =
                instances(&inventories.read().slots(craft.crafting_operator_id)).collect();
            requests
                .relock()
                .entry(craft.crafting_operator_id)
                .or_insert((Instant::now(), held));
        })?;
        scheduler.every(SWEEP, move || {
            let due: Vec<(Entity, HashSet<ItemInstance>)> = {
                let mut pending = pending.relock();
                let now = Instant::now();
                let ready: Vec<Entity> = pending
                    .iter()
                    .filter(|(_, (at, _))| now.duration_since(*at) >= CRAFT_WINDOW)
                    .map(|(&player, _)| player)
                    .collect();
                ready
                    .into_iter()
                    .filter_map(|player| Some((player, pending.remove(&player)?.1)))
                    .collect()
            };
            for (player, before) in due {
                for item in
                    instances(&inventories.read().slots(player)).filter(|i| !before.contains(i))
                {
                    try_roll(affixes, item, rule.chance, rule.rules);
                }
            }
            core::ops::ControlFlow::Continue(())
        });
        Ok(())
    }
}
