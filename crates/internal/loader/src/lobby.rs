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
    names: &crate::join::Names,
) -> (Vec<String>, Option<String>) {
    let differences = crate::join::differences(host, ours, names);
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
            .map(|id| enshwick::mods::label(names.get(id).map_or("", String::as_str), id))
            .collect();
        parts.push(format!(
            "The host of this game does not run these Enshwick mods, so they are off until you restart the game:\n\n{}",
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
pub(crate) use runtime::{host_runs_enshwick, poll, register};

#[cfg(windows)]
mod runtime {
    use core::ffi::{CStr, c_char, c_void};
    use std::collections::BTreeMap;
    use std::ffi::CString;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    use enshwick::abi::UiDialog;
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};

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

    type Export = unsafe extern "system" fn() -> isize;
    type Matchmaking = unsafe extern "C" fn() -> *mut c_void;
    type GetLobbyData =
        unsafe extern "C" fn(this: *mut c_void, lobby: u64, key: *const c_char) -> *const c_char;
    type SetLobbyData = unsafe extern "C" fn(
        this: *mut c_void,
        lobby: u64,
        key: *const c_char,
        value: *const c_char,
    ) -> bool;
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

    static API: OnceLock<Api> = OnceLock::new();
    /// The lobby the client last entered, 0 for none.
    static LOBBY: AtomicU64 = AtomicU64::new(0);
    /// The lobby whose entry was handled, so a second notice of the same
    /// entry turns nothing off twice.
    static HANDLED: AtomicU64 = AtomicU64::new(0);
    /// The lobby the hosting client put its data into.
    static PUBLISHED: AtomicU64 = AtomicU64::new(0);
    /// Set while the lobby the client entered names a host with Enshwick.
    static HOST_RUNS: AtomicBool = AtomicBool::new(false);

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

    unsafe extern "C" fn run(_callback: *mut Callback, data: *const u8) {
        if data.is_null() {
            return;
        }
        // SAFETY: Steam hands a `LobbyEnter_t`, `LOBBY_ENTER_SIZE` bytes.
        let record = unsafe { data.cast::<[u8; LOBBY_ENTER_BYTES]>().read() };
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
        if std::panic::catch_unwind(|| entered(lobby)).is_err() {
            crate::log!(WARN, "hosted games: a lobby entry panicked");
        }
    }

    unsafe extern "C" fn size(_callback: *mut Callback) -> i32 {
        LOBBY_ENTER_SIZE
    }

    /// Asks Steam to tell the loader of each lobby the client enters, on the
    /// game's thread that runs Steam's callbacks.
    pub(crate) fn register() {
        let Some((api, register)) = load() else {
            crate::log!(
                WARN,
                "hosted games: steam_api64.dll lacks the lobby API, friends of a host get no join values"
            );
            return;
        };
        if API.set(api).is_err() {
            return;
        }
        let callback = Box::leak(Box::new(Callback {
            table: &TABLE,
            flags: 0,
            id: 0,
        }));
        // SAFETY: a callback object that lives for the process, laid out as
        // Steam's; Steam writes its flags and id.
        unsafe { register(&raw mut *callback, LOBBY_ENTER) };
    }

    fn load() -> Option<(Api, RegisterCallback)> {
        let module_name: Vec<u16> = "steam_api64.dll".encode_utf16().chain([0]).collect();
        // SAFETY: a NUL-terminated module name.
        let module = unsafe { GetModuleHandleW(module_name.as_ptr()) };
        if module.is_null() {
            return None;
        }
        let get = |name: &str| {
            let name = CString::new(name).ok()?;
            // SAFETY: a loaded module and a NUL-terminated name.
            unsafe { GetProcAddress(module, name.as_ptr().cast()) }
        };
        let matchmaking = get("SteamAPI_SteamMatchmaking_v009")?;
        let get_data = get("SteamAPI_ISteamMatchmaking_GetLobbyData")?;
        let set_data = get("SteamAPI_ISteamMatchmaking_SetLobbyData")?;
        let register = get("SteamAPI_RegisterCallback")?;
        // SAFETY: each export has the flat API's signature for its name.
        let matchmaking = unsafe { core::mem::transmute::<Export, Matchmaking>(matchmaking) };
        // SAFETY: as above.
        let get_data = unsafe { core::mem::transmute::<Export, GetLobbyData>(get_data) };
        // SAFETY: as above.
        let set_data = unsafe { core::mem::transmute::<Export, SetLobbyData>(set_data) };
        // SAFETY: as above.
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
            // SAFETY: the flat API's accessor takes no arguments; null
            // before Steam starts.
            let this = unsafe { (self.matchmaking)() };
            (!this.is_null()).then_some(this)
        }

        /// The lobby's value under `key`, empty when it has none.
        fn get(&self, lobby: u64, key: &str) -> String {
            let (Some(this), Ok(key)) = (self.interface(), CString::new(key)) else {
                return String::new();
            };
            // SAFETY: the interface, a lobby id and a NUL-terminated key;
            // Steam returns a NUL-terminated string it keeps, or null.
            let value = unsafe { (self.get)(this, lobby, key.as_ptr()) };
            if value.is_null() {
                return String::new();
            }
            // SAFETY: as above.
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
            // SAFETY: the interface, a lobby id and NUL-terminated strings
            // Steam copies.
            unsafe { (self.set)(this, lobby, key.as_ptr(), value.as_ptr()) }
        }
    }

    /// Whether the lobby the client entered last names a host with
    /// Enshwick, which takes the client's hello.
    pub(crate) fn host_runs_enshwick() -> bool {
        HOST_RUNS.load(Ordering::Acquire)
    }

    /// Puts the hosting client's data into its lobby once per lobby; on the
    /// client's thread that sends chat, every frame.
    pub(crate) fn poll() {
        let lobby = LOBBY.load(Ordering::Acquire);
        if lobby == 0 || PUBLISHED.load(Ordering::Relaxed) == lobby || !crate::world::hosting() {
            return;
        }
        publish(lobby);
    }

    #[cold]
    fn publish(lobby: u64) {
        PUBLISHED.store(lobby, Ordering::Relaxed);
        let Some(api) = API.get() else {
            return;
        };
        let mods = crate::advertise::mod_list(&crate::host::join_mods());
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
        LOBBY.store(lobby, Ordering::Release);
        if crate::world::hosting() {
            HOST_RUNS.store(false, Ordering::Release);
            return;
        }
        let Some(api) = API.get() else {
            return;
        };
        let version = api.get(lobby, super::KEY);
        // A dedicated server's lobby names nothing: its rules went through
        // the join check before the game connected.
        HOST_RUNS.store(!version.is_empty(), Ordering::Release);
        if version.is_empty() || HANDLED.swap(lobby, Ordering::AcqRel) == lobby {
            return;
        }
        let host = api.get(lobby, super::KEY_MODS);
        crate::log!(
            INFO,
            "joining a game hosted with Enshwick {version}: mods {host}"
        );
        crate::net::joining(false);
        crate::host::apply_join_values(&BTreeMap::new());
        let ours = crate::advertise::mod_list(&crate::host::join_mods());
        let names = crate::join::names();
        let (turn_off, note) = super::plan(&host, &ours, &names);
        for id in &turn_off {
            crate::host::turn_off_mod(
                id,
                "the host does not run it; restart the game to use it again",
            );
            crate::reload::hold(id);
        }
        if let Some(text) = note {
            crate::ui::show("Enshwick", &text, "", 1 << UiDialog::OK, |_| {});
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_with_the_same_mods_needs_nothing() {
        let names = crate::join::Names::new();
        assert_eq!(plan("a@1.0,b@2.0", "a@1.0,b@2.0", &names), (vec![], None));
    }

    #[test]
    fn mods_the_host_lacks_turn_off_and_other_versions_only_warn() {
        let mut names = crate::join::Names::new();
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
        let names = crate::join::Names::new();
        let (off, note) = plan("a@1.0", "a@2.0", &names);
        assert!(off.is_empty());
        assert!(note.is_some());
    }
}
