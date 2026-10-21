//! Knowledge: the quests of the running game, a world counter granted and
//! read back the next step, the refusals, and what clients ask for in their
//! input with the guard's verdict.

use std::time::Duration;

use enshwick_loader_api::events::{KnowledgeChanged, PlayerDied, QuestCompleted};
use enshwick_loader_api::knowledge::{Grant, Knowledge, Revoke};
use enshwick_loader_api::schedule::Scheduler;
use enshwick_loader_api::{Load, Off};

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
            enshwick_loader_api::info!(
                "event: knowledge {} of {} is {}",
                changed.id,
                changed.entity,
                changed.value
            );
        })?;
        match load.on::<PlayerDied>(|death| {
            enshwick_loader_api::info!(
                "event: player {} died, cause {}, at {:?}, killer {:02x?}, deadly shroud {}, drowned {}",
                death.entity,
                death.cause,
                death.position,
                death.killer,
                death.deadly_shroud,
                death.drowned
            );
        }) {
            Ok(()) => enshwick_loader_api::info!("PASS player deaths subscribed"),
            Err(off) => enshwick_loader_api::warn!("FAIL player deaths: {off}"),
        }
        load.on::<QuestCompleted>(|done| {
            enshwick_loader_api::info!("event: quest {} completed by {}", done.quest, done.entity);
        })?;
        match load.on_knowledge_input(|request| {
            enshwick_loader_api::info!(
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
            Ok(()) => enshwick_loader_api::info!("PASS knowledge input filter registered"),
            Err(off) => enshwick_loader_api::warn!("FAIL knowledge input filter: {off}"),
        }
        Ok(Self {
            knowledge: load.knowledge()?,
            scheduler,
        })
    }

    pub(crate) fn run(self) {
        let quests = self.knowledge.quests();
        if quests.is_empty() {
            enshwick_loader_api::warn!("FAIL quests: none");
        } else {
            enshwick_loader_api::info!("PASS quests: {}", quests.len());
        }
        let Some(before) = self.knowledge.world(WORLD_COUNTER) else {
            enshwick_loader_api::warn!("FAIL knowledge: the world counter does not read");
            return;
        };
        enshwick_loader_api::info!("PASS knowledge reads {before:?}");
        match self.knowledge.grant_world(WORLD_COUNTER, Grant::Set) {
            Err(why) => {
                enshwick_loader_api::info!("PASS a bool grant of a counter is refused: {why}");
            }
            Ok(()) => enshwick_loader_api::warn!("FAIL a bool grant of a counter was taken"),
        }
        match self.knowledge.grant_world(1, Grant::Set) {
            Err(why) => enshwick_loader_api::info!("PASS an unknown id is refused: {why}"),
            Ok(()) => enshwick_loader_api::warn!("FAIL an unknown id was taken"),
        }
        if let Err(why) = self.knowledge.grant_world(WORLD_COUNTER, Grant::Add(1)) {
            enshwick_loader_api::warn!("FAIL knowledge grant: {why}");
            return;
        }
        self.scheduler
            .after(SETTLE, move || match self.knowledge.world(WORLD_COUNTER) {
                Some(now) if now.value == before.value.saturating_add(1) => {
                    enshwick_loader_api::info!("PASS knowledge grant reads {}", now.value);
                    self.revoke(now.value);
                }
                other => enshwick_loader_api::warn!("FAIL knowledge grant reads {other:?}"),
            });
        if let Some(&quest) = quests.first() {
            match self.knowledge.grant_account(NOBODY, quest, Grant::Set) {
                Ok(()) => enshwick_loader_api::info!(
                    "PASS a grant for an account not in the game waits (quest {quest})"
                ),
                Err(why) => enshwick_loader_api::warn!("FAIL a grant for an account: {why}"),
            }
            match self
                .knowledge
                .grant_account(NOBODY, WORLD_COUNTER, Grant::Add(1))
            {
                Err(why) => enshwick_loader_api::info!(
                    "PASS an account grant of a world id is refused: {why}"
                ),
                Ok(()) => {
                    enshwick_loader_api::warn!("FAIL an account grant of a world id was taken");
                }
            }
        }
    }

    /// Takes the world counter's grant back and reads it at once.
    fn revoke(self, granted: u32) {
        if let Err(why) = self.knowledge.revoke_world(WORLD_COUNTER, Revoke::Sub(1)) {
            enshwick_loader_api::warn!("FAIL knowledge revoke: {why}");
            return;
        }
        match self.knowledge.world(WORLD_COUNTER) {
            Some(now) if now.value == granted - 1 => {
                enshwick_loader_api::info!("PASS knowledge revoke reads {}", now.value);
            }
            other => enshwick_loader_api::warn!("FAIL knowledge revoke reads {other:?}"),
        }
        match self.knowledge.revoke_world(WORLD_COUNTER, Revoke::Clear) {
            Err(why) => {
                enshwick_loader_api::info!("PASS a bool revoke of a counter is refused: {why}");
            }
            Ok(()) => enshwick_loader_api::warn!("FAIL a bool revoke of a counter was taken"),
        }
    }
}
