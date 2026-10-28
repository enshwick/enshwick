//! Knowledge in the server's world.
//!
//! The values the game keeps for the world and for each player, which recipes,
//! quests, journal pages, skill points and much more read; reading them,
//! granting them, and the quests.
//!
//! Every knowledge id is either the world's, shared by every player, or
//! each player's own, and is a bool, a counter or flags. A recipe's id is a
//! player's flags knowledge (seen bit 0, crafted bit 1); a quest's id is a
//! player's bool the game sets when it rewards the quest. A grant goes the
//! game's way: the next step applies it with what follows, such as
//! experience, unlocks chained on it, the clients and the save. A revoke
//! takes a value back at once and sends it to the clients again; what the
//! grant brought, such as experience, stays. A player's knowledge lives in
//! the character's save on the player's machine, so a change for an account
//! that is not in the game waits with the world until its character loads
//! there.
//! [`KnowledgeChanged`](crate::events::KnowledgeChanged) and
//! [`QuestCompleted`](crate::events::QuestCompleted) report what each step
//! changed.
//!
//! A vanilla server grants whatever knowledge a client asks for in its
//! input, so a modified client could grant itself any of it. The loader's
//! knowledge guard (`knowledge_guard` in the loader's settings) knows what
//! a vanilla client asks for: a player's bools and the seen bit (0) of a
//! player's flags, and never an id that gives experience or skill points or
//! a quest. By default it watches, logging the rest and letting it
//! through; enforcing, it refuses the rest. A mod widens or narrows it with
//! [`Load::on_knowledge_input`].
//!
//! Every call belongs on the server's game thread, from a game event, a
//! [`Scheduler`](crate::schedule::Scheduler) task or an action; on a client,
//! while it hosts a game.
//!
//! # Examples
//!
//! ```
//! use enshwick::actions::{Action, Arg, Outcome, Value};
//! use enshwick::knowledge::{Grant, KnowledgeId};
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let knowledge = load.knowledge();
//!     let actions = load.actions(enshwick::i18n::Texts::default())?;
//!     let learn = Action::new("learn", "Learn a recipe")
//!         .admins()
//!         .arg(Arg::number("Recipe id", 0.0..=f64::from(u32::MAX), 1.0));
//!     actions.add(learn, move |request| {
//!         let [Value::Number(recipe)] = request.args else {
//!             return Outcome::refused("Name a recipe");
//!         };
//!         Outcome::done(format!("{:?}", knowledge.grant(request.player.entity, KnowledgeId::new(*recipe as u32), Grant::Flag(0))))
//!     })
//! }
//! ```

use core::ffi::c_void;

use crate::WorldError;
pub use crate::abi::KnowledgeId;
use crate::abi::{Header, KnowledgeInfo, KnowledgeInput, Status};
use crate::{Entity, PlayerKey};
use crate::{Host, Load, Off};

/// What a knowledge value is.
///
/// # Examples
///
/// See [`Known`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum KnowledgeType {
    /// Set or not.
    Bool,
    /// A count.
    Counter,
    /// Bits.
    Flags,
    /// A kind this API does not name.
    Other(u8),
}

/// Whose a knowledge value is.
///
/// # Examples
///
/// See [`Known`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Scope {
    /// The world's, shared by every player.
    World,
    /// Each player's own.
    Player,
}

/// A knowledge value, from [`Knowledge::get`] or [`Knowledge::world`].
///
/// # Examples
///
/// ```
/// use enshwick::knowledge::KnowledgeType;
///
/// fn crafted(knowledge: &enshwick::knowledge::Knowledge, player: enshwick::Entity, recipe: enshwick::RecipeId) -> bool {
///     knowledge
///         .get(player, recipe.into())
///         .is_some_and(|known| known.r#type == KnowledgeType::Flags && known.has_flag(1))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Known {
    /// The knowledge id.
    pub id: KnowledgeId,
    /// What it is.
    pub r#type: KnowledgeType,
    /// Whose it is.
    pub scope: Scope,
    /// The value: 0 or 1, a count, or bits.
    pub value: u32,
    /// The bits the game expects of a flags value; 0 for any.
    pub expected_flags: u32,
}

impl Known {
    /// Whether the value is anything but 0: a bool set, a counter above 0,
    /// any bit.
    ///
    /// # Examples
    ///
    /// See [`Knowledge::completed`].
    #[must_use]
    pub const fn is_set(&self) -> bool {
        self.value != 0
    }

    /// Whether bit `bit` of a flags value is set.
    ///
    /// # Examples
    ///
    /// See [`Known`].
    #[must_use]
    pub const fn has_flag(&self, bit: u32) -> bool {
        bit < 32 && self.value & (1 << bit) != 0
    }

    fn from_info(info: &KnowledgeInfo) -> Self {
        Self {
            id: info.id,
            r#type: kind_of(info.kind),
            scope: if info.scope == KnowledgeInfo::WORLD {
                Scope::World
            } else {
                Scope::Player
            },
            value: info.value,
            expected_flags: info.flags,
        }
    }
}

/// What a grant does, by the kind of the knowledge.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Grant {
    /// Sets a bool.
    Set,
    /// Adds to a counter.
    Add(u32),
    /// Sets one bit of flags.
    Flag(u32),
}

/// What a revoke takes back, by the kind of the knowledge.
///
/// # Examples
///
/// See [`Knowledge::revoke`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Revoke {
    /// Clears a bool.
    Clear,
    /// Takes from a counter, down to 0.
    Sub(u32),
    /// Clears one bit of flags.
    Unflag(u32),
}

impl Revoke {
    const fn raw(self) -> (u32, u32) {
        match self {
            Self::Clear => (KnowledgeInfo::BOOL as u32, 0),
            Self::Sub(amount) => (KnowledgeInfo::COUNTER as u32, amount),
            Self::Unflag(bit) => (KnowledgeInfo::FLAGS as u32, bit),
        }
    }
}

impl Grant {
    const fn raw(self) -> (u32, u32) {
        match self {
            Self::Set => (KnowledgeInfo::BOOL as u32, 0),
            Self::Add(amount) => (KnowledgeInfo::COUNTER as u32, amount),
            Self::Flag(bit) => (KnowledgeInfo::FLAGS as u32, bit),
        }
    }
}

/// The knowledge of the server's world; get it from [`Load::knowledge`].
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug)]
pub struct Knowledge {
    host: Host,
}

impl Knowledge {
    fn read(&self, entity: Entity, id: KnowledgeId) -> Option<Known> {
        let mut info = KnowledgeInfo {
            header: Header::of::<KnowledgeInfo>(),
            id: KnowledgeId::default(),
            kind: 0,
            scope: 0,
            reserved: 0,
            value: 0,
            flags: 0,
        };
        let status = (self.host.api().knowledge)(self.host.handle(), entity, id, &raw mut info);
        (status == Status::OK).then(|| Known::from_info(&info))
    }

    /// Knowledge `id` of the player whose character is `player`; for an id
    /// of the world's, the world's value. `None` for an id the game does
    /// not know, a player not in the game, or off the game thread.
    ///
    /// # Examples
    ///
    /// See [`Known`].
    #[must_use]
    pub fn get(&self, player: Entity, id: KnowledgeId) -> Option<Known> {
        self.read(player, id)
    }

    /// Knowledge `id` of the world; `None` for an id the game does not
    /// know, a player's id, or off the game thread.
    ///
    /// # Examples
    ///
    /// ```
    /// fn found(knowledge: &enshwick::knowledge::Knowledge, place: enshwick::knowledge::KnowledgeId) -> bool {
    ///     knowledge.world(place).is_some_and(|known| known.is_set())
    /// }
    /// ```
    #[must_use]
    pub fn world(&self, id: KnowledgeId) -> Option<Known> {
        self.read(Entity::NONE, id)
    }

    /// Grants knowledge `id` to the player whose character is `player` (or
    /// to the world, for an id of the world's), as the game grants it: the
    /// next step applies it with what follows. A grant only adds: a bool
    /// set, a counter raised, a bit set.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for an id the game does not know, a grant
    /// that does not fit the id's kind,
    /// a bit the game does not expect, a player not in the game, or a step
    /// with too many grants; [`WorldError::WrongThread`] off the game
    /// thread; [`WorldError::Missing`] on a build without knowledge.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn grant(&self, player: Entity, id: KnowledgeId, grant: Grant) -> Result<(), WorldError> {
        let (kind, value) = grant.raw();
        WorldError::from_status((self.host.api().grant_knowledge)(
            self.host.handle(),
            player,
            id,
            kind,
            value,
        ))
    }

    /// Grants knowledge `id` of the world's, as [`Knowledge::grant`] does.
    ///
    /// # Errors
    ///
    /// As for [`Knowledge::grant`]; a player's id is refused.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::knowledge::Grant;
    ///
    /// fn reveal(knowledge: &enshwick::knowledge::Knowledge, place: enshwick::knowledge::KnowledgeId) {
    ///     let _granted = knowledge.grant_world(place, Grant::Set);
    /// }
    /// ```
    pub fn grant_world(&self, id: KnowledgeId, grant: Grant) -> Result<(), WorldError> {
        self.grant(Entity(0), id, grant)
    }

    /// Takes knowledge `id` back from the player whose character is
    /// `player` (or from the world, for an id of the world's): a bool
    /// cleared, a counter lowered, a bit cleared. It applies at once, the
    /// clients get it again, and the next save keeps it; what the grant
    /// brought, such as experience or unlocks chained on it, stays, and a
    /// quest whose steps still hold is rewarded again.
    /// [`KnowledgeChanged`](crate::events::KnowledgeChanged) reports it
    /// after the next step.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for an id the game does not know, a revoke
    /// that does not fit the id's kind, a bit past 31, or a player not in
    /// the game or whose save has not loaded yet;
    /// [`WorldError::WrongThread`] off the game thread;
    /// [`WorldError::Missing`] on a build without it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::knowledge::Revoke;
    ///
    /// fn unlearn(knowledge: &enshwick::knowledge::Knowledge, player: enshwick::Entity, recipe: enshwick::RecipeId) {
    ///     let _taken = knowledge.revoke(player, recipe.into(), Revoke::Unflag(1));
    /// }
    /// ```
    pub fn revoke(
        &self,
        player: Entity,
        id: KnowledgeId,
        revoke: Revoke,
    ) -> Result<(), WorldError> {
        let (kind, value) = revoke.raw();
        WorldError::from_status((self.host.api().revoke_knowledge)(
            self.host.handle(),
            player,
            id,
            kind,
            value,
        ))
    }

    /// Takes knowledge `id` of the world's back, as [`Knowledge::revoke`]
    /// does.
    ///
    /// # Errors
    ///
    /// As for [`Knowledge::revoke`]; a player's id is refused.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::knowledge::Revoke;
    ///
    /// fn hide(knowledge: &enshwick::knowledge::Knowledge, place: enshwick::knowledge::KnowledgeId) {
    ///     let _taken = knowledge.revoke_world(place, Revoke::Clear);
    /// }
    /// ```
    pub fn revoke_world(&self, id: KnowledgeId, revoke: Revoke) -> Result<(), WorldError> {
        self.revoke(Entity(0), id, revoke)
    }

    /// Grants knowledge `id` of a player's to the account whose
    /// [`Player::key`](crate::world::Player::key) is `account`, as
    /// [`Knowledge::grant`] does: now when its character is in the game,
    /// else once its next character has loaded in this world. The loader
    /// keeps waiting changes with the world, so they outlast restarts.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for an id the game does not know, an id of
    /// the world's or a grant that does not fit the id's kind, and as for
    /// [`Knowledge::grant`] when the character is in the game;
    /// [`WorldError::WrongThread`] off the game thread;
    /// [`WorldError::Missing`] while no world runs.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::knowledge::Grant;
    ///
    /// fn reward(knowledge: &enshwick::knowledge::Knowledge, account: enshwick::PlayerKey, recipe: enshwick::RecipeId) {
    ///     let _queued = knowledge.grant_account(account, recipe.into(), Grant::Flag(0));
    /// }
    /// ```
    pub fn grant_account(
        &self,
        account: PlayerKey,
        id: KnowledgeId,
        grant: Grant,
    ) -> Result<(), WorldError> {
        let (kind, value) = grant.raw();
        self.for_account(account, id, kind, value, false)
    }

    /// Takes knowledge `id` of a player's back from the account whose
    /// [`Player::key`](crate::world::Player::key) is `account`, as
    /// [`Knowledge::revoke`] does, now or once its next character has
    /// loaded in this world, as [`Knowledge::grant_account`] waits.
    ///
    /// # Errors
    ///
    /// As for [`Knowledge::grant_account`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::knowledge::Revoke;
    ///
    /// fn punish(knowledge: &enshwick::knowledge::Knowledge, account: enshwick::PlayerKey, recipe: enshwick::RecipeId) {
    ///     let _queued = knowledge.revoke_account(account, recipe.into(), Revoke::Unflag(1));
    /// }
    /// ```
    pub fn revoke_account(
        &self,
        account: PlayerKey,
        id: KnowledgeId,
        revoke: Revoke,
    ) -> Result<(), WorldError> {
        let (kind, value) = revoke.raw();
        self.for_account(account, id, kind, value, true)
    }

    fn for_account(
        &self,
        account: PlayerKey,
        id: KnowledgeId,
        kind: u32,
        value: u32,
        revoke: bool,
    ) -> Result<(), WorldError> {
        WorldError::from_status((self.host.api().account_knowledge)(
            self.host.handle(),
            account,
            id,
            kind,
            value,
            u32::from(revoke),
        ))
    }

    /// Every quest id of the running game; empty off the game thread or
    /// while no world runs.
    ///
    /// # Examples
    ///
    /// ```
    /// fn done(knowledge: &enshwick::knowledge::Knowledge, player: enshwick::Entity) -> usize {
    ///     knowledge.quests().into_iter().filter(|&quest| knowledge.completed(player, quest)).count()
    /// }
    /// ```
    #[must_use]
    pub fn quests(&self) -> Vec<KnowledgeId> {
        let mut ids = vec![KnowledgeId::default(); 256];
        loop {
            let mut count = 0u32;
            let capacity = u32::try_from(ids.len()).unwrap_or(u32::MAX);
            let status = (self.host.api().quests)(
                self.host.handle(),
                ids.as_mut_ptr(),
                capacity,
                &raw mut count,
            );
            if status != Status::OK {
                return Vec::new();
            }
            let count = count as usize;
            if count <= ids.len() {
                ids.truncate(count);
                return ids;
            }
            ids = vec![KnowledgeId::default(); count];
        }
    }

    /// Whether the player whose character is `player` completed quest
    /// `quest` and got its rewards.
    ///
    /// # Examples
    ///
    /// See [`Knowledge::quests`].
    #[must_use]
    pub fn completed(&self, player: Entity, quest: KnowledgeId) -> bool {
        self.get(player, quest).is_some_and(|known| known.is_set())
    }
}

/// Why the loader's knowledge guard refuses a client's request.
///
/// # Examples
///
/// See [`Load::on_knowledge_input`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Refusal {
    /// An id the game does not know.
    Unknown,
    /// A kind that is not the id's.
    Kind,
    /// An id of the world's, which every player shares.
    World,
    /// A counter.
    Counter,
    /// A flag bit other than the seen bit, 0.
    FlagBit,
    /// An id that gives experience.
    Experience,
    /// An id that gives skill points.
    SkillPoints,
    /// A quest's id.
    Quest,
    /// A reason this API does not name.
    Other(u32),
}

impl Refusal {
    const fn from_raw(raw: u32) -> Option<Self> {
        Some(match raw {
            KnowledgeInput::ALLOWED => return None,
            KnowledgeInput::UNKNOWN => Self::Unknown,
            KnowledgeInput::KIND => Self::Kind,
            KnowledgeInput::WORLD => Self::World,
            KnowledgeInput::COUNTER => Self::Counter,
            KnowledgeInput::FLAG_BIT => Self::FlagBit,
            KnowledgeInput::EXPERIENCE => Self::Experience,
            KnowledgeInput::SKILL_POINTS => Self::SkillPoints,
            KnowledgeInput::QUEST => Self::Quest,
            other => Self::Other(other),
        })
    }
}

/// Knowledge a client asks the server for in its input, from
/// [`Load::on_knowledge_input`].
///
/// # Examples
///
/// See [`Load::on_knowledge_input`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct KnowledgeRequest {
    /// The asking player's character.
    pub player: Entity,
    /// The knowledge id.
    pub id: KnowledgeId,
    /// The kind the client names.
    pub kind: KnowledgeType,
    /// Whose the id is; `None` for an id the game does not know.
    pub scope: Option<Scope>,
    /// Ignored for a bool, added for a counter, the bit to set for flags.
    pub value: u32,
    /// Why the loader's guard would refuse it, in any mode.
    pub refusal: Option<Refusal>,
    /// Whether it goes through so far: after the guard, which refuses only
    /// when enforcing, and the filters that ran before.
    pub allowed: bool,
}

unsafe extern "system" fn run_input<F>(call: *const KnowledgeInput, user: *mut c_void) -> u8
where
    F: Fn(&KnowledgeRequest) -> bool,
{
    // SAFETY: the loader passes a valid call for the duration of this one.
    let Some(call) = (unsafe { call.as_ref() }) else {
        return 0;
    };
    let allowed = call.allowed != 0;
    let request = KnowledgeRequest {
        player: call.player,
        id: call.id,
        kind: kind_of(call.kind),
        scope: match call.scope {
            KnowledgeInfo::WORLD => Some(Scope::World),
            KnowledgeInfo::PLAYER => Some(Scope::Player),
            _ => None,
        },
        value: call.value,
        refusal: Refusal::from_raw(call.refusal),
        allowed,
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let filter = unsafe { &*user.cast::<F>() };
    u8::from(crate::host::guard(
        "panic in a knowledge input filter",
        || allowed,
        || filter(&request),
    ))
}

const fn kind_of(raw: u8) -> KnowledgeType {
    match raw {
        KnowledgeInfo::BOOL => KnowledgeType::Bool,
        KnowledgeInfo::COUNTER => KnowledgeType::Counter,
        KnowledgeInfo::FLAGS => KnowledgeType::Flags,
        other => KnowledgeType::Other(other),
    }
}

impl Load<'_> {
    /// Runs `filter` on the knowledge each client asks the server for in
    /// its input, after the loader's guard; its answer, `true` to let the
    /// request through, replaces the verdict so far, so a mod can refuse
    /// more than the guard or allow what it refuses. A filter's refusal
    /// stands whatever the guard's mode; a guard that only watches lets
    /// through what it would refuse, so `filter` sees it allowed, with the
    /// reason in [`KnowledgeRequest::refusal`]. A refused request changes
    /// nothing and is not asked again. It runs on the server's worker
    /// threads, rarely: only when a client asks.
    ///
    /// # Errors
    ///
    /// [`Off`] after init or on a build without the input site.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::knowledge::Refusal;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     const TUTORIAL_COUNTER: enshwick::knowledge::KnowledgeId = enshwick::knowledge::KnowledgeId::new(0x51);
    ///     load.on_knowledge_input(|request| {
    ///         request.allowed
    ///             || (request.id == TUTORIAL_COUNTER && request.refusal == Some(Refusal::Counter))
    ///     })
    /// }
    /// ```
    pub fn on_knowledge_input<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&KnowledgeRequest) -> bool + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::hand_over(
            filter,
            |user| (host.api().knowledge_input_filter)(host.handle(), run_input::<F>, user),
            "knowledge input filter",
        )
    }

    /// The [`Knowledge`] of the server's world; on a client, the built-in
    /// server's while it hosts a game.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn knowledge(&mut self) -> Knowledge {
        Knowledge { host: self.host() }
    }
}
