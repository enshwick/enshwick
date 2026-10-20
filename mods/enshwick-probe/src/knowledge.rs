//! Knowledge: the quests of the running game, a world counter granted and
//! read back the next step, and the refusals.

use std::time::Duration;

use enshwick_loader_api::events::{KnowledgeChanged, QuestCompleted};
use enshwick_loader_api::knowledge::{Grant, Knowledge};
use enshwick_loader_api::schedule::Scheduler;
use enshwick_loader_api::{Load, Off};

/// A world counter of the 1.0 game, one of its three.
const WORLD_COUNTER: u32 = 3_229_540_729;
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
        load.on::<QuestCompleted>(|done| {
            enshwick_loader_api::info!("event: quest {} completed by {}", done.quest, done.entity);
        })?;
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
                }
                other => enshwick_loader_api::warn!("FAIL knowledge grant reads {other:?}"),
            });
    }
}
