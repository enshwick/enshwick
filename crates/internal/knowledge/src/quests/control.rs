//! Moving a quest on for a player or the world: the grants that make its
//! steps' queries hold, and the revokes that take them back.

use crate::content::knowledge::{KnowledgeQuery, KnowledgeTest};
use crate::knowledge::{Grant, KnowledgeId, KnowledgeType, Known, Revoke, Scope};
use crate::{Entity, Host, WorldError};
use enshwick_world::__internal::player_entity;
use enshwick_world::world::PlayerRef;

use super::{QuestInfo, Quests, StepInfo};

/// Whose progress in a quest a call reads or moves: the world's, which every
/// player shares, or one player's.
///
/// The name is enshwick's, not the game's.
///
/// # Examples
///
/// ```
/// use enshwick::quests::{Holder, Quests};
///
/// fn begin(quests: &Quests, player: &enshwick::world::Player, quest: enshwick::knowledge::KnowledgeId) {
///     let _started = quests.start(Holder::player(player), quest);
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum Holder {
    /// The world's knowledge.
    World,
    /// The knowledge of this player, who is connected: a call for a player
    /// it cannot find fails as for a quest it cannot move.
    Player(PlayerRef),
}

impl Holder {
    /// The holder for one player's knowledge.
    ///
    /// # Examples
    ///
    /// See [`Holder`].
    #[must_use]
    pub fn player(player: impl Into<PlayerRef>) -> Self {
        Self::Player(player.into())
    }

    /// The holder with its player's character found, which every
    /// knowledge call takes.
    pub(super) fn resolved(self, host: Host) -> Result<Self, WorldError> {
        match self {
            Self::World => Ok(self),
            Self::Player(player) => Ok(Self::player(player_entity(host, player)?)),
        }
    }

    /// The player's character once [`resolved`](Self::resolved); `None`
    /// for the world.
    pub(super) const fn character(self) -> Option<Entity> {
        match self {
            Self::World => None,
            Self::Player(player) => player.entity(),
        }
    }

    /// The entity the knowledge calls take: the character, or none for the
    /// world.
    pub(super) const fn entity(self) -> Entity {
        match self.character() {
            Some(character) => character,
            None => Entity::NONE,
        }
    }

    /// The scope of the entries this holder owns.
    const fn scope(self) -> Scope {
        match self {
            Self::World => Scope::World,
            Self::Player(_) => Scope::Player,
        }
    }
}

/// Why a quest did not move as asked.
///
/// # Examples
///
/// ```
/// use enshwick::quests::{Holder, MoveQuestError, Quests};
///
/// fn finish(quests: &Quests, quest: enshwick::knowledge::KnowledgeId) -> bool {
///     match quests.complete(Holder::World, quest) {
///         Ok(()) => true,
///         Err(MoveQuestError::Unwritable { step, .. }) => {
///             enshwick::warn!("step {step} moves only through its game query");
///             false
///         }
///         Err(_) => false,
///     }
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error, crate::script::ErrorKind)]
#[non_exhaustive]
pub enum MoveQuestError {
    /// No quest of the journal has this id, or the journal has not read yet,
    /// as during init.
    #[error("no quest {0} in the journal")]
    NoQuest(KnowledgeId),
    /// The step asks for knowledge no grant makes hold: a game query, whose
    /// state the game computes from its parts, a count below or exactly at a
    /// value the entry is past, or a form enshwick does not read.
    #[error("quest step {step}: knowledge query {query:?} holds through no grant")]
    Unwritable {
        /// The step, from 0.
        step: u32,
        /// Its query.
        query: KnowledgeQuery,
    },
    /// The knowledge call failed, as for a player not in the game or a call
    /// off the game thread.
    #[error(transparent)]
    World(#[from] WorldError),
}

crate::load::off_from!(MoveQuestError);

/// The grant that makes `test` hold for an entry of `kind` holding `held`:
/// `Ok(None)` when it holds already, `Err(())` when no grant does, since a
/// grant only adds.
fn grant_for(test: KnowledgeTest, kind: KnowledgeType, held: u32) -> Result<Option<Grant>, ()> {
    if enshwick_content::__internal::passes(test, held) == Some(true) {
        return Ok(None);
    }
    let grant = match (test, kind) {
        (KnowledgeTest::IsSet, KnowledgeType::Bool) => Grant::Set,
        (KnowledgeTest::IsSet, KnowledgeType::Counter) => Grant::Add(1),
        (KnowledgeTest::AtLeast(count) | KnowledgeTest::Equals(count), KnowledgeType::Counter)
            if held < count =>
        {
            Grant::Add(count - held)
        }
        (KnowledgeTest::HasFlag(bits), KnowledgeType::Flags) if bits != 0 => {
            Grant::Flag(bits.trailing_zeros())
        }
        _ => return Err(()),
    };
    Ok(Some(grant))
}

/// The value an entry holding `held` holds once `grant` applies.
const fn after(grant: Grant, held: u32) -> u32 {
    match grant {
        Grant::Set => 1,
        Grant::Add(amount) => held.saturating_add(amount),
        Grant::Flag(bit) => held | 1u32.wrapping_shl(bit),
    }
}

/// The grants of one call, each made against the values the ones before
/// it leave, since a grant applies only at the next step of the world: two
/// steps that test one entry grant it once.
struct Moves<'a> {
    quests: &'a Quests,
    holder: Holder,
    /// The entries read so far, with the grants made on them.
    held: Vec<Known>,
}

impl<'a> Moves<'a> {
    const fn new(quests: &'a Quests, holder: Holder) -> Self {
        Self {
            quests,
            holder,
            held: Vec::new(),
        }
    }

    /// Makes `query`, step `step`'s, hold with the one grant it needs;
    /// nothing for no query, or one that holds already.
    fn hold(&mut self, step: u32, query: Option<KnowledgeQuery>) -> Result<(), MoveQuestError> {
        let Some(query) = query else {
            return Ok(());
        };
        let unwritable = MoveQuestError::Unwritable { step, query };
        if matches!(query.test, KnowledgeTest::Holds | KnowledgeTest::Unknown) {
            return Err(unwritable);
        }
        if !self.held.iter().any(|known| known.id == query.id) {
            self.held.push(self.quests.known(self.holder, query.id)?);
        }
        let Some(known) = self.held.iter_mut().find(|known| known.id == query.id) else {
            return Ok(());
        };
        match grant_for(query.test, known.r#type, known.value) {
            Ok(None) => Ok(()),
            Ok(Some(grant)) => {
                self.quests
                    .knowledge
                    .grant(self.holder.entity(), query.id, grant)?;
                known.value = after(grant, known.value);
                Ok(())
            }
            Err(()) => Err(unwritable),
        }
    }

    /// Makes step `at` of `steps` show and be done.
    fn finish(&mut self, steps: &[StepInfo], at: u32) -> Result<(), MoveQuestError> {
        let Some(step) = steps.get(at as usize) else {
            return Ok(());
        };
        self.hold(at, step.shown_when)?;
        self.hold(at, step.done_when)
    }
}

/// The revokes that take an entry of `kind` holding `held` back to 0.
fn revokes_for(kind: KnowledgeType, held: u32) -> Vec<Revoke> {
    match kind {
        _ if held == 0 => Vec::new(),
        KnowledgeType::Bool => vec![Revoke::Clear],
        KnowledgeType::Counter => vec![Revoke::Sub(held)],
        KnowledgeType::Flags => (0..32)
            .filter(|bit| held & (1 << bit) != 0)
            .map(Revoke::Unflag)
            .collect(),
        _ => Vec::new(),
    }
}

/// The step a holder stands at in a quest of `steps` steps whose done
/// states are `done`: the first not done, or `steps` when all are.
fn first_open(done: impl IntoIterator<Item = Option<bool>>) -> u32 {
    let open = done.into_iter().take_while(|&done| done == Some(true));
    u32::try_from(open.count()).unwrap_or(u32::MAX)
}

impl Quests {
    /// The quest `quest` of the journal.
    fn info(&self, quest: KnowledgeId) -> Result<QuestInfo, MoveQuestError> {
        self.quest(quest).ok_or(MoveQuestError::NoQuest(quest))
    }

    /// The holder's entry `id`, as the game would test it.
    pub(super) fn known(&self, holder: Holder, id: KnowledgeId) -> Result<Known, MoveQuestError> {
        let known = match holder {
            Holder::World => self.knowledge.world(id),
            Holder::Player(_) => self.knowledge.get(holder.entity(), id),
        };
        known.ok_or(MoveQuestError::World(WorldError::Refused))
    }

    /// Where `holder` stands in `quest`, from its steps' done states.
    fn at(&self, holder: Holder, quest: KnowledgeId) -> Result<u32, MoveQuestError> {
        let player = match holder {
            Holder::World => None,
            Holder::Player(player) => player.entity(),
        };
        let state = self
            .state_of(player, quest)
            .ok_or(MoveQuestError::NoQuest(quest))?;
        Ok(first_open(state.steps.iter().map(|step| step.done)))
    }
}

#[crate::script(control)]
impl Quests {
    /// The step `holder` stands at in `quest`, from 0: the first step not
    /// done, or the number of steps once every one is; a step whose state
    /// is not known counts as not done. `None` for no quest of the journal.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::quests::{Holder, Quests};
    ///
    /// fn last(quests: &Quests, player: enshwick::Entity, quest: enshwick::knowledge::KnowledgeId) -> bool {
    ///     let steps = quests.quest(quest).map_or(0, |info| info.steps.len());
    ///     quests.step(Holder::player(player), quest) == u32::try_from(steps).ok()
    /// }
    /// ```
    #[must_use]
    #[script]
    pub fn step(&self, holder: Holder, quest: KnowledgeId) -> Option<u32> {
        self.at(holder.resolved(self.knowledge.host).ok()?, quest)
            .ok()
    }

    /// Starts `quest` for `holder`: grants what its first step's
    /// `shown_when` asks, so the journal lists it. Grants go the game's way
    /// and apply at the next step of the world, so the quest reads as before
    /// until then.
    ///
    /// # Errors
    ///
    /// [`MoveQuestError::NoQuest`] for no quest of the journal;
    /// [`MoveQuestError::Unwritable`] for a first step shown on a game query
    /// or on a test no grant makes hold; [`MoveQuestError::World`] as for
    /// [`Knowledge::grant`](crate::knowledge::Knowledge::grant), and
    /// [`WorldError::Refused`] for an entry that does not read for the
    /// holder, such as a player's for the world.
    ///
    /// # Examples
    ///
    /// See [`Holder`].
    #[script]
    pub fn start(&self, holder: Holder, quest: KnowledgeId) -> Result<(), MoveQuestError> {
        let holder = holder.resolved(self.knowledge.host)?;
        let info = self.info(quest)?;
        let first = info.steps.first().and_then(|step| step.shown_when);
        Moves::new(self, holder).hold(0, first)
    }

    /// Finishes the step `holder` stands at in `quest` and shows the next,
    /// granting what each asks; returns the step the holder stands at once
    /// the grants apply at the next step of the world. A quest whose last
    /// step is done stays as it is. Until those grants apply, a second call
    /// that moves the quest for the holder reads the steps as they were
    /// and grants the same step again, so a mod moves a quest once per
    /// holder and step of the world, and finishes it with
    /// [`Quests::complete`].
    ///
    /// # Errors
    ///
    /// As for [`Quests::start`], for the step it finishes or the next.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::quests::{Holder, Quests};
    ///
    /// fn skip(quests: &Quests, player: enshwick::Entity, quest: enshwick::knowledge::KnowledgeId) {
    ///     if let Ok(step) = quests.advance(Holder::player(player), quest) {
    ///         enshwick::info!("now at step {step}");
    ///     }
    /// }
    /// ```
    #[script]
    pub fn advance(&self, holder: Holder, quest: KnowledgeId) -> Result<u32, MoveQuestError> {
        let holder = holder.resolved(self.knowledge.host)?;
        let info = self.info(quest)?;
        let at = self.at(holder, quest)?;
        let Some(next) = at
            .checked_add(1)
            .filter(|_| (at as usize) < info.steps.len())
        else {
            return Ok(at);
        };
        let mut moves = Moves::new(self, holder);
        moves.finish(&info.steps, at)?;
        if let Some(step) = info.steps.get(next as usize) {
            moves.hold(next, step.shown_when)?;
        }
        Ok(next)
    }

    /// Completes `quest` for `holder`: grants what every step from the one
    /// it stands at to the last asks. The game then pays each player for
    /// whom the last step holds and the quest's bool is unset, once, as for
    /// a quest played through; experience, item notices and the journal's
    /// mark are the game's. As with [`Quests::advance`], a call that moves
    /// the quest for the holder in the same step of the world reads the
    /// steps from before this one's grants.
    ///
    /// # Errors
    ///
    /// As for [`Quests::start`], for any step it finishes; the steps before
    /// that one keep their grants.
    ///
    /// # Examples
    ///
    /// See [`MoveQuestError`].
    #[script]
    pub fn complete(&self, holder: Holder, quest: KnowledgeId) -> Result<(), MoveQuestError> {
        let holder = holder.resolved(self.knowledge.host)?;
        let info = self.info(quest)?;
        let steps = u32::try_from(info.steps.len()).unwrap_or(u32::MAX);
        let mut moves = Moves::new(self, holder);
        (self.at(holder, quest)?..steps).try_for_each(|at| moves.finish(&info.steps, at))
    }

    /// Takes `quest` back for `holder`: clears every entry of the holder's
    /// that its steps test (shown, done and counted parts) and, for a
    /// player, the quest's own bool, so it leaves the journal and the game
    /// pays it again when it is completed again. An entry another quest,
    /// recipe or page tests too is cleared for that as well, and what the
    /// grants brought, such as experience, stays; entries of the other
    /// scope, such as the world's for a player, are left alone. A step on a
    /// game query is left as the game computes it.
    ///
    /// # Errors
    ///
    /// [`MoveQuestError::NoQuest`] for no quest of the journal;
    /// [`MoveQuestError::World`] as for
    /// [`Knowledge::revoke`](crate::knowledge::Knowledge::revoke); the
    /// entries before the failing one stay cleared.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::quests::{Holder, Quests};
    ///
    /// fn again(quests: &Quests, player: enshwick::Entity, quest: enshwick::knowledge::KnowledgeId) {
    ///     let _reset = quests.reset(Holder::player(player), quest);
    /// }
    /// ```
    #[script]
    pub fn reset(&self, holder: Holder, quest: KnowledgeId) -> Result<(), MoveQuestError> {
        let holder = holder.resolved(self.knowledge.host)?;
        let info = self.info(quest)?;
        let mut ids: Vec<KnowledgeId> = info
            .steps
            .iter()
            .flat_map(|step| {
                step.shown_when
                    .into_iter()
                    .chain(step.done_when)
                    .chain(step.progress.iter().copied())
            })
            .filter(|query| !matches!(query.test, KnowledgeTest::Holds | KnowledgeTest::Unknown))
            .map(|query| query.id)
            .collect();
        if let Holder::Player(_) = holder {
            ids.push(quest);
        }
        ids.sort_unstable_by_key(|id| id.get());
        ids.dedup();
        for id in ids {
            // An entry that does not read for this holder is the other scope's.
            let Ok(known) = self.known(holder, id) else {
                continue;
            };
            if known.scope != holder.scope() {
                continue;
            }
            for revoke in revokes_for(known.r#type, known.value) {
                self.knowledge.revoke(holder.entity(), id, revoke)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grant_makes_each_writable_test_hold_and_nothing_for_one_that_holds() {
        use KnowledgeTest as T;
        use KnowledgeType as K;
        assert_eq!(grant_for(T::IsSet, K::Bool, 0), Ok(Some(Grant::Set)));
        assert_eq!(grant_for(T::IsSet, K::Bool, 1), Ok(None));
        assert_eq!(grant_for(T::IsSet, K::Counter, 0), Ok(Some(Grant::Add(1))));
        assert_eq!(
            grant_for(T::AtLeast(5), K::Counter, 2),
            Ok(Some(Grant::Add(3)))
        );
        assert_eq!(grant_for(T::AtLeast(5), K::Counter, 7), Ok(None));
        assert_eq!(
            grant_for(T::Equals(4), K::Counter, 1),
            Ok(Some(Grant::Add(3)))
        );
        assert_eq!(
            grant_for(T::HasFlag(0b1100), K::Flags, 1),
            Ok(Some(Grant::Flag(2)))
        );
        assert_eq!(grant_for(T::HasFlag(0b1100), K::Flags, 8), Ok(None));
    }

    #[test]
    fn a_test_no_grant_reaches_is_refused_since_a_grant_only_adds() {
        use KnowledgeTest as T;
        use KnowledgeType as K;
        for (test, kind, held) in [
            (T::Equals(2), K::Counter, 3),
            (T::Equals(0), K::Counter, 1),
            (T::Below(2), K::Counter, 5),
            (T::IsSet, K::Flags, 0),
            (T::HasFlag(0), K::Flags, 0),
            (T::AtLeast(3), K::Bool, 0),
            (T::Holds, K::Bool, 0),
            (T::Unknown, K::Counter, 0),
        ] {
            assert_eq!(
                grant_for(test, kind, held),
                Err(()),
                "{test:?} of {kind:?} at {held}"
            );
        }
        assert_eq!(
            grant_for(T::Below(2), K::Counter, 1),
            Ok(None),
            "holds already"
        );
    }

    #[test]
    fn a_grant_leaves_the_value_the_game_s_apply_makes() {
        assert_eq!(after(Grant::Set, 0), 1);
        assert_eq!(after(Grant::Add(3), 2), 5);
        assert_eq!(after(Grant::Add(3), u32::MAX), u32::MAX);
        assert_eq!(after(Grant::Flag(2), 1), 0b101);
    }

    #[test]
    fn a_reset_takes_each_kind_back_to_zero_and_leaves_an_unset_entry() {
        assert_eq!(revokes_for(KnowledgeType::Bool, 1), [Revoke::Clear]);
        assert_eq!(revokes_for(KnowledgeType::Counter, 7), [Revoke::Sub(7)]);
        assert_eq!(
            revokes_for(KnowledgeType::Flags, 0b1010),
            [Revoke::Unflag(1), Revoke::Unflag(3)]
        );
        assert!(revokes_for(KnowledgeType::Counter, 0).is_empty());
        assert!(revokes_for(KnowledgeType::Other(9), 1).is_empty());
    }

    #[test]
    fn a_holder_stands_at_the_first_step_not_known_done() {
        assert_eq!(first_open([Some(true), Some(false), Some(true)]), 1);
        assert_eq!(
            first_open([Some(true), None]),
            1,
            "unknown counts as not done"
        );
        assert_eq!(first_open([Some(true), Some(true)]), 2);
        assert_eq!(first_open([]), 0);
    }
}
