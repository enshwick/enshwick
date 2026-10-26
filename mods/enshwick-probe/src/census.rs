//! Game events counted by type.

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use enshwick::Load;
use enshwick::game_events::{EVENT_TYPE_NAMES, RawGameEvent};

/// Counts of game events by type hash, with the names the census knows.
#[derive(Debug)]
pub(crate) struct Census {
    names: BTreeMap<u32, &'static str>,
    seen: Mutex<BTreeMap<u32, (u64, u32)>>,
}

impl Census {
    /// A census naming the types this build reflects.
    pub(crate) fn new(load: &Load<'_>) -> Self {
        let names = EVENT_TYPE_NAMES
            .iter()
            .filter_map(|&name| Some((load.event_type(name).ok()?.hash(), name)))
            .collect();
        Self {
            names,
            seen: Mutex::new(BTreeMap::new()),
        }
    }

    /// Counts `event`.
    pub(crate) fn count(&self, event: &RawGameEvent<'_>) {
        let mut seen = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = seen.entry(event.type_hash()).or_default();
        entry.0 += 1;
        entry.1 = u32::try_from(event.bytes().len()).unwrap_or(u32::MAX);
    }

    /// One line per type seen: name or hash, count and size.
    pub(crate) fn lines(&self) -> Vec<String> {
        let seen = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
        if seen.is_empty() {
            return vec!["no game events yet".to_owned()];
        }
        seen.iter()
            .map(|(hash, (count, size))| match self.names.get(hash) {
                Some(name) => format!("{name} x{count} ({size} bytes)"),
                None => format!("{hash:#010x} x{count} ({size} bytes)"),
            })
            .collect()
    }
}
