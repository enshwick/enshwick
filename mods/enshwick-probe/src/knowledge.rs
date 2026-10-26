//! Knowledge: the quests of the running game, a world counter granted and
//! read back the next step, the refusals, and what clients ask for in their
//! input with the guard's verdict.

use std::time::Duration;

use enshwick::ecs::Entity;
use enshwick::events::{KnowledgeChanged, PlayerDied, QuestCompleted};
use enshwick::knowledge::{Grant, Knowledge, KnowledgeKind, Revoke};
use enshwick::schedule::Scheduler;
use enshwick::{Load, Off};

/// A world counter of the 1.0 game, one of its three.
const WORLD_COUNTER: u32 = 3_229_540_729;
/// An account no player has, for a change that waits.
const NOBODY: u64 = 0x0e45_4157_0000_0001;
/// How long after a grant the probe reads it back.
const SETTLE: Duration = Duration::from_secs(1);

/// The knowledge checks.
#[derive(Debug, Clone, Copy)]
pub(crate) struct KnowledgeChecks {
    knowledge: Knowledge,
    scheduler: Scheduler,
}

impl KnowledgeChecks {
    pub(crate) fn new(load: &mut Load<'_>, scheduler: Scheduler) -> Result<Self, Off> {
        load.on::<KnowledgeChanged>(|changed| {
            enshwick::info!(
                "event: knowledge {} of {} is {}",
                changed.id,
                changed.entity,
                changed.value
            );
        })?;
        match load.on::<PlayerDied>(|death| {
            enshwick::info!(
                "event: player {} died, cause {}, at {:?}, killer {:02x?} entity {}, deadly shroud {}, drowned {}",
                death.entity,
                death.cause,
                death.position,
                death.killer,
                death.killer_entity,
                death.deadly_shroud,
                death.drowned
            );
        }) {
            Ok(()) => enshwick::info!("PASS player deaths subscribed"),
            Err(off) => enshwick::warn!("FAIL player deaths: {off}"),
        }
        load.on::<QuestCompleted>(|done| {
            enshwick::info!("event: quest {} completed by {}", done.quest, done.entity);
        })?;
        match load.on_knowledge_input(|request| {
            enshwick::info!(
                "event: knowledge input {:#x} ({:?} {}) from {}: {:?}, allowed {}",
                request.id,
                request.kind,
                request.value,
                request.player.0,
                request.refusal,
                request.allowed
            );
            request.allowed
        }) {
            Ok(()) => enshwick::info!("PASS knowledge input filter registered"),
            Err(off) => enshwick::warn!("FAIL knowledge input filter: {off}"),
        }
        Ok(Self {
            knowledge: load.knowledge(),
            scheduler,
        })
    }

    pub(crate) fn run(self) {
        let quests = self.knowledge.quests();
        if quests.is_empty() {
            enshwick::warn!("FAIL quests: none");
        } else {
            enshwick::info!("PASS quests: {}", quests.len());
        }
        let Some(before) = self.knowledge.world(WORLD_COUNTER) else {
            enshwick::warn!("FAIL knowledge: the world counter does not read");
            return;
        };
        enshwick::info!("PASS knowledge reads {before:?}");
        match self.knowledge.grant_world(WORLD_COUNTER, Grant::Set) {
            Err(why) => {
                enshwick::info!("PASS a bool grant of a counter is refused: {why}");
            }
            Ok(()) => enshwick::warn!("FAIL a bool grant of a counter was taken"),
        }
        match self.knowledge.grant_world(1, Grant::Set) {
            Err(why) => enshwick::info!("PASS an unknown id is refused: {why}"),
            Ok(()) => enshwick::warn!("FAIL an unknown id was taken"),
        }
        if let Err(why) = self.knowledge.grant_world(WORLD_COUNTER, Grant::Add(1)) {
            enshwick::warn!("FAIL knowledge grant: {why}");
            return;
        }
        self.scheduler
            .after(SETTLE, move || match self.knowledge.world(WORLD_COUNTER) {
                Some(now) if now.value == before.value.saturating_add(1) => {
                    enshwick::info!("PASS knowledge grant reads {}", now.value);
                    self.revoke(now.value);
                }
                other => enshwick::warn!("FAIL knowledge grant reads {other:?}"),
            });
        if let Some(&quest) = quests.first() {
            match self.knowledge.grant_account(NOBODY, quest, Grant::Set) {
                Ok(()) => enshwick::info!(
                    "PASS a grant for an account not in the game waits (quest {quest})"
                ),
                Err(why) => enshwick::warn!("FAIL a grant for an account: {why}"),
            }
            match self
                .knowledge
                .grant_account(NOBODY, WORLD_COUNTER, Grant::Add(1))
            {
                Err(why) => {
                    enshwick::info!("PASS an account grant of a world id is refused: {why}")
                }
                Ok(()) => {
                    enshwick::warn!("FAIL an account grant of a world id was taken");
                }
            }
        }
    }

    /// Takes the world counter's grant back and reads it at once.
    fn revoke(self, granted: u32) {
        if let Err(why) = self.knowledge.revoke_world(WORLD_COUNTER, Revoke::Sub(1)) {
            enshwick::warn!("FAIL knowledge revoke: {why}");
            return;
        }
        match self.knowledge.world(WORLD_COUNTER) {
            Some(now) if now.value == granted - 1 => {
                enshwick::info!("PASS knowledge revoke reads {}", now.value);
            }
            other => enshwick::warn!("FAIL knowledge revoke reads {other:?}"),
        }
        match self.knowledge.revoke_world(WORLD_COUNTER, Revoke::Clear) {
            Err(why) => {
                enshwick::info!("PASS a bool revoke of a counter is refused: {why}");
            }
            Ok(()) => enshwick::warn!("FAIL a bool revoke of a counter was taken"),
        }
    }
}

/// The `knowledge`, `learn` and `unlearn` actions: read, grant and take back
/// knowledge `id` of the caller; the kind comes from the id, `bit` is the
/// flag or the amount (1, the crafted bit of a recipe, when `None`).
pub(crate) fn command(
    knowledge: Knowledge,
    me: Entity,
    word: &str,
    id: u32,
    bit: Option<u32>,
) -> String {
    let bit = bit.unwrap_or(1);
    let Some(known) = knowledge.get(me, id) else {
        return format!("knowledge {id} does not read: unknown id, or no world");
    };
    let grant = match known.kind {
        KnowledgeKind::Bool => Grant::Set,
        KnowledgeKind::Counter => Grant::Add(bit),
        _ => Grant::Flag(bit),
    };
    let revoke = match known.kind {
        KnowledgeKind::Bool => Revoke::Clear,
        KnowledgeKind::Counter => Revoke::Sub(bit),
        _ => Revoke::Unflag(bit),
    };
    let done = match word {
        "learn" => knowledge.grant(me, id, grant),
        "unlearn" => knowledge.revoke(me, id, revoke),
        _ => return format!("knowledge {id}: {known:?}"),
    };
    let line = format!("{word} {id} ({:?}): {done:?}; before {known:?}", known.kind);
    enshwick::info!("{line}");
    line
}
