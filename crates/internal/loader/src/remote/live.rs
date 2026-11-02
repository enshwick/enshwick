//! The messages in the running game: what a client heard, and how its server answers.

use crate::locks::Relock;
use std::sync::Mutex;
use std::time::Instant;

use enshwick::abi::PlayerInfo;
use enshwick::config::Schema;

use super::{AdmitEditError, Gate, Message, Queries, admit, redacted, texts, values_text};
use crate::host::Loader;

/// When each player last changed a setting.
static LAST_EDIT: Mutex<Option<rustc_hash::FxHashMap<u64, Instant>>> = Mutex::new(None);
/// The rates of the queries of players who are no administrators.
static QUERIES: Mutex<Option<Queries>> = Mutex::new(None);
/// What the client last heard from its server, for the Mods screen.
static HEARD: Mutex<Heard> = Mutex::new(Heard::new());

/// What the client last heard from its server.
#[derive(Debug)]
pub(crate) struct Heard {
    /// The server's mods with settings, `(id, name)`, once it answered.
    pub(crate) mods: Option<Vec<(String, String)>>,
    /// The settings of one mod: id, whether this player may change
    /// them, the schema and the values.
    pub(crate) settings: Option<(String, bool, Schema, String)>,
    /// The last change's outcome.
    pub(crate) result: Option<(bool, String)>,
    /// Counts what was heard, so the screen notices news.
    pub(crate) news: u64,
}

impl Heard {
    const fn new() -> Self {
        Self {
            mods: None,
            settings: None,
            result: None,
            news: 0,
        }
    }
}

/// Reads what the client last heard, under the lock.
pub(crate) fn heard<R>(read: impl FnOnce(&Heard) -> R) -> R {
    read(&HEARD.relock())
}

/// Forgets what was heard, as when the screen asks again.
pub(crate) fn forget() {
    let mut heard = HEARD.relock();
    heard.mods = None;
    heard.settings = None;
    heard.result = None;
}

/// Pushes mod `id`'s new join values to every client that runs Enshwick.
pub(crate) fn push_sync(id: &str, values: &str) {
    crate::net::broadcast_control(&Message::Sync {
        id: id.to_owned(),
        values: values.to_owned(),
    });
}

/// Sends the client of `key`, which just said hello, the join values
/// of every running join mod: the values it read before it joined may
/// be older, and a hosted game advertises none.
pub(crate) fn sync_to(key: u64) -> bool {
    super::sync_messages(crate::host::join_values())
        .iter()
        .all(|message| crate::net::to_player(key, message))
}

/// Takes a message the server sent, on the client.
pub(crate) fn client(message: Message) {
    match message {
        Message::Sync { id, values } => crate::host::apply_server_value(&id, &values),
        Message::Snapshot {
            id,
            admin,
            schema,
            text,
        } => {
            let mut heard = HEARD.relock();
            if id.is_empty() {
                heard.mods = Some(
                    text.lines()
                        .filter_map(|line| line.split_once('\t'))
                        .map(|(id, name)| (id.to_owned(), name.to_owned()))
                        .collect(),
                );
            } else if let Ok(schema) = Schema::parse(&schema) {
                heard.settings = Some((id, admin, schema, text));
            }
            heard.news += 1;
        }
        Message::Result { ok, text } => {
            let mut heard = HEARD.relock();
            heard.result = Some((ok, text));
            heard.news += 1;
        }
        Message::Recipes { first, last, ids } => crate::recipes::received(first, last, &ids),
        Message::ActionsSheet {
            first,
            last,
            id,
            label,
            text,
        } => crate::actions::sheet_heard(first, last, id, label, &text),
        Message::ActionResult { ok, text } => crate::actions::result_heard(ok, text),
        Message::ActionHints { text } => crate::actions::hints_heard(&text),
        Message::Query { .. }
        | Message::Edit { .. }
        | Message::ActionsQuery { .. }
        | Message::ActionRun { .. } => {}
    }
}

/// The connected player with account key `key`.
fn player(loader: &Loader, key: u64) -> Option<PlayerInfo> {
    crate::world::players(loader)
        .ok()?
        .into_iter()
        .find(|p| p.key == enshwick::PlayerKey::new(key))
}

fn name(player: &PlayerInfo) -> String {
    String::from_utf8_lossy(&player.name[..usize::from(player.name_len).min(64)]).into_owned()
}

/// Takes a message a client sent, on the server's game thread; the
/// answer is in the language the client's player reads.
pub(crate) fn server(loader: &Loader, key: u64, message: Message) {
    texts::for_player(key, || answer(loader, key, message));
}

/// Answers a message the client `key` sent.
fn answer(loader: &Loader, key: u64, message: Message) {
    let admin = player(loader, key).is_some_and(|p| p.permissions & PlayerInfo::CAN_KICK_BAN != 0);
    match gate(key, admin, &message) {
        Gate::Pass => {}
        Gate::Busy(answer) => {
            crate::net::to_player(key, &answer);
            return;
        }
        Gate::Drop => return,
    }
    match message {
        Message::Query { id } if id.is_empty() => {
            let mut text = String::new();
            for record in loader
                .mods
                .iter()
                .filter(|m| m.schema.relock().is_some())
                .filter(|m| admin || m.synced.relock().is_some())
            {
                text.push_str(&record.id);
                text.push('\t');
                text.push_str(&record.label());
                text.push('\n');
            }
            crate::net::to_player(
                key,
                &Message::Snapshot {
                    id: String::new(),
                    admin,
                    schema: String::new(),
                    text,
                },
            );
        }
        Message::Query { id } => snapshot(loader, key, &id, admin),
        Message::Edit {
            id,
            key: setting,
            value,
        } => {
            let reply = edit(loader, key, &id, &setting, &value, admin);
            let ok = reply.is_ok();
            let text = reply.unwrap_or_else(|why| why.reply());
            crate::net::to_player(key, &Message::Result { ok, text });
            snapshot(loader, key, &id, admin);
        }
        Message::ActionsQuery { language } => crate::actions::query(loader, key, language),
        Message::ActionRun { id, action, args } => {
            crate::actions::run(loader, key, &id, &action, &args);
        }
        Message::Sync { .. }
        | Message::Snapshot { .. }
        | Message::Result { .. }
        | Message::Recipes { .. }
        | Message::ActionsSheet { .. }
        | Message::ActionResult { .. }
        | Message::ActionHints { .. } => {}
    }
}

/// What the server does with `message` of the player with account key
/// `key` now.
fn gate(key: u64, admin: bool, message: &Message) -> Gate {
    QUERIES
        .relock()
        .get_or_insert_with(Queries::default)
        .gate(key, admin, message, Instant::now())
}

/// Forgets the player with account key `key`, who left the server.
pub(crate) fn forget_player(key: u64) {
    if let Some(queries) = QUERIES.relock().as_mut() {
        queries.forget(key);
    }
    if let Some(last) = LAST_EDIT.relock().as_mut() {
        last.remove(&key);
    }
}

/// Sends mod `id`'s settings to the client of `key`: the file without
/// secrets for an administrator, the join values for anyone else.
fn snapshot(loader: &Loader, key: u64, id: &str, admin: bool) {
    let Some((_, record)) = loader.find_mod(id) else {
        return;
    };
    let Some(schema) = record.schema.relock().clone() else {
        return;
    };
    let text = if admin {
        let file = crate::worlds::show_text(loader, record);
        redacted(&schema, &file)
    } else {
        record
            .synced
            .relock()
            .clone()
            .map(|values| values_text(&values))
            .unwrap_or_default()
    };
    let mut shown = schema.clone();
    shown.fields.retain(|f| !f.secret);
    let mut message = Message::Snapshot {
        id: id.to_owned(),
        admin,
        schema: shown.to_text(),
        text: text.clone(),
    };
    if message.body().len() > crate::net::MAX_CONTROL {
        // Too long with its documentation: send the schema bare.
        for field in &mut shown.fields {
            field.doc.clear();
        }
        message = Message::Snapshot {
            id: id.to_owned(),
            admin,
            schema: shown.to_text(),
            text,
        };
    }
    if message.body().len() > crate::net::MAX_CONTROL {
        crate::net::to_player(
            key,
            &Message::Result {
                ok: false,
                text: texts::fill("remote.too-long", &[("id", id)]),
            },
        );
        return;
    }
    crate::net::to_player(key, &message);
}

fn edit(
    loader: &Loader,
    key: u64,
    id: &str,
    setting: &str,
    value: &str,
    admin: bool,
) -> Result<String, EditSettingError> {
    let (index, record) = loader
        .find_mod(id)
        .ok_or_else(|| EditSettingError::UnknownMod { id: id.to_owned() })?;
    let schema = record
        .schema
        .relock()
        .clone()
        .ok_or_else(|| EditSettingError::NoSchema { id: id.to_owned() })?;
    let now = Instant::now();
    {
        let mut last = LAST_EDIT.relock();
        let last = last.get_or_insert_with(rustc_hash::FxHashMap::default);
        admit(&schema, setting, admin, last.get(&key).copied(), now)?;
        last.insert(key, now);
    }
    let who = player(loader, key).map_or_else(|| format!("{key:016x}"), |p| name(&p));
    let change = crate::configure::apply(loader, index, setting, value, &who)?;
    Ok(crate::configure::changed(id, setting, &change))
}

/// Why the server did not change a setting a client sent.
#[derive(Debug, thiserror::Error)]
enum EditSettingError {
    /// No mod of the id runs.
    #[error("no mod {id} runs")]
    UnknownMod { id: String },
    /// The mod has no settings.
    #[error("{id} has no settings")]
    NoSchema { id: String },
    /// The change is refused before it reaches the file.
    #[error(transparent)]
    Admit(#[from] AdmitEditError),
    /// The change was not written.
    #[error(transparent)]
    Apply(#[from] crate::configure::ApplySettingError),
}

impl EditSettingError {
    /// The reply for the client, in its player's language.
    fn reply(&self) -> String {
        match self {
            Self::UnknownMod { id } => texts::fill("remote.unknown", &[("id", id)]),
            Self::NoSchema { id } => texts::fill("remote.no-schema", &[("id", id)]),
            Self::Admit(error) => error.reply(),
            Self::Apply(error) => error.reply(),
        }
    }
}
