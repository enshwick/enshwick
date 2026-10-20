//! A world past 16 altar zones cannot open safely with the vanilla grouping,
//! so the dedicated server holds the game on it while it has one.

use std::time::Duration;

use enshwick_loader_api::events::AltarsChanged;
use enshwick_loader_api::state::Hold;
use enshwick_loader_api::{Load, Off};

const VANILLA_ZONES: u32 = 16;
const EVERY: Duration = Duration::from_secs(5);

/// Keeps the hold on a thread of its own, woken when the altars change.
pub(crate) fn keep(load: &mut Load<'_>) -> Result<(), Off> {
    let hold = Hold::new(&load.data_dir()?);
    // Following the altars is what raises `AltarsChanged`.
    let _altars = load.altars()?;
    let handle = std::thread::Builder::new()
        .name("flame-altar-count-hold".into())
        .spawn(move || run(&hold))
        .map_err(|e| Off::new(format!("hold thread: {e}")))?;
    let thread = handle.thread().clone();
    load.on::<AltarsChanged>(move |()| thread.unpark())
}

/// The reason a held world must not open with the vanilla grouping.
pub(crate) fn reason(load: &Load<'_>) -> Result<Option<String>, Off> {
    Ok(Hold::new(&load.data_dir()?).reason())
}

fn run(hold: &Hold) {
    let mut set = hold.reason().is_some();
    loop {
        std::thread::park_timeout(EVERY);
        let Some(grouping) = enshwick_loader_api::altars::grouping() else {
            continue;
        };
        let past = grouping.zones > VANILLA_ZONES;
        if past == set {
            continue;
        }
        let zones = grouping.zones;
        let done = if past {
            hold.set(&format!(
                "the world has {zones} altar zones; the vanilla game groups only {VANILLA_ZONES}"
            ))
        } else {
            hold.clear()
        };
        match done {
            Ok(()) => {
                set = past;
                enshwick_loader_api::info!(
                    "hold {}: {zones} altar zones",
                    if past { "set" } else { "released" }
                );
            }
            Err(off) => enshwick_loader_api::warn_every!(Duration::from_secs(600), "hold: {off}"),
        }
    }
}
