//! Example: a mod that a development server reloads while it runs.
//!
//! With `dev_reload = true` in `mod-configs/enshwick.toml`, the dedicated
//! server watches `mods/` and reloads this mod when its DLL changes; a
//! client does the same for the mods it runs. The mod allows it with
//! `reload: true`, uses only what a reload can unlink (an event and a log
//! line), and stops the thread it starts in `shutdown`.
//! Change [`GREETING`], run `cargo enshwick install`, and the server logs the
//! new text without a restart.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use enshwick::{Host, Load, Mod, Off, events};

/// What this copy of the mod says; change it to see a reload.
pub const GREETING: &str = "hello from the first copy";

/// How often the ticker speaks.
const TICK: Duration = Duration::from_secs(30);

/// The mod's state: its ticker and the flag that stops it.
#[derive(Debug, Default)]
pub struct HotReload {
    stop: AtomicBool,
    ticker: Mutex<Option<JoinHandle<()>>>,
}

impl Mod for HotReload {
    fn manifest() -> &'static enshwick::abi::Manifest {
        enshwick::manifest!(id: "you.hot-reload", side: SERVER, reload: true)
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        load.on::<events::WorldSaved>(|()| enshwick::info!("saved: {GREETING}"))?;
        Ok(Self::default())
    }

    fn ready(&'static self, _host: Host) {
        enshwick::info!("{GREETING}");
        let ticker = std::thread::Builder::new()
            .name("hot-reload-ticker".into())
            .spawn(move || {
                let mut waited = Duration::ZERO;
                while !self.stop.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(100));
                    waited += Duration::from_millis(100);
                    if waited >= TICK {
                        waited = Duration::ZERO;
                        enshwick::info!("tick: {GREETING}");
                    }
                }
            });
        if let (Ok(ticker), Ok(mut slot)) = (ticker, self.ticker.lock()) {
            *slot = Some(ticker);
        }
    }

    fn shutdown(&'static self) {
        self.stop.store(true, Ordering::Release);
        let ticker = self.ticker.lock().ok().and_then(|mut slot| slot.take());
        if let Some(ticker) = ticker
            && ticker.join().is_err()
        {
            enshwick::warn!("the ticker panicked");
        }
        enshwick::info!("shut down: {GREETING}");
    }
}

enshwick::declare_mod!(HotReload);

#[cfg(test)]
mod tests {
    use enshwick::abi::{ManifestFlags, Status};
    use enshwick::test::FakeHost;

    use super::*;

    #[test]
    fn the_mod_allows_reloads_and_stops_its_ticker() {
        assert!(HotReload::manifest().flags.contains(ManifestFlags::RELOAD));
        let loader = FakeHost::new().install();
        // SAFETY: both functions are the exports of this crate's `declare_mod!(HotReload)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        enshwick_mod_shutdown();
        let state = HotReload::instance().unwrap();
        assert!(state.stop.load(Ordering::Acquire));
        assert!(state.ticker.lock().unwrap().is_none());
        assert!(
            loader
                .logs()
                .iter()
                .any(|(_, line)| line == &format!("shut down: {GREETING}"))
        );
    }
}
