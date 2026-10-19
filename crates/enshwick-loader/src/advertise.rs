//! The server tells the server browser which mod set it runs.
//!
//! Once the dedicated server has logged on to Steam, the loader appends
//! `[enshwick <fingerprint>]` to its Steam name and sets the rules `enshwick`
//! (the fingerprint) and `enshwick.mods` (the mods that count, `id@version`).
//! Players see it before they join, with or without mods, and a modded client
//! compares it with its own. The game sets its name once at start, so the
//! loader sets it again now and then.

/// Steam keeps at most 63 bytes of a server name.
const NAME_LIMIT: usize = 63;
/// Steam keeps at most 127 bytes of a rule value.
const RULE_LIMIT: usize = 127;

/// The rule that carries the fingerprint.
pub const RULE: &str = "enshwick";
/// The rule that lists the mods.
pub const RULE_MODS: &str = "enshwick.mods";

/// The fingerprint of a mod set: FNV-1a over the loader version and every
/// `id@version`, sorted, as eight hex digits.
#[must_use]
pub fn fingerprint(loader_version: &str, mods: &[(String, String)]) -> String {
    let mut entries: Vec<String> = mods
        .iter()
        .map(|(id, version)| format!("{id}@{version}"))
        .collect();
    entries.sort();
    let text = format!("enshwick@{loader_version}\n{}", entries.join("\n"));
    format!("{:08x}", enshwick_reflect::fnv1a32(text.as_bytes()))
}

/// The mod list for the `enshwick.mods` rule, cut to Steam's limit.
#[must_use]
pub fn mod_list(mods: &[(String, String)]) -> String {
    let mut entries: Vec<String> = mods
        .iter()
        .map(|(id, version)| format!("{id}@{version}"))
        .collect();
    entries.sort();
    let mut list = String::new();
    for entry in entries {
        let extra = usize::from(!list.is_empty()) + entry.len();
        if list.len() + extra > RULE_LIMIT {
            break;
        }
        if !list.is_empty() {
            list.push(',');
        }
        list.push_str(&entry);
    }
    list
}

/// `name` with the fingerprint appended, shortened to fit Steam's limit.
#[must_use]
pub fn server_name(name: &str, fingerprint: &str) -> String {
    let suffix = format!(" [enshwick {fingerprint}]");
    let base = name.trim();
    let base = base
        .find(" [enshwick ")
        .map_or(base, |at| base[..at].trim_end());
    let room = NAME_LIMIT.saturating_sub(suffix.len());
    let mut cut = base.len().min(room);
    while !base.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}{suffix}", base[..cut].trim_end())
}

/// The `"name"` of an `enshrouded_server.json`.
#[must_use]
pub fn configured_name(json: &str) -> Option<String> {
    let start = json.find("\"name\"")? + "\"name\"".len();
    let rest = json[start..]
        .trim_start()
        .strip_prefix(':')?
        .trim_start()
        .strip_prefix('"')?;
    let mut name = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(name),
            '\\' => match chars.next()? {
                'n' => name.push('\n'),
                't' => name.push('\t'),
                other => name.push(other),
            },
            c => name.push(c),
        }
    }
    None
}

#[cfg(windows)]
pub(crate) use runtime::start;

#[cfg(windows)]
mod runtime {
    use std::ffi::{CString, c_void};
    use std::path::Path;
    use std::time::{Duration, Instant};

    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};

    type Interface = unsafe extern "C" fn() -> *mut c_void;
    type LoggedOn = unsafe extern "C" fn(server: *mut c_void) -> bool;
    type SetName = unsafe extern "C" fn(server: *mut c_void, name: *const i8);
    type SetKeyValue = unsafe extern "C" fn(server: *mut c_void, key: *const i8, value: *const i8);

    type Export = unsafe extern "system" fn() -> isize;

    const POLL: Duration = Duration::from_secs(2);
    const GIVE_UP: Duration = Duration::from_secs(300);
    const REASSERT: Duration = Duration::from_secs(60);

    /// Starts the thread that advertises the mod set.
    pub(crate) fn start(game_dir: &Path, mods: &[(String, String)]) {
        let config = std::fs::read_to_string(game_dir.join("enshrouded_server.json")).ok();
        let configured = config
            .and_then(|json| super::configured_name(&json))
            .unwrap_or_else(|| "Enshrouded Server".to_owned());
        let fingerprint = super::fingerprint(env!("CARGO_PKG_VERSION"), mods);
        let name = super::server_name(&configured, &fingerprint);
        let list = super::mod_list(mods);
        let spawned = std::thread::Builder::new()
            .name("enshwick-advertise".into())
            .spawn(move || {
                if std::panic::catch_unwind(|| run(&name, &fingerprint, &list)).is_err() {
                    crate::log!(WARN, "mod set advertisement stopped: it panicked");
                }
            });
        drop(spawned);
    }

    fn run(name: &str, fingerprint: &str, list: &str) {
        let Some(api) = Api::load() else {
            crate::log!(
                WARN,
                "mod set advertisement off: steam_api64.dll lacks the game server API"
            );
            return;
        };
        let (Ok(name), Ok(rule), Ok(value), Ok(rule_mods), Ok(mods)) = (
            CString::new(name),
            CString::new(super::RULE),
            CString::new(fingerprint),
            CString::new(super::RULE_MODS),
            CString::new(list),
        ) else {
            crate::log!(WARN, "mod set advertisement off: a value holds a NUL");
            return;
        };
        let started = Instant::now();
        let server = loop {
            // SAFETY: the flat API's accessor takes no arguments.
            let server = unsafe { (api.interface)() };
            // SAFETY: a non-null game server interface.
            if !server.is_null() && unsafe { (api.logged_on)(server) } {
                break server;
            }
            if started.elapsed() > GIVE_UP {
                crate::log!(
                    WARN,
                    "mod set advertisement off: the server never logged on to Steam"
                );
                return;
            }
            std::thread::sleep(POLL);
        };
        crate::log!(
            INFO,
            "advertising \"{}\", mods {}",
            name.to_string_lossy(),
            mods.to_string_lossy()
        );
        loop {
            // SAFETY: the logged-on interface and NUL-terminated strings.
            unsafe { (api.set_name)(server, name.as_ptr()) };
            // SAFETY: as above.
            unsafe { (api.set_key_value)(server, rule.as_ptr(), value.as_ptr()) };
            // SAFETY: as above.
            unsafe { (api.set_key_value)(server, rule_mods.as_ptr(), mods.as_ptr()) };
            std::thread::sleep(REASSERT);
        }
    }

    struct Api {
        interface: Interface,
        logged_on: LoggedOn,
        set_name: SetName,
        set_key_value: SetKeyValue,
    }

    impl Api {
        fn load() -> Option<Self> {
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
            let interface = get("SteamAPI_SteamGameServer_v015")?;
            let logged_on = get("SteamAPI_ISteamGameServer_BLoggedOn")?;
            let set_name = get("SteamAPI_ISteamGameServer_SetServerName")?;
            let set_key_value = get("SteamAPI_ISteamGameServer_SetKeyValue")?;
            // SAFETY: each export has the flat API's signature for its name.
            let interface = unsafe { core::mem::transmute::<Export, Interface>(interface) };
            // SAFETY: as above.
            let logged_on = unsafe { core::mem::transmute::<Export, LoggedOn>(logged_on) };
            // SAFETY: as above.
            let set_name = unsafe { core::mem::transmute::<Export, SetName>(set_name) };
            // SAFETY: as above.
            let set_key_value =
                unsafe { core::mem::transmute::<Export, SetKeyValue>(set_key_value) };
            Some(Self {
                interface,
                logged_on,
                set_name,
                set_key_value,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mods() -> Vec<(String, String)> {
        vec![
            ("zones".into(), "0.1.0".into()),
            ("count".into(), "0.2.0".into()),
        ]
    }

    #[test]
    fn the_fingerprint_ignores_order_and_follows_versions() {
        let mut reversed = mods();
        reversed.reverse();
        assert_eq!(
            fingerprint("0.1.0", &mods()),
            fingerprint("0.1.0", &reversed)
        );
        assert_ne!(fingerprint("0.1.0", &mods()), fingerprint("0.1.1", &mods()));
        assert_eq!(fingerprint("0.1.0", &mods()).len(), 8);
    }

    #[test]
    fn the_name_carries_the_fingerprint_once_and_fits() {
        assert_eq!(
            server_name("Homelab", "0a1b2c3d"),
            "Homelab [enshwick 0a1b2c3d]"
        );
        assert_eq!(
            server_name("Homelab [enshwick 00000000]", "0a1b2c3d"),
            "Homelab [enshwick 0a1b2c3d]"
        );
        let long = server_name(&"ї".repeat(40), "0a1b2c3d");
        assert!(long.len() <= NAME_LIMIT && long.ends_with(" [enshwick 0a1b2c3d]"));
    }

    #[test]
    fn the_mod_list_is_sorted_and_fits() {
        assert_eq!(mod_list(&mods()), "count@0.2.0,zones@0.1.0");
        let many: Vec<(String, String)> = (0..40)
            .map(|i| (format!("mod-{i:02}"), "1.0.0".into()))
            .collect();
        assert!(mod_list(&many).len() <= RULE_LIMIT);
    }

    #[test]
    fn the_name_is_read_from_the_server_config() {
        let json = "{\n\t\"name\": \"Our \\\"Base\\\"\",\n\t\"saveDirectory\": \"./savegame\"\n}";
        assert_eq!(configured_name(json).as_deref(), Some("Our \"Base\""));
        assert_eq!(configured_name("{}"), None);
    }
}
