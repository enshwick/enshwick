//! Engine events counted by type.

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use enshwick_loader_api::Load;
use enshwick_loader_api::game_events::GameEvent;

/// Engine event types the census names; others show by hash.
const NAMES: &[&str] = &[
    "keen::ecs::HitEvent",
    "keen::ecs::EntityDiedEvent",
    "keen::ecs::LevelUpEvent",
    "keen::ecs::XPGainedEvent",
    "keen::ecs::CraftRecipeEvent",
    "keen::ecs::CraftRecipeSuccessEvent",
    "keen::ecs::AcceptOfferedInteractionEvent",
    "keen::ecs::InteractionAcceptedEvent",
    "keen::ecs::BuildingPlaceEvent",
    "keen::ecs::BuildingPickUpEvent",
    "keen::ecs::BuildingTearDownEvent",
    "keen::ecs::FlameAltarEvent",
    "keen::ecs::LootInteractionEvent",
    "keen::ecs::HandleEquipmentSlotChangedEvent",
    "keen::ecs::RevivedPlayerEvent",
    "keen::ecs::OnDestroyNotifyEvent",
    "keen::ecs::LoginEvent",
    "keen::ecs::PickupEvent",
    "keen::ecs::ParryEvent",
    "keen::ecs::AllItemsRepairedEvent",
    "keen::ecs::TransformedToLootEvent",
    "keen::ecs::AppliedBuffEvent",
    "keen::ecs::LogicEntityPlacedEvent",
    "keen::ecs::ProhibitedHit",
    "keen::ecs::PlayerSpawnInfoChangeEvent",
    "keen::ecs::CookingEvent",
    "keen::ecs::GameSettingsChangedEvent",
    "keen::ecs::AccountActionEvent",
    "keen::ecs::KillFaction",
    "keen::ecs::NoiseEvent",
    "keen::ecs::ResourceChangeEvent",
    "keen::ecs::PuzzleCompletedEvent",
    "keen::ecs::DestroyedWithoutSurfaceEvent",
    "keen::ecs::NotifyEvent",
    "keen::ecs::SetToggleStateEvent",
    "keen::ecs::ToggleInteractionEvent",
    "keen::ecs::NpcCommandEvent",
    "keen::ecs::FootDown",
    "keen::ecs::GenericEnemyEvent",
    "keen::ecs::EnemyAttackStartedEvent",
    "keen::ecs::CallForHelpEvent",
    "keen::ecs::AddThreatTargetEvent",
    "keen::ecs::RestoreEnemyHealthEvent",
    "keen::ecs::SetEnemyVariableEvent",
    "keen::ecs::StartGuardingEvent",
    "keen::ecs::StopGuardingEvent",
    "keen::ecs::UiWorldEvent",
    "keen::ecs::UiPingEvent",
    "keen::ecs::OpenHudScreenEvent",
    "keen::ecs::KnowledgeGrantsExperienceEvent",
    "keen::ecs::KnowledgeAnalyticsEvent",
    "keen::ecs::TerraformingHitEvent",
    "keen::ecs::ResourceCandidate",
    "keen::ecs::HitCandidate",
    "keen::ecs::CollisionEvent",
    "keen::ecs::InventoryFull",
    "keen::ecs::AreaDepositEvent",
    "keen::ecs::ApplyPushBackEvent",
    "keen::ecs::ActivateFactionPacifyEvent",
    "keen::ecs::VisceralAttackExecutedEvent",
    "keen::ecs::StartPlaySongEvent",
];

/// Counts of engine events by type hash, with the names the census knows.
#[derive(Debug)]
pub(crate) struct Census {
    names: BTreeMap<u32, &'static str>,
    seen: Mutex<BTreeMap<u32, (u64, u32)>>,
}

impl Census {
    /// A census naming the types this build reflects.
    pub(crate) fn new(load: &Load<'_>) -> Self {
        let names = NAMES
            .iter()
            .filter_map(|&name| Some((load.event_type(name).ok()?.hash(), name)))
            .collect();
        Self {
            names,
            seen: Mutex::new(BTreeMap::new()),
        }
    }

    /// Counts `event`.
    pub(crate) fn count(&self, event: &GameEvent<'_>) {
        let mut seen = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = seen.entry(event.type_hash()).or_default();
        entry.0 += 1;
        entry.1 = u32::try_from(event.bytes().len()).unwrap_or(u32::MAX);
    }

    /// One line per type seen: name or hash, count and size.
    pub(crate) fn lines(&self) -> Vec<String> {
        let seen = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
        if seen.is_empty() {
            return vec!["no engine events yet".to_owned()];
        }
        seen.iter()
            .map(|(hash, (count, size))| match self.names.get(hash) {
                Some(name) => format!("{name} x{count} ({size} bytes)"),
                None => format!("{hash:#010x} x{count} ({size} bytes)"),
            })
            .collect()
    }
}
