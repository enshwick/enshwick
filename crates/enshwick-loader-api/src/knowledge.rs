//! Knowledge in the server's world: the values the game keeps for the world
//! and for each player, which recipes, quests, journal pages, skill points
//! and much more read; reading them, granting them, and the quests.
//!
//! Every knowledge id is either the world's, shared by every player, or
//! each player's own, and is a bool, a counter or flags. A recipe's id is a
//! player's flags knowledge (seen bit 0, crafted bit 1); a quest's id is a
//! player's bool the game sets when it rewards the quest. A grant goes the
//! game's way: the next step applies it with what follows, such as
//! experience, unlocks chained on it, the clients and the save.
//! [`KnowledgeChanged`](crate::events::KnowledgeChanged) and
//! [`QuestCompleted`](crate::events::QuestCompleted) report what each step
//! changed.
//!
//! A vanilla server grants whatever knowledge a client asks for in its
//! input, so a modified client could grant itself any of it. The loader's
//! knowledge guard, on by default (`knowledge_guard` in the loader's
//! settings), lets through only what a vanilla client asks for: a player's
//! bools and the seen bit (0) of a player's flags, and never an id that
//! gives experience or skill points or a quest. A mod widens or narrows it
//! with [`Load::on_knowledge_input`].
//!
//! Every call belongs on the server's game thread, from a game event, a
//! [`Scheduler`](crate::schedule::Scheduler) task or a command; on a client,
//! while it hosts a game.
//!
//! # Examples
//!
//! ```
//! use enshwick_loader_api::knowledge::Grant;
//!
//! fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
//!     let knowledge = load.knowledge()?;
//!     load.add_command("learn", "learns a recipe: /learn <recipe id>", move |ctx, args| {
//!         let Some(recipe) = args.first().and_then(|a| a.parse().ok()) else {
//!             return "/learn <recipe id>".into();
//!         };
//!         format!("{:?}", knowledge.grant(ctx.player.entity, recipe, Grant::Flag(0))).into()
//!     })
//! }
//! ```

use core::ffi::c_void;

use crate::abi::{Header, HostApi, KnowledgeInfo, KnowledgeInput, Status};
use crate::ecs::Entity;
use crate::world::WorldError;
use crate::{Host, Load, Off};

/// What a knowledge value is.
///
/// # Examples
///
/// See [`Known`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum KnowledgeKind {
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
/// use enshwick_loader_api::knowledge::KnowledgeKind;
///
/// fn crafted(knowledge: &enshwick_loader_api::knowledge::Knowledge, player: enshwick_loader_api::ecs::Entity, recipe: u32) -> bool {
///     knowledge
///         .get(player, recipe)
///         .is_some_and(|known| known.kind == KnowledgeKind::Flags && known.has_flag(1))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Known {
    /// The knowledge id.
    pub id: u32,
    /// What it is.
    pub kind: KnowledgeKind,
    /// Whose it is.
    pub scope: Scope,
    /// The value: 0 or 1, a count, or bits.
    pub value: u32,
    /// The bits the game expects of a flags value; 0 for any.
    pub expected: u32,
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
            kind: kind_of(info.kind),
            scope: if info.scope == KnowledgeInfo::WORLD {
                Scope::World
            } else {
                Scope::Player
            },
            value: info.value,
            expected: info.flags,
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

fn result(status: Status) -> Result<(), WorldError> {
    match status {
        Status::OK => Ok(()),
        Status::WRONG_THREAD => Err(WorldError::WrongThread),
        Status::REFUSED | Status::BAD_ARGS => Err(WorldError::Refused),
        _ => Err(WorldError::Missing),
    }
}

impl Knowledge {
    fn read(&self, entity: u32, id: u32) -> Option<Known> {
        let mut info = KnowledgeInfo {
            header: Header::of::<KnowledgeInfo>(),
            id: 0,
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
    pub fn get(&self, player: Entity, id: u32) -> Option<Known> {
        self.read(player.0, id)
    }

    /// Knowledge `id` of the world; `None` for an id the game does not
    /// know, a player's id, or off the game thread.
    ///
    /// # Examples
    ///
    /// ```
    /// fn found(knowledge: &enshwick_loader_api::knowledge::Knowledge, place: u32) -> bool {
    ///     knowledge.world(place).is_some_and(|known| known.is_set())
    /// }
    /// ```
    #[must_use]
    pub fn world(&self, id: u32) -> Option<Known> {
        self.read(0, id)
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
    pub fn grant(&self, player: Entity, id: u32, grant: Grant) -> Result<(), WorldError> {
        let (kind, value) = grant.raw();
        result((self.host.api().grant_knowledge)(
            self.host.handle(),
            player.0,
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
    /// use enshwick_loader_api::knowledge::Grant;
    ///
    /// fn reveal(knowledge: &enshwick_loader_api::knowledge::Knowledge, place: u32) {
    ///     let _granted = knowledge.grant_world(place, Grant::Set);
    /// }
    /// ```
    pub fn grant_world(&self, id: u32, grant: Grant) -> Result<(), WorldError> {
        self.grant(Entity(0), id, grant)
    }

    /// Every quest id of the running game; empty off the game thread or
    /// while no world runs.
    ///
    /// # Examples
    ///
    /// ```
    /// fn done(knowledge: &enshwick_loader_api::knowledge::Knowledge, player: enshwick_loader_api::ecs::Entity) -> usize {
    ///     knowledge.quests().into_iter().filter(|&quest| knowledge.completed(player, quest)).count()
    /// }
    /// ```
    #[must_use]
    pub fn quests(&self) -> Vec<u32> {
        let mut ids = vec![0u32; 256];
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
            ids = vec![0; count];
        }
    }

    /// Whether the player whose character is `player` completed quest
    /// `quest` and got its rewards.
    ///
    /// # Examples
    ///
    /// See [`Knowledge::quests`].
    #[must_use]
    pub fn completed(&self, player: Entity, quest: u32) -> bool {
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
    pub id: u32,
    /// The kind the client names.
    pub kind: KnowledgeKind,
    /// Whose the id is; `None` for an id the game does not know.
    pub scope: Option<Scope>,
    /// Ignored for a bool, added for a counter, the bit to set for flags.
    pub value: u32,
    /// Why the loader's guard refuses it, whether the guard is on or not.
    pub refusal: Option<Refusal>,
    /// Whether it goes through so far: after the guard, when it is on, and
    /// the filters that ran before.
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
        player: Entity(call.player),
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

const fn kind_of(raw: u8) -> KnowledgeKind {
    match raw {
        KnowledgeInfo::BOOL => KnowledgeKind::Bool,
        KnowledgeInfo::COUNTER => KnowledgeKind::Counter,
        KnowledgeInfo::FLAGS => KnowledgeKind::Flags,
        other => KnowledgeKind::Other(other),
    }
}

impl Load<'_> {
    /// Runs `filter` on the knowledge each client asks the server for in
    /// its input, after the loader's guard; its answer, `true` to let the
    /// request through, replaces the verdict so far, so a mod can refuse
    /// more than the guard or allow what it refuses. A refused request
    /// changes nothing and is not asked again. It runs on the server's
    /// worker threads, rarely: only when a client asks.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, on a build without the input site, or under a
    /// loader older than ABI minor 55.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::knowledge::Refusal;
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     const TUTORIAL_COUNTER: u32 = 0x51;
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
        crate::load::offered(
            host.api(),
            core::mem::offset_of!(HostApi, knowledge_input_filter),
            "knowledge input filters",
        )?;
        crate::load::hand_over(
            filter,
            |user| (host.api().knowledge_input_filter)(host.handle(), run_input::<F>, user),
            "knowledge input filter",
        )
    }

    /// The [`Knowledge`] of the server's world; on a client, the built-in
    /// server's while it hosts a game.
    ///
    /// # Errors
    ///
    /// [`Off`] under a loader older than ABI minor 32.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn knowledge(&mut self) -> Result<Knowledge, Off> {
        let host = self.host();
        crate::load::offered(
            host.api(),
            core::mem::offset_of!(HostApi, quests),
            "knowledge",
        )?;
        Ok(Knowledge { host })
    }
}
