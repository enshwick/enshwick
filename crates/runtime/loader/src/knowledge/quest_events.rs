//! The quest events, a quest started, a step done and a part counted,
//! derived from the knowledge changes the store raises.
//!
//! The index maps each knowledge id to the quest steps whose queries test
//! it, built once a world from the journal and the query database the game
//! serves, the mods' quests and queries in, read as the mods read them. A
//! step a game query decides sits under each knowledge id that query reads;
//! the game answers it from its own query pass, which may come a step
//! later, so such a step is asked again for [`EXTERN_STEPS`] steps after a
//! change, and a player's such step again after a change of the world's
//! knowledge its query reads. Each holder's values of the indexed ids are
//! kept from the first step the loader sees the holder, a character only
//! once its save has loaded, so a change raises only what it moved, and
//! what a player holds when they join raises nothing. The loader's own
//! writes (revokes, exact sets) count as changes too.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use arc_swap::ArcSwapOption;
use enshwick_core::abi::{Event, Header, KnowledgeId, QuestData, QuestProgressData, QuestStepData};
use enshwick_core::content::knowledge::{KnowledgeQuery, KnowledgeTest};
use enshwick_core::knowledge::Scope;
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
/// Words of the seen holders, a bit for the world and each record.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the record limit, 4096, fits any usize"
)]
const SEEN_WORDS: usize = (super::store::MAX_RECORDS as usize + 1).div_ceil(64);

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
/// queries, as last seen; a query whose answer never read has none.
#[derive(Debug, Default)]
struct Holder {
    values: FxHashMap<u32, u32>,
    answers: FxHashMap<u32, bool>,
}

/// A step a game query decides, to ask again: holder, use, steps left.
type Ask = (Entity, Use, u8);

/// A holder's query asked this step: its answer before, if one read, and
/// now; `None` when it did not answer.
type Asked = Option<(Option<bool>, bool)>;

/// What the module follows of the running world.
#[derive(Debug, Default)]
struct Held {
    holders: FxHashMap<u32, Holder>,
    pending: Vec<Ask>,
    /// When the journal last failed to read.
    failed: Option<Instant>,
}

/// The index, read without a lock on every change, what the module holds of
/// the running world, and what each step reads of it without the lock.
struct Events {
    index: ArcSwapOption<Index>,
    held: Mutex<Held>,
    /// Whether `held` has steps to ask again.
    asking: AtomicBool,
    /// The holders in `held`, a bit for `Entity(n)` at bit `n`.
    seen: [AtomicU64; SEEN_WORDS],
}

static STATE: Events = Events {
    index: ArcSwapOption::const_empty(),
    held: Mutex::new(Held {
        holders: FxHashMap::with_hasher(rustc_hash::FxBuildHasher),
        pending: Vec::new(),
        failed: None,
    }),
    asking: AtomicBool::new(false),
    seen: [const { AtomicU64::new(0) }; SEEN_WORDS],
};

impl Events {
    /// Whether `entity` is a holder the module follows.
    fn seen(&self, entity: Entity) -> bool {
        let at = entity.0 as usize;
        self.seen
            .get(at / 64)
            .is_some_and(|word| word.load(Ordering::Relaxed) & (1 << (at % 64)) != 0)
    }

    /// Notes whether the module follows `entity`, under the `held` lock.
    fn mark(&self, entity: Entity, seen: bool) {
        let at = entity.0 as usize;
        if let Some(word) = self.seen.get(at / 64) {
            let bit = 1 << (at % 64);
            if seen {
                word.fetch_or(bit, Ordering::Relaxed);
            } else {
                word.fetch_and(!bit, Ordering::Relaxed);
            }
        }
    }
}

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
/// values read, so its log of this step raises nothing. A character counts
/// from the step its save has loaded, so the values the save brings raise
/// nothing either.
pub(crate) fn look(loader: &Loader, entity: Entity) {
    if !wanted() || STATE.seen(entity) {
        return;
    }
    if entity != Entity(0) && !super::store::changes::character_loaded(loader, entity) {
        return;
    }
    let Some(index) = index() else {
        return;
    };
    let holder = snapshot(loader, &index, entity);
    STATE.held.relock().holders.insert(entity.0, holder);
    STATE.mark(entity, true);
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
    if STATE.seen(entity) {
        let mut state = STATE.held.relock();
        state.holders.remove(&entity.0);
        state.pending.retain(|(asked, ..)| *asked != entity);
        STATE.mark(entity, false);
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
            if entity == Entity(0) && each.query.scope == Scope::Player {
                for &player in holders.keys().filter(|&&key| key != 0) {
                    ask_again(pending, Entity(player), each);
                }
            } else {
                ask_again(pending, entity, each);
            }
            continue;
        }
        let pass = |held| enshwick_core::__private::quests::passes(test, held).unwrap_or(false);
        raised.extend(Raise::of(each, entity, pass(old), pass(value), old, value));
    }
    if !pending.is_empty() {
        STATE.asking.store(true, Ordering::Relaxed);
    }
    raised
}

/// Has `each` asked again for `entity` the next [`EXTERN_STEPS`] steps,
/// counted from this change though it waits already.
fn ask_again(pending: &mut Vec<Ask>, entity: Entity, each: &Use) {
    match pending
        .iter_mut()
        .find(|(e, u, _)| *e == entity && u == each)
    {
        Some((_, _, left)) => *left = EXTERN_STEPS,
        None => pending.push((entity, *each, EXTERN_STEPS)),
    }
}

/// Asks again the game queries the last changes named, on the game thread
/// after the step.
pub(crate) fn after_step(loader: &Loader) {
    if !STATE.asking.load(Ordering::Relaxed) {
        return;
    }
    let raised = {
        let mut state = STATE.held.relock();
        let Held {
            holders, pending, ..
        } = &mut *state;
        let raised = ask(holders, pending, |entity, query| {
            crate::knowledge::store::query::holds(loader, entity, query).ok()
        });
        STATE.asking.store(!pending.is_empty(), Ordering::Relaxed);
        raised
    };
    for raise in raised {
        raise.emit(loader);
    }
}

/// What the pending steps raise, each holder's game query asked once by
/// `holds` (`None` when it does not answer); a step stays pending while
/// its answer has not moved and steps are left. An answer that never read
/// before is the holder's first and raises nothing.
fn ask(
    holders: &mut FxHashMap<u32, Holder>,
    pending: &mut Vec<Ask>,
    mut holds: impl FnMut(Entity, u32) -> Option<bool>,
) -> Vec<Raise> {
    let mut asked: FxHashMap<(u32, u32), Asked> = FxHashMap::default();
    let mut raised = Vec::new();
    pending.retain_mut(|(entity, each, left)| {
        let query = each.query.id.get();
        let Some(holder) = holders.get_mut(&entity.0) else {
            return false;
        };
        let answer = *asked.entry((entity.0, query)).or_insert_with(|| {
            holds(*entity, query).map(|now| (holder.answers.insert(query, now), now))
        });
        *left -= 1;
        let Some((before, now)) = answer else {
            return *left > 0;
        };
        let was = before.unwrap_or(now);
        raised.extend(Raise::of(each, *entity, was, now, 0, 0));
        *left > 0 && was == now
    });
    raised
}

/// Forgets the index and every holder of a hosted game that ended.
pub(crate) fn forget() {
    STATE.index.store(None);
    *STATE.held.relock() = Held::default();
    STATE.asking.store(false, Ordering::Relaxed);
    for word in &STATE.seen {
        word.store(0, Ordering::Relaxed);
    }
}

/// Follows `entity` from value 0 on an index of one counted step under `id`,
/// for the tests of the store's writes.
#[cfg(test)]
pub(crate) fn follow_for_test(entity: Entity, id: u32) {
    forget();
    let mut index = Index::default();
    index.0.insert(
        id,
        vec![Use {
            quest: 0x51,
            step: 0,
            role: Role::Done,
            query: KnowledgeQuery {
                id: KnowledgeId::new(id),
                scope: Scope::Player,
                test: KnowledgeTest::AtLeast(3),
            },
        }],
    );
    STATE.index.store(Some(Arc::new(index)));
    STATE
        .held
        .relock()
        .holders
        .insert(entity.0, Holder::default());
    STATE.mark(entity, true);
}

/// The value of `id` the module holds for `entity`.
#[cfg(test)]
pub(crate) fn held_for_test(entity: Entity, id: u32) -> Option<u32> {
    STATE
        .held
        .relock()
        .holders
        .get(&entity.0)
        .and_then(|holder| holder.values.get(&id).copied())
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

    fn game_query_step(quest: u32, scope: Scope) -> Use {
        Use {
            quest,
            step: 0,
            role: Role::Done,
            query: KnowledgeQuery {
                scope,
                ..query(0x900, KnowledgeTest::Holds)
            },
        }
    }

    #[test]
    fn every_step_a_game_query_decides_raises_when_its_answer_comes_to_hold() {
        let (a, b) = (
            game_query_step(0x51, Scope::Player),
            game_query_step(0x52, Scope::Player),
        );
        let mut holders = FxHashMap::default();
        holders.insert(2, Holder::default());
        holders.get_mut(&2).unwrap().answers.insert(0x900, false);
        let mut pending = vec![(Entity(2), a, EXTERN_STEPS), (Entity(2), b, EXTERN_STEPS)];
        let mut asked = 0;
        let raised = ask(&mut holders, &mut pending, |_, _| {
            asked += 1;
            Some(true)
        });
        let quests: Vec<u32> = raised
            .iter()
            .filter_map(|raise| match raise {
                Raise::Done(data) => Some(data.quest.get()),
                _ => None,
            })
            .collect();
        assert_eq!(quests, [0x51, 0x52], "the second step shares the query");
        assert_eq!(asked, 1, "one question per holder and query");
        assert!(pending.is_empty(), "both answers moved");
    }

    #[test]
    fn an_answer_that_never_read_before_raises_nothing_and_waits_while_unanswered() {
        let each = game_query_step(0x51, Scope::Player);
        let mut holders = FxHashMap::default();
        holders.insert(2, Holder::default());
        let mut pending = vec![(Entity(2), each, EXTERN_STEPS)];
        assert!(ask(&mut holders, &mut pending, |_, _| None).is_empty());
        assert_eq!(pending[0].2, EXTERN_STEPS - 1, "unanswered, asked again");
        assert!(
            ask(&mut holders, &mut pending, |_, _| Some(true)).is_empty(),
            "the first answer is the baseline"
        );
        assert_eq!(holders[&2].answers.get(&0x900), Some(&true));
    }

    #[test]
    fn a_change_asks_again_from_the_start_and_a_world_change_asks_each_player() {
        forget();
        let player = game_query_step(0x51, Scope::Player);
        let world = game_query_step(0x52, Scope::World);
        {
            let mut state = STATE.held.relock();
            for holder in [0, 2, 3] {
                state.holders.insert(holder, Holder::default());
            }
            state.pending.push((Entity(2), player, 1));
        }
        assert!(moved(&[player], Entity(2), 7, 1).is_empty());
        let mut pending = STATE.held.relock().pending.clone();
        assert_eq!(
            pending,
            [(Entity(2), player, EXTERN_STEPS)],
            "counted again"
        );
        assert!(moved(&[player, world], Entity(0), 7, 1).is_empty());
        pending = STATE.held.relock().pending.clone();
        pending.sort_by_key(|(entity, each, _)| (entity.0, each.quest));
        assert_eq!(
            pending,
            [
                (Entity(0), world, EXTERN_STEPS),
                (Entity(2), player, EXTERN_STEPS),
                (Entity(3), player, EXTERN_STEPS),
            ],
            "a player's query is each player's, the world's the world's"
        );
        assert!(STATE.asking.load(Ordering::Relaxed));
        forget();
    }

    #[test]
    fn a_holder_is_seen_from_its_mark_until_it_goes() {
        forget();
        assert!(!STATE.seen(Entity(4096)));
        STATE.mark(Entity(4096), true);
        STATE.mark(Entity(0), true);
        assert!(STATE.seen(Entity(4096)) && STATE.seen(Entity(0)));
        assert!(!STATE.seen(Entity(64)));
        gone(Entity(4096));
        assert!(!STATE.seen(Entity(4096)) && STATE.seen(Entity(0)));
        assert!(!STATE.seen(Entity(u32::MAX)), "past every record");
        forget();
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
