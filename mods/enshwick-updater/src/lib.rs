//! Updates Enshwick and its mods from their signed releases, with the
//! player's consent.
//!
//! At start, on a thread of its own, the updater lists the loaded mods, asks
//! each mod's source for its releases (a GitHub repository, or the document
//! of a `url(...)` source, see [`document`]) and, when newer ones exist, asks
//! the player. What the player accepts is downloaded, checked against
//! its minisign signature by a key the installed version names (see
//! [`trust`]) and staged in `mods/.staged/`, which the
//! `powrprof.dll` stub moves into place at the next start. When the join
//! check keeps the player out of a server that runs mods this game lacks or
//! has in another version, the updater offers the server's versions the
//! same way, each checked against the author key the server names for it.
//! Settings files are never touched.
//!
//! On a game build the loader has no table for yet, on the client and the
//! dedicated server alike, the updater fetches the table for that build (see
//! [`tables`]); the loader uses it from the next start.

pub mod document;
pub mod release;
pub mod stage;
pub mod tables;
pub mod trust;
pub mod verify;

mod ask;
mod net;

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use enshwick_loader_api::abi::{BuildTable, Side};
use enshwick_loader_api::events::JoinRefused;
use enshwick_loader_api::mods::{LOADER_FILES, LOADER_ID, ModEntry, Updates};
use enshwick_loader_api::{Load, Mod, Off};
use semver::Version;

use crate::document::Document;
use crate::net::{Client, Failure};
use crate::release::Pick;
use crate::trust::{Refusal, Trust};

/// How long after start the updater looks, so the game's window is up.
const START_DELAY: Duration = Duration::from_secs(15);
/// The file under the data folder that holds when the last check ran.
const LAST_CHECK: &str = "last-check";
/// Where archives are unpacked before they are staged, under the data folder.
const SCRATCH: &str = "incoming";

/// Updater settings.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
#[serde(deny_unknown_fields)]
#[config(validate = Settings::check)]
struct Settings {
    /// Look for updates at start, and offer a server's versions when it runs
    /// mods this game lacks. Off: the updater never goes online.
    #[config(client)]
    enabled: bool,
    /// Hours between checks at start; 0 checks at every start.
    #[config(client, range = 0..=8760)]
    check_interval_hours: u32,
    /// Public keys accepted for updates besides the ones each installed mod
    /// names: the second line of a minisign .pub file, such as "RWQ...".
    /// For an author who changed keys; add only keys of authors you trust.
    #[config(client)]
    trusted_keys: Vec<String>,
    /// Offer prereleases too.
    #[config(client)]
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

    /// The trusted keys that read as keys.
    fn trusted(&self) -> Vec<String> {
        self.trusted_keys
            .iter()
            .filter(|key| verify::key(key).is_ok())
            .map(|key| key.trim().to_owned())
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
    /// The name players read; empty for none.
    pub name: String,
    /// The version this game has; `None` when it lacks the mod.
    pub from: Option<String>,
    /// The mod's DLL as the loader lists it; empty when the game lacks it.
    pub file: String,
    /// The release to install.
    pub pick: Pick,
    /// Where the release comes from, as the player reads it.
    pub source: String,
    /// The keys one of which must have signed it, or why it is not installed
    /// automatically.
    pub trust: Result<Trust, Refusal>,
}

impl Offer {
    /// The offer's line in a question: `Name (id): from → to`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_updater::Offer;
    /// use enshwick_updater::release::Pick;
    /// use enshwick_updater::trust::Refusal;
    ///
    /// let offer = Offer {
    ///     id: "safes".into(),
    ///     name: "Safes".into(),
    ///     from: Some("0.1.0".into()),
    ///     file: "mods/enshwick-safes.dll".into(),
    ///     pick: Pick {
    ///         tag: "enshwick-safes-v0.2.0".into(),
    ///         version: semver::Version::new(0, 2, 0),
    ///         archive: "enshwick-safes.zip".into(),
    ///         archive_url: String::new(),
    ///         signature_url: String::new(),
    ///     },
    ///     source: "github.com/themaiby/enshwick".into(),
    ///     trust: Err(Refusal::Unsigned),
    /// };
    /// assert_eq!(offer.line(), "Safes (safes): 0.1.0 \u{2192} 0.2.0");
    /// ```
    ///
    /// A version older than the installed one says so:
    ///
    /// ```
    /// # use enshwick_updater::Offer;
    /// # use enshwick_updater::release::Pick;
    /// # use enshwick_updater::trust::Refusal;
    /// # let offer = Offer {
    /// #     id: "safes".into(),
    /// #     name: String::new(),
    /// #     from: Some("0.3.0".into()),
    /// #     file: String::new(),
    /// #     pick: Pick {
    /// #         tag: String::new(),
    /// #         version: semver::Version::new(0, 2, 0),
    /// #         archive: String::new(),
    /// #         archive_url: String::new(),
    /// #         signature_url: String::new(),
    /// #     },
    /// #     source: String::new(),
    /// #     trust: Err(Refusal::Unsigned),
    /// # };
    /// assert_eq!(offer.line(), "safes: 0.3.0 \u{2192} 0.2.0 (older)");
    /// ```
    #[must_use]
    pub fn line(&self) -> String {
        let older = self
            .from
            .as_deref()
            .and_then(|from| Version::parse(from).ok())
            .is_some_and(|from| self.pick.version < from);
        format!(
            "{}: {} \u{2192} {}{}",
            self.label(),
            self.from.as_deref().unwrap_or("not installed"),
            self.pick.version,
            if older { " (older)" } else { "" }
        )
    }

    /// `Name (id)`, or the id when the mod names none.
    fn label(&self) -> String {
        enshwick_loader_api::mods::label(&self.name, &self.id)
    }

    fn key(&self) -> String {
        format!("{}@{}", self.id, self.pick.version)
    }

    /// The offer's lines in the question after a refused join, for a mod the
    /// game lacks: the version, where it comes from and the key the server
    /// names.
    fn server_lines(&self) -> String {
        let keys = self.trust.as_ref().map_or(&[][..], Trust::keys).join(", ");
        format!(
            "{} {}\n    from {}\n    author key {keys}",
            self.label(),
            self.pick.version,
            self.source
        )
    }
}

/// Where `updates` comes from, as the player reads it.
fn source_of(updates: &Updates) -> String {
    match updates {
        Updates::GitHub {
            repository,
            component,
        } => format!("github.com/{repository} ({component})"),
        Updates::Url(url) => url.clone(),
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
        enshwick_loader_api::info!("updates are off in the settings");
        return;
    }
    let record = context.data.join(LAST_CHECK);
    let last = std::fs::read_to_string(&record)
        .ok()
        .and_then(|text| text.trim().parse().ok());
    if !due(last, now(), context.settings.check_interval_hours) {
        enshwick_loader_api::info!(
            "checked within the last {} hours; not checking again yet",
            context.settings.check_interval_hours
        );
        return;
    }
    let client = Client::new();
    let (offers, answered) = match newer(&client, &context, &enshwick_loader_api::mods()) {
        Ok(checked) => checked,
        Err(failure) => {
            enshwick_loader_api::warn!("no update check this time: {failure}");
            return;
        }
    };
    // A source that did not answer, such as one offline, is asked again at
    // the next start rather than after a whole interval.
    if !answered {
        enshwick_loader_api::info!("not every source answered: checking again at the next start");
    } else if let Err(e) = std::fs::write(&record, now().to_string()) {
        enshwick_loader_api::warn!("cannot record the check in {}: {e}", record.display());
    }
    let _busy = BUSY.lock().unwrap_or_else(PoisonError::into_inner);
    let offers = unoffered(offers);
    if offers.is_empty() {
        enshwick_loader_api::info!("everything is up to date");
        return;
    }
    let (offers, by_hand) = signed(offers);
    let by_hand = (!by_hand.is_empty()).then(|| format!("Not downloaded:\n\n{by_hand}"));
    if offers.is_empty() {
        if let Some(by_hand) = by_hand {
            ask::tell(&format!("Updates for Enshwick are available.\n\n{by_hand}"));
        }
        return;
    }
    let mut question = vec![format!(
        "Updates for Enshwick are available:\n\n{}\n\nDownload them now? Each is checked against the signature of the author of the version you have and takes effect the next time you start the game. Your settings stay as they are.",
        lines(&offers)
    )];
    question.extend(by_hand);
    if !ask::confirm(&question.join("\n\n")) {
        enshwick_loader_api::info!("the player declined {}", lines(&offers).replace('\n', ", "));
        return;
    }
    install_all(
        &client,
        &context,
        &offers,
        "They take effect the next time you start the game.",
    );
}

/// On a build the loader runs on patterns: the table for it, when the
/// project has published one.
fn fetch_table() {
    std::thread::sleep(START_DELAY);
    let (Some(context), Some(host)) = (context(), enshwick_loader_api::host()) else {
        return;
    };
    let build = host.build();
    if build.table != BuildTable::PATTERNS || !context.settings.enabled {
        return;
    }
    let Some(file) = tables::file_name(build.side, build.fingerprint) else {
        return;
    };
    if context.game.join(tables::DIR).join(&file).exists() {
        enshwick_loader_api::info!("the build table {file} waits for the next start");
        return;
    }
    let repository = enshwick_loader_api::mods()
        .into_iter()
        .find(|m| m.id == LOADER_ID)
        .and_then(|m| match m.updates {
            Some(Updates::GitHub { repository, .. }) => Some(repository),
            _ => None,
        });
    let Some(repository) = repository else {
        enshwick_loader_api::info!(
            "the loader names no GitHub source, so no build table is fetched"
        );
        return;
    };
    let client = Client::new();
    let url = tables::url(&repository, &file);
    let fetched = client
        .table(&url)
        .and_then(|table| Ok((table, client.signature(&format!("{url}.minisig"))?)));
    let (table, signature) = match fetched {
        Ok(fetched) => fetched,
        Err(Failure::NotFound) => {
            enshwick_loader_api::info!(
                "no build table is published for this game build yet; the loader runs on patterns until there is"
            );
            return;
        }
        Err(failure) => {
            enshwick_loader_api::warn!("the build table was not fetched: {failure}");
            return;
        }
    };
    if let Err(why) = tables::store(&context.game, &file, &table, &signature) {
        enshwick_loader_api::warn!("the build table {file} is not used: {why}");
        return;
    }
    if build.side == Side::CLIENT {
        enshwick_loader_api::info!("fetched the build table {file}");
        ask::tell(
            "Enshwick downloaded what it needs for this version of the game.

Restart the game to use every mod again.",
        );
    } else {
        enshwick_loader_api::warn!(
            "fetched the build table {file}: restart the server to use every mod again"
        );
    }
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
    let missing = enshwick_loader_api::mods::join_missing();
    if missing.is_empty() {
        return;
    }
    let client = Client::new();
    let installed = enshwick_loader_api::mods();
    let trusted = context.settings.trusted();
    let mut offers = Vec::new();
    let mut unavailable = Vec::new();
    for entry in &missing {
        match server_version(&client, entry, &installed, &trusted) {
            Ok(offer) => offers.push(offer),
            Err(why) => {
                enshwick_loader_api::warn!("{} {}: {why}", entry.id, entry.version);
                unavailable.push(format!("{} {}: {why}", entry.label(), entry.version));
            }
        }
    }
    let _busy = BUSY.lock().unwrap_or_else(PoisonError::into_inner);
    let offers = unoffered(offers);
    let (offers, by_hand) = signed(offers);
    if !by_hand.is_empty() {
        unavailable.push(by_hand);
    }
    if offers.is_empty() {
        if !unavailable.is_empty() {
            ask::tell(&format!(
                "This server runs mods you do not have in its version.\n\nNot available for download:\n{}",
                unavailable.join("\n")
            ));
        }
        return;
    }
    let (updates, new): (Vec<&Offer>, Vec<&Offer>) =
        offers.iter().partition(|offer| offer.from.is_some());
    let mut question = vec!["This server runs mods you do not have in its version.".to_owned()];
    if !updates.is_empty() {
        question.push(format!(
            "Updates of mods you have, each checked against the signature of the author of the version you have:\n\n{}",
            updates.iter().map(|offer| offer.line()).collect::<Vec<_>>().join("\n")
        ));
    }
    if !new.is_empty() {
        question.push(format!(
            "Mods you do not have yet:\n\n{}\n\nThe server names the author key of each. A mod is installed only if its download is signed by that key: you trust these mods because you trust this server.",
            new.iter().map(|offer| offer.server_lines()).collect::<Vec<_>>().join("\n")
        ));
    }
    if !unavailable.is_empty() {
        question.push(format!(
            "Not available for download:\n{}",
            unavailable.join("\n")
        ));
    }
    question.push(
        "Download them? Restart the game afterwards and join again. Your settings stay as they are."
            .to_owned(),
    );
    let question = question.join("\n\n");
    if !ask::confirm(&question) {
        enshwick_loader_api::info!(
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
    trusted: &[String],
) -> Result<Offer, String> {
    let ours = installed.iter().find(|m| m.id == entry.id);
    let trust = trust::for_missing(entry, ours, trusted);
    let mut version = Version::parse(&entry.version).map_err(|e| format!("bad version: {e}"))?;
    // A content pack's version carries the hash of its files for the join
    // check; its release is the version alone.
    version.build = semver::BuildMetadata::EMPTY;
    // A mod this game has comes from its own source, never one the server
    // names: the server could otherwise pass another release signed by the
    // same key for it.
    let updates = match ours {
        Some(ours) => ours.updates.as_ref(),
        None => entry.updates.as_ref(),
    };
    let pick = match updates {
        Some(Updates::GitHub {
            repository,
            component,
        }) => {
            let tag = format!("{component}-v{version}");
            let json = client
                .api(&release::tag_url(repository, &tag))
                .map_err(|failure| match failure {
                    Failure::NotFound => format!("no release {tag} in {repository}"),
                    other => other.to_string(),
                })?;
            let releases = release::parse(&json)?;
            release::exact(&releases, repository, component, &version)
                .ok_or_else(|| format!("{tag} has no signed {component}.zip"))?
        }
        Some(Updates::Url(url)) => {
            let document = fetch_document(client, url)?;
            document::exact(&document, &entry.id, &version)
                .ok_or_else(|| format!("{url} lists no {version} with a signed .zip"))?
        }
        None => return Err("no release source".to_owned()),
    };
    Ok(Offer {
        id: entry.id.clone(),
        name: ours.map_or_else(|| entry.name.clone(), |m| m.name.clone()),
        from: ours.map(|m| m.version.clone()),
        file: ours.map(|m| m.file.clone()).unwrap_or_default(),
        pick,
        source: updates.map(source_of).unwrap_or_default(),
        trust,
    })
}

/// The document of the `url(...)` source `url`.
fn fetch_document(client: &Client, url: &str) -> Result<Document, String> {
    if !document::is_https(url) {
        return Err(format!("{url} is not an https:// address"));
    }
    let json = client.document(url).map_err(|failure| match failure {
        Failure::NotFound => format!("{url}: not found"),
        other => other.to_string(),
    })?;
    document::parse(&json).map_err(|e| format!("{url}: {e}"))
}

/// Newer releases of `mods`, one request per repository or document, and
/// whether every source answered.
fn newer(
    client: &Client,
    context: &Context,
    mods: &[ModEntry],
) -> Result<(Vec<Offer>, bool), Failure> {
    let mut answered = true;
    let mut lists: BTreeMap<&str, Vec<release::Release>> = BTreeMap::new();
    let mut documents: BTreeMap<&str, Option<Document>> = BTreeMap::new();
    let mut offers = Vec::new();
    for entry in mods {
        let Some(updates) = &entry.updates else {
            continue;
        };
        let Ok(installed) = Version::parse(&entry.version) else {
            continue;
        };
        if stage::waiting(&context.game, &entry.file) {
            enshwick_loader_api::info!("an update of {} waits for the next start", entry.id);
            continue;
        }
        let prereleases = context.settings.prereleases;
        let trust = || trust::for_installed(entry, &context.settings.trusted());
        let (repository, component) = match updates {
            Updates::GitHub {
                repository,
                component,
            } => (repository, component),
            Updates::Url(url) => {
                let document = documents.entry(url.as_str()).or_insert_with(|| {
                    fetch_document(client, url)
                        .map_err(|e| {
                            answered = false;
                            enshwick_loader_api::warn!("{e}");
                        })
                        .ok()
                });
                if let Some(pick) = document
                    .as_ref()
                    .and_then(|d| document::newest(d, &entry.id, &installed, prereleases))
                {
                    offers.push(Offer {
                        id: entry.id.clone(),
                        name: entry.name.clone(),
                        from: Some(entry.version.clone()),
                        file: entry.file.clone(),
                        pick,
                        source: source_of(updates),
                        trust: trust(),
                    });
                }
                continue;
            }
        };
        if !lists.contains_key(repository.as_str()) {
            let releases = match client.api(&release::list_url(repository)) {
                Ok(json) => release::parse(&json).unwrap_or_else(|e| {
                    answered = false;
                    enshwick_loader_api::warn!("{repository}: {e}");
                    Vec::new()
                }),
                Err(Failure::RateLimited) => return Err(Failure::RateLimited),
                Err(failure) => {
                    answered = false;
                    enshwick_loader_api::warn!("{repository}: {failure}");
                    Vec::new()
                }
            };
            lists.insert(repository, releases);
        }
        let releases = lists
            .get(repository.as_str())
            .map_or(&[][..], Vec::as_slice);
        if let Some(pick) =
            release::newest(releases, repository, component, &installed, prereleases)
        {
            offers.push(Offer {
                id: entry.id.clone(),
                name: entry.name.clone(),
                from: Some(entry.version.clone()),
                file: entry.file.clone(),
                pick,
                source: source_of(updates),
                trust: trust(),
            });
        }
    }
    Ok((offers, answered))
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

/// `offers` split into those that can be installed and the lines of those
/// the player updates by hand, which are not downloaded.
fn signed(offers: Vec<Offer>) -> (Vec<Offer>, String) {
    let (signed, by_hand): (Vec<Offer>, Vec<Offer>) =
        offers.into_iter().partition(|offer| offer.trust.is_ok());
    let by_hand = by_hand
        .iter()
        .filter_map(|offer| {
            let note = offer.trust.as_ref().err()?.note();
            enshwick_loader_api::info!("not offered {}: {note}", offer.line());
            Some(format!("{}: {note}", offer.line()))
        })
        .collect::<Vec<_>>()
        .join("\n");
    (signed, by_hand)
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
    let mut done = Vec::new();
    let mut failed = Vec::new();
    for offer in offers {
        match install(client, context, offer) {
            Ok(files) => {
                enshwick_loader_api::info!("staged {}: {}", offer.line(), files.join(", "));
                done.push(offer.line());
            }
            Err(why) => {
                enshwick_loader_api::warn!("did not install {}: {why}", offer.line());
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
        ask::tell(&text.join("\n\n"));
    }
}

fn install(client: &Client, context: &Context, offer: &Offer) -> Result<Vec<String>, String> {
    let trust = offer.trust.as_ref().map_err(Refusal::note)?;
    let keys: Vec<_> = trust
        .keys()
        .iter()
        .filter_map(|key| verify::key(key).ok())
        .collect();
    let pick = &offer.pick;
    let archive = client
        .archive(&pick.archive_url)
        .map_err(|e| e.to_string())?;
    let signature = client
        .signature(&pick.signature_url)
        .map_err(|e| e.to_string())?;
    verify::check(&keys, &archive, &signature, &pick.tag, &pick.archive).map_err(|why| {
        if why == verify::UNSIGNED {
            format!("refused: {}", trust.refusal())
        } else {
            format!("refused: {why}")
        }
    })?;
    let installed = Some(offer.file.as_str()).filter(|file| !file.is_empty());
    let taken = taken(&offer.id, &enshwick_loader_api::mods());
    stage::stage(
        &archive,
        installed,
        &taken,
        &context.game,
        &context.data.join(SCRATCH),
    )
}

/// The files an update of `id` must not write: those of every other mod, and
/// the loader's own unless the update is the loader's.
fn taken(id: &str, installed: &[ModEntry]) -> Vec<String> {
    let mut taken: Vec<String> = installed
        .iter()
        .filter(|m| m.id != id && !m.file.is_empty())
        .map(|m| m.file.clone())
        .collect();
    if id != LOADER_ID {
        taken.extend(LOADER_FILES.iter().map(|&file| file.to_owned()));
    }
    taken
}

/// Runs `work` on a thread of its own; a panic only ends that thread.
fn spawn(name: &'static str, work: fn()) {
    let spawned = std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            if std::panic::catch_unwind(work).is_err() {
                enshwick_loader_api::warn!("{name}: stopped by a panic");
            }
        });
    if let Err(e) = spawned {
        enshwick_loader_api::warn!("cannot start {name}: {e}");
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
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(
            id: "updater",
            side: BOTH,
            name: "Updater",
            updates: github("themaiby/enshwick", "enshwick-updater"),
            signed_by: ["RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF"],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        let client = load.host().build().side == Side::CLIENT;
        if client {
            ask::set(load.ui()?);
        }
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
        spawn("enshwick-updater-table", fetch_table);
        if !client {
            return Ok(Self);
        }
        if load
            .on::<JoinRefused>(|()| spawn("enshwick-updater-join", after_refused_join))
            .is_err()
        {
            enshwick_loader_api::warn!(
                "this loader does not report a server's missing mods; update the loader to get them here"
            );
        }
        spawn("enshwick-updater", at_start);
        Ok(Self)
    }
}

enshwick_loader_api::declare_mod!(Updater);

#[cfg(test)]
mod tests {
    use super::*;

    fn listed(id: &str, file: &str) -> ModEntry {
        ModEntry {
            id: id.into(),
            version: "1.0.0".into(),
            side: Side::BOTH,
            join: true,
            running: true,
            file: file.into(),
            updates: None,
            signed_by: Vec::new(),
            ..ModEntry::default()
        }
    }

    #[test]
    fn an_update_never_writes_another_mods_or_the_loaders_files() {
        let installed = [
            listed("enshwick", "mods/enshwick/enshwick.dll"),
            listed("safes", "mods/enshwick-safes.dll"),
            listed("updater", "mods/enshwick-updater.dll"),
        ];
        let taken = taken("safes", &installed);
        assert!(!taken.contains(&"mods/enshwick-safes.dll".to_owned()));
        assert!(taken.contains(&"mods/enshwick-updater.dll".to_owned()));
        assert!(taken.contains(&"powrprof.dll".to_owned()));
        assert!(taken.contains(&"mods/enshwick/enshwick.dll".to_owned()));
        let loader = super::taken("enshwick", &installed);
        assert!(!loader.contains(&"powrprof.dll".to_owned()));
        assert!(!loader.contains(&"mods/enshwick/enshwick.dll".to_owned()));
    }

    #[test]
    fn the_default_settings_parse() {
        use enshwick_loader_api::config::Config;
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert!(settings.enabled);
        assert_eq!(settings.check_interval_hours, 24);
        assert!(!settings.prereleases);
        assert!(settings.validate().is_ok());
        assert!(settings.trusted().is_empty());
    }

    #[test]
    fn the_updater_declares_the_key_it_ships() {
        let manifest = Updater::manifest();
        // SAFETY: the manifest's static strings.
        let declared = unsafe { manifest.signed_by[0].as_str() };
        assert_eq!(declared, verify::ENSHWICK_KEY.lines().last());
    }

    #[test]
    fn trusted_keys_must_be_minisign_keys() {
        use enshwick_loader_api::config::Config;
        let mut settings = Settings::default();
        settings
            .trusted_keys
            .push("RWSVvUJNE6Z0IzsG2I1G/+JlpjWYHLgP8QVhvMlSWCg0AShBnBmVIt4r".into());
        assert!(settings.validate().is_ok());
        assert_eq!(settings.trusted().len(), 1);
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
            name: String::new(),
            from: None,
            file: String::new(),
            pick: Pick {
                tag: format!("{id}-v{version}"),
                version,
                archive: format!("{id}.zip"),
                archive_url: String::new(),
                signature_url: String::new(),
            },
            source: String::new(),
            trust: Err(Refusal::Unsigned),
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
