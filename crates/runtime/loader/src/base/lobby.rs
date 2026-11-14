//! How a friend's client tells that the game it joins is hosted by a client
//! with Enshwick.
//!
//! A hosted game advertises no Steam rules, so the join check finds nothing
//! to read. Every session of the game, hosted or on a dedicated server, sits
//! in a Steam lobby that each player enters before the game connects (client
//! build 1269579: the game's log shows `[online] Entered lobby` for both). A
//! hosting client with the loader puts its loader version and join mods into
//! its lobby's data; a client that enters a lobby with that data says hello
//! to the host over the mod messages, and the host answers with its join
//! values as a dedicated server does.
//!
//! A hosted game cannot keep a client out before it connects: the client
//! turns off the join mods the host lacks and says which of the host's mods
//! it lacks or runs in another version, and joins all the same.

/// The lobby data key with the host's loader version.
pub(crate) const KEY: &str = "enshwick";
/// The lobby data key with the host's join mods, `id@version,...`.
pub(crate) const KEY_MODS: &str = "enshwick.mods";
/// The longest value Steam keeps under one lobby data key, in bytes.
pub(crate) const MAX_VALUE: usize = 8192;

/// What a client does on entering a hosted game whose host runs the join
/// mods `host` (`id@version,...`), with its own `ours`: the mods to turn
/// off, which the host lacks, and the note for the player, naming mods by
/// `names`.
pub(crate) fn plan(
    host: &str,
    ours: &str,
    names: &crate::base::join::Names,
) -> (Vec<String>, Option<String>) {
    let differences = crate::base::join::differences(host, ours, names);
    let turn_off: Vec<String> = differences
        .iter()
        .filter(|(_, on_host, _)| !on_host)
        .map(|(id, _, _)| id.clone())
        .collect();
    let lacking: Vec<&str> = differences
        .iter()
        .filter(|(_, on_host, _)| *on_host)
        .map(|(_, _, line)| line.as_str())
        .collect();
    let mut parts = Vec::new();
    if !turn_off.is_empty() {
        let lines: Vec<String> = turn_off
            .iter()
            .map(|id| enshwick_core::mods::label(names.get(id).map_or("", String::as_str), id))
            .collect();
        parts.push(format!(
            "The host of this game does not run these Enshwick mods, so they are off while you play in it:\n\n{}",
            lines.join("\n")
        ));
    }
    if !lacking.is_empty() {
        parts.push(format!(
            "The host of this game runs Enshwick mods you do not have in its version:\n\n{}\n\nInstall the host's versions, or things may not work.",
            lacking.join("\n")
        ));
    }
    (turn_off, (!parts.is_empty()).then(|| parts.join("\n\n")))
}

#[cfg(windows)]
pub(crate) use runtime::{discovery_on, forget, host_runs_enshwick, left, poll, register};

#[cfg(windows)]
mod runtime {
    use core::ffi::{CStr, c_char, c_void};
    use std::ffi::CString;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Mutex, OnceLock};

    use enshwick_base::Relock;

    use enshwick_core::abi::UiDialog;

    use crate::engine::fuse::{Fuse, Off};
    use crate::engine::module::Export;

    /// The hosted lobby: a hosting client names its mods in its Steam
    /// lobby, and a client entering a lobby reads the host's.
    static LOBBY_FUSE: Fuse = Fuse::new("the hosted lobby");

    /// Steam's callback id of `LobbyEnter_t`: matchmaking callbacks start at
    /// 500, and it is the fourth.
    const LOBBY_ENTER: i32 = 504;
    /// `LobbyEnter_t`'s size, packed to eight bytes on Windows: the lobby's
    /// id, chat permissions, a locked flag and the enter response.
    const LOBBY_ENTER_BYTES: usize = 24;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        reason = "24 bytes"
    )]
    const LOBBY_ENTER_SIZE: i32 = LOBBY_ENTER_BYTES as i32;
    const ENTER_RESPONSE: usize = 16;
    /// `k_EChatRoomEnterResponseSuccess`.
    const ENTERED: u32 = 1;

    type Matchmaking = unsafe extern "C" fn() -> *mut c_void;
    type GetLobbyData =
        unsafe extern "C" fn(this: *mut c_void, lobby: u64, key: *const c_char) -> *const c_char;
    type SetLobbyData = unsafe extern "C" fn(
        this: *mut c_void,
        lobby: u64,
        key: *const c_char,
        value: *const c_char,
    ) -> u8;
    type RegisterCallback = unsafe extern "C" fn(callback: *mut Callback, id: i32);

    struct Api {
        matchmaking: Matchmaking,
        get: GetLobbyData,
        set: SetLobbyData,
    }

    impl core::fmt::Debug for Api {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str("Api")
        }
    }

    /// What the hosted lobby keeps: the Steam calls found once, and the
    /// lobbies of the game that runs.
    struct State {
        api: OnceLock<Api>,
        /// The lobby the client last entered, 0 for none.
        lobby: AtomicU64,
        /// The joins the client started from the game's menus, each of which
        /// leaves the lobby it was in.
        joins: AtomicU64,
        /// The lobby whose entry was handled and the join it came in, so a
        /// second notice of one entry turns nothing off twice while a new
        /// entry into the same lobby is checked again.
        handled: Mutex<(u64, u64)>,
        /// The lobby the hosting client put its data into.
        published: AtomicU64,
        /// Whether the lobby the client entered names a host with Enshwick,
        /// in bit 0, and the join it entered in, above: it holds only until
        /// the next join.
        host_runs: AtomicU64,
    }

    impl State {
        const fn new() -> Self {
            Self {
                api: OnceLock::new(),
                lobby: AtomicU64::new(0),
                joins: AtomicU64::new(0),
                handled: Mutex::new((0, 0)),
                published: AtomicU64::new(0),
                host_runs: AtomicU64::new(0),
            }
        }

        /// The client starts a join, leaving its lobby.
        fn left(&self) {
            self.joins.fetch_add(1, Ordering::AcqRel);
        }

        /// Notes the client's entry into `lobby`, whose host runs Enshwick
        /// when `host_runs`; true when the host's mods are to be checked,
        /// false for a second notice of the same entry.
        fn note_entry(&self, lobby: u64, host_runs: bool) -> bool {
            let join = self.joins.load(Ordering::Acquire);
            self.host_runs
                .store(join << 1 | u64::from(host_runs), Ordering::Release);
            host_runs
                && core::mem::replace(&mut *self.handled.relock(), (lobby, join)) != (lobby, join)
        }

        /// Whether the lobby the client entered since its last join names a
        /// host with Enshwick.
        fn host_runs(&self) -> bool {
            let noted = self.host_runs.load(Ordering::Acquire);
            noted & 1 == 1 && noted >> 1 == self.joins.load(Ordering::Acquire)
        }

        /// Forgets the lobby the hosting client published into; the Steam
        /// calls stay. The entry guard and whether a host runs Enshwick
        /// stay too: the next join ends both, and the next lobby entry sets
        /// both on Steam's thread, maybe before a hosted game's end reaches
        /// this.
        fn forget(&self) {
            self.lobby.store(0, Ordering::Release);
            self.published.store(0, Ordering::Relaxed);
        }
    }

    static STATE: State = State::new();

    /// Forgets the lobby a client's hosted game ran in, so the next game it
    /// hosts or joins starts from no lobby.
    pub(crate) fn forget() {
        STATE.forget();
    }

    /// Notes that the client starts a join from the game's menus, which
    /// leaves the lobby it was in: until it enters the next, no host takes
    /// its hello.
    pub(crate) fn left() {
        STATE.left();
    }

    /// Steam's `CCallbackBase` as MSVC lays it out: the table, then the
    /// flags Steam keeps and the callback id.
    #[repr(C)]
    struct Callback {
        table: &'static Table,
        flags: u8,
        id: i32,
    }

    /// The virtual functions: MSVC puts the overloads of `Run` in reverse
    /// order, the call-result form first; both take the callback's data
    /// first, so one function serves both.
    #[repr(C)]
    struct Table {
        run_result: unsafe extern "C" fn(*mut Callback, *const u8),
        run: unsafe extern "C" fn(*mut Callback, *const u8),
        size: unsafe extern "C" fn(*mut Callback) -> i32,
    }

    static TABLE: Table = Table {
        run_result: run,
        run,
        size,
    };

    /// Steam's `Run` for `LobbyEnter_t`: notes the lobby the client entered.
    ///
    /// # Safety
    ///
    /// - Called by Steam on the thread that runs its callbacks, with null or
    ///   a `LobbyEnter_t` of `LOBBY_ENTER_SIZE` readable bytes that Steam
    ///   does not change during the call.
    unsafe extern "C" fn run(_callback: *mut Callback, data: *const u8) {
        if data.is_null() {
            return;
        }
        // SAFETY: `data` is non-null, checked by the guard before it, and
        // points at Steam's `LobbyEnter_t` of `LOBBY_ENTER_BYTES` bytes,
        // unchanged during the call, by this function's `# Safety`.
        let record: [u8; LOBBY_ENTER_BYTES] = unsafe { enshwick_core::raw::read_at(data, 0) };
        let lobby = record
            .first_chunk::<8>()
            .map_or(0, |b| u64::from_le_bytes(*b));
        let response = record
            .get(ENTER_RESPONSE..ENTER_RESPONSE + 4)
            .and_then(|b| b.try_into().ok())
            .map_or(0, u32::from_le_bytes);
        if response != ENTERED || lobby == 0 {
            return;
        }
        enter(lobby, entered);
    }

    /// Runs `entered` for the client's entry into `lobby`, on Steam's
    /// callback thread, under the hosted lobby's fuse.
    fn enter(lobby: u64, entered: fn(u64)) {
        let _ran = LOBBY_FUSE.run("a lobby entry", || entered(lobby));
    }

    /// Steam's `GetCallbackSizeBytes`: the size of `LobbyEnter_t`.
    ///
    /// # Safety
    ///
    /// - Called by Steam with the callback object it was registered with.
    unsafe extern "C" fn size(_callback: *mut Callback) -> i32 {
        LOBBY_ENTER_SIZE
    }

    /// Asks Steam to tell the loader of each lobby the client enters, on the
    /// game's thread that runs Steam's callbacks.
    pub(crate) fn register() {
        let Some((api, register)) = load() else {
            let why = "steam_api64.dll lacks the lobby API";
            LOBBY_FUSE.turn_off(&Off::reported(
                format!(
                    "{} off: {why}, friends of a host get no join values",
                    LOBBY_FUSE.feature()
                ),
                why.to_owned(),
            ));
            return;
        };
        if STATE.api.set(api).is_err() {
            return;
        }
        let callback = Box::leak(Box::new(Callback {
            table: &TABLE,
            flags: 0,
            id: 0,
        }));
        // SAFETY: `register` is `SteamAPI_RegisterCallback`, typed by the
        // flat API; `callback` is leaked, so it lives for the process, is
        // laid out as MSVC's `CCallbackBase` (`repr(C)`), and Rust never
        // touches it again while Steam writes its flags and id.
        unsafe { register(&raw mut *callback, LOBBY_ENTER) };
    }

    fn load() -> Option<(Api, RegisterCallback)> {
        let module = crate::engine::module::Module::loaded("steam_api64.dll")?;
        let get = |name: &str| module.export(name);
        let matchmaking = get("SteamAPI_SteamMatchmaking_v009")?;
        let get_data = get("SteamAPI_ISteamMatchmaking_GetLobbyData")?;
        let set_data = get("SteamAPI_ISteamMatchmaking_SetLobbyData")?;
        let register = get("SteamAPI_RegisterCallback")?;
        // SAFETY: the Steamworks flat API declares
        // `SteamAPI_SteamMatchmaking_v009` as `Matchmaking`.
        let matchmaking = unsafe { core::mem::transmute::<Export, Matchmaking>(matchmaking) };
        // SAFETY: the Steamworks flat API declares
        // `SteamAPI_ISteamMatchmaking_GetLobbyData` as `GetLobbyData`.
        let get_data = unsafe { core::mem::transmute::<Export, GetLobbyData>(get_data) };
        // SAFETY: the Steamworks flat API declares
        // `SteamAPI_ISteamMatchmaking_SetLobbyData` as `SetLobbyData`, its
        // C++ `bool` read as a byte.
        let set_data = unsafe { core::mem::transmute::<Export, SetLobbyData>(set_data) };
        // SAFETY: the Steamworks flat API declares `SteamAPI_RegisterCallback`
        // as `RegisterCallback`.
        let register = unsafe { core::mem::transmute::<Export, RegisterCallback>(register) };
        Some((
            Api {
                matchmaking,
                get: get_data,
                set: set_data,
            },
            register,
        ))
    }

    impl Api {
        fn interface(&self) -> Option<*mut c_void> {
            // SAFETY: `self.matchmaking` is the flat API's
            // `SteamAPI_SteamMatchmaking_v009`, which takes no arguments and
            // returns null before Steam starts.
            let this = unsafe { (self.matchmaking)() };
            (!this.is_null()).then_some(this)
        }

        /// The lobby's value under `key`, empty when it has none.
        fn get(&self, lobby: u64, key: &str) -> String {
            let (Some(this), Ok(key)) = (self.interface(), CString::new(key)) else {
                return String::new();
            };
            // SAFETY: `this` is the non-null matchmaking interface
            // `interface` returned, which lives for the process; `key` is a
            // live NUL-terminated string. Steam returns null or a
            // NUL-terminated string it keeps.
            let value = unsafe { (self.get)(this, lobby, key.as_ptr()) };
            if value.is_null() {
                return String::new();
            }
            // SAFETY: `value` is non-null, checked by the guard before it, and
            // is the NUL-terminated string Steam keeps until its next lobby
            // call, which this thread does not make before the copy below.
            unsafe { CStr::from_ptr(value) }
                .to_string_lossy()
                .into_owned()
        }

        /// Sets the lobby's value under `key`; false when Steam refuses, as
        /// for a lobby the client does not own.
        fn set(&self, lobby: u64, key: &str, value: &str) -> bool {
            let (Some(this), Ok(key), Ok(value)) =
                (self.interface(), CString::new(key), CString::new(value))
            else {
                return false;
            };
            // SAFETY: `this` is the non-null matchmaking interface
            // `interface` returned, which lives for the process; `key` and
            // `value` are live NUL-terminated strings Steam copies.
            unsafe { (self.set)(this, lobby, key.as_ptr(), value.as_ptr()) != 0 }
        }
    }

    /// Whether the game's Steam library has the lobby calls, so a hosted
    /// game and the friends who join it learn each other's mods; false
    /// until [`register`] found them.
    pub(crate) fn discovery_on() -> bool {
        STATE.api.get().is_some()
    }

    /// Whether the lobby the client entered last names a host with
    /// Enshwick, which takes the client's hello.
    pub(crate) fn host_runs_enshwick() -> bool {
        STATE.host_runs()
    }

    /// Puts the hosting client's data into its lobby once per lobby; on the
    /// client's thread that sends chat, every frame.
    pub(crate) fn poll() {
        let lobby = STATE.lobby.load(Ordering::Acquire);
        if lobby == 0
            || STATE.published.load(Ordering::Relaxed) == lobby
            || !crate::engine::world::hosting()
        {
            return;
        }
        let _ran = LOBBY_FUSE.run("the lobby data", || publish(lobby));
    }

    #[cold]
    fn publish(lobby: u64) {
        STATE.published.store(lobby, Ordering::Relaxed);
        let Some(api) = STATE.api.get() else {
            return;
        };
        let mods = crate::base::advertise::mod_list(&crate::base::join::client::join_mods());
        if mods.len() >= super::MAX_VALUE {
            crate::log!(
                WARN,
                "hosted games: the join mods are too long for the lobby, friends of this host get no join values"
            );
            return;
        }
        let set = api.set(lobby, super::KEY_MODS, &mods)
            && api.set(lobby, super::KEY, env!("CARGO_PKG_VERSION"));
        if set {
            crate::log!(
                INFO,
                "hosted games: lobby {lobby} names Enshwick, mods {mods}"
            );
        } else {
            crate::log!(
                WARN,
                "hosted games: Steam refused the data of lobby {lobby}"
            );
        }
    }

    /// The client entered `lobby`, on the game's thread.
    fn entered(lobby: u64) {
        STATE.lobby.store(lobby, Ordering::Release);
        if crate::engine::world::hosting() {
            let _own = STATE.note_entry(lobby, false);
            return;
        }
        let Some(api) = STATE.api.get() else {
            return;
        };
        let version = api.get(lobby, super::KEY);
        // A dedicated server's lobby names nothing: its rules went through
        // the join check before the game connected.
        if !STATE.note_entry(lobby, !version.is_empty()) {
            return;
        }
        let host = api.get(lobby, super::KEY_MODS);
        crate::log!(
            INFO,
            "joining a game hosted with Enshwick {version}: mods {host}"
        );
        crate::base::net::joining(false);
        crate::base::join::client::forget_server_values();
        crate::base::join::client::back_on();
        let ours = crate::base::advertise::mod_list(&crate::base::join::client::join_mods());
        let names = crate::base::join::names();
        let (turn_off, note) = super::plan(&host, &ours, &names);
        for id in &turn_off {
            crate::base::join::client::turn_off_for_world(id);
        }
        if let Some(text) = note {
            crate::ui::interface::show("Enshwick", &text, "", 1 << UiDialog::OK, |_| {});
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_panicking_lobby_entry_turns_the_hosted_lobby_off_and_lists_it() {
            enter(7, |_| panic!("a broken entry"));
            assert!(LOBBY_FUSE.is_off());
            assert_eq!(
                crate::base::off_reason(&LOBBY_FUSE).as_deref(),
                Some("the loader panicked in a lobby entry: a broken entry")
            );
        }

        #[test]
        fn a_hosted_game_s_end_forgets_its_lobby_and_keeps_what_the_next_entry_sets() {
            let state = State::new();
            state.lobby.store(7, Ordering::Release);
            assert!(state.note_entry(8, true));
            state.published.store(7, Ordering::Relaxed);
            state.forget();
            assert_eq!(state.lobby.load(Ordering::Acquire), 0);
            assert_eq!(state.published.load(Ordering::Relaxed), 0);
            assert!(
                !state.note_entry(8, true),
                "a second notice of an entry turns nothing off twice"
            );
            assert!(state.host_runs(), "an entry before the end keeps its host");
        }

        #[test]
        fn a_join_ends_the_last_lobby_s_host_and_a_later_entry_sets_it_again() {
            let state = State::new();
            assert!(!state.host_runs());
            assert!(state.note_entry(7, true));
            assert!(state.host_runs());
            state.left();
            assert!(
                !state.host_runs(),
                "a vanilla server joined next gets no hello before its own lobby"
            );
            assert!(!state.note_entry(9, false), "a dedicated server's lobby");
            assert!(!state.host_runs());
            state.left();
            assert!(state.note_entry(7, true));
            assert!(state.host_runs());
        }

        #[test]
        fn entering_the_same_lobby_again_after_a_join_checks_it_again_but_a_second_notice_does_not()
        {
            let state = State::new();
            assert!(state.note_entry(7, true));
            assert!(!state.note_entry(7, true), "Steam's second notice");
            state.left();
            assert!(state.note_entry(7, true), "the same lobby entered again");
            assert!(!state.note_entry(7, true));
        }

        #[test]
        fn a_steam_library_without_the_lobby_api_lists_the_hosted_lobby_off() {
            register();
            assert!(LOBBY_FUSE.is_off());
            assert_eq!(
                crate::base::off_reason(&LOBBY_FUSE).as_deref(),
                Some("steam_api64.dll lacks the lobby API")
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_with_the_same_mods_needs_nothing() {
        let names = crate::base::join::Names::new();
        assert_eq!(plan("a@1.0,b@2.0", "a@1.0,b@2.0", &names), (vec![], None));
    }

    #[test]
    fn mods_the_host_lacks_turn_off_and_other_versions_only_warn() {
        let mut names = crate::base::join::Names::new();
        names.insert("b".into(), "Bee".into());
        let (off, note) = plan("a@1.0,c@3.0", "a@2.0,b@1.0", &names);
        assert_eq!(off, vec!["b".to_owned()]);
        let note = note.unwrap();
        assert!(note.contains("Bee") && !note.contains("(b)"), "{note}");
        assert!(note.contains("a: server 1.0, yours 2.0"), "{note}");
        assert!(note.contains("c: server 3.0, not installed"), "{note}");
    }

    #[test]
    fn a_host_s_mod_in_another_version_is_not_turned_off() {
        let names = crate::base::join::Names::new();
        let (off, note) = plan("a@1.0", "a@2.0", &names);
        assert!(off.is_empty());
        assert!(note.is_some());
    }
}
