//! Updates Enshwick and its mods from their GitHub releases, with the
//! player's consent.
//!
//! At start, on a thread of its own, the updater lists the loaded mods, asks
//! each mod's GitHub repository for its releases and, when newer ones exist,
//! asks the player. What the player accepts is downloaded, checked against
//! its minisign signature and staged in `mods/.staged/`, which the
//! `powrprof.dll` stub moves into place at the next start. When the join
//! check keeps the player out of a server that runs mods this game lacks or
//! has in another version, the updater offers the server's versions the
//! same way. Settings files are never touched.

pub mod release;
pub mod stage;
pub mod verify;

mod ask;
mod net;

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use enshwick_api::events::JoinMissing;
use enshwick_api::mods::{ModEntry, Updates};
use enshwick_api::{Load, Mod, Off};
use semver::Version;

use crate::net::{Client, Failure};
use crate::release::Pick;

/// How long after start the updater looks, so the game's window is up.
const START_DELAY: Duration = Duration::from_secs(15);
/// The file under the data folder that holds when the last check ran.
const LAST_CHECK: &str = "last-check";
/// Where archives are unpacked before they are staged, under the data folder.
const SCRATCH: &str = "incoming";

/// Updater settings.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, enshwick_api::Config)]
#[serde(deny_unknown_fields)]
#[config(validate = Settings::check)]
struct Settings {
    /// Look for updates at start, and offer a server's versions when it runs
    /// mods this game lacks. Off: the updater never goes online.
    enabled: bool,
    /// Hours between checks at start; 0 checks at every start.
    check_interval_hours: u32,
    /// Public keys, besides Enshwick's own, whose signed releases are
    /// installed: the second line of a minisign .pub file, such as
    /// "RWQ...". Add only keys of authors you trust.
    trusted_keys: Vec<String>,
    /// Offer prereleases too.
    prereleases: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval_hours: 24,
            trusted_keys: Vec::new(),
            prereleases: false,
        }
    }
}

impl Settings {
    fn check(&self) -> Result<(), String> {
        for key in &self.trusted_keys {
            verify::key(key).map_err(|e| format!("trusted_keys: {key:?} is {e}"))?;
        }
        Ok(())
    }

    /// Enshwick's key and the trusted ones.
    fn keys(&self) -> Vec<minisign_verify::PublicKey> {
        core::iter::once(verify::ENSHWICK_KEY)
            .chain(self.trusted_keys.iter().map(String::as_str))
            .filter_map(|key| verify::key(key).ok())
            .collect()
    }
}

/// What a check works with.
#[derive(Debug)]
struct Context {
    settings: Settings,
    /// The game's folder.
    game: PathBuf,
    /// The updater's data folder.
    data: PathBuf,
}

/// The current context; settings changes replace it.
static CONTEXT: Mutex<Option<Arc<Context>>> = Mutex::new(None);
/// Held while the updater talks to the player or stages files, so two
/// offers never overlap.
static BUSY: Mutex<()> = Mutex::new(());
/// `id@version` offered this session, accepted or not; not offered again.
static OFFERED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn context() -> Option<Arc<Context>> {
    CONTEXT
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// One update the player is offered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    /// The mod id.
    pub id: String,
    /// The version this game has; `None` when it lacks the mod.
    pub from: Option<String>,
    /// The mod's DLL as the loader lists it; empty when the game lacks it.
    pub file: String,
    /// The release to install.
    pub pick: Pick,
}

impl Offer {
    /// The offer's line in a question: `id: from → to`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_updater::Offer;
    /// use enshwick_updater::release::Pick;
    ///
    /// let offer = Offer {
    ///     id: "safes".into(),
    ///     from: Some("0.1.0".into()),
    ///     file: "mods/enshwick-safes.dll".into(),
    ///     pick: Pick {
    ///         tag: "enshwick-safes-v0.2.0".into(),
    ///         version: semver::Version::new(0, 2, 0),
    ///         archive: "enshwick-safes.zip".into(),
    ///         archive_url: String::new(),
    ///         signature_url: String::new(),
    ///     },
    /// };
    /// assert_eq!(offer.line(), "safes: 0.1.0 \u{2192} 0.2.0");
    /// ```
    #[must_use]
    pub fn line(&self) -> String {
        format!(
            "{}: {} \u{2192} {}",
            self.id,
            self.from.as_deref().unwrap_or("not installed"),
            self.pick.version
        )
    }

    fn key(&self) -> String {
        format!("{}@{}", self.id, self.pick.version)
    }
}

/// Whether a check at start is due, `interval_hours` after the one at
/// `last` (Unix seconds); always without a record or with a clock that went
/// back.
///
/// # Examples
///
/// ```
/// use enshwick_updater::due;
///
/// assert!(due(None, 1_000_000, 24));
/// assert!(!due(Some(1_000_000), 1_000_000 + 3600, 24));
/// assert!(due(Some(1_000_000), 1_000_000 + 24 * 3600, 24));
/// assert!(due(Some(1_000_000), 1_000_000, 0));
/// ```
#[must_use]
pub fn due(last: Option<u64>, now: u64, interval_hours: u32) -> bool {
    match last {
        Some(last) if last <= now => now - last >= u64::from(interval_hours) * 3600,
        _ => true,
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// The check at start: newer releases of the loaded mods, offered to the
/// player.
fn at_start() {
    std::thread::sleep(START_DELAY);
    let Some(context) = context() else {
        return;
    };
    if !context.settings.enabled {
        enshwick_api::info!("updates are off in the settings");
        return;
    }
    let record = context.data.join(LAST_CHECK);
    let last = std::fs::read_to_string(&record)
        .ok()
        .and_then(|text| text.trim().parse().ok());
    if !due(last, now(), context.settings.check_interval_hours) {
        enshwick_api::info!(
            "checked within the last {} hours; not checking again yet",
            context.settings.check_interval_hours
        );
        return;
    }
    let client = Client::new();
    let offers = match newer(&client, &context, &enshwick_api::mods()) {
        Ok(offers) => offers,
        Err(failure) => {
            enshwick_api::warn!("no update check this time: {failure}");
            return;
        }
    };
    if let Err(e) = std::fs::write(&record, now().to_string()) {
        enshwick_api::warn!("cannot record the check in {}: {e}", record.display());
    }
    let _busy = BUSY.lock().unwrap_or_else(PoisonError::into_inner);
    let offers = unoffered(offers);
    if offers.is_empty() {
        enshwick_api::info!("everything is up to date");
        return;
    }
    let question = format!(
        "Updates for Enshwick are available:\n\n{}\n\nDownload them now? They are checked against their signatures and take effect the next time you start the game. Your settings stay as they are.",
        lines(&offers)
    );
    if !ask::confirm(&question) {
        enshwick_api::info!("the player declined {}", lines(&offers).replace('\n', ", "));
        return;
    }
    install_all(
        &client,
        &context,
        &offers,
        "They take effect the next time you start the game.",
    );
}

/// After a refused join: the server's versions of the mods this game lacks,
/// offered to the player.
fn after_refused_join() {
    let Some(context) = context() else {
        return;
    };
    if !context.settings.enabled {
        return;
    }
    let missing = enshwick_api::mods::join_missing();
    if missing.is_empty() {
        return;
    }
    let client = Client::new();
    let installed = enshwick_api::mods();
    let mut offers = Vec::new();
    let mut unavailable = Vec::new();
    for entry in &missing {
        match server_version(&client, entry, &installed) {
            Ok(offer) => offers.push(offer),
            Err(why) => {
                enshwick_api::warn!("{} {}: {why}", entry.id, entry.version);
                unavailable.push(format!("{} {}: {why}", entry.id, entry.version));
            }
        }
    }
    let _busy = BUSY.lock().unwrap_or_else(PoisonError::into_inner);
    let offers = unoffered(offers);
    if offers.is_empty() {
        return;
    }
    let mut question = vec![format!(
        "This server runs mods you do not have in its version:\n\n{}\n\nDownload the server's versions? They are checked against their signatures; restart the game afterwards and join again. Your settings stay as they are.",
        lines(&offers)
    )];
    if !unavailable.is_empty() {
        question.push(format!(
            "Not available for download:\n{}",
            unavailable.join("\n")
        ));
    }
    let question = question.join("\n\n");
    if !ask::confirm(&question) {
        enshwick_api::info!(
            "the player declined the server's {}",
            lines(&offers).replace('\n', ", ")
        );
        return;
    }
    install_all(
        &client,
        &context,
        &offers,
        "Restart the game and join the server again.",
    );
}

/// The offer of the server's version of `entry`, a mod the join check found
/// missing.
fn server_version(
    client: &Client,
    entry: &ModEntry,
    installed: &[ModEntry],
) -> Result<Offer, String> {
    let Some(Updates::GitHub {
        repository,
        component,
    }) = &entry.updates
    else {
        return Err("no GitHub release source".to_owned());
    };
    let version = Version::parse(&entry.version).map_err(|e| format!("bad version: {e}"))?;
    let tag = format!("{component}-v{version}");
    let json =
        client
            .api(&release::tag_url(repository, &tag))
            .map_err(|failure| match failure {
                Failure::NotFound => format!("no release {tag} in {repository}"),
                other => other.to_string(),
            })?;
    let releases = release::parse(&json)?;
    let pick = release::exact(&releases, repository, component, &version)
        .ok_or_else(|| format!("{tag} has no signed {component}.zip"))?;
    let ours = installed.iter().find(|m| m.id == entry.id);
    Ok(Offer {
        id: entry.id.clone(),
        from: ours.map(|m| m.version.clone()),
        file: ours.map_or_else(|| entry.file.clone(), |m| m.file.clone()),
        pick,
    })
}

/// Newer releases of `mods`, one request per repository.
fn newer(client: &Client, context: &Context, mods: &[ModEntry]) -> Result<Vec<Offer>, Failure> {
    let mut lists: BTreeMap<&str, Vec<release::Release>> = BTreeMap::new();
    let mut offers = Vec::new();
    for entry in mods {
        let Some(Updates::GitHub {
            repository,
            component,
        }) = &entry.updates
        else {
            continue;
        };
        let Ok(installed) = Version::parse(&entry.version) else {
            continue;
        };
        if stage::waiting(&context.game, &entry.file) {
            enshwick_api::info!("an update of {} waits for the next start", entry.id);
            continue;
        }
        if !lists.contains_key(repository.as_str()) {
            let releases = match client.api(&release::list_url(repository)) {
                Ok(json) => release::parse(&json).unwrap_or_else(|e| {
                    enshwick_api::warn!("{repository}: {e}");
                    Vec::new()
                }),
                Err(Failure::RateLimited) => return Err(Failure::RateLimited),
                Err(failure) => {
                    enshwick_api::warn!("{repository}: {failure}");
                    Vec::new()
                }
            };
            lists.insert(repository, releases);
        }
        let releases = lists
            .get(repository.as_str())
            .map_or(&[][..], Vec::as_slice);
        let prereleases = context.settings.prereleases;
        if let Some(pick) =
            release::newest(releases, repository, component, &installed, prereleases)
        {
            offers.push(Offer {
                id: entry.id.clone(),
                from: Some(entry.version.clone()),
                file: entry.file.clone(),
                pick,
            });
        }
    }
    Ok(offers)
}

/// `offers` without those already offered this session, which are now
/// marked offered.
fn unoffered(offers: Vec<Offer>) -> Vec<Offer> {
    let mut held = OFFERED.lock().unwrap_or_else(PoisonError::into_inner);
    let offered = held.get_or_insert_with(HashSet::new);
    offers
        .into_iter()
        .filter(|offer| offered.insert(offer.key()))
        .collect()
}

fn lines(offers: &[Offer]) -> String {
    offers
        .iter()
        .map(Offer::line)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Downloads, checks and stages every offer, then tells the player what
/// came of it, ending with `next`.
fn install_all(client: &Client, context: &Context, offers: &[Offer], next: &str) {
    let keys = context.settings.keys();
    let mut done = Vec::new();
    let mut failed = Vec::new();
    for offer in offers {
        match install(client, context, &keys, offer) {
            Ok(files) => {
                enshwick_api::info!("staged {}: {}", offer.line(), files.join(", "));
                done.push(offer.line());
            }
            Err(why) => {
                enshwick_api::warn!("did not install {}: {why}", offer.line());
                failed.push(format!("{}: {why}", offer.line()));
            }
        }
    }
    let mut text = Vec::new();
    if !done.is_empty() {
        text.push(format!("Downloaded:\n\n{}\n\n{next}", done.join("\n")));
    }
    if failed.is_empty() {
        ask::tell(&text.join("\n\n"));
    } else {
        text.push(format!(
            "Not installed:\n\n{}\n\nThe log is logs/enshwick/mods/updater.log.",
            failed.join("\n")
        ));
        ask::warn(&text.join("\n\n"));
    }
}

fn install(
    client: &Client,
    context: &Context,
    keys: &[minisign_verify::PublicKey],
    offer: &Offer,
) -> Result<Vec<String>, String> {
    let pick = &offer.pick;
    let archive = client
        .archive(&pick.archive_url)
        .map_err(|e| e.to_string())?;
    let signature = client
        .signature(&pick.signature_url)
        .map_err(|e| e.to_string())?;
    verify::check(keys, &archive, &signature, &pick.tag, &pick.archive)
        .map_err(|why| format!("refused: {why}"))?;
    let installed = Some(offer.file.as_str()).filter(|file| !file.is_empty());
    stage::stage(
        &archive,
        installed,
        &context.game,
        &context.data.join(SCRATCH),
    )
}

/// Runs `work` on a thread of its own; a panic only ends that thread.
fn spawn(name: &'static str, work: fn()) {
    let spawned = std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            if std::panic::catch_unwind(work).is_err() {
                enshwick_api::warn!("{name}: stopped by a panic");
            }
        });
    if let Err(e) = spawned {
        enshwick_api::warn!("cannot start {name}: {e}");
    }
}

fn set_context(settings: Settings, game: &Path, data: &Path) {
    *CONTEXT.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(Context {
        settings,
        game: game.to_path_buf(),
        data: data.to_path_buf(),
    }));
}

/// The mod's state.
#[derive(Debug)]
pub struct Updater;

impl Mod for Updater {
    fn manifest() -> &'static enshwick_api::abi::Manifest {
        enshwick_api::manifest!(
            id: "updater",
            side: CLIENT,
            updates: github("themaiby/enshwick", "enshwick-updater"),
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        let data = load.data_dir()?;
        let game = data
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| Off::new("the data folder is not under the game's folder"))?
            .to_path_buf();
        set_context(settings, &game, &data);
        let (game_dir, data_dir) = (game.clone(), data.clone());
        load.on_config_change(move |settings: Settings| {
            set_context(settings, &game_dir, &data_dir);
        })?;
        if load
            .on::<JoinMissing>(|()| spawn("enshwick-updater-join", after_refused_join))
            .is_err()
        {
            enshwick_api::warn!(
                "this loader does not report a server's missing mods; update the loader to get them here"
            );
        }
        spawn("enshwick-updater", at_start);
        Ok(Self)
    }
}

enshwick_api::declare_mod!(Updater);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_settings_parse() {
        use enshwick_api::config::Config;
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert!(settings.enabled);
        assert_eq!(settings.check_interval_hours, 24);
        assert!(!settings.prereleases);
        assert!(settings.validate().is_ok());
        assert_eq!(settings.keys().len(), 1);
    }

    #[test]
    fn trusted_keys_must_be_minisign_keys() {
        use enshwick_api::config::Config;
        let mut settings = Settings::default();
        settings
            .trusted_keys
            .push("RWSVvUJNE6Z0IzsG2I1G/+JlpjWYHLgP8QVhvMlSWCg0AShBnBmVIt4r".into());
        assert!(settings.validate().is_ok());
        assert_eq!(settings.keys().len(), 2);
        settings.trusted_keys.push("not a key".into());
        assert!(settings.validate().is_err());
    }

    #[test]
    fn checks_follow_the_interval() {
        assert!(due(None, 10, 24));
        assert!(!due(Some(10), 10 + 23 * 3600, 24));
        assert!(due(Some(10), 10 + 24 * 3600, 24));
        assert!(due(Some(10 + 3600), 10, 24));
        assert!(due(Some(10), 10, 0));
    }

    fn offer(id: &str, version: Version) -> Offer {
        Offer {
            id: id.into(),
            from: None,
            file: String::new(),
            pick: Pick {
                tag: format!("{id}-v{version}"),
                version,
                archive: format!("{id}.zip"),
                archive_url: String::new(),
                signature_url: String::new(),
            },
        }
    }

    #[test]
    fn an_offer_is_made_once_a_session() {
        let first = unoffered(vec![
            offer("unoffered-a", Version::new(1, 0, 0)),
            offer("unoffered-b", Version::new(1, 0, 0)),
        ]);
        assert_eq!(first.len(), 2);
        let again = unoffered(vec![
            offer("unoffered-a", Version::new(1, 0, 0)),
            offer("unoffered-a", Version::new(1, 1, 0)),
        ]);
        assert_eq!(again, [offer("unoffered-a", Version::new(1, 1, 0))]);
        assert_eq!(
            offer("x", Version::new(1, 0, 0)).line(),
            "x: not installed \u{2192} 1.0.0"
        );
    }
}
