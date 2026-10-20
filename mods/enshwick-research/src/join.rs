//! What a join attempt leads to: the server change the game asks for, what
//! the call answers (0 when the join check refused it), and for the half
//! minute after it how often the main and pause menus draw their entries,
//! which says whether the game is back in a menu, in the world or stuck.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use enshwick_loader_api::events::JoinRefused;
use enshwick_loader_api::{Load, Next, Off};

use crate::memory;

type ServerChange = unsafe extern "system" fn(provider: *mut u8, request: *const u8) -> u64;
type MainEntry = unsafe extern "system" fn(menu: usize, entry: *const u8) -> u8;
type PauseEntry = unsafe extern "system" fn(text: *const u8, enabled: u8, attention: u8) -> u8;

/// The request names the server's query address in its first bytes.
const ADDRESS_BYTES: usize = 0x40;
/// How long after a server change the menus are watched.
const WATCH: Duration = Duration::from_secs(30);
/// How often the watch logs.
const WATCH_STEP: Duration = Duration::from_secs(1);

static MAIN_ENTRIES: AtomicU64 = AtomicU64::new(0);
static PAUSE_ENTRIES: AtomicU64 = AtomicU64::new(0);
static SERVER_CHANGES: AtomicU64 = AtomicU64::new(0);
static WATCHING: AtomicBool = AtomicBool::new(false);

enshwick_loader_api::detour! {
    // Ahead of the loader's join check, so the link sees refused joins too.
    static SERVER_CHANGE: unsafe extern "system" fn(provider: *mut u8, request: *const u8) -> u64 =
        server_change, priority = i32::MIN;
}

enshwick_loader_api::detour! {
    static MAIN_ENTRY: unsafe extern "system" fn(menu: usize, entry: *const u8) -> u8 = main_entry;
}

enshwick_loader_api::detour! {
    static PAUSE_ENTRY: unsafe extern "system" fn(text: *const u8, enabled: u8, attention: u8) -> u8 =
        pause_entry;
}

/// Links the server change and the menu entries, and logs refused joins.
pub(crate) fn link(load: &mut Load<'_>) -> Result<(), Off> {
    load.hook(&SERVER_CHANGE, load.symbol("net.server_change")?)?;
    for (hook, symbol) in [
        (
            load.hook(&MAIN_ENTRY, load.symbol("ui.main_entry")?),
            "main menu",
        ),
        (
            load.hook(&PAUSE_ENTRY, load.symbol("ui.pause_entry")?),
            "pause menu",
        ),
    ] {
        if let Err(off) = hook {
            enshwick_loader_api::warn!("R8 the {symbol} is not watched: {off}");
        }
    }
    load.on::<JoinRefused>(|()| {
        let missing: Vec<String> = enshwick_loader_api::mods::join_missing()
            .iter()
            .map(|entry| format!("{} {}", entry.id, entry.version))
            .collect();
        enshwick_loader_api::info!(
            "R8 the refusal note was closed; the loader reports missing: {missing:?}"
        );
    })
}

fn server_change(next: Next<ServerChange>, provider: *mut u8, request: *const u8) -> u64 {
    let mut bytes = [0u8; ADDRESS_BYTES];
    let read = memory::read(request.addr(), &mut bytes);
    let end = bytes[..read].iter().position(|&b| b == 0).unwrap_or(read);
    let address = String::from_utf8_lossy(&bytes[..end]).into_owned();
    let number = SERVER_CHANGES.fetch_add(1, Ordering::Relaxed) + 1;
    enshwick_loader_api::info!("R8 server change {number} asked: {address:?}; {}", menus());
    let start = Instant::now();
    // SAFETY: forwarded unchanged to the rest of the chain.
    let answer = unsafe { next.get()(provider, request) };
    enshwick_loader_api::info!(
        "R8 server change {number} answered {answer:#x} after {} ms{}",
        start.elapsed().as_millis(),
        if answer == 0 {
            " (0: refused, by the join check or the game)"
        } else {
            ""
        }
    );
    watch();
    answer
}

fn main_entry(next: Next<MainEntry>, menu: usize, entry: *const u8) -> u8 {
    MAIN_ENTRIES.fetch_add(1, Ordering::Relaxed);
    // SAFETY: forwarded unchanged to the rest of the chain.
    unsafe { next.get()(menu, entry) }
}

fn pause_entry(next: Next<PauseEntry>, text: *const u8, enabled: u8, attention: u8) -> u8 {
    PAUSE_ENTRIES.fetch_add(1, Ordering::Relaxed);
    // SAFETY: forwarded unchanged to the rest of the chain.
    unsafe { next.get()(text, enabled, attention) }
}

/// The menu entries drawn so far.
fn menus() -> String {
    format!(
        "main-menu entries drawn so far {}, pause-menu entries {}",
        MAIN_ENTRIES.load(Ordering::Relaxed),
        PAUSE_ENTRIES.load(Ordering::Relaxed)
    )
}

/// Logs, once a second for the watch, how many menu entries were drawn in
/// that second; one watch at a time.
fn watch() {
    if WATCHING.swap(true, Ordering::AcqRel) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("enshwick-research-join".into())
        .spawn(|| {
            let start = Instant::now();
            let mut main = MAIN_ENTRIES.load(Ordering::Relaxed);
            let mut pause = PAUSE_ENTRIES.load(Ordering::Relaxed);
            while start.elapsed() < WATCH {
                std::thread::sleep(WATCH_STEP);
                let (main_now, pause_now) = (
                    MAIN_ENTRIES.load(Ordering::Relaxed),
                    PAUSE_ENTRIES.load(Ordering::Relaxed),
                );
                enshwick_loader_api::info!(
                    "R8 +{} s: main-menu entries {}/s, pause-menu entries {}/s, server changes {}",
                    start.elapsed().as_secs(),
                    main_now - main,
                    pause_now - pause,
                    SERVER_CHANGES.load(Ordering::Relaxed)
                );
                (main, pause) = (main_now, pause_now);
            }
            WATCHING.store(false, Ordering::Release);
        });
    if spawned.is_err() {
        WATCHING.store(false, Ordering::Release);
    }
}
