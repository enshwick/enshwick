//! Updates Enshwick and its mods from their releases, with the player's
//! consent.
//!
//! At start, on a thread of its own, the updater lists the loaded mods, asks
//! each mod's source for its releases (a GitHub repository, see [`release`],
//! or the document of a `url(...)` source, see [`document`]) and, when newer
//! ones exist, asks the player. What the player accepts is downloaded over
//! `https://`, checked against its minisign signature when the installed
//! version names author keys (see [`trust`]) and staged in `mods/.staged/`,
//! which the `powrprof.dll` stub moves into place at the next start. When
//! the join check keeps the player out of a server that runs mods this game
//! lacks or has in another version, the updater offers the server's versions
//! the same way, a mod the game lacks checked against the author key the
//! server names for it, if any. Settings files are never touched.
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
mod say;

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use enshwick_loader_api::abi::{BuildTable, Side};
use enshwick_loader_api::events::JoinRefused;
use enshwick_loader_api::i18n::Language;
use enshwick_loader_api::mods::{LOADER_FILES, LOADER_ID, ModEntry, Updates};
use enshwick_loader_api::{Load, Mod, Off};
use semver::Version;

use crate::document::Document;
use crate::net::{Client, Failure};
use crate::release::Pick;
use crate::trust::Trust;

/// How long after start the updater looks, so the game's window is up.
const START_DELAY: Duration = Duration::from_secs(15);
/// The file under the data folder that holds when the last check ran.
const LAST_CHECK: &str = "last-check";
/// Where archives are unpacked before they are staged, under the data folder.
const SCRATCH: &str = "incoming";
/// Where GitHub's release lists are kept with their `ETag`, under the data
/// folder.
const CACHE: &str = "cache";
/// The file under the data folder that holds until when GitHub refused more
/// requests.
const PAUSE: &str = "github-pause";

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
/// The updates the Mods screen shows, and the ids staged this session.
static SCREEN: Mutex<(Vec<Offer>, Vec<String>)> = Mutex::new((Vec::new(), Vec::new()));

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
    /// The keys one of which must have signed it; none for an unsigned mod.
    pub trust: Trust,
}

impl Offer {
    /// The offer's line in a question: `Name (id): from → to`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_updater::Offer;
    /// use enshwick_updater::release::Pick;
    /// use enshwick_updater::trust::Trust;
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
    ///         signature_url: None,
    ///     },
    ///     source: "github.com/themaiby/enshwick".into(),
    ///     trust: Trust::Installed(vec!["RW...".into()]),
    /// };
    /// assert_eq!(offer.line(), "Safes (safes): 0.1.0 \u{2192} 0.2.0");
    /// let ukrainian = offer.line_in(enshwick_loader_api::i18n::Language::UK_UA);
    /// assert_eq!(ukrainian, "Safes (safes): 0.1.0 \u{2192} 0.2.0");
    /// ```
    ///
    /// A version older than the installed one says so, and so does an
    /// unsigned one:
    ///
    /// ```
    /// # use enshwick_updater::Offer;
    /// # use enshwick_updater::release::Pick;
    /// # use enshwick_updater::trust::Trust;
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
    /// #         signature_url: None,
    /// #     },
    /// #     source: String::new(),
    /// #     trust: Trust::Unsigned,
    /// # };
    /// assert_eq!(offer.line(), "safes: 0.3.0 \u{2192} 0.2.0 (older, unsigned)");
    /// ```
    #[must_use]
    pub fn line(&self) -> String {
        self.line_in(Language::FALLBACK)
    }

    /// [`Offer::line`] in `language`, as the player reads it.
    ///
    /// # Examples
    ///
    /// See [`Offer::line`].
    #[must_use]
    pub fn line_in(&self, language: Language) -> String {
        let older = self
            .from
            .as_deref()
            .and_then(|from| Version::parse(from).ok())
            .is_some_and(|from| self.pick.version < from);
        let missing = say::in_language("line.missing", language, &[]);
        let to = self.pick.version.to_string();
        let key = match (older, self.trust.signed()) {
            (false, true) => "line",
            (true, true) => "line.older",
            (false, false) => "line.unsigned",
            (true, false) => "line.older-unsigned",
        };
        say::in_language(
            key,
            language,
            &[
                ("name", &self.label()),
                ("from", self.from.as_deref().unwrap_or(&missing)),
                ("to", &to),
            ],
        )
    }

    /// [`Offer::line`] in the player's language.
    fn said(&self) -> String {
        self.line_in(enshwick_loader_api::i18n::language())
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
    /// names, or that it is unsigned.
    fn server_lines(&self) -> String {
        let signed = if self.trust.signed() {
            say::text("server.key", &[("keys", &self.trust.keys().join(", "))])
        } else {
            say::text("server.unsigned", &[])
        };
        say::text(
            "server.line",
            &[
                ("name", &self.label()),
                ("version", &self.pick.version.to_string()),
                ("source", &self.source),
                ("signed", &signed),
            ],
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

pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// Until when GitHub refused more requests, from the record in `data`;
/// `None` when it takes them.
fn paused(data: &Path) -> Option<u64> {
    std::fs::read_to_string(data.join(PAUSE))
        .ok()
        .and_then(|text| text.trim().parse::<u64>().ok())
        .filter(|&until| until > now())
}

/// Records that GitHub takes no requests before `until`.
fn pause(data: &Path, until: u64) {
    let record = data.join(PAUSE);
    if let Err(e) = std::fs::write(&record, until.to_string()) {
        enshwick_loader_api::warn!("cannot record the pause in {}: {e}", record.display());
    }
}

/// A client that keeps GitHub's lists in the data folder.
fn client(context: &Context) -> Client {
    Client::new(Some(&context.data.join(CACHE)))
}

/// What the player reads about a failed request.
fn said(failure: &Failure) -> String {
    match failure {
        Failure::RateLimited { until } => say::text(
            "why.rate-limited",
            &[("minutes", &net::minutes_until(*until, now()).to_string())],
        ),
        other => other.to_string(),
    }
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
    let is_due = due(last, now(), context.settings.check_interval_hours);
    let paused = paused(&context.data);
    if !is_due {
        enshwick_loader_api::info!(
            "checked within the last {} hours; not checking again yet",
            context.settings.check_interval_hours
        );
    } else if let Some(until) = paused {
        enshwick_loader_api::info!(
            "GitHub refused more requests for {} more minutes; checking at a later start",
            net::minutes_until(until, now())
        );
    }
    // A start that does not check still shows updates on the Mods screen,
    // from the lists kept at the last check.
    let online = is_due && paused.is_none();
    let client = if online {
        client(&context)
    } else {
        Client::kept(&context.data.join(CACHE))
    };
    let installed = enshwick_loader_api::mods();
    let (offers, answered) = match newer(&client, &context, &installed) {
        Ok(checked) => checked,
        Err(failure) => {
            if let Failure::RateLimited { until } = failure {
                pause(&context.data, until);
            }
            enshwick_loader_api::warn!("no update check this time: {failure}");
            return;
        }
    };
    publish(offers.clone());
    if !online {
        return;
    }
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
    let question = format!(
        "{}\n\n{}\n\n{}",
        say::text("available.list", &[]),
        said_lines(&offers),
        say::text("available.ask", &[])
    );
    if !ask::confirm(&question) {
        enshwick_loader_api::info!("the player declined {}", lines(&offers).replace('\n', ", "));
        return;
    }
    let staged = install_all(&client, &context, &offers, &say::text("next-start", &[]));
    let mut screen = SCREEN.lock().unwrap_or_else(PoisonError::into_inner);
    screen.1.extend(staged);
    show(&screen);
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
    let client = client(&context);
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
        ask::tell(&format!(
            "{}\n\n{}",
            say::text("table.fetched", &[]),
            say::text("table.restart", &[])
        ));
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
    let client = client(&context);
    let installed = enshwick_loader_api::mods();
    let trusted = context.settings.trusted();
    let mut offers = Vec::new();
    let mut unavailable = Vec::new();
    for entry in &missing {
        match server_version(&client, &context, entry, &installed, &trusted) {
            Ok(offer) => offers.push(offer),
            Err(why) => {
                enshwick_loader_api::warn!("{} {}: {why}", entry.id, entry.version);
                unavailable.push(format!("{} {}: {why}", entry.label(), entry.version));
            }
        }
    }
    let _busy = BUSY.lock().unwrap_or_else(PoisonError::into_inner);
    let offers = unoffered(offers);
    let runs = say::text("server.runs", &[]);
    let unavailable = (!unavailable.is_empty()).then(|| {
        format!(
            "{}\n{}",
            say::text("unavailable", &[]),
            unavailable.join("\n")
        )
    });
    if offers.is_empty() {
        if let Some(unavailable) = unavailable {
            ask::tell(&format!("{runs}\n\n{unavailable}"));
        }
        return;
    }
    let (updates, new): (Vec<&Offer>, Vec<&Offer>) =
        offers.iter().partition(|offer| offer.from.is_some());
    let mut question = vec![runs];
    if !updates.is_empty() {
        question.push(format!(
            "{}\n\n{}",
            say::text("server.updates", &[]),
            updates
                .iter()
                .map(|offer| offer.said())
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !new.is_empty() {
        question.push(format!(
            "{}\n\n{}\n\n{}",
            say::text("server.new", &[]),
            new.iter()
                .map(|offer| offer.server_lines())
                .collect::<Vec<_>>()
                .join("\n"),
            say::text("server.trust", &[])
        ));
    }
    question.extend(unavailable);
    question.push(say::text("server.ask", &[]));
    let question = question.join("\n\n");
    if !ask::confirm(&question) {
        enshwick_loader_api::info!(
            "the player declined the server's {}",
            lines(&offers).replace('\n', ", ")
        );
        return;
    }
    install_all(&client, &context, &offers, &say::text("server.next", &[]));
}

/// The offer of the server's version of `entry`, a mod the join check found
/// missing; why not, as the player reads it.
fn server_version(
    client: &Client,
    context: &Context,
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
    let signed = trust.signed();
    let pick = match updates {
        Some(Updates::GitHub {
            repository,
            component,
        }) => {
            if let Some(until) = paused(&context.data) {
                return Err(said(&Failure::RateLimited { until }));
            }
            let found = |releases: &[release::Release]| {
                release::exact(releases, repository, component, &version, signed)
            };
            let releases = releases_of(client, repository, |releases| found(releases).is_some())
                .map_err(|failure| match failure {
                    Failure::RateLimited { until } => {
                        pause(&context.data, until);
                        said(&failure)
                    }
                    other => other.to_string(),
                })?;
            found(&releases).ok_or_else(|| {
                say::text(
                    "why.no-release",
                    &[
                        ("repository", repository),
                        ("component", component),
                        ("version", &version.to_string()),
                    ],
                )
            })?
        }
        Some(Updates::Url(url)) => {
            let document = fetch_document(client, url)?;
            document::exact(&document, &entry.id, &version, signed).ok_or_else(|| {
                say::text(
                    "why.not-listed",
                    &[("url", url), ("version", &version.to_string())],
                )
            })?
        }
        None => return Err(say::text("why.no-source", &[])),
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

/// The releases of `repository`, page by page until the pages read so far
/// are `enough`.
///
/// # Errors
///
/// When a page fails, or is not a release list.
fn releases_of(
    client: &Client,
    repository: &str,
    enough: impl Fn(&[release::Release]) -> bool,
) -> Result<Vec<release::Release>, Failure> {
    let mut releases = Vec::new();
    let mut unreadable = None;
    client.api_pages(&release::list_url(repository), |page| {
        match release::parse(page) {
            Ok(more) => releases.extend(more),
            Err(e) => unreadable = Some(e),
        }
        unreadable.is_some() || enough(&releases)
    })?;
    match unreadable {
        Some(e) => Err(Failure::Other(format!("{repository}: {e}"))),
        None => Ok(releases),
    }
}

/// Newer releases of `mods`, one list per repository and one request per
/// document, and whether every source answered.
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
        let trust = trust::for_installed(entry, &context.settings.trusted());
        let signed = trust.signed();
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
                    .and_then(|d| document::newest(d, &entry.id, &installed, prereleases, signed))
                {
                    offers.push(Offer {
                        id: entry.id.clone(),
                        name: entry.name.clone(),
                        from: Some(entry.version.clone()),
                        file: entry.file.clone(),
                        pick,
                        source: source_of(updates),
                        trust,
                    });
                }
                continue;
            }
        };
        if !lists.contains_key(repository.as_str()) {
            let components = components_of(mods, repository);
            let seen = |releases: &[release::Release]| release::seen_all(releases, &components);
            let releases = match releases_of(client, repository, seen) {
                Ok(releases) => releases,
                Err(failure @ Failure::RateLimited { .. }) => return Err(failure),
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
        if let Some(pick) = release::newest(
            releases,
            repository,
            component,
            &installed,
            prereleases,
            signed,
        ) {
            offers.push(Offer {
                id: entry.id.clone(),
                name: entry.name.clone(),
                from: Some(entry.version.clone()),
                file: entry.file.clone(),
                pick,
                source: source_of(updates),
                trust,
            });
        }
    }
    Ok((offers, answered))
}

/// The update `offer` as the Mods screen shows it.
fn update(offer: &Offer, staged: bool) -> enshwick_loader_api::updates::Update {
    let note = if staged {
        say::text("pick.staged", &[])
    } else if offer.trust.signed() {
        String::new()
    } else {
        say::text("pick.unsigned", &[])
    };
    enshwick_loader_api::updates::Update::new(&offer.id, &offer.pick.version.to_string())
        .note(&note)
        .ready(!staged)
}

/// Shows `offers` on the Mods screen, in place of the updates before.
fn publish(offers: Vec<Offer>) {
    let mut screen = SCREEN.lock().unwrap_or_else(PoisonError::into_inner);
    screen.0 = offers;
    show(&screen);
}

fn show(screen: &(Vec<Offer>, Vec<String>)) {
    let updates: Vec<_> = screen
        .0
        .iter()
        .map(|offer| update(offer, screen.1.contains(&offer.id)))
        .collect();
    if let Err(off) = enshwick_loader_api::updates::publish(&updates) {
        enshwick_loader_api::info!("the Mods screen shows no updates: {off}");
    }
}

/// The player pressed Update under mod `id` on the Mods screen: asks, then
/// downloads it as an update from the start check would be.
fn picked(id: &str) {
    let Some(context) = context() else {
        return;
    };
    let offer = {
        let screen = SCREEN.lock().unwrap_or_else(PoisonError::into_inner);
        if screen.1.iter().any(|staged| staged == id) {
            return;
        }
        screen.0.iter().find(|offer| offer.id == id).cloned()
    };
    let Some(offer) = offer else {
        return;
    };
    let _busy = BUSY.lock().unwrap_or_else(PoisonError::into_inner);
    if !ask::confirm(&say::text("pick.ask", &[("line", &offer.said())])) {
        enshwick_loader_api::info!("the player declined {} on the Mods screen", offer.line());
        return;
    }
    let staged = install_all(
        &client(&context),
        &context,
        &[offer],
        &say::text("next-start", &[]),
    );
    let mut screen = SCREEN.lock().unwrap_or_else(PoisonError::into_inner);
    screen.1.extend(staged);
    show(&screen);
}

/// The components of `mods` released from `repository`.
fn components_of<'a>(mods: &'a [ModEntry], repository: &str) -> Vec<&'a str> {
    mods.iter()
        .filter_map(|m| match &m.updates {
            Some(Updates::GitHub {
                repository: from,
                component,
            }) if from == repository => Some(component.as_str()),
            _ => None,
        })
        .collect()
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

/// The offers' lines for the log.
fn lines(offers: &[Offer]) -> String {
    offers
        .iter()
        .map(Offer::line)
        .collect::<Vec<_>>()
        .join("\n")
}

/// The offers' lines in the player's language.
fn said_lines(offers: &[Offer]) -> String {
    offers
        .iter()
        .map(Offer::said)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Downloads, checks and stages every offer, then tells the player what
/// came of it, ending with `next`; the ids staged.
fn install_all(client: &Client, context: &Context, offers: &[Offer], next: &str) -> Vec<String> {
    let mut staged = Vec::new();
    let mut done = Vec::new();
    let mut failed = Vec::new();
    let replaced = enshwick_loader_api::mods::replaced();
    for offer in offers {
        match install(client, context, offer) {
            Ok(files) => {
                enshwick_loader_api::info!("staged {}: {}", offer.line(), files.join(", "));
                staged.push(offer.id.clone());
                done.push(offer.said());
                let taken = taken("", &enshwick_loader_api::mods());
                let (deleted, kept) =
                    stage::delete_replaced(&context.game, &offer.id, &replaced, &taken);
                for file in deleted {
                    enshwick_loader_api::info!("deleted {file}, which {} replaces", offer.id);
                    done.push(say::text(
                        "deleted",
                        &[("file", &file), ("name", &offer.label())],
                    ));
                }
                for why in kept {
                    enshwick_loader_api::warn!("not deleted: {why}");
                }
            }
            Err(why) => {
                enshwick_loader_api::warn!("did not install {}: {why}", offer.line());
                failed.push(format!("{}: {why}", offer.said()));
            }
        }
    }
    let mut text = Vec::new();
    if !done.is_empty() {
        text.push(format!(
            "{}\n\n{}\n\n{next}",
            say::text("downloaded", &[]),
            done.join("\n")
        ));
    }
    if !failed.is_empty() {
        text.push(format!(
            "{}\n\n{}\n\n{}",
            say::text("not-installed", &[]),
            failed.join("\n"),
            say::text("see-log", &[])
        ));
    }
    ask::tell(&text.join("\n\n"));
    staged
}

fn install(client: &Client, context: &Context, offer: &Offer) -> Result<Vec<String>, String> {
    let (trust, pick) = (&offer.trust, &offer.pick);
    let archive = client.archive(&pick.archive_url).map_err(|e| said(&e))?;
    if trust.signed() {
        let keys: Vec<_> = trust
            .keys()
            .iter()
            .filter_map(|key| verify::key(key).ok())
            .collect();
        let url = pick
            .signature_url
            .as_deref()
            .ok_or_else(|| say::text("why.no-signature", &[]))?;
        let signature = client.signature(url).map_err(|e| said(&e))?;
        verify::check(&keys, &archive, &signature, &pick.tag, &pick.archive).map_err(|why| {
            let why = if why == verify::UNSIGNED {
                say::text(trust.refusal_key(), &[])
            } else {
                why
            };
            say::text("why.refused", &[("why", &why)])
        })?;
    }
    let installed = Some(offer.file.as_str()).filter(|file| !file.is_empty());
    let taken = taken(&offer.id, &enshwick_loader_api::mods());
    let download = stage::Download {
        name: &pick.archive,
        bytes: &archive,
    };
    stage::stage(
        download,
        installed,
        !trust.signed(),
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
fn spawn(name: &'static str, work: impl FnOnce() + Send + std::panic::UnwindSafe + 'static) {
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
        say::set(load.texts(say::EMBEDDED)?);
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
        let picks = load.on_update_pick(|id| {
            let id = id.to_owned();
            spawn("enshwick-updater-pick", move || picked(&id));
        });
        if let Err(off) = picks {
            enshwick_loader_api::info!("no Update buttons on the Mods screen: {off}");
        }
        spawn("enshwick-updater", at_start);
        Ok(Self)
    }
}

enshwick_loader_api::declare_mod!(Updater);

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads Enshwick's own release list from GitHub, past its first page;
    /// online, so only with `ENSHWICK_ONLINE_TESTS` set.
    #[test]
    fn a_long_release_list_is_read_page_by_page() {
        if std::env::var_os("ENSHWICK_ONLINE_TESTS").is_none() {
            return;
        }
        let cache = std::env::temp_dir().join(format!("enshwick-pages-{}", std::process::id()));
        let client = Client::new(Some(&cache));
        let seen = |components: &'static [&'static str]| {
            move |releases: &[release::Release]| release::seen_all(releases, components)
        };
        let all = releases_of(&client, "themaiby/enshwick", seen(&["no-such-component"])).unwrap();
        assert!(all.len() > 100, "{} releases", all.len());
        let again =
            releases_of(&client, "themaiby/enshwick", seen(&["no-such-component"])).unwrap();
        assert_eq!(again.len(), all.len(), "the kept pages read the same");
        let first = releases_of(&client, "themaiby/enshwick", seen(&["enshwick-updater"])).unwrap();
        assert!(first.len() <= all.len());
        assert!(release::seen_all(&first, &["enshwick-updater"]));
        std::fs::remove_dir_all(&cache).unwrap();
    }

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
                signature_url: None,
            },
            source: String::new(),
            trust: Trust::Installed(Vec::new()),
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

    #[test]
    fn an_unsigned_update_shows_ready_and_marked() {
        let unsigned = Offer {
            trust: Trust::Unsigned,
            ..offer("marked", Version::new(1, 0, 0))
        };
        let shown = update(&unsigned, false);
        assert!(shown.ready);
        assert_eq!(shown.note, say::text("pick.unsigned", &[]));
        assert!(
            !update(&unsigned, true).ready,
            "a staged update is not offered again"
        );
        let signed = update(&offer("signed", Version::new(1, 0, 0)), false);
        assert!(signed.ready && signed.note.is_empty());
        assert!(unsigned.line().ends_with("(unsigned)"));
    }
}
