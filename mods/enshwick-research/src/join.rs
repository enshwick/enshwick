//! What a join attempt leads to: the join the game starts, with its host and
//! address, and for the half minute after it how often the main and pause
//! menus draw their entries, which says whether the game is back in a menu,
//! in the world or stuck.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use enshwick_loader_api::events::JoinRefused;
use enshwick_loader_api::{Load, Next, Off};

use crate::memory;

type JoinGame = unsafe extern "system" fn(provider: *mut u8, target: *mut u8) -> u8;
type MainEntry = unsafe extern "system" fn(menu: usize, entry: *const u8) -> u8;
type PauseEntry = unsafe extern "system" fn(text: *const u8, enabled: u8, attention: u8) -> u8;

/// Where the join target holds the host's Steam id, the server's IPv4, its
/// game and query ports and the flag that the address is set (client builds
/// 410 and 426).
const HOST: usize = 0;
const IP: usize = 8;
const GAME_PORT: usize = 0xc;
const QUERY_PORT: usize = 0xe;
const HAS_ADDRESS: usize = 0x20;
/// The join target's bytes up to its address flag.
const TARGET_BYTES: usize = HAS_ADDRESS + 1;
/// How long after a join the menus are watched.
const WATCH: Duration = Duration::from_secs(30);
/// How often the watch logs.
const WATCH_STEP: Duration = Duration::from_secs(1);

static MAIN_ENTRIES: AtomicU64 = AtomicU64::new(0);
static PAUSE_ENTRIES: AtomicU64 = AtomicU64::new(0);
static JOINS: AtomicU64 = AtomicU64::new(0);
static WATCHING: AtomicBool = AtomicBool::new(false);

enshwick_loader_api::detour! {
    // Ahead of the loader's join check, so the link sees refused joins too.
    static JOIN_GAME: unsafe extern "system" fn(provider: *mut u8, target: *mut u8) -> u8 =
        join_game, priority = i32::MIN;
}

enshwick_loader_api::detour! {
    static MAIN_ENTRY: unsafe extern "system" fn(menu: usize, entry: *const u8) -> u8 = main_entry;
}

enshwick_loader_api::detour! {
    static PAUSE_ENTRY: unsafe extern "system" fn(text: *const u8, enabled: u8, attention: u8) -> u8 =
        pause_entry;
}

/// Links the join and the menu entries, and logs refused joins.
pub(crate) fn link(load: &mut Load<'_>) -> Result<(), Off> {
    let join_game = load.symbol("net.join_game")?;
    let pinned = load.constant("net.join_game.bytes")?;
    let found = usize::try_from(join_game).ok().and_then(memory::read_u64);
    if found != Some(pinned) {
        return Err(Off::new(format!(
            "the code at the join {join_game:#x} is {found:x?}, not the pinned {pinned:#018x}"
        )));
    }
    // SAFETY: the table's `net.join_game` is the function `JOIN_GAME`
    // declares, its first bytes checked above.
    unsafe { load.hook(&JOIN_GAME, join_game) }?;
    let (main_entry, pause_entry) = (
        load.symbol("ui.main_entry")?,
        load.symbol("ui.pause_entry")?,
    );
    for (hook, symbol) in [
        (
            // SAFETY: the table's `ui.main_entry` is the function
            // `MAIN_ENTRY` declares.
            unsafe { load.hook(&MAIN_ENTRY, main_entry) },
            "main menu",
        ),
        (
            // SAFETY: the table's `ui.pause_entry` is the function
            // `PAUSE_ENTRY` declares.
            unsafe { load.hook(&PAUSE_ENTRY, pause_entry) },
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

fn join_game(next: Next<JoinGame>, provider: *mut u8, target: *mut u8) -> u8 {
    let number = JOINS.fetch_add(1, Ordering::Relaxed) + 1;
    enshwick_loader_api::info!("R8 join {number} asked: {}; {}", described(target), menus());
    let start = Instant::now();
    // SAFETY: forwarded unchanged to the rest of the chain.
    let state_changed = unsafe { next.get()(provider, target) };
    enshwick_loader_api::info!(
        "R8 join {number} returned {state_changed} after {} ms",
        start.elapsed().as_millis()
    );
    watch();
    state_changed
}

/// The host, address, ports and address flag of a join target.
fn described(target: *mut u8) -> String {
    let mut bytes = [0u8; TARGET_BYTES];
    if memory::read(target.addr(), &mut bytes) != TARGET_BYTES {
        return format!("unreadable target {target:p}");
    }
    let host = u64::from_le_bytes(field(&bytes, HOST));
    let ip = std::net::Ipv4Addr::from(u32::from_le_bytes(field(&bytes, IP)));
    let game = u16::from_le_bytes(field(&bytes, GAME_PORT));
    let query = u16::from_le_bytes(field(&bytes, QUERY_PORT));
    format!(
        "host {host}, address {ip} game port {game} query port {query}, address set {}",
        bytes[HAS_ADDRESS]
    )
}

/// The `N` bytes of a join target from `at`, inside the target.
fn field<const N: usize>(bytes: &[u8; TARGET_BYTES], at: usize) -> [u8; N] {
    core::array::from_fn(|i| bytes[at + i])
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
                    "R8 +{} s: main-menu entries {}/s, pause-menu entries {}/s, joins {}",
                    start.elapsed().as_secs(),
                    main_now - main,
                    pause_now - pause,
                    JOINS.load(Ordering::Relaxed)
                );
                (main, pause) = (main_now, pause_now);
            }
            WATCHING.store(false, Ordering::Release);
        });
    if spawned.is_err() {
        WATCHING.store(false, Ordering::Release);
    }
}
