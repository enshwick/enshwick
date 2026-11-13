//! The server tells the server browser which mod set it runs.
//!
//! Once the dedicated server has logged on to Steam, the loader appends
//! `[mods: <count of mods of side both>]` to its Steam name and sets the
//! rules `enshwick` (the fingerprint) and `enshwick.mods`, `enshwick.mods.1`,
//! ... (the mods that count, `id@version`, split to fit Steam's rule length).
//! It advertises no homepage or name: a client shows a mod by id and version
//! alone, so a server points no player anywhere.
//! Players see it before they join, with or without mods, and a modded client
//! compares it with its own. The game sets its name once at start, so the
//! loader sets it again now and then.

/// Steam keeps at most 63 bytes of a server name.
const NAME_LIMIT: usize = 63;
/// Steam keeps at most 127 bytes of a rule value.
const RULE_LIMIT: usize = 127;

/// The rule that carries the fingerprint.
pub const RULE: &str = "enshwick";
/// The rule that lists the mods; further parts are `enshwick.mods.1` and on.
pub const RULE_MODS: &str = "enshwick.mods";
/// The most rules the mod list spreads over, about 4 KiB: a reply the query
/// splits is put back together from up to 16 packets.
pub const MOD_RULES: usize = 32;
/// The rule that gives the number of mods when the list did not fit in
/// [`MOD_RULES`] rules, so a client knows it reads only part of it.
pub const RULE_MOD_COUNT: &str = "enshwick.mods.count";

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
    format!(
        "{:08x}",
        enshwick_gamedata::reflect::fnv1a32(text.as_bytes())
    )
}

/// What a server advertises at a moment: its Steam name, the fingerprint and
/// every rule.
#[derive(Debug, PartialEq, Eq)]
pub struct Advert {
    /// The server name with the fingerprint.
    pub name: String,
    /// The fingerprint, the value of [`RULE`].
    pub fingerprint: String,
    /// The mod list rules.
    pub rules: Vec<(String, String)>,
    /// Mods left out of the mod list rules, which hold at most
    /// [`MOD_RULES`]; they still count in the fingerprint.
    pub mods_cut: usize,
}

impl Advert {
    /// The advert of a server named `configured` running `mods`.
    #[must_use]
    pub fn new(loader_version: &str, configured: &str, mods: &[(String, String)]) -> Self {
        let (mut rules, mods_cut) = mod_rules(mods);
        if mods_cut > 0 {
            rules.push((RULE_MOD_COUNT.to_owned(), mods.len().to_string()));
        }
        Self {
            name: server_name(configured, mods.len()),
            fingerprint: fingerprint(loader_version, mods),
            rules,
            mods_cut,
        }
    }
}

/// The name of part `index` of the rules named `base`.
#[must_use]
pub fn part_name(base: &str, index: usize) -> String {
    if index == 0 {
        base.to_owned()
    } else {
        format!("{base}.{index}")
    }
}

/// The whole mod list, `id@version` sorted and joined by commas.
#[must_use]
pub fn mod_list(mods: &[(String, String)]) -> String {
    let mut entries: Vec<String> = mods
        .iter()
        .map(|(id, version)| format!("{id}@{version}"))
        .collect();
    entries.sort();
    entries.join(",")
}

/// The mod list as rules, each value within Steam's limit:
/// `enshwick.mods`, then `enshwick.mods.1` and on, at most [`MOD_RULES`];
/// and the number of mods that did not fit, the last in sorted order.
#[must_use]
pub fn mod_rules(mods: &[(String, String)]) -> (Vec<(String, String)>, usize) {
    let mut parts: Vec<String> = vec![String::new()];
    let mut cut = 0;
    for entry in mod_list(mods).split(',').filter(|e| !e.is_empty()) {
        let Some(last) = parts.last_mut() else {
            break;
        };
        if cut > 0 || (!last.is_empty() && last.len() + 1 + entry.len() > RULE_LIMIT) {
            if cut > 0 || parts.len() == MOD_RULES {
                cut += 1;
                continue;
            }
            parts.push(String::new());
        }
        if let Some(last) = parts.last_mut() {
            if !last.is_empty() {
                last.push(',');
            }
            last.push_str(entry);
        }
    }
    let rules = parts
        .into_iter()
        .enumerate()
        .map(|(i, value)| (rule_name(i), value))
        .collect();
    (rules, cut)
}

/// The name of mod list rule `index`.
#[must_use]
pub fn rule_name(index: usize) -> String {
    part_name(RULE_MODS, index)
}

/// `name` with the count of its join mods appended, such as `[mods: 6]`,
/// shortened to fit Steam's limit; a suffix the loader appended before is
/// replaced.
#[must_use]
pub fn server_name(name: &str, mods: usize) -> String {
    let suffix = format!(" [mods: {mods}]");
    let base = name.trim();
    let base = [" [mods: ", " [enshwick "]
        .iter()
        .filter_map(|mark| base.find(mark))
        .min()
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
pub(crate) use runtime::{start, wake};

#[cfg(windows)]
mod runtime {
    use std::ffi::{CString, c_void};
    use std::path::Path;
    use std::time::{Duration, Instant};

    use crate::engine::fuse::{Fuse, Off};
    use crate::engine::module::Export;

    /// The mod set a dedicated server advertises to Steam, for the join
    /// check of clients that query it.
    static ADVERTISE_FUSE: Fuse = Fuse::new("mod set advertisement");

    type Interface = unsafe extern "C" fn() -> *mut c_void;
    /// Returns a C++ `bool`, read as its byte so no other value can make an
    /// invalid Rust `bool`.
    type LoggedOn = unsafe extern "C" fn(server: *mut c_void) -> u8;
    type SetName = unsafe extern "C" fn(server: *mut c_void, name: *const i8);
    type SetKeyValue = unsafe extern "C" fn(server: *mut c_void, key: *const i8, value: *const i8);

    const POLL: Duration = Duration::from_secs(2);
    const GIVE_UP: Duration = Duration::from_secs(300);
    const REASSERT: Duration = Duration::from_secs(60);

    /// The running join mods, `(id, version)`, read each round: a mod the
    /// server turns off stops being advertised.
    pub(crate) type JoinMods = fn() -> Vec<(String, String)>;

    /// The advertising thread, once it runs.
    static THREAD: std::sync::OnceLock<std::thread::Thread> = std::sync::OnceLock::new();

    /// Sets the advertisement again now, not at the next round: the join
    /// mods changed, and a player who checks before the round would read
    /// the old ones.
    pub(crate) fn wake() {
        if let Some(thread) = THREAD.get() {
            thread.unpark();
        }
    }

    /// Starts the thread that advertises the mod set.
    pub(crate) fn start(game_dir: &Path, mods: JoinMods) {
        let config = std::fs::read_to_string(game_dir.join("enshrouded_server.json")).ok();
        let configured = config
            .and_then(|json| super::configured_name(&json))
            .unwrap_or_else(|| "Enshrouded Server".to_owned());
        let spawned = std::thread::Builder::new()
            .name("enshwick-advertise".into())
            .spawn(move || {
                let _first = THREAD.set(std::thread::current());
                advertise(&configured, mods, run);
            });
        drop(spawned);
    }

    /// Runs `run`, the advertising thread's work, under the advertisement's
    /// fuse.
    fn advertise(configured: &str, mods: JoinMods, run: fn(&str, JoinMods)) {
        let _ran = ADVERTISE_FUSE.run("the advertising thread", || run(configured, mods));
    }

    /// Turns the advertisement off because `why`.
    #[cold]
    fn off(why: &str) {
        ADVERTISE_FUSE.turn_off(&Off::reported(
            format!("{} off: {why}", ADVERTISE_FUSE.feature()),
            why.to_owned(),
        ));
    }

    fn run(configured: &str, mods: JoinMods) {
        let Some(api) = Api::load() else {
            off("steam_api64.dll lacks the game server API");
            return;
        };
        let started = Instant::now();
        let server = loop {
            // SAFETY: `api.interface` is `steam_api64.dll`'s export
            // `SteamAPI_SteamGameServer_v015`, typed by the flat API, which
            // takes no arguments and returns null before the server starts.
            let server = unsafe { (api.interface)() };
            // SAFETY: `server` is non-null, checked first, and is the game
            // server interface the flat API handed out, which lives for the
            // process; `api.logged_on` is its `BLoggedOn` export.
            if !server.is_null() && unsafe { (api.logged_on)(server) } != 0 {
                break server;
            }
            if started.elapsed() > GIVE_UP {
                off("the server never logged on to Steam");
                return;
            }
            std::thread::sleep(POLL);
        };
        let mut last: Option<super::Advert> = None;
        loop {
            let advert = super::Advert::new(env!("CARGO_PKG_VERSION"), configured, &mods());
            let mut stale = Vec::new();
            if last.as_ref() != Some(&advert) {
                describe(&advert);
                // Rules no longer advertised are emptied, which reads as absent.
                stale = last
                    .iter()
                    .flat_map(|old| &old.rules)
                    .filter(|(key, _)| advert.rules.iter().all(|(k, _)| k != key))
                    .map(|(key, _)| (key.clone(), String::new()))
                    .collect();
            }
            let pairs = [(super::RULE.to_owned(), advert.fingerprint.clone())]
                .into_iter()
                .chain(stale)
                .chain(advert.rules.iter().cloned());
            if let Ok(name) = CString::new(advert.name.as_str()) {
                // SAFETY: `server` is the non-null logged-on interface from
                // the loop above, which lives for the process;
                // `api.set_name` is its `SetServerName` export, and `name` is
                // a live NUL-terminated string the call copies.
                unsafe { (api.set_name)(server, name.as_ptr()) };
            }
            for (key, value) in pairs {
                let (Ok(key), Ok(value)) = (CString::new(key), CString::new(value)) else {
                    continue;
                };
                // SAFETY: `server` is the non-null logged-on interface from
                // the loop above, which lives for the process;
                // `api.set_key_value` is its `SetKeyValue` export, and `key`
                // and `value` are live NUL-terminated strings the call copies.
                unsafe { (api.set_key_value)(server, key.as_ptr(), value.as_ptr()) };
            }
            last = Some(advert);
            std::thread::park_timeout(REASSERT);
        }
    }

    /// Logs what the server now advertises.
    fn describe(advert: &super::Advert) {
        let mods: Vec<&str> = advert
            .rules
            .iter()
            .filter(|(key, _)| key.starts_with(super::RULE_MODS) && key != super::RULE_MOD_COUNT)
            .map(|(_, value)| value.as_str())
            .collect();
        crate::log!(
            INFO,
            "advertising \"{}\", mods {}",
            advert.name,
            mods.join(",")
        );
        if advert.mods_cut > 0 {
            crate::log!(
                WARN,
                "{} mods do not fit in the advertised mod list; joining players compare only the listed ones",
                advert.mods_cut
            );
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
            let module = crate::engine::module::Module::loaded("steam_api64.dll")?;
            let get = |name: &str| module.export(name);
            let interface = get("SteamAPI_SteamGameServer_v015")?;
            let logged_on = get("SteamAPI_ISteamGameServer_BLoggedOn")?;
            let set_name = get("SteamAPI_ISteamGameServer_SetServerName")?;
            let set_key_value = get("SteamAPI_ISteamGameServer_SetKeyValue")?;
            // SAFETY: the Steamworks flat API declares
            // `SteamAPI_SteamGameServer_v015` as `Interface`.
            let interface = unsafe { core::mem::transmute::<Export, Interface>(interface) };
            // SAFETY: the Steamworks flat API declares
            // `SteamAPI_ISteamGameServer_BLoggedOn` as `LoggedOn`.
            let logged_on = unsafe { core::mem::transmute::<Export, LoggedOn>(logged_on) };
            // SAFETY: the Steamworks flat API declares
            // `SteamAPI_ISteamGameServer_SetServerName` as `SetName`.
            let set_name = unsafe { core::mem::transmute::<Export, SetName>(set_name) };
            // SAFETY: the Steamworks flat API declares
            // `SteamAPI_ISteamGameServer_SetKeyValue` as `SetKeyValue`.
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

    #[cfg(test)]
    mod tests {
        use super::*;

        fn none() -> Vec<(String, String)> {
            Vec::new()
        }

        fn listed() -> Option<String> {
            let off = crate::engine::diag::parts_off();
            off.into_iter()
                .find(|p| p.part == ADVERTISE_FUSE.feature())
                .map(|p| p.reason)
        }

        #[test]
        fn a_panicking_advertisement_turns_off_only_its_feature_and_lists_it() {
            advertise("Lanterns", none, |_, _| panic!("a broken round"));
            assert!(ADVERTISE_FUSE.is_off());
            assert!(!crate::base::carrier::CHANNEL_FUSE.is_off());
            assert_eq!(
                listed().as_deref(),
                Some("the loader panicked in the advertising thread: a broken round")
            );
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
    fn the_advert_lists_the_mods_and_no_settings() {
        let advert = Advert::new("0.1.0", "Homelab", &mods());
        assert_eq!(
            advert.rules,
            [(
                "enshwick.mods".to_owned(),
                "count@0.2.0,zones@0.1.0".to_owned()
            )]
        );
        assert_eq!(advert.name, format!("Homelab [mods: {}]", mods().len()));
        assert_eq!(advert.fingerprint, fingerprint("0.1.0", &mods()));
    }

    #[test]
    fn the_name_counts_the_mods_once_and_fits() {
        assert_eq!(server_name("Homelab", 6), "Homelab [mods: 6]");
        assert_eq!(server_name("Homelab [mods: 2]", 6), "Homelab [mods: 6]");
        assert_eq!(
            server_name("Homelab [enshwick 0a1b2c3d]", 6),
            "Homelab [mods: 6]"
        );
        let long = server_name(&"ї".repeat(40), 12);
        assert!(long.len() <= NAME_LIMIT && long.ends_with("ї [mods: 12]"));
        let advert = Advert::new("0.1.0", "Homelab", &mods());
        assert_eq!(advert.name, format!("Homelab [mods: {}]", mods().len()));
    }

    #[test]
    fn the_mod_list_is_sorted_and_spread_over_rules_that_fit() {
        assert_eq!(mod_list(&mods()), "count@0.2.0,zones@0.1.0");
        assert_eq!(
            mod_rules(&mods()).0,
            [(
                "enshwick.mods".to_owned(),
                "count@0.2.0,zones@0.1.0".to_owned()
            )]
        );
        let many: Vec<(String, String)> = (0..40)
            .map(|i| (format!("mod-{i:02}"), "1.0.0".into()))
            .collect();
        let (rules, cut) = mod_rules(&many);
        assert_eq!(cut, 0);
        assert!(rules.len() > 1);
        assert!(rules.iter().all(|(_, value)| value.len() <= RULE_LIMIT));
        assert_eq!(rules[1].0, "enshwick.mods.1");
        let joined: Vec<&str> = rules.iter().map(|(_, v)| v.as_str()).collect();
        assert_eq!(joined.join(","), mod_list(&many));
    }

    #[test]
    fn a_mod_list_too_long_for_its_rules_is_cut_and_counted() {
        let many: Vec<(String, String)> = (0..400)
            .map(|i| (format!("a-long-mod-name-{i:03}"), "1.0.0".into()))
            .collect();
        let (rules, cut) = mod_rules(&many);
        assert_eq!(rules.len(), MOD_RULES);
        let listed = rules
            .iter()
            .map(|(_, v)| v.split(',').count())
            .sum::<usize>();
        assert_eq!(listed + cut, many.len());
        assert!(cut > 0);
        let joined: Vec<&str> = rules.iter().map(|(_, v)| v.as_str()).collect();
        assert!(mod_list(&many).starts_with(&joined.join(",")));
        let advert = Advert::new("0.1.0", "Homelab", &many);
        assert_eq!(advert.mods_cut, cut);
        assert!(
            advert
                .rules
                .contains(&(RULE_MOD_COUNT.to_owned(), "400".to_owned()))
        );
        let small = Advert::new("0.1.0", "Homelab", &mods());
        assert_eq!(small.mods_cut, 0);
        assert!(small.rules.iter().all(|(key, _)| key != RULE_MOD_COUNT));
    }

    #[test]
    fn the_name_is_read_from_the_server_config() {
        let json = "{\n\t\"name\": \"Our \\\"Base\\\"\",\n\t\"saveDirectory\": \"./savegame\"\n}";
        assert_eq!(configured_name(json).as_deref(), Some("Our \"Base\""));
        assert_eq!(configured_name("{}"), None);
    }
}
