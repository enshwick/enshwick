//! Damage filters: counts the damage the filter sees by cause, and halves
//! it when the settings say so.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use enshwick_loader_api::damage::DamageCause;
use enshwick_loader_api::{Load, Off};

/// What the damage filter saw, by cause: hits, falls, shroud, deadly shroud.
#[derive(Debug, Default)]
pub(crate) struct DamageSeen {
    counts: [AtomicU64; 4],
}

impl DamageSeen {
    /// Registers the probe's damage filter; it halves every damage when
    /// `soften` is on.
    pub(crate) fn register(load: &mut Load<'_>, soften: bool) -> Result<Arc<Self>, Off> {
        let seen = Arc::new(Self::default());
        let counted = Arc::clone(&seen);
        load.on_damage(move |damage| {
            let index = match damage.cause() {
                DamageCause::Hit => 0,
                DamageCause::Fall => 1,
                DamageCause::Shroud => 2,
                _ => 3,
            };
            counted.counts[index].fetch_add(1, Ordering::Relaxed);
            if soften {
                damage.scale(0.5);
            }
        })?;
        Ok(seen)
    }

    /// The counts, for the log.
    pub(crate) fn line(&self) -> String {
        let [hits, falls, shroud, death] = &self.counts;
        format!(
            "damage seen: {} hits, {} falls, {} shroud, {} deadly shroud",
            hits.load(Ordering::Relaxed),
            falls.load(Ordering::Relaxed),
            shroud.load(Ordering::Relaxed),
            death.load(Ordering::Relaxed),
        )
    }
}
