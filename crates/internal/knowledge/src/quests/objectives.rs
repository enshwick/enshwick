//! Counting quest step objectives on the events the mod already gets: one
//! subscription per kind of event, never per objective, and a lookup that
//! leaves at once for an event no objective counts.

use std::sync::{Arc, Mutex, OnceLock};

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
#[derive(Debug, Default)]
pub(crate) struct Table {
    by: FxHashMap<Key, Vec<Entry>>,
    /// The reach objectives, checked on a tick while any is open.
    reach: Vec<Entry>,
    /// The items each crafted item objective counts the recipes of.
    craft_items: Vec<(ItemId, Entry)>,
}

impl Table {
    /// Adds `step`'s objectives.
    pub(crate) fn add(&mut self, step: CountedStep) {
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
        && step
            .objectives
            .iter()
            .enumerate()
            .all(|(i, (objective, _))| {
                let value = if i == at {
                    after
                } else {
                    held.get(i).copied().unwrap_or(0)
                };
                value >= objective.count()
            });
    Some((after - current, done))
}

/// The objectives a mod counts and the quests they read, shared by its
/// handlers.
#[derive(Debug)]
pub(crate) struct Counting {
    quests: Quests,
    table: Mutex<Table>,
    /// The recipes each crafted item objective counts, read on the first
    /// craft once the game's recipes read.
    craft_recipes: OnceLock<Vec<(RecipeId, Entry)>>,
}

impl Counting {
    /// Counts `to` for the player whose character is `player` on every
    /// objective `entries` names: a player's quest for that player, a world
    /// quest for the world.
    fn count(&self, player: Entity, entries: &[Entry], to: impl Fn(&Objective) -> Count) {
        for entry in entries {
            let holder = match entry.step.scope {
                Scope::World => Holder::World,
                _ => Holder::Player(player),
            };
            self.credit(holder, entry, to(&entry.step.objectives[entry.at].0));
        }
    }

    /// Grants what `to` adds to `entry`'s counter for `holder`, while its
    /// step shows and is not done.
    pub(crate) fn credit(&self, holder: Holder, entry: &Entry, to: Count) {
        let step = &entry.step;
        let reader = match holder {
            Holder::World => None,
            Holder::Player(player) => Some(player),
        };
        let open = self.quests.holds(reader, step.shown_when) == Some(true)
            && self.quests.holds(reader, step.done_when) == Some(false);
        if !open {
            return;
        }
        let knowledge = self.quests.knowledge;
        let held: Vec<u32> = step
            .objectives
            .iter()
            .map(|(_, counter)| {
                self.quests
                    .known(holder, *counter)
                    .map_or(0, |known| known.value)
            })
            .collect();
        let Some((add, done)) = credit(step, entry.at, &held, to) else {
            return;
        };
        let counter = step.objectives[entry.at].1;
        if let Err(error) = knowledge.grant(holder.entity(), counter, Grant::Add(add)) {
            crate::warn_every!(
                core::time::Duration::from_secs(60),
                "quests: objective counter {counter} not raised: {error}"
            );
            return;
        }
        if let (true, Some(bool)) = (done, step.done_bool)
            && let Err(error) = knowledge.grant(holder.entity(), bool, Grant::Set)
        {
            crate::warn_every!(
                core::time::Duration::from_secs(60),
                "quests: step bool {bool} not set: {error}"
            );
        }
    }

    /// The objectives that count a craft of `recipe`: those of the recipe
    /// and those of the vanilla item it makes.
    fn crafted(&self, recipe: RecipeId) -> Vec<Entry> {
        let mut entries = self.table.relock().entries(&Key::Recipe(recipe)).to_vec();
        let items = self.craft_recipes.get_or_init(|| {
            let table = self.table.relock();
            let recipes = crate::content::vanilla::recipes().unwrap_or_default();
            table
                .craft_items
                .iter()
                .flat_map(|(item, entry)| {
                    recipes
                        .iter()
                        .filter(|r| r.output.is_some_and(|out| ItemId::from_guid(out) == *item))
                        .map(|r| (RecipeId::from_guid(r.guid), entry.clone()))
                })
                .collect()
        });
        entries.extend(
            items
                .iter()
                .filter(|(r, _)| *r == recipe)
                .map(|(_, e)| e.clone()),
        );
        entries
    }
}

/// Starts counting `steps` for the mod: the first step of a kind of event
/// subscribes to it, once per mod.
///
/// # Errors
///
/// [`Off`] when a subscription fails, as on a build without the event.
pub(crate) fn count(load: &mut crate::BaseLoad, steps: Vec<CountedStep>) -> Result<(), Off> {
    if steps.is_empty() {
        return Ok(());
    }
    let quests = super::LoadMethods::quests(load);
    let counting = *quests.counting.get_or_init(|| {
        Box::leak(Box::new(Counting {
            quests,
            table: Mutex::default(),
            craft_recipes: OnceLock::new(),
        }))
    });
    let before = Kinds::of(&counting.table.relock());
    for step in steps {
        counting.table.relock().add(step);
    }
    let table = counting.table.relock();
    let after = Kinds::of(&table);
    let items: Vec<ItemId> = table
        .by
        .keys()
        .filter_map(|key| match key {
            Key::Collect(item) => Some(*item),
            _ => None,
        })
        .collect();
    drop(table);
    subscribe(load, counting, before, after, &items)
}

/// Which kinds of event a table counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Kinds(u8);

impl Kinds {
    const CRAFT: u8 = 1;
    const KILL: u8 = 2;
    const BUILD: u8 = 4;
    const REACH: u8 = 8;

    fn of(table: &Table) -> Self {
        let craft = !table.craft_items.is_empty() || table.counts(|k| matches!(k, Key::Recipe(_)));
        let kill = table.counts(|k| matches!(k, Key::Template(_) | Key::Faction(_)));
        let build = table.counts(|k| matches!(k, Key::Build(_)));
        let reach = !table.reach.is_empty();
        let kinds = [
            (craft, Self::CRAFT),
            (kill, Self::KILL),
            (build, Self::BUILD),
            (reach, Self::REACH),
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

/// Subscribes to each kind `after` counts and `before` did not, and to the
/// pickups of each item in `items` not yet followed.
fn subscribe(
    load: &mut crate::BaseLoad,
    counting: &'static Counting,
    before: Kinds,
    after: Kinds,
    items: &[ItemId],
) -> Result<(), Off> {
    use enshwick_inventory::__load::*;
    use enshwick_world::__load::*;
    if before.gains(after, Kinds::CRAFT) {
        crate::rewards::on_crafted(
            load,
            |_| true,
            move |player, recipe| {
                let entries = counting.crafted(recipe);
                counting.count(player, &entries, |_| Count::Add(1));
            },
        )?;
    }
    if before.gains(after, Kinds::KILL) {
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
            for key in keys {
                let entries = counting.table.relock().entries(&key).to_vec();
                counting.count(player, &entries, |_| Count::Add(1));
            }
        })?;
    }
    if before.gains(after, Kinds::BUILD) {
        load.on::<BuildingPlaced>(move |placed| {
            if placed.kind != crate::abi::BuildingData::PLACED || placed.replay != 0 {
                return;
            }
            let entries = counting
                .table
                .relock()
                .entries(&Key::Build(placed.item))
                .to_vec();
            counting.count(placed.character, &entries, |_| Count::Add(1));
        })?;
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
            let entries = counting
                .table
                .relock()
                .entries(&Key::Collect(item))
                .to_vec();
            let holds = inventories.count(picked.player, item);
            counting.count(picked.player, &entries, |_| Count::UpTo(holds));
            true
        })?;
        load.slot::<FollowedItems>()
            .get_or_insert_with(Default::default)
            .0
            .push(item);
    }
    if before.gains(after, Kinds::REACH) {
        let world = load.world()?;
        load.on_tick(core::time::Duration::from_secs(1), move |_| {
            let reach = counting.table.relock().reach.clone();
            for player in world.players() {
                let Some(at) = world.position(player.entity) else {
                    continue;
                };
                let near: Vec<Entry> = reach
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
    move |item| table.entries(&Key::Build(item)).len()
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
    ///     quests.signal(Holder::Player(player), id!("beacon-lit"), 1);
    /// }
    /// ```
    #[script]
    pub fn signal(&self, holder: Holder, signal: crate::content::Id, amount: u32) {
        let Some(counting) = self.counting.get() else {
            return;
        };
        let entries = counting
            .table
            .relock()
            .entries(&Key::Signal(signal.guid()))
            .to_vec();
        let world = holder == Holder::World;
        for entry in entries
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
        assert_eq!(kinds, Kinds(Kinds::CRAFT | Kinds::BUILD));
    }
}
