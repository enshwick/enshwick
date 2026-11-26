//! Counting quest step objectives on the events the mod already gets: one
//! subscription per kind of event, never per objective, and a lookup that
//! leaves at once for an event no objective counts.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use arc_swap::ArcSwap;
use rustc_hash::FxHashMap;

use crate::content::Guid;
use crate::content::journal::{CountedStep, CraftTarget, KillTarget, Objective};
use crate::events::BuildingPlaced;
use crate::game_events::EntityDiedEvent;
use crate::knowledge::{Grant, Scope};
use crate::locks::Relock as _;
use crate::typed::FactionType;
use crate::{Entity, ItemId, Off, RecipeId};

use super::{Holder, Quests};

/// What an event names that objectives count.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Key {
    /// An item picked up.
    Collect(ItemId),
    /// A recipe crafted.
    Recipe(RecipeId),
    /// An entity of a template killed.
    Template(Guid),
    /// An entity of a faction killed.
    Faction(FactionType),
    /// An item placed.
    Build(ItemId),
    /// A mod's signal.
    Signal(Guid),
}

/// One objective of one step, as the table holds it.
#[derive(Clone, Debug)]
pub(crate) struct Entry {
    step: Arc<CountedStep>,
    /// The objective's place in the step's list.
    at: usize,
}

/// How an event moves a counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Count {
    /// Up by this many.
    Add(u32),
    /// Up to what the inventory holds.
    UpTo(u32),
}

/// The objectives of a mod, by what they count.
#[derive(Clone, Debug, Default)]
pub(crate) struct Table {
    by: FxHashMap<Key, Vec<Entry>>,
    /// The reach objectives, checked on a tick while any is open.
    reach: Vec<Entry>,
    /// The items each crafted item objective counts the recipes of.
    craft_items: Vec<(ItemId, Entry)>,
    /// Whether a step ends on its own done bool, which a tick sets.
    own_done: bool,
}

impl Table {
    /// Adds `step`'s objectives.
    pub(crate) fn add(&mut self, step: CountedStep) {
        self.own_done |= step.done_bool.is_some();
        let step = Arc::new(step);
        for (at, (objective, _)) in step.objectives.iter().enumerate() {
            let entry = Entry {
                step: Arc::clone(&step),
                at,
            };
            let key = match objective {
                Objective::Collect { item, .. } => Key::Collect(*item),
                Objective::Kill {
                    target: KillTarget::Template(template),
                    ..
                } => Key::Template(*template),
                Objective::Kill {
                    target: KillTarget::Faction(faction),
                    ..
                } => Key::Faction(faction.clone()),
                Objective::Craft {
                    target: CraftTarget::Recipe(recipe),
                    ..
                } => Key::Recipe(*recipe),
                Objective::Craft {
                    target: CraftTarget::Item(item),
                    ..
                } => {
                    self.craft_items.push((*item, entry));
                    continue;
                }
                Objective::Build { item, .. } => Key::Build(*item),
                Objective::Custom { signal, .. } => Key::Signal(signal.guid()),
                Objective::Reach { .. } => {
                    self.reach.push(entry);
                    continue;
                }
                _ => continue,
            };
            self.by.entry(key).or_default().push(entry);
        }
    }

    /// The objectives that count `key`: the per-event check, one lookup.
    #[must_use]
    pub(crate) fn entries(&self, key: &Key) -> &[Entry] {
        self.by.get(key).map_or(&[], Vec::as_slice)
    }

    /// Whether any objective counts a key of this kind.
    fn counts(&self, kind: impl Fn(&Key) -> bool) -> bool {
        self.by.keys().any(kind)
    }
}

/// What one counted event grants on an objective at `at` of `step` whose
/// counters hold `held`, while the step is open for the holder: the amount
/// its counter goes up by, and whether the step's own done bool follows
/// because every counter then reaches its count; `None` for nothing.
pub(crate) fn credit(
    step: &CountedStep,
    at: usize,
    held: &[u32],
    to: Count,
) -> Option<(u32, bool)> {
    let target = step.objectives.get(at)?.0.count();
    let current = *held.get(at)?;
    let after = match to {
        Count::Add(amount) => current.saturating_add(amount),
        Count::UpTo(holds) => holds,
    }
    .min(target);
    if after <= current {
        return None;
    }
    let done = step.done_bool.is_some()
        && reached(step, |i| {
            if i == at {
                after
            } else {
                held.get(i).copied().unwrap_or(0)
            }
        });
    Some((after - current, done))
}

/// Whether every counter of `step`, at the value `held` gives it by place,
/// reaches its count.
fn reached(step: &CountedStep, held: impl Fn(usize) -> u32) -> bool {
    step.objectives
        .iter()
        .enumerate()
        .all(|(i, (objective, _))| held(i) >= objective.count())
}

/// How many steps a mod's counting keeps waiting for their done bool at
/// once; each holds a holder and a step, and a full list drops new ones.
const WAITING_MAX: usize = 256;

/// The objectives a mod counts and the quests they read, shared by its
/// handlers.
#[derive(Debug)]
pub(crate) struct Counting {
    quests: Quests,
    /// Written only while the mod loads, swapped whole, so an event reads
    /// it without a lock.
    table: ArcSwap<Table>,
    /// The recipes each crafted item objective counts, once the game's
    /// recipes read.
    craft_recipes: OnceLock<Vec<(RecipeId, Entry)>>,
    /// The kinds of event subscribed so far, so a failed subscription is
    /// tried again by the next quest that needs it.
    subscribed: AtomicU8,
    /// Steps of their own done bool whose last counter reached its count,
    /// for the holder: a grant applies only at the next step of the world,
    /// so two counters raised in one step each read the other's old value,
    /// and a tick sets the bool once both read reached.
    waiting: Mutex<Vec<(Holder, Arc<CountedStep>)>>,
}

impl Counting {
    /// Counts `to` for the player whose character is `player` on every
    /// objective `entries` names: a player's quest for that player, a world
    /// quest for the world.
    fn count(&self, player: Entity, entries: &[Entry], to: impl Fn(&Objective) -> Count) {
        for entry in entries {
            let holder = match entry.step.scope {
                Scope::World => Holder::World,
                _ => Holder::player(player),
            };
            self.credit(holder, entry, to(&entry.step.objectives[entry.at].0));
        }
    }

    /// Grants what `to` adds to `entry`'s counter for `holder`, while its
    /// step shows and is not done.
    pub(crate) fn credit(&self, holder: Holder, entry: &Entry, to: Count) {
        let step = &entry.step;
        if !self.open(holder, step) {
            return;
        }
        let knowledge = self.quests.knowledge;
        let held = self.held(holder, step);
        let Some((add, done)) = credit(step, entry.at, &held, to) else {
            return;
        };
        let (objective, counter) = &step.objectives[entry.at];
        if let Err(error) = knowledge.grant(holder.entity(), *counter, Grant::Add(add)) {
            crate::warn_every!(
                core::time::Duration::from_secs(60),
                "quests: objective counter {counter} not raised: {error}"
            );
            return;
        }
        let Some(bool) = step.done_bool else {
            return;
        };
        if !done {
            if held[entry.at].saturating_add(add) >= objective.count() {
                self.wait(holder, step);
            }
            return;
        }
        if let Err(error) = knowledge.grant(holder.entity(), bool, Grant::Set) {
            crate::warn_every!(
                core::time::Duration::from_secs(60),
                "quests: step bool {bool} not set: {error}"
            );
        }
    }

    /// Whether `step` shows and is not done for `holder`.
    fn open(&self, holder: Holder, step: &CountedStep) -> bool {
        let reader = match holder {
            Holder::World => None,
            Holder::Player(player) => player.entity(),
        };
        self.quests.holds(reader, step.shown_when) == Some(true)
            && self.quests.holds(reader, step.done_when) == Some(false)
    }

    /// The values of `step`'s counters for `holder`, 0 for one that does
    /// not read.
    fn held(&self, holder: Holder, step: &CountedStep) -> Vec<u32> {
        step.objectives
            .iter()
            .map(|(_, counter)| {
                self.quests
                    .known(holder, *counter)
                    .map_or(0, |known| known.value)
            })
            .collect()
    }

    /// Keeps `step` waiting for its done bool for `holder`, once.
    fn wait(&self, holder: Holder, step: &Arc<CountedStep>) {
        let mut waiting = self.waiting.relock();
        if waiting
            .iter()
            .any(|(at, held)| *at == holder && Arc::ptr_eq(held, step))
        {
            return;
        }
        if waiting.len() >= WAITING_MAX {
            crate::warn_every!(
                core::time::Duration::from_secs(60),
                "quests: {WAITING_MAX} steps wait for their done bool; one more waits for its next event"
            );
            return;
        }
        waiting.push((holder, Arc::clone(step)));
    }

    /// Sets the done bool of each waiting step whose counters all read
    /// reached, and stops waiting on a step that closed.
    fn settle(&self) {
        let waiting = core::mem::take(&mut *self.waiting.relock());
        if waiting.is_empty() {
            return;
        }
        let left: Vec<_> = waiting
            .into_iter()
            .filter(|(holder, step)| !self.settled(*holder, step))
            .collect();
        self.waiting.relock().extend(left);
    }

    /// Sets `step`'s done bool for `holder` once its counters read reached;
    /// whether the step waits no more.
    fn settled(&self, holder: Holder, step: &CountedStep) -> bool {
        let Some(bool) = step.done_bool else {
            return true;
        };
        if !self.open(holder, step) {
            return true;
        }
        let held = self.held(holder, step);
        if !reached(step, |i| held.get(i).copied().unwrap_or(0)) {
            return false;
        }
        match self
            .quests
            .knowledge
            .grant(holder.entity(), bool, Grant::Set)
        {
            Ok(()) => true,
            Err(error) => {
                crate::warn_every!(
                    core::time::Duration::from_secs(60),
                    "quests: step bool {bool} not set: {error}"
                );
                false
            }
        }
    }

    /// The recipes each crafted item objective counts, read from the
    /// game's vanilla recipes once they read; `None` until then.
    fn craft_recipes(&self) -> Option<&[(RecipeId, Entry)]> {
        if let Some(recipes) = self.craft_recipes.get() {
            return Some(recipes);
        }
        let recipes = match crate::content::vanilla::recipes() {
            Ok(recipes) => recipes,
            Err(off) => {
                crate::warn_every!(
                    core::time::Duration::from_secs(60),
                    "quests: crafted item objectives wait for the game's recipes: {off}"
                );
                return None;
            }
        };
        let table = self.table.load();
        let found = table
            .craft_items
            .iter()
            .flat_map(|(item, entry)| {
                recipes
                    .iter()
                    .filter(|r| r.output.is_some_and(|out| ItemId::from_guid(out) == *item))
                    .map(|r| (RecipeId::from_guid(r.guid), entry.clone()))
            })
            .collect();
        Some(self.craft_recipes.get_or_init(|| found))
    }

    /// Whether an objective counts a craft of `recipe`, without
    /// allocating: the check a craft request passes before it is queued.
    fn counts_craft(&self, recipe: RecipeId) -> bool {
        let table = self.table.load();
        !table.entries(&Key::Recipe(recipe)).is_empty()
            || (!table.craft_items.is_empty()
                && self
                    .craft_recipes()
                    .is_some_and(|items| items.iter().any(|(r, _)| *r == recipe)))
    }

    /// The objectives that count a craft of `recipe`: those of the recipe
    /// and those of the vanilla item it makes.
    fn crafted(&self, recipe: RecipeId) -> Vec<Entry> {
        let mut entries = self.table.load().entries(&Key::Recipe(recipe)).to_vec();
        if let Some(items) = self.craft_recipes() {
            entries.extend(
                items
                    .iter()
                    .filter(|(r, _)| *r == recipe)
                    .map(|(_, e)| e.clone()),
            );
        }
        entries
    }
}

/// Starts counting `steps` for the mod: the first step of a kind of event
/// subscribes to it, once per mod, and a kind whose subscription failed is
/// tried again by the next.
///
/// # Errors
///
/// [`Off`] when a subscription fails, as on a build without the event, or
/// when a kill objective by faction cannot read a dying entity's faction;
/// the steps count on every other kind.
pub(crate) fn count(load: &mut crate::BaseLoad, steps: Vec<CountedStep>) -> Result<(), Off> {
    if steps.is_empty() {
        return Ok(());
    }
    let quests = super::LoadMethods::quests(load);
    let counting = *quests.counting.get_or_init(|| {
        Box::leak(Box::new(Counting {
            quests,
            table: ArcSwap::default(),
            craft_recipes: OnceLock::new(),
            subscribed: AtomicU8::new(0),
            waiting: Mutex::default(),
        }))
    });
    let by_faction = steps.iter().any(|step| {
        step.objectives.iter().any(|(objective, _)| {
            matches!(
                objective,
                Objective::Kill {
                    target: KillTarget::Faction(_),
                    ..
                }
            )
        })
    });
    let mut table = Table::clone(&counting.table.load());
    for step in steps {
        table.add(step);
    }
    let after = Kinds::of(&table);
    let items: Vec<ItemId> = table
        .by
        .keys()
        .filter_map(|key| match key {
            Key::Collect(item) => Some(*item),
            _ => None,
        })
        .collect();
    counting.table.store(Arc::new(table));
    subscribe(load, counting, after, &items)?;
    if by_faction {
        crate::typed::ComponentType::<crate::typed::ecs::Faction>::resolve(load).map_err(|off| {
            Off::new(format!(
                "kill objectives by faction do not count, a dying entity's faction does not read: {off}"
            ))
        })?;
    }
    Ok(())
}

/// Which kinds of event a table counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Kinds(u8);

impl Kinds {
    const CRAFT: u8 = 1;
    const KILL: u8 = 2;
    const BUILD: u8 = 4;
    /// A tick: reach objectives and steps of their own done bool.
    const TICK: u8 = 8;

    fn of(table: &Table) -> Self {
        let craft = !table.craft_items.is_empty() || table.counts(|k| matches!(k, Key::Recipe(_)));
        let kill = table.counts(|k| matches!(k, Key::Template(_) | Key::Faction(_)));
        let build = table.counts(|k| matches!(k, Key::Build(_)));
        let tick = !table.reach.is_empty() || table.own_done;
        let kinds = [
            (craft, Self::CRAFT),
            (kill, Self::KILL),
            (build, Self::BUILD),
            (tick, Self::TICK),
        ];
        Self(
            kinds
                .iter()
                .filter(|(counts, _)| *counts)
                .fold(0, |all, (_, kind)| all | kind),
        )
    }

    /// Whether `after` counts the kind `kind` and `self` did not.
    const fn gains(self, after: Self, kind: u8) -> bool {
        after.0 & kind != 0 && self.0 & kind == 0
    }
}

impl Counting {
    /// Whether the table `after` counts the kind `kind` and no handler of
    /// it is subscribed yet.
    fn needs(&self, after: Kinds, kind: u8) -> bool {
        Kinds(self.subscribed.load(Ordering::Relaxed)).gains(after, kind)
    }

    /// Records the kind `kind` as subscribed.
    fn subscribed(&self, kind: u8) {
        self.subscribed.fetch_or(kind, Ordering::Relaxed);
    }
}

/// Subscribes to each kind `after` counts that has no handler yet, and to
/// the pickups of each item in `items` not yet followed.
fn subscribe(
    load: &mut crate::BaseLoad,
    counting: &'static Counting,
    after: Kinds,
    items: &[ItemId],
) -> Result<(), Off> {
    use enshwick_inventory::__load::*;
    use enshwick_world::__load::*;
    if counting.needs(after, Kinds::CRAFT) {
        crate::rewards::on_crafted(
            load,
            |recipe| counting.counts_craft(recipe),
            move |player, recipe| {
                let entries = counting.crafted(recipe);
                counting.count(player, &entries, |_| Count::Add(1));
            },
        )?;
        counting.subscribed(Kinds::CRAFT);
    }
    if counting.needs(after, Kinds::KILL) {
        let world = load.world()?;
        let faction = crate::typed::ComponentType::<crate::typed::ecs::Faction>::resolve(load).ok();
        load.on::<EntityDiedEvent>(move |died| {
            let Some(player) = died.killer_player else {
                return;
            };
            let mut keys = Vec::with_capacity(2);
            if let Some(template) = world.template(died.dead_entity_id) {
                keys.push(Key::Template(template));
            }
            if let Some(Ok(Some(of))) = faction.map(|ty| ty.read(world, died.dead_entity_id)) {
                keys.push(Key::Faction(of.faction));
            }
            let table = counting.table.load();
            for key in keys {
                counting.count(player, table.entries(&key), |_| Count::Add(1));
            }
        })?;
        counting.subscribed(Kinds::KILL);
    }
    if counting.needs(after, Kinds::BUILD) {
        load.on::<BuildingPlaced>(move |placed| {
            if placed.kind != crate::abi::BuildingData::PLACED || placed.replay != 0 {
                return;
            }
            let table = counting.table.load();
            let entries = table.entries(&Key::Build(placed.item));
            counting.count(placed.character, entries, |_| Count::Add(1));
        })?;
        counting.subscribed(Kinds::BUILD);
    }
    let followed = load
        .slot::<FollowedItems>()
        .get_or_insert_with(Default::default)
        .0
        .clone();
    for &item in items.iter().filter(|item| !followed.contains(item)) {
        // Resolved only for a collect objective, which needs a build with inventories.
        let inventories = load.inventories()?;
        load.on_item(item, crate::items::Kind::Pickup, move |picked| {
            let table = counting.table.load();
            let Ok(holds) = inventories.count(picked.player, item) else {
                return true;
            };
            let entries = table.entries(&Key::Collect(item));
            counting.count(picked.player, entries, |_| Count::UpTo(holds));
            true
        })?;
        load.slot::<FollowedItems>()
            .get_or_insert_with(Default::default)
            .0
            .push(item);
    }
    if counting.needs(after, Kinds::TICK) {
        let world = load.world()?;
        load.on_tick(core::time::Duration::from_secs(1), move |_| {
            counting.settle();
            let table = counting.table.load();
            if table.reach.is_empty() {
                return;
            }
            for player in world.players() {
                let Some(at) = world.position(player.entity) else {
                    continue;
                };
                let near: Vec<Entry> = table
                    .reach
                    .iter()
                    .filter(|entry| match entry.step.objectives[entry.at].0 {
                        Objective::Reach { at: place, radius } => {
                            let d2: f64 =
                                (0..3).map(|i| (at[i] - f64::from(place[i])).powi(2)).sum();
                            d2 <= f64::from(radius).powi(2)
                        }
                        _ => false,
                    })
                    .cloned()
                    .collect();
                counting.count(player.entity, &near, |_| Count::Add(1));
            }
        })?;
        counting.subscribed(Kinds::TICK);
    }
    Ok(())
}

/// The per-event check over a table of `objectives` build objectives, for
/// [`__internal::objective_lookup`](crate::__internal::objective_lookup).
pub(crate) fn lookup(objectives: u32) -> impl Fn(ItemId) -> usize {
    let mut table = Table::default();
    for item in 1..=objectives {
        table.add(CountedStep {
            quest: crate::knowledge::KnowledgeId::new(1),
            step: item,
            scope: Scope::Player,
            shown_when: crate::content::knowledge::KnowledgeQuery::is_set(
                crate::knowledge::KnowledgeId::new(2),
                Scope::Player,
            ),
            done_when: crate::content::knowledge::KnowledgeQuery::is_set(
                crate::knowledge::KnowledgeId::new(3),
                Scope::Player,
            ),
            done_bool: None,
            objectives: vec![(
                Objective::Build {
                    item: ItemId::new(item),
                    count: 1,
                },
                crate::knowledge::KnowledgeId::new(item),
            )],
        });
    }
    let table = ArcSwap::from_pointee(table);
    move |item| table.load().entries(&Key::Build(item)).len()
}

/// The items whose pickups the mod's counting follows.
#[derive(Debug, Default)]
struct FollowedItems(Vec<ItemId>);

#[crate::script(objectives)]
impl Quests {
    /// Sends the mod's signal `signal` for `holder`: each open step of the
    /// mod's quests with an [`Objective::Custom`] of that signal counts
    /// `amount` towards it, a player's quest for a player and a world quest
    /// for the world. A signal no objective of the mod counts does nothing.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::id;
    /// use enshwick::quests::{Holder, Quests};
    ///
    /// fn lit(quests: &Quests, player: enshwick::Entity) {
    ///     quests.signal(Holder::player(player), id!("beacon-lit"), 1);
    /// }
    /// ```
    #[script]
    pub fn signal(&self, holder: Holder, signal: crate::content::Id, amount: u32) {
        let Ok(holder) = holder.resolved(self.knowledge.host) else {
            return;
        };
        let Some(counting) = self.counting.get() else {
            return;
        };
        let table = counting.table.load();
        let world = holder == Holder::World;
        for entry in table
            .entries(&Key::Signal(signal.guid()))
            .iter()
            .filter(|e| (e.step.scope == Scope::World) == world)
        {
            counting.credit(holder, entry, Count::Add(amount));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::knowledge::KnowledgeQuery;
    use crate::knowledge::KnowledgeId;

    fn step(objectives: Vec<Objective>, own_done: bool) -> CountedStep {
        let q = |id| KnowledgeQuery::is_set(KnowledgeId::new(id), Scope::Player);
        CountedStep {
            quest: KnowledgeId::new(1),
            step: 0,
            scope: Scope::Player,
            shown_when: q(2),
            done_when: q(3),
            done_bool: own_done.then_some(KnowledgeId::new(3)),
            objectives: objectives
                .into_iter()
                .zip((10..).map(KnowledgeId::new))
                .collect(),
        }
    }

    fn craft(count: u32) -> Objective {
        Objective::Craft {
            target: CraftTarget::Recipe(RecipeId::new(7)),
            count,
        }
    }

    #[test]
    fn a_counter_rises_up_to_its_count_and_no_further() {
        let one = step(vec![craft(3)], false);
        assert_eq!(credit(&one, 0, &[0], Count::Add(1)), Some((1, false)));
        assert_eq!(credit(&one, 0, &[2], Count::Add(5)), Some((1, false)));
        assert_eq!(credit(&one, 0, &[3], Count::Add(1)), None, "reached");
        assert_eq!(credit(&one, 0, &[1], Count::UpTo(2)), Some((1, false)));
        assert_eq!(
            credit(&one, 0, &[2], Count::UpTo(1)),
            None,
            "an inventory that shrank takes nothing back"
        );
    }

    #[test]
    fn the_step_s_own_bool_follows_only_once_every_counter_is_reached() {
        let two = step(
            vec![
                craft(2),
                Objective::Custom {
                    signal: crate::content::Id::new("you.q:s"),
                    count: 1,
                },
            ],
            true,
        );
        assert_eq!(credit(&two, 0, &[1, 0], Count::Add(1)), Some((1, false)));
        assert_eq!(credit(&two, 0, &[1, 1], Count::Add(1)), Some((1, true)));
        assert_eq!(credit(&two, 1, &[2, 0], Count::Add(1)), Some((1, true)));
    }

    #[test]
    fn an_event_no_objective_counts_finds_nothing_in_one_lookup() {
        let mut table = Table::default();
        assert!(table.entries(&Key::Recipe(RecipeId::new(7))).is_empty());
        table.add(step(
            vec![
                craft(2),
                Objective::Build {
                    item: ItemId::new(4),
                    count: 1,
                },
            ],
            true,
        ));
        assert_eq!(table.entries(&Key::Recipe(RecipeId::new(7))).len(), 1);
        assert_eq!(table.entries(&Key::Build(ItemId::new(4))).len(), 1);
        assert!(table.entries(&Key::Recipe(RecipeId::new(8))).is_empty());
        let kinds = Kinds::of(&table);
        assert_eq!(
            kinds,
            Kinds(Kinds::CRAFT | Kinds::BUILD | Kinds::TICK),
            "a step of its own done bool settles on a tick"
        );
    }

    #[test]
    fn two_counters_raised_in_one_world_step_each_miss_the_bool_and_a_tick_settles_it() {
        let two = step(
            vec![
                craft(1),
                Objective::Custom {
                    signal: crate::content::Id::new("you.q:s"),
                    count: 1,
                },
            ],
            true,
        );
        // Both events read the counters before either grant applies.
        assert_eq!(credit(&two, 0, &[0, 0], Count::Add(1)), Some((1, false)));
        assert_eq!(credit(&two, 1, &[0, 0], Count::Add(1)), Some((1, false)));
        assert_eq!(
            credit(&two, 0, &[1, 1], Count::Add(1)),
            None,
            "no later event of a reached counter sets it"
        );
        assert!(reached(&two, |i| [1, 1][i]), "what the tick reads");
        assert!(!reached(&two, |i| [1, 0][i]), "a grant not yet applied");
    }
}
