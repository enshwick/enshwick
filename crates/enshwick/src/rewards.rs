//! Rewards for actions: "when a player does this, give that", declared
//! once and kept per world, from code or a pack's `rewards.toml`.
//!
//! A reward names an [`Action`], what it [gives](Give), a [`Limit`] and a
//! chance. The server gives it in the server's world, on the game thread,
//! and keeps who got a limited reward in the mod's data beside the world's
//! save, so a limit holds across restarts. [`Load::on_reward`] refuses a
//! grant; [`Rewards::grant`] gives a declared reward from code.
//!
//! # Examples
//!
//! ```
//! use enshwick::rewards::{Action, Give, Limit, Reward};
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     const TONIC: enshwick::ItemId = enshwick::ItemId::named("you.tonics:tonic");
//!     load.add_reward(
//!         Reward::new("first-level", Action::LevelUp(2))
//!             .with(Give::Items { item: TONIC, count: 1 })
//!             .with(Give::Experience(50))
//!             .with_limit(Limit::Once),
//!     )
//! }
//! ```

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::abi::{BuffTypeId, KnowledgeId, ModHandle, RecipeId};
use crate::game_events::{
    CraftRecipeEvent, CraftRecipeSuccessEvent, LevelUpEvent, LootInteractionEvent,
};
use crate::locks::Relock as _;
use crate::{Entity, ItemId, Load, Off};

/// What a player does that a reward follows.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub enum Action {
    /// Crafts the recipe: the server's craft of a request the player made.
    Craft(RecipeId),
    /// Reaches the level; 0 for every level.
    LevelUp(u32),
    /// Joins the world.
    Join,
    /// Loots a container.
    Loot,
    /// Completes a quest.
    Quest,
    /// Eats, drinks or uses the item.
    Consume(ItemId),
    /// Picks the item up.
    Pickup(ItemId),
}

/// What a reward gives.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub enum Give {
    /// `count` of `item`, all or nothing: with a full inventory the player
    /// gets none and the log says so.
    Items {
        /// The item.
        item: ItemId,
        /// How many.
        count: u32,
    },
    /// Experience, added to the player's next gains, at most a level a tick.
    Experience(u32),
    /// A knowledge entry, set.
    Knowledge(KnowledgeId),
    /// A buff, for its type's own duration.
    Buff(BuffTypeId),
}

/// How often one player gets a reward.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum Limit {
    /// Every time.
    #[default]
    Every,
    /// Once per player in the world.
    Once,
    /// Once per player each day, by the calendar day in UTC.
    Daily,
    /// Once in the world, to the first player.
    World,
}

/// A reward: what follows which action, how often and how likely.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct Reward {
    /// Its name in the mod, 1 to 64 of `[a-z0-9_-]`, which the world's data
    /// keeps who got it under.
    pub name: String,
    /// The action it follows.
    pub on: Action,
    /// What it gives, in order.
    pub give: Vec<Give>,
    /// How often one player gets it.
    #[script(default)]
    pub limit: Limit,
    /// How likely, from 0 to 1; none for always.
    #[script(default)]
    pub chance: Option<f32>,
}

impl Reward {
    /// A reward named `name` that follows `on` and gives nothing yet.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn new(name: &str, on: Action) -> Self {
        Self {
            name: name.to_owned(),
            on,
            give: Vec::new(),
            limit: Limit::Every,
            chance: None,
        }
    }

    /// The same that also gives `give`.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn with(mut self, give: Give) -> Self {
        self.give.push(give);
        self
    }

    /// The same with `limit`.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn with_limit(mut self, limit: Limit) -> Self {
        self.limit = limit;
        self
    }

    /// The same, given with `chance`, from 0 to 1.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::rewards::{Action, Reward};
    ///
    /// let rare = Reward::new("rare", Action::Loot).with_chance(0.1);
    /// assert_eq!(rare.chance, Some(0.1));
    /// ```
    #[must_use]
    pub const fn with_chance(mut self, chance: f32) -> Self {
        self.chance = Some(chance);
        self
    }

    /// What is wrong with the reward, `None` for nothing.
    fn problem(&self) -> Option<String> {
        let name = &self.name;
        let well_formed = (1..=64).contains(&name.len())
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
        if !well_formed {
            return Some(format!("reward {name:?}: a name is 1 to 64 of [a-z0-9_-]"));
        }
        if self.give.is_empty() {
            return Some(format!("reward {name}: it gives nothing"));
        }
        if self.chance.is_some_and(|c| !(0.0..=1.0).contains(&c)) {
            return Some(format!("reward {name}: a chance is from 0 to 1"));
        }
        None
    }
}

/// A reward about to be given, from [`Load::on_reward`].
///
/// # Examples
///
/// See [`Load::on_reward`].
#[derive(Clone, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct RewardGrant {
    /// The reward's name.
    pub reward: String,
    /// The player's character.
    pub player: Entity,
    /// What it gives.
    pub give: Vec<Give>,
}

/// Who got which limited reward, one world's file `rewards.toml`.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
struct Given {
    /// The players' account keys by reward, for [`Limit::Once`].
    #[serde(default)]
    once: BTreeMap<String, BTreeSet<u64>>,
    /// The day each player last got the reward, for [`Limit::Daily`].
    #[serde(default)]
    daily: BTreeMap<String, BTreeMap<String, u64>>,
    /// The rewards given in this world, for [`Limit::World`].
    #[serde(default)]
    world: BTreeSet<String>,
}

impl Given {
    /// Whether the player `key` may get `reward` on `day`.
    fn allows(&self, reward: &Reward, key: u64, day: u64) -> bool {
        let name = &reward.name;
        match reward.limit {
            Limit::Once => !self.once.get(name).is_some_and(|keys| keys.contains(&key)),
            Limit::Daily => self
                .daily
                .get(name)
                .and_then(|days| days.get(&key.to_string()))
                .is_none_or(|&last| last < day),
            Limit::World => !self.world.contains(name),
            Limit::Every => true,
        }
    }

    /// Notes that the player `key` got `reward` on `day`.
    fn note(&mut self, reward: &Reward, key: u64, day: u64) {
        let name = reward.name.clone();
        match reward.limit {
            Limit::Once => {
                self.once.entry(name).or_default().insert(key);
            }
            Limit::Daily => {
                self.daily
                    .entry(name)
                    .or_default()
                    .insert(key.to_string(), day);
            }
            Limit::World => {
                self.world.insert(name);
            }
            Limit::Every => {}
        }
    }
}

/// How many players' pending experience the mod keeps at once.
const PENDING: usize = 32;

/// A grant filter of the mod's.
type Filter = Arc<dyn Fn(&RewardGrant) -> bool + Send + Sync>;

/// The mod's rewards, the handles they give through and who got them.
struct Shared {
    rewards: Mutex<Vec<Reward>>,
    /// Cloned out before a grant calls them, so a filter may grant too.
    filters: Mutex<Vec<Filter>>,
    store: crate::state::WorldStore<Given>,
    world: crate::world::World,
    /// The inventories, `None` on a build without them: items are not given.
    inventories: Option<crate::inventory::Inventories>,
    buffs: crate::buffs::Buffs,
    knowledge: crate::knowledge::Knowledge,
    scheduler: crate::schedule::Scheduler,
    experience: Pending,
}

/// Each player's experience still to add, in slots a worker thread takes
/// from without a lock: the entity in a slot's high half, the amount in the
/// low half, 0 for a free slot.
struct Pending([AtomicU64; PENDING]);

impl Pending {
    const fn new() -> Self {
        Self([const { AtomicU64::new(0) }; PENDING])
    }

    /// Adds `amount` to `player`'s experience, in the slot that holds theirs
    /// or else a free one; false for no player or when every slot holds
    /// another player's.
    fn add(&self, player: Entity, amount: u32) -> bool {
        if player.is_none() {
            return false;
        }
        let entity = u64::from(player.0) << 32;
        let theirs = |current: u64| current != 0 && current & !u64::from(u32::MAX) == entity;
        for claim_free in [false, true] {
            for slot in &self.0 {
                let mut current = slot.load(Ordering::Acquire);
                while theirs(current) || (claim_free && current == 0) {
                    let held = u32::try_from(current & u64::from(u32::MAX)).unwrap_or(u32::MAX);
                    let next = entity | u64::from(held.saturating_add(amount));
                    match slot.compare_exchange(current, next, Ordering::AcqRel, Ordering::Acquire)
                    {
                        Ok(_) => return true,
                        Err(seen) => current = seen,
                    }
                }
            }
        }
        false
    }

    /// Drops what `player` still waits for, freeing their slot.
    fn forget(&self, player: Entity) {
        if player.is_none() {
            return;
        }
        let entity = u64::from(player.0) << 32;
        for slot in &self.0 {
            let _gone = slot.try_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                (current != 0 && current & !u64::from(u32::MAX) == entity).then_some(0)
            });
        }
    }

    /// Drops what every player waits for, as a new world starts.
    fn clear(&self) {
        for slot in &self.0 {
            slot.store(0, Ordering::Release);
        }
    }

    /// Takes up to `room` of `player`'s experience.
    fn take(&self, player: Entity, room: u32) -> u32 {
        if room == 0 || player.is_none() {
            return 0;
        }
        let entity = u64::from(player.0) << 32;
        for slot in &self.0 {
            let mut current = slot.load(Ordering::Acquire);
            while current != 0 && current & !u64::from(u32::MAX) == entity {
                let held = u32::try_from(current & u64::from(u32::MAX)).unwrap_or(0);
                let taken = held.min(room);
                let left = held - taken;
                let next = if left == 0 {
                    0
                } else {
                    entity | u64::from(left)
                };
                match slot.compare_exchange(current, next, Ordering::AcqRel, Ordering::Acquire) {
                    Ok(_) => return taken,
                    Err(seen) => current = seen,
                }
            }
        }
        0
    }
}

/// Each mod's rewards state by its handle: a mod DLL holds one, the
/// loader's own copy of this crate one per data pack or script it runs.
static SHARED: crate::fresh::Fresh<Mutex<Vec<(ModHandle, &'static Shared)>>> =
    crate::fresh::Fresh::new();

/// The rewards state of mod `me`, `None` before its first reward.
fn find(me: ModHandle) -> Option<&'static Shared> {
    SHARED
        .get_or_init(|| Mutex::new(Vec::new()))
        .relock()
        .iter()
        .find(|(handle, _)| *handle == me)
        .map(|&(_, shared)| shared)
}

/// The mod's rewards state, made on the first call during init.
fn shared(load: &mut Load<'_>) -> Result<&'static Shared, Off> {
    let me = load.host().handle();
    if let Some(shared) = find(me) {
        return Ok(shared);
    }
    let made = Shared {
        rewards: Mutex::new(Vec::new()),
        filters: Mutex::new(Vec::new()),
        store: load.world_store("rewards")?,
        world: load.world()?,
        inventories: load.inventories().ok(),
        buffs: load.buffs(),
        knowledge: load.knowledge(),
        scheduler: load.scheduler()?,
        experience: Pending::new(),
    };
    let shared: &'static Shared = Box::leak(Box::new(made));
    load.on_experience(move |rows, _levels| {
        for row in rows {
            let room = row
                .need
                .saturating_sub(row.progress)
                .saturating_sub(row.gained);
            row.gained = row
                .gained
                .saturating_add(shared.experience.take(row.entity, room));
        }
    })?;
    load.on::<crate::events::PlayerLeft>(move |left| shared.experience.forget(left.entity))?;
    load.on::<crate::events::WorldLoaded>(move |()| shared.experience.clear())?;
    SHARED
        .get_or_init(|| Mutex::new(Vec::new()))
        .relock()
        .push((me, shared));
    Ok(shared)
}

impl Shared {
    /// Gives `reward` to `player` on the game thread if its limit, chance
    /// and the mod's filters allow; whether it was given.
    fn grant(&self, reward: &Reward, player: Entity) -> bool {
        let Some(key) = self.world.player(player).map(|p| p.key.get()) else {
            return false;
        };
        let day = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs() / 86_400);
        if !self
            .store
            .with(|given| given.allows(reward, key, day))
            .unwrap_or(false)
        {
            return false;
        }
        if reward
            .chance
            .is_some_and(|chance| fastrand::f32() >= chance)
        {
            return false;
        }
        let grant = RewardGrant {
            reward: reward.name.clone(),
            player,
            give: reward.give.clone(),
        };
        let filters = self.filters.relock().clone();
        if !filters.iter().all(|filter| filter(&grant)) {
            return false;
        }
        let _noted = self.store.update(|given| given.note(reward, key, day));
        self.give(player, &reward.give);
        crate::debug!("reward {} given to player {}", reward.name, player.0);
        true
    }

    /// Gives each of `give` to `player`, on the game thread.
    fn give(&self, player: Entity, give: &[Give]) {
        for &one in give {
            let given = match one {
                Give::Items { item, count } => match self.inventories {
                    Some(inventories) => {
                        let _ticket = inventories.give(player, item, count, move |outcome| {
                            if !outcome.is_committed() {
                                crate::warn!("reward: {count} items {} did not fit", item.get());
                            }
                        });
                        Ok(())
                    }
                    None => Err("this game build has no inventories".to_owned()),
                },
                Give::Experience(amount) => {
                    if self.experience.add(player, amount) {
                        Ok(())
                    } else {
                        Err("too many players wait for experience".to_owned())
                    }
                }
                Give::Knowledge(id) => self
                    .knowledge
                    .grant(player, id, crate::knowledge::Grant::Set)
                    .map_err(|e| e.to_string()),
                Give::Buff(id) => self
                    .buffs
                    .apply(player, id, crate::buffs::Apply::default())
                    .map_err(|e| e.to_string()),
            };
            if let Err(why) = given {
                crate::warn!("reward: {one:?} not given to player {}: {why}", player.0);
            }
        }
    }
}

/// Puts `reward` in place of mod `me`'s declared reward of its name, for a
/// pack whose numbers follow the world's settings.
pub(crate) fn replace(me: ModHandle, reward: Reward) {
    if let Some(shared) = find(me)
        && let Some(old) = shared
            .rewards
            .relock()
            .iter_mut()
            .find(|r| r.name == reward.name)
    {
        *old = reward;
    }
}

/// How long a craft request waits for its success; a request the crafting
/// system refused or never made leaves no success, and expires.
const CRAFT_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// The player of the oldest request in `asked` still waiting at `now`, for a
/// success of its recipe; expired requests go first.
fn crafted_by(asked: &mut VecDeque<(Entity, Instant)>, now: Instant) -> Option<Entity> {
    while let Some(&(player, at)) = asked.front() {
        asked.pop_front();
        if now.saturating_duration_since(at) <= CRAFT_WAIT {
            return Some(player);
        }
    }
    None
}

/// Gives the reward named `name` to `player` on the game thread's next
/// pass, from any thread.
fn grant_later(shared: &'static Shared, name: &str, player: Entity) {
    let name = name.to_owned();
    shared.scheduler.next_tick(move || {
        let reward = shared
            .rewards
            .relock()
            .iter()
            .find(|r| r.name == name)
            .cloned();
        if let Some(reward) = reward {
            let _given = shared.grant(&reward, player);
        }
    });
}

/// Gives the mod's declared rewards from code, on the server.
///
/// Get it from [`Load::rewards`]. Copy it anywhere, and call it on the game
/// thread: in an event handler or a [`Scheduler`](crate::schedule::Scheduler)
/// task. An item handler runs on the thread of the game's system, so it
/// schedules the grant.
///
/// # Examples
///
/// ```
/// use enshwick::rewards::{Action, Give, Reward};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.add_reward(Reward::new("greeting", Action::Join).with(Give::Experience(10)))?;
///     let rewards = load.rewards()?;
///     load.on::<enshwick::events::PlayerJoined>(move |joined| {
///         let _given = rewards.grant(joined.entity, "greeting");
///     })
/// }
/// ```
#[derive(Clone, Copy)]
pub struct Rewards(ModHandle);

impl core::fmt::Debug for Rewards {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Rewards")
    }
}

impl crate::script::Bound for Rewards {
    const NAME: &'static str = "rewards";
    const DOC: &'static str = "The mod's rewards for players' actions, given from code.";
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[Self::SCRIPT_GIVING];

    /// Opened for every script, so it makes no state: a script that
    /// declares no reward pays nothing.
    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(Self(load.host().handle()))
    }
}

#[crate::script(giving)]
impl Rewards {
    /// Gives the declared reward named `reward` to `player` as its action
    /// would: within its limit and chance, if the mod's filters allow; false
    /// for a reward the mod did not declare, or one its limit, its chance or
    /// a filter holds back.
    ///
    /// # Examples
    ///
    /// See [`Rewards`].
    #[script]
    #[must_use]
    pub fn grant(self, player: Entity, reward: &str) -> bool {
        let Some(shared) = find(self.0) else {
            return false;
        };
        let found = shared
            .rewards
            .relock()
            .iter()
            .find(|r| r.name == reward)
            .cloned();
        found.is_some_and(|reward| shared.grant(&reward, player))
    }

    /// Gives `give` to `player` now, with no limit, chance or filter; a mod
    /// that declared no reward and no filter gives nothing, and the log
    /// says so.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::rewards::{Give, Rewards};
    ///
    /// fn bonus(rewards: Rewards, player: enshwick::Entity) {
    ///     rewards.give(player, &[Give::Experience(25)]);
    /// }
    /// ```
    #[script]
    pub fn give(self, player: Entity, give: &[Give]) {
        match find(self.0) {
            Some(shared) => shared.give(player, give),
            None => crate::warn!("rewards: {give:?} not given, the mod declared no reward"),
        }
    }
}

#[crate::script(rewards)]
impl Load<'_> {
    /// Declares `reward`: the server gives it in the server's world when a
    /// player does its action, within its limit and chance, if the mod's
    /// filters allow.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, for a malformed name, a name declared before, a
    /// reward that gives nothing, a chance outside 0 to 1, or a build
    /// without what its action or the world's data need.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[script]
    pub fn add_reward(&mut self, reward: Reward) -> Result<(), Off> {
        if let Some(problem) = reward.problem() {
            return Err(Off::new(problem));
        }
        let shared = shared(self)?;
        {
            let mut rewards = shared.rewards.relock();
            if rewards.iter().any(|r| r.name == reward.name) {
                return Err(Off::new(format!("reward {}: declared twice", reward.name)));
            }
            rewards.push(reward.clone());
        }
        let name = reward.name;
        match reward.on {
            Action::Craft(recipe) => {
                let asked: Arc<Mutex<VecDeque<(Entity, Instant)>>> = Arc::default();
                let pending = Arc::clone(&asked);
                self.on::<CraftRecipeEvent>(move |request| {
                    if request.recipe_id == recipe && !request.crafting_operator_id.is_none() {
                        let mut pending = pending.relock();
                        if pending.len() >= 64 {
                            pending.pop_front();
                        }
                        pending.push_back((request.crafting_operator_id, Instant::now()));
                    }
                })?;
                let reward = name.clone();
                self.on::<CraftRecipeSuccessEvent>(move |made| {
                    if made.recipe_id == recipe
                        && let Some(player) = crafted_by(&mut asked.relock(), Instant::now())
                    {
                        grant_later(shared, &reward, player);
                    }
                })
            }
            Action::LevelUp(level) => self.on::<LevelUpEvent>(move |up| {
                if level == 0 || up.level == level {
                    grant_later(shared, &name, up.player_entity_id);
                }
            }),
            Action::Join => self.on::<crate::events::PlayerJoined>(move |joined| {
                grant_later(shared, &name, joined.entity);
            }),
            Action::Loot => self.on::<LootInteractionEvent>(move |looted| {
                grant_later(shared, &name, looted.guest_id);
            }),
            Action::Quest => self.on::<crate::events::QuestCompleted>(move |quest| {
                grant_later(shared, &name, quest.entity);
            }),
            Action::Consume(item) => self.on_item(item, crate::items::Kind::Consume, move |used| {
                grant_later(shared, &name, used.player);
                true
            }),
            Action::Pickup(item) => self.on_item(item, crate::items::Kind::Pickup, move |picked| {
                grant_later(shared, &name, picked.player);
                true
            }),
        }
    }

    /// Runs `filter` before the mod gives any of its rewards, on the game
    /// thread; returning `false` holds the grant back, and the limit stays
    /// unused. To give something else instead, give it with
    /// [`Rewards::give`] and return `false`.
    ///
    /// # Errors
    ///
    /// As [`Load::add_reward`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     // No rewards for the first player, entity 1.
    ///     load.on_reward(|grant| grant.player != enshwick::Entity(1))
    /// }
    /// ```
    #[script]
    pub fn on_reward<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&RewardGrant) -> bool + Send + Sync + 'static,
    {
        shared(self)?.filters.relock().push(Arc::new(filter));
        Ok(())
    }

    /// The handle that gives the mod's declared rewards from code.
    ///
    /// # Errors
    ///
    /// As [`Load::add_reward`].
    ///
    /// # Examples
    ///
    /// See [`Rewards`].
    pub fn rewards(&mut self) -> Result<Rewards, Off> {
        shared(self).map(|_| Rewards(self.host().handle()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_limit_holds_once_per_player_per_day_or_per_world() {
        let once = Reward::new("first", Action::Join)
            .with(Give::Experience(1))
            .with_limit(Limit::Once);
        let daily = Reward::new("daily", Action::Loot)
            .with(Give::Experience(1))
            .with_limit(Limit::Daily);
        let world = Reward::new("world", Action::Quest)
            .with(Give::Experience(1))
            .with_limit(Limit::World);
        let mut given = Given::default();
        for reward in [&once, &daily, &world] {
            assert!(given.allows(reward, 7, 100));
            given.note(reward, 7, 100);
            assert!(!given.allows(reward, 7, 100), "{}", reward.name);
        }
        assert!(given.allows(&once, 8, 100), "another player still gets it");
        assert!(given.allows(&daily, 7, 101), "the next day gives it again");
        assert!(!given.allows(&world, 8, 100), "once in the world");
    }

    #[test]
    fn pending_experience_comes_out_a_level_room_at_a_time_per_player() {
        let pending = Pending::new();
        assert!(pending.add(Entity(5), 120));
        assert!(pending.add(Entity(6), 10));
        assert!(pending.add(Entity(5), 30));
        assert_eq!(pending.take(Entity(5), 100), 100);
        assert_eq!(pending.take(Entity(5), 0), 0);
        assert_eq!(pending.take(Entity(5), 100), 50);
        assert_eq!(pending.take(Entity(5), 100), 0, "a slot frees once taken");
        assert_eq!(pending.take(Entity(6), 100), 10);
        for player in 1..=32 {
            assert!(pending.add(Entity(100 + player), 1));
        }
        assert!(!pending.add(Entity(500), 1), "every slot is held");
    }

    #[test]
    fn a_craft_pairs_with_a_recent_request_and_never_a_stale_one() {
        let start = Instant::now();
        let later = start + CRAFT_WAIT + std::time::Duration::from_secs(1);
        let mut asked = VecDeque::from([(Entity(1), start), (Entity(2), later)]);
        assert_eq!(
            crafted_by(&mut asked, later),
            Some(Entity(2)),
            "player 1's request expired"
        );
        assert_eq!(crafted_by(&mut asked, later), None);
    }

    #[test]
    fn a_player_keeps_one_slot_and_entity_zero_holds_none() {
        let pending = Pending::new();
        assert!(pending.add(Entity(1), 5));
        assert!(pending.add(Entity(2), 5));
        assert_eq!(pending.take(Entity(1), 10), 5);
        assert!(
            pending.add(Entity(2), 7),
            "the free first slot is not taken twice"
        );
        assert_eq!(pending.take(Entity(2), 100), 12, "one slot holds all of it");
        assert!(!pending.add(Entity(0), 3));
        assert!(pending.add(Entity(9), 4));
        assert_eq!(
            pending.take(Entity(0), 100),
            0,
            "no other player's experience"
        );
    }

    #[test]
    fn a_player_who_leaves_or_a_new_world_frees_the_slots() {
        let pending = Pending::new();
        for player in 1..=32 {
            assert!(pending.add(Entity(player), 1));
        }
        pending.forget(Entity(7));
        pending.forget(Entity(0));
        assert!(pending.add(Entity(40), 1), "the leaver's slot is free");
        assert!(!pending.add(Entity(41), 1));
        assert_eq!(pending.take(Entity(7), 10), 0, "nothing kept for them");
        pending.clear();
        assert_eq!(pending.take(Entity(1), 10), 0);
        assert!(pending.add(Entity(41), 1), "a new world starts empty");
    }

    #[test]
    fn a_reward_without_a_gift_a_bad_name_or_chance_is_refused() {
        assert!(Reward::new("none", Action::Join).problem().is_some());
        let ok = Reward::new("ok", Action::Join).with(Give::Experience(1));
        assert_eq!(ok.problem(), None);
        assert!(ok.clone().with_chance(1.5).problem().is_some());
        let mut bad = ok;
        bad.name = "Bad Name".to_owned();
        assert!(bad.problem().is_some());
    }
}
