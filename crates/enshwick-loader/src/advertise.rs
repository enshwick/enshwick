//! The server tells the server browser which mod set it runs.
//!
//! Once the dedicated server has logged on to Steam, the loader appends
//! `[enshwick <fingerprint>]` to its Steam name and sets the rules `enshwick`
//! (the fingerprint) and `enshwick.mods`, `enshwick.mods.1`, ... (the mods that
//! count, `id@version`, split to fit Steam's rule length), and for each join
//! mod with join values `enshwick.sync.<id>`, `enshwick.sync.<id>.1`, ...
//! (its values as an inline TOML table, split the same way), and for each join
//! mod with an update source `enshwick.src.<id>` (`github:<owner/name>:<component>`
//! or `url:<url>`), so a client that lacks the mod knows where to get it.
//! Players see it before they join, with or without mods, and a modded client
//! compares it with its own. The game sets its name once at start, so the
//! loader sets it again now and then, and with it the join values, which a
//! settings change may have changed.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Steam keeps at most 63 bytes of a server name.
const NAME_LIMIT: usize = 63;
/// Steam keeps at most 127 bytes of a rule value.
const RULE_LIMIT: usize = 127;

/// The rule that carries the fingerprint.
pub const RULE: &str = "enshwick";
/// The rule that lists the mods; further parts are `enshwick.mods.1` and on.
pub const RULE_MODS: &str = "enshwick.mods";
/// The most rules the mod list spreads over.
pub const MOD_RULES: usize = 16;
/// The prefix of a mod's join value rules, `enshwick.sync.<id>`.
pub const RULE_SYNC: &str = "enshwick.sync";
/// The most rules one mod's join values spread over.
pub const SYNC_RULES: usize = 8;
/// The prefix of a mod's update source rule, `enshwick.src.<id>`.
pub const RULE_SOURCE: &str = "enshwick.src";

/// The fingerprint of a mod set: FNV-1a over the loader version, every
/// `id@version` and every mod's join values, sorted, as eight hex digits.
/// Without join values it is the fingerprint older loaders compute.
#[must_use]
pub fn fingerprint(
    loader_version: &str,
    mods: &[(String, String)],
    synced: &BTreeMap<String, String>,
) -> String {
    let mut entries: Vec<String> = mods
        .iter()
        .map(|(id, version)| format!("{id}@{version}"))
        .collect();
    entries.sort();
    let mut text = format!("enshwick@{loader_version}\n{}", entries.join("\n"));
    for (id, values) in synced {
        let _written = write!(text, "\nsync {id} {values}");
    }
    format!("{:08x}", enshwick_reflect::fnv1a32(text.as_bytes()))
}

/// `value` cut into parts of at most [`RULE_LIMIT`] bytes, on character
/// boundaries; one empty part for an empty value.
#[must_use]
pub fn split(value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut rest = value;
    while rest.len() > RULE_LIMIT {
        let mut cut = RULE_LIMIT;
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        let (part, tail) = rest.split_at(cut);
        parts.push(part);
        rest = tail;
    }
    parts.push(rest);
    parts
}

/// The join value rules of mod `id`: `enshwick.sync.<id>`, then `.1` and on;
/// `None` when the values need more than [`SYNC_RULES`] rules.
#[must_use]
pub fn sync_rules(id: &str, values: &str) -> Option<Vec<(String, String)>> {
    let parts = split(values);
    (parts.len() <= SYNC_RULES).then(|| {
        let base = format!("{RULE_SYNC}.{id}");
        parts
            .into_iter()
            .enumerate()
            .map(|(i, part)| (part_name(&base, i), part.to_owned()))
            .collect()
    })
}

/// The update source rules of the mods `sources`, `(id, source line)`: one
/// `enshwick.src.<id>` each, leaving out a source too long for one rule.
#[must_use]
pub fn source_rules(sources: &[(String, String)]) -> Vec<(String, String)> {
    sources
        .iter()
        .filter(|(_, line)| !line.is_empty() && line.len() <= RULE_LIMIT)
        .map(|(id, line)| (format!("{RULE_SOURCE}.{id}"), line.clone()))
        .collect()
}

/// What a server advertises at a moment: its Steam name, the fingerprint and
/// every rule, and the mods whose join values were too long to advertise.
#[derive(Debug, PartialEq, Eq)]
pub struct Advert {
    /// The server name with the fingerprint.
    pub name: String,
    /// The fingerprint, the value of [`RULE`].
    pub fingerprint: String,
    /// The mod list and join value rules.
    pub rules: Vec<(String, String)>,
    /// Mods whose join values need more than [`SYNC_RULES`] rules; they are
    /// left out of the rules and the fingerprint.
    pub too_long: Vec<String>,
}

impl Advert {
    /// The advert of a server named `configured` running `mods` with the join
    /// values `synced` and the update sources `sources`, which stay out of
    /// the fingerprint.
    #[must_use]
    pub fn new(
        loader_version: &str,
        configured: &str,
        mods: &[(String, String)],
        sources: &[(String, String)],
        synced: &BTreeMap<String, String>,
    ) -> Self {
        let mut fit = BTreeMap::new();
        let mut rules = mod_rules(mods);
        rules.extend(source_rules(sources));
        let mut too_long = Vec::new();
        for (id, values) in synced {
            match sync_rules(id, values) {
                Some(parts) => {
                    rules.extend(parts);
                    fit.insert(id.clone(), values.clone());
                }
                None => too_long.push(id.clone()),
            }
        }
        let fingerprint = fingerprint(loader_version, mods, &fit);
        Self {
            name: server_name(configured, &fingerprint),
            fingerprint,
            rules,
            too_long,
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
/// `enshwick.mods`, then `enshwick.mods.1` and on, at most [`MOD_RULES`].
#[must_use]
pub fn mod_rules(mods: &[(String, String)]) -> Vec<(String, String)> {
    let mut parts: Vec<String> = vec![String::new()];
    for entry in mod_list(mods).split(',').filter(|e| !e.is_empty()) {
        let Some(last) = parts.last_mut() else {
            break;
        };
        if !last.is_empty() && last.len() + 1 + entry.len() > RULE_LIMIT {
            if parts.len() == MOD_RULES {
                break;
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
    parts
        .into_iter()
        .enumerate()
        .map(|(i, value)| (rule_name(i), value))
        .collect()
}

/// The name of mod list rule `index`.
#[must_use]
pub fn rule_name(index: usize) -> String {
    part_name(RULE_MODS, index)
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
    use std::collections::BTreeMap;
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

    /// The join values of the running join mods, by id; read each time the
    /// advertisement is set.
    pub(crate) type Synced = fn() -> BTreeMap<String, String>;

    /// Starts the thread that advertises the mod set.
    pub(crate) fn start(
        game_dir: &Path,
        mods: Vec<(String, String)>,
        sources: Vec<(String, String)>,
        synced: Synced,
    ) {
        let config = std::fs::read_to_string(game_dir.join("enshrouded_server.json")).ok();
        let configured = config
            .and_then(|json| super::configured_name(&json))
            .unwrap_or_else(|| "Enshrouded Server".to_owned());
        let spawned = std::thread::Builder::new()
            .name("enshwick-advertise".into())
            .spawn(move || {
                if std::panic::catch_unwind(|| run(&configured, &mods, &sources, synced)).is_err() {
                    crate::log!(WARN, "mod set advertisement stopped: it panicked");
                }
            });
        drop(spawned);
    }

    fn run(
        configured: &str,
        mods: &[(String, String)],
        sources: &[(String, String)],
        synced: Synced,
    ) {
        let Some(api) = Api::load() else {
            crate::log!(
                WARN,
                "mod set advertisement off: steam_api64.dll lacks the game server API"
            );
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
        let mut last: Option<super::Advert> = None;
        loop {
            let advert = super::Advert::new(
                env!("CARGO_PKG_VERSION"),
                configured,
                mods,
                sources,
                &synced(),
            );
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
                // SAFETY: the logged-on interface and a NUL-terminated string.
                unsafe { (api.set_name)(server, name.as_ptr()) };
            }
            for (key, value) in pairs {
                let (Ok(key), Ok(value)) = (CString::new(key), CString::new(value)) else {
                    continue;
                };
                // SAFETY: the logged-on interface and NUL-terminated strings.
                unsafe { (api.set_key_value)(server, key.as_ptr(), value.as_ptr()) };
            }
            last = Some(advert);
            std::thread::sleep(REASSERT);
        }
    }

    /// Logs what the server now advertises.
    fn describe(advert: &super::Advert) {
        let mods: Vec<&str> = advert
            .rules
            .iter()
            .filter(|(key, _)| key.starts_with(super::RULE_MODS))
            .map(|(_, value)| value.as_str())
            .collect();
        let synced: Vec<&str> = advert
            .rules
            .iter()
            .filter_map(|(key, _)| key.strip_prefix(super::RULE_SYNC)?.strip_prefix('.'))
            .filter(|rest| !rest.contains('.'))
            .collect();
        crate::log!(
            INFO,
            "advertising \"{}\", mods {}, join values of [{}]",
            advert.name,
            mods.join(","),
            synced.join(", ")
        );
        for id in &advert.too_long {
            crate::log!(
                WARN,
                "the join values of {id} are too long to advertise; players keep their own"
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
            fingerprint("0.1.0", &mods(), &none()),
            fingerprint("0.1.0", &reversed, &none())
        );
        assert_ne!(
            fingerprint("0.1.0", &mods(), &none()),
            fingerprint("0.1.1", &mods(), &none())
        );
        assert_eq!(fingerprint("0.1.0", &mods(), &none()).len(), 8);
    }

    fn none() -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    fn synced(values: &str) -> BTreeMap<String, String> {
        BTreeMap::from([("count".to_owned(), values.to_owned())])
    }

    #[test]
    fn join_values_count_in_the_fingerprint() {
        let plain = fingerprint("0.1.0", &mods(), &none());
        let two = fingerprint("0.1.0", &mods(), &synced("{ a = [2] }"));
        assert_ne!(plain, two);
        assert_ne!(two, fingerprint("0.1.0", &mods(), &synced("{ a = [3] }")));
        assert_eq!(two, fingerprint("0.1.0", &mods(), &synced("{ a = [2] }")));
    }

    #[test]
    fn values_split_on_character_boundaries_within_the_limit() {
        assert_eq!(split(""), [""]);
        assert_eq!(split("{ a = 1 }"), ["{ a = 1 }"]);
        let long = format!("{{ name = '{}' }}", "ї".repeat(150));
        let parts = split(&long);
        assert_eq!(parts.len(), 3);
        assert!(parts.iter().all(|part| part.len() <= RULE_LIMIT));
        assert_eq!(parts.concat(), long);
    }

    #[test]
    fn join_values_are_rules_per_mod_or_left_out_when_too_long() {
        let rules = sync_rules("count", &"x".repeat(200)).unwrap();
        let names: Vec<&str> = rules.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["enshwick.sync.count", "enshwick.sync.count.1"]);
        assert_eq!(
            sync_rules("count", &"x".repeat(SYNC_RULES * RULE_LIMIT + 1)),
            None
        );

        let advert = Advert::new("0.1.0", "Homelab", &mods(), &[], &synced("{ a = [2] }"));
        assert_eq!(
            advert.rules,
            [
                (
                    "enshwick.mods".to_owned(),
                    "count@0.2.0,zones@0.1.0".to_owned()
                ),
                ("enshwick.sync.count".to_owned(), "{ a = [2] }".to_owned()),
            ]
        );
        assert_eq!(
            advert.name,
            format!("Homelab [enshwick {}]", advert.fingerprint)
        );
        let huge = Advert::new("0.1.0", "Homelab", &mods(), &[], &synced(&"x".repeat(2000)));
        assert_eq!(huge.too_long, ["count"]);
        assert_eq!(huge.fingerprint, fingerprint("0.1.0", &mods(), &none()));
    }

    #[test]
    fn sources_are_rules_that_fit_and_stay_out_of_the_fingerprint() {
        let sources = vec![
            (
                "count".to_owned(),
                "github:themaiby/enshwick:enshwick-flame-altar-count".to_owned(),
            ),
            (
                "zones".to_owned(),
                format!("url:https://{}", "x".repeat(200)),
            ),
            ("plain".to_owned(), String::new()),
        ];
        assert_eq!(
            source_rules(&sources),
            [(
                "enshwick.src.count".to_owned(),
                "github:themaiby/enshwick:enshwick-flame-altar-count".to_owned()
            )]
        );
        let with = Advert::new("0.1.0", "Homelab", &mods(), &sources, &none());
        let without = Advert::new("0.1.0", "Homelab", &mods(), &[], &none());
        assert_eq!(with.fingerprint, without.fingerprint);
        assert_eq!(with.rules.len(), without.rules.len() + 1);
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
    fn the_mod_list_is_sorted_and_spread_over_rules_that_fit() {
        assert_eq!(mod_list(&mods()), "count@0.2.0,zones@0.1.0");
        assert_eq!(
            mod_rules(&mods()),
            [(
                "enshwick.mods".to_owned(),
                "count@0.2.0,zones@0.1.0".to_owned()
            )]
        );
        let many: Vec<(String, String)> = (0..40)
            .map(|i| (format!("mod-{i:02}"), "1.0.0".into()))
            .collect();
        let rules = mod_rules(&many);
        assert!(rules.len() > 1);
        assert!(rules.iter().all(|(_, value)| value.len() <= RULE_LIMIT));
        assert_eq!(rules[1].0, "enshwick.mods.1");
        let joined: Vec<&str> = rules.iter().map(|(_, v)| v.as_str()).collect();
        assert_eq!(joined.join(","), mod_list(&many));
    }

    #[test]
    fn the_name_is_read_from_the_server_config() {
        let json = "{\n\t\"name\": \"Our \\\"Base\\\"\",\n\t\"saveDirectory\": \"./savegame\"\n}";
        assert_eq!(configured_name(json).as_deref(), Some("Our \"Base\""));
        assert_eq!(configured_name("{}"), None);
    }
}
