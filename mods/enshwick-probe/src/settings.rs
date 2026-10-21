//! Game settings: a typed read, a change that reads back, the old values
//! put back, and a kept field: one run keeps the bosses' health, the next
//! sees it come back with the world and forgets it.

use enshwick_loader_api::game_settings::GameSettings;
use enshwick_loader_api::{Load, Off};

/// The bosses' health a run keeps.
const KEPT: f32 = 1.25;

/// The game settings checks.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SettingsChecks {
    settings: GameSettings,
}

impl SettingsChecks {
    pub(crate) fn new(load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(Self {
            settings: load.game_settings()?,
        })
    }

    /// Reads the settings, raises the perk cost by a half, reads it back
    /// and puts the settings back as they were.
    pub(crate) fn run(self) {
        let Some((before, preset)) = self.settings.read() else {
            enshwick_loader_api::warn!("FAIL game settings: no reading");
            return;
        };
        enshwick_loader_api::info!("PASS game settings read, preset {preset:?}: {before:?}");
        let wanted = before.perk_cost + 0.5;
        match self.settings.change(|s| s.perk_cost = wanted) {
            Ok(_) => match self.settings.read() {
                Some((now, preset)) if (now.perk_cost - wanted).abs() < 1e-6 => {
                    enshwick_loader_api::info!(
                        "PASS game settings change reads back perk cost {}, preset {preset:?}",
                        now.perk_cost
                    );
                }
                other => enshwick_loader_api::warn!("FAIL game settings change reads {other:?}"),
            },
            Err(why) => enshwick_loader_api::warn!("FAIL game settings change: {why}"),
        }
        match self
            .settings
            .change(|s| s.night_length = core::time::Duration::ZERO)
        {
            Err(why) => enshwick_loader_api::info!("PASS a night of zero is refused: {why}"),
            Ok(_) => enshwick_loader_api::warn!("FAIL a night of zero was taken"),
        }
        match self.settings.change(|s| *s = before) {
            Ok(_) => enshwick_loader_api::info!("PASS game settings put back"),
            Err(why) => enshwick_loader_api::warn!("FAIL game settings put back: {why}"),
        }
        self.kept(before.boss_health);
    }

    /// Keeps the bosses' health at [`KEPT`], or, when it came back with the
    /// world, forgets it and sees the loaded value return.
    fn kept(self, now: f32) {
        if (now - KEPT).abs() < 1e-6 {
            enshwick_loader_api::info!(
                "PASS a kept field came back with the world: boss health {now}"
            );
            match self.settings.forget_kept() {
                Ok(()) => match self.settings.read() {
                    Some((after, _)) if (after.boss_health - KEPT).abs() > 1e-6 => {
                        enshwick_loader_api::info!(
                            "PASS forgetting puts back boss health {}",
                            after.boss_health
                        );
                    }
                    other => enshwick_loader_api::warn!("FAIL forgetting reads {other:?}"),
                },
                Err(why) => enshwick_loader_api::warn!("FAIL forgetting kept settings: {why}"),
            }
            return;
        }
        match self.settings.keep(|s| s.boss_health = KEPT) {
            Ok(_) => enshwick_loader_api::info!(
                "PASS boss health {KEPT} kept; the next start should bring it back"
            ),
            Err(why) => enshwick_loader_api::warn!("FAIL keeping a game setting: {why}"),
        }
    }
}
