//! The quest events, a quest started, a step done and a part counted,
//! derived from the knowledge changes the store raises.
//!
//! The index maps each knowledge id to the quest steps whose queries test
//! it, built once a world from the journal and the query database the game
//! serves, the mods' quests and queries in, read as the mods read them. A
//! step a game query decides sits under each knowledge id that query reads;
//! the game answers it from its own query pass, which may come a step
//! later, so such a step is asked again for [`EXTERN_STEPS`] steps after a
//! change. Each holder's values of the indexed ids are kept from the first
//! step the loader sees the holder, so a change raises only what it moved,
//! and what a player holds when they join raises nothing.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use arc_swap::ArcSwapOption;
use enshwick_core::abi::{Event, Header, KnowledgeId, QuestData, QuestProgressData, QuestStepData};
use enshwick_core::content::knowledge::{KnowledgeQuery, KnowledgeTest};
use enshwick_core::quests::QuestInfo;
use enshwick_core::{Entity, Relock as _};
use rustc_hash::FxHashMap;

use crate::engine::loader::Loader;

/// The events this module raises.
const EVENTS: [Event; 3] = [
    Event::QUEST_STARTED,
    Event::QUEST_STEP_DONE,
    Event::QUEST_PROGRESSED,
];
/// How many steps after a change a step a game query decides is asked again.
const EXTERN_STEPS: u8 = 3;
/// How long a journal that did not read waits before the next try.
const RETRY: Duration = Duration::from_secs(10);

/// What a step's query decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    /// The quest's first step shows: the quest started.
    Started,
    /// The step is done.
    Done,
    /// A part the journal counts, and the count it asks for.
    Part { part: u32, target: u32 },
}

/// One query of a quest step, under a knowledge id it reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Use {
    quest: u32,
    step: u32,
    role: Role,
    query: KnowledgeQuery,
}

/// Every step query by the knowledge id it reads.
#[derive(Debug, Default)]
struct Index(FxHashMap<u32, Vec<Use>>);

impl Index {
    /// The index of `quests`, a game query's steps under each of the ids
    /// `game_queries` says it reads.
    fn of(quests: &[QuestInfo], game_queries: &[(u32, Vec<u32>)]) -> Self {
        let mut index = Self::default();
        for quest in quests {
            for (step, info) in (0u32..).zip(&quest.steps) {
                let started = (step == 0).then_some(info.shown_when).flatten();
                let parts = (0u32..).zip(&info.progress).filter_map(|(part, query)| {
                    target(query.test).map(|target| (Role::Part { part, target }, *query))
                });
                let roles = started
                    .map(|query| (Role::Started, query))
                    .into_iter()
                    .chain(info.done_when.map(|query| (Role::Done, query)))
                    .chain(parts);
                for (role, query) in roles {
                    let each = Use {
                        quest: quest.id.get(),
                        step,
                        role,
                        query,
                    };
                    index.add(each, game_queries);
                }
            }
        }
        index
    }

    fn add(&mut self, each: Use, game_queries: &[(u32, Vec<u32>)]) {
        match each.query.test {
            KnowledgeTest::Holds => {
                let found = game_queries.binary_search_by_key(&each.query.id.get(), |(id, _)| *id);
                for &id in found.map_or(&[][..], |at| &game_queries[at].1) {
                    self.0.entry(id).or_default().push(each);
                }
            }
            KnowledgeTest::Unknown => {}
            _ => self.0.entry(each.query.id.get()).or_default().push(each),
        }
    }

    /// Every knowledge id the index reads.
    fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.0.keys().copied()
    }
}

/// The count a progress part asks for, `None` for a part that counts
/// nothing.
const fn target(test: KnowledgeTest) -> Option<u32> {
    match test {
        KnowledgeTest::AtLeast(count) | KnowledgeTest::Equals(count) => Some(count),
        KnowledgeTest::IsSet => Some(1),
        _ => None,
    }
}

/// An event to raise, made under the state's lock and raised after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Raise {
    Started(QuestData),
    Done(QuestStepData),
    Counted(QuestProgressData),
}

impl Raise {
    /// What `each` raises for `entity` when its query went from `before` to
    /// `now` and its entry from `old` to `value`.
    fn of(
        each: &Use,
        entity: Entity,
        before: bool,
        now: bool,
        old: u32,
        value: u32,
    ) -> Option<Self> {
        let quest = KnowledgeId::new(each.quest);
        match each.role {
            Role::Started => (now && !before).then(|| {
                Self::Started(QuestData {
                    header: Header::of::<QuestData>(),
                    entity,
                    quest,
                })
            }),
            Role::Done => (now && !before).then(|| {
                Self::Done(QuestStepData {
                    header: Header::of::<QuestStepData>(),
                    entity,
                    quest,
                    step: each.step,
                })
            }),
            Role::Part { part, target } => (value != old).then(|| {
                Self::Counted(QuestProgressData {
                    header: Header::of::<QuestProgressData>(),
                    entity,
                    quest,
                    step: each.step,
                    part,
                    value,
                    target,
                })
            }),
        }
    }

    fn emit(&self, loader: &Loader) {
        match self {
            Self::Started(data) => {
                loader.emit(Event::QUEST_STARTED, None, (&raw const *data).cast());
            }
            Self::Done(data) => {
                loader.emit(Event::QUEST_STEP_DONE, None, (&raw const *data).cast());
            }
            Self::Counted(data) => {
                loader.emit(Event::QUEST_PROGRESSED, None, (&raw const *data).cast());
            }
        }
    }
}

/// One holder's values of the indexed ids and answers of their game
/// queries, as last seen.
#[derive(Debug, Default)]
struct Holder {
    values: FxHashMap<u32, u32>,
    answers: FxHashMap<u32, bool>,
}

/// What the module follows of the running world.
#[derive(Debug, Default)]
struct Held {
    holders: FxHashMap<u32, Holder>,
    /// Steps a game query decides, to ask again: holder, use, steps left.
    pending: Vec<(Entity, Use, u8)>,
    /// When the journal last failed to read.
    failed: Option<Instant>,
}

/// The index, read without a lock on every change, and what the module
/// holds of the running world.
struct Events {
    index: ArcSwapOption<Index>,
    held: Mutex<Held>,
}

static STATE: Events = Events {
    index: ArcSwapOption::const_empty(),
    held: Mutex::new(Held {
        holders: FxHashMap::with_hasher(rustc_hash::FxBuildHasher),
        pending: Vec::new(),
        failed: None,
    }),
};

/// Whether a mod hears any of the quest events.
pub(crate) fn wanted() -> bool {
    EVENTS
        .iter()
        .any(|&event| crate::engine::loader::handles(event))
}

/// Builds the index once a world, from the journal the game serves.
fn index() -> Option<Arc<Index>> {
    if let Some(index) = STATE.index.load_full() {
        return Some(index);
    }
    let mut state = STATE.held.relock();
    if state.failed.is_some_and(|at| at.elapsed() < RETRY) {
        return None;
    }
    let host = enshwick_core::host()?;
    let read = enshwick_core::__private::quests::journal_quests(host).and_then(|quests| {
        let game = enshwick_core::__private::quests::game_query_knowledge(host)?;
        Ok(Index::of(&quests, &game))
    });
    match read {
        Ok(index) => {
            crate::log!(
                INFO,
                "quest events: {} knowledge ids index the quest steps",
                index.0.len()
            );
            let index = Arc::new(index);
            STATE.index.store(Some(Arc::clone(&index)));
            Some(index)
        }
        Err(off) => {
            crate::log_once!(WARN, "quest events: the journal does not read yet: {off}");
            state.failed = Some(Instant::now());
            None
        }
    }
}

/// Notes the holder `entity`, the world for `Entity(0)`, before the loader
/// raises its log of this step: a holder seen for the first time has its
/// values read, so its log of this step raises nothing.
pub(crate) fn look(loader: &Loader, entity: Entity) {
    if !wanted() {
        return;
    }
    let Some(index) = index() else {
        return;
    };
    if STATE.held.relock().holders.contains_key(&entity.0) {
        return;
    }
    let holder = snapshot(loader, &index, entity);
    STATE.held.relock().holders.insert(entity.0, holder);
}

/// The holder's values of every indexed id and answers of every game query
/// the index asks, as they read now.
#[cold]
fn snapshot(loader: &Loader, index: &Index, entity: Entity) -> Holder {
    let mut holder = Holder::default();
    for id in index.ids() {
        let value = crate::knowledge::store::value(loader, entity, id).map_or(0, |info| info.value);
        holder.values.insert(id, value);
    }
    for each in index.0.values().flatten() {
        if let KnowledgeTest::Holds = each.query.test {
            let query = each.query.id.get();
            if let Ok(held) = crate::knowledge::store::query::holds(loader, entity, query) {
                holder.answers.insert(query, held);
            }
        }
    }
    holder
}

/// Forgets the holder `entity`, whose record went away.
pub(crate) fn gone(entity: Entity) {
    if STATE.index.load().is_some() {
        STATE.held.relock().holders.remove(&entity.0);
    }
}

/// Raises what the change of `id` to `value` for `entity` moved.
pub(crate) fn changed(loader: &Loader, entity: Entity, id: u32, value: u32) {
    let index = STATE.index.load();
    let Some(uses) = index.as_ref().and_then(|index| index.0.get(&id)) else {
        return;
    };
    let raised = moved(uses, entity, id, value);
    for raise in raised {
        raise.emit(loader);
    }
}

/// What the change moved, and the game queries to ask again noted.
#[cold]
fn moved(uses: &[Use], entity: Entity, id: u32, value: u32) -> Vec<Raise> {
    let mut state = STATE.held.relock();
    let Held {
        holders, pending, ..
    } = &mut *state;
    let Some(holder) = holders.get_mut(&entity.0) else {
        return Vec::new();
    };
    let old = holder.values.insert(id, value).unwrap_or(0);
    let mut raised = Vec::new();
    for each in uses {
        let test = each.query.test;
        if let KnowledgeTest::Holds = test {
            if !pending.iter().any(|(e, u, _)| *e == entity && u == each) {
                pending.push((entity, *each, EXTERN_STEPS));
            }
            continue;
        }
        let pass = |held| enshwick_core::__private::quests::passes(test, held).unwrap_or(false);
        raised.extend(Raise::of(each, entity, pass(old), pass(value), old, value));
    }
    raised
}

/// Asks again the game queries the last changes named, on the game thread
/// after the step.
pub(crate) fn after_step(loader: &Loader) {
    let mut raised = Vec::new();
    {
        let mut state = STATE.held.relock();
        if state.pending.is_empty() {
            return;
        }
        let Held {
            holders, pending, ..
        } = &mut *state;
        pending.retain_mut(|(entity, each, left)| {
            let query = each.query.id.get();
            let Some(holder) = holders.get_mut(&entity.0) else {
                return false;
            };
            let Ok(held) = crate::knowledge::store::query::holds(loader, *entity, query) else {
                *left -= 1;
                return *left > 0;
            };
            let before = holder.answers.insert(query, held).unwrap_or(false);
            let (was, now) = (before, held);
            raised.extend(Raise::of(each, *entity, was, now, 0, 0));
            *left -= 1;
            *left > 0 && was == now
        });
    }
    for raise in raised {
        raise.emit(loader);
    }
}

/// Forgets the index and every holder of a hosted game that ended.
pub(crate) fn forget() {
    STATE.index.store(None);
    *STATE.held.relock() = Held::default();
}

/// The check every knowledge change pays, on an index of `ids` knowledge
/// ids, for the benchmark: whether the change's id is one a quest step reads.
#[cfg(feature = "test")]
#[doc(hidden)]
pub fn bench_quest_change(ids: u32) -> impl Fn(u32) -> bool {
    let mut index = Index::default();
    for id in 1..=ids {
        index.0.insert(id.wrapping_mul(0x9e37_79b9), Vec::new());
    }
    STATE.index.store(Some(Arc::new(index)));
    |id| {
        STATE
            .index
            .load()
            .as_ref()
            .is_some_and(|index| index.0.contains_key(&id))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use enshwick_core::knowledge::Scope;

    fn query(id: u32, test: KnowledgeTest) -> KnowledgeQuery {
        KnowledgeQuery {
            id: KnowledgeId::new(id),
            scope: Scope::Player,
            test,
        }
    }

    fn use_of(role: Role, test: KnowledgeTest) -> Use {
        Use {
            quest: 0x51,
            step: 1,
            role,
            query: query(7, test),
        }
    }

    #[test]
    fn a_step_raises_once_as_its_query_comes_to_hold() {
        let done = use_of(Role::Done, KnowledgeTest::AtLeast(3));
        let player = Entity(2);
        assert!(Raise::of(&done, player, false, false, 1, 2).is_none());
        let raised = Raise::of(&done, player, false, true, 2, 3);
        assert!(
            matches!(raised, Some(Raise::Done(data)) if data.step == 1 && data.entity == player)
        );
        assert!(
            Raise::of(&done, player, true, true, 3, 4).is_none(),
            "already done"
        );
        let started = use_of(Role::Started, KnowledgeTest::IsSet);
        assert!(matches!(
            Raise::of(&started, player, false, true, 0, 1),
            Some(Raise::Started(_))
        ));
    }

    #[test]
    fn a_counted_part_raises_on_every_change_of_its_counter() {
        let part = use_of(Role::Part { part: 0, target: 5 }, KnowledgeTest::AtLeast(5));
        let raised = Raise::of(&part, Entity(2), false, false, 2, 3);
        assert!(
            matches!(raised, Some(Raise::Counted(data)) if (data.value, data.target) == (3, 5))
        );
        assert!(Raise::of(&part, Entity(2), false, false, 3, 3).is_none());
    }

    #[test]
    fn a_game_query_step_sits_under_each_id_its_query_reads() {
        let mut index = Index::default();
        let each = Use {
            quest: 0x51,
            step: 0,
            role: Role::Done,
            query: query(0x900, KnowledgeTest::Holds),
        };
        index.add(each, &[(0x900, vec![3, 4])]);
        index.add(
            Use {
                query: query(0x901, KnowledgeTest::Holds),
                ..each
            },
            &[(0x900, vec![3, 4])],
        );
        index.add(
            Use {
                query: query(5, KnowledgeTest::Unknown),
                ..each
            },
            &[],
        );
        let mut ids: Vec<u32> = index.ids().collect();
        ids.sort_unstable();
        assert_eq!(ids, [3, 4], "an unknown game query or form reads nothing");
    }

    #[test]
    fn only_a_part_that_counts_has_a_target() {
        assert_eq!(target(KnowledgeTest::AtLeast(4)), Some(4));
        assert_eq!(target(KnowledgeTest::IsSet), Some(1));
        assert_eq!(target(KnowledgeTest::HasFlag(2)), None);
    }

    #[test]
    fn a_change_of_an_unseen_holder_raises_nothing_and_a_seen_one_its_move() {
        forget();
        let uses = [use_of(Role::Done, KnowledgeTest::AtLeast(3))];
        assert!(moved(&uses, Entity(9), 7, 3).is_empty(), "not seen yet");
        STATE.held.relock().holders.insert(9, Holder::default());
        assert_eq!(moved(&uses, Entity(9), 7, 3).len(), 1);
        assert!(moved(&uses, Entity(9), 7, 4).is_empty());
        forget();
    }
}
