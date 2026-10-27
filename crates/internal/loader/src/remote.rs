//! Mod settings across the wire: a server pushes changed join values to the
//! clients joined to it, and an administrator's client reads and changes a
//! dedicated server's settings from its Mods screen.
//!
//! The messages ride the loader's control frames. The server decides
//! everything: it checks the sender's permissions on its own player record,
//! never sends a field marked `secret`, and changes a setting only through
//! the same checks as `enshwick config` on the console.

use std::time::{Duration, Instant};

use enshwick::config::Schema;

use crate::texts;
use crate::wire::Peer;

/// The control frame kinds, after the protocol's notice (3), item data (4)
/// and demo (5).
pub const SYNC: u8 = 6;
/// A client asks for a mod's settings.
pub const QUERY: u8 = 7;
/// The server answers with a mod's settings.
pub const SNAPSHOT: u8 = 8;
/// A client changes one setting.
pub const EDIT: u8 = 9;
/// The server answers a change.
pub const RESULT: u8 = 10;
/// The server's disabled recipes.
pub const RECIPES: u8 = 11;
/// A client asks for the actions its player may use.
pub const ACTIONS_QUERY: u8 = 12;
/// The server's actions of one mod for the player.
pub const ACTIONS_SHEET: u8 = 13;
/// A client runs an action.
pub const ACTION_RUN: u8 = 14;
/// The server's answer to a run.
pub const ACTION_RESULT: u8 = 15;
/// The actions with a hint the player has at hand.
pub const ACTION_HINTS: u8 = 16;

/// The least time between two changes one player sends.
pub const EDIT_GAP: Duration = Duration::from_millis(250);
/// The [rated](Message::is_rated) messages a player who is no administrator
/// sends per second, and in one burst.
///
/// The server answers each with up to a whole control message, so the frame
/// rate alone would let a player make it send far more than the player sends.
pub const QUERY_RATE: f64 = 2.0;
/// See [`QUERY_RATE`].
pub const QUERY_BURST: f64 = 8.0;

/// A message between a client's loader and its server's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    /// The server's new join values of mod `id`, an inline TOML table.
    Sync {
        /// The mod.
        id: String,
        /// Its join values.
        values: String,
    },
    /// A client asks for mod `id`'s settings.
    Query {
        /// The mod.
        id: String,
    },
    /// The server's settings of mod `id`, with its schema without secret
    /// fields: the whole file without them for an administrator, the join
    /// values alone for anyone else. For an empty `id`, the mods with
    /// settings, a line `id<TAB>name` each.
    Snapshot {
        /// The mod.
        id: String,
        /// Whether the asker may change them.
        admin: bool,
        /// The mod's schema as text.
        schema: String,
        /// The settings as TOML.
        text: String,
    },
    /// A client changes mod `id`'s `key` to `value`, TOML.
    Edit {
        /// The mod.
        id: String,
        /// The setting.
        key: String,
        /// The value.
        value: String,
    },
    /// The server's answer to a change.
    Result {
        /// Whether it was written.
        ok: bool,
        /// What the server said.
        text: String,
    },
    /// A part of the server's disabled recipes, by game id.
    Recipes {
        /// Whether it is the first part: the client forgets earlier parts.
        first: bool,
        /// Whether it is the last part: the list is complete.
        last: bool,
        /// The recipes of this part.
        ids: Vec<u32>,
    },
    /// A client asks for its player's actions, in the language it reads.
    ActionsQuery {
        /// The client's language.
        language: u32,
    },
    /// The actions of mod `id` the player may use, as a sheet.
    ActionsSheet {
        /// Whether it is the first mod: the client forgets earlier sheets.
        first: bool,
        /// Whether it is the last mod.
        last: bool,
        /// The mod; empty when no mod has actions.
        id: String,
        /// Its name.
        label: String,
        /// The sheet's text.
        text: String,
    },
    /// A client runs mod `id`'s action `action` with `args`.
    ActionRun {
        /// The mod.
        id: String,
        /// The action.
        action: String,
        /// The arguments, in the sheet's encoding.
        args: String,
    },
    /// The server's answer to a run.
    ActionResult {
        /// Whether it was done.
        ok: bool,
        /// What the player reads.
        text: String,
    },
    /// The actions with a hint the player has at hand: a line
    /// `mod<TAB>action<TAB>label<TAB>subject` each.
    ActionHints {
        /// The lines.
        text: String,
    },
}

fn put(out: &mut Vec<u8>, text: &str) {
    let len = u16::try_from(text.len()).unwrap_or(u16::MAX);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&text.as_bytes()[..usize::from(len)]);
}

fn take<'a>(bytes: &mut &'a [u8]) -> Option<&'a str> {
    let (len, rest) = bytes.split_first_chunk::<2>()?;
    let len = usize::from(u16::from_le_bytes(*len));
    let text = rest.get(..len)?;
    *bytes = &rest[len..];
    core::str::from_utf8(text).ok()
}

impl Message {
    /// The message's kind byte.
    #[must_use]
    pub const fn kind(&self) -> u8 {
        match self {
            Self::Sync { .. } => SYNC,
            Self::Query { .. } => QUERY,
            Self::Snapshot { .. } => SNAPSHOT,
            Self::Edit { .. } => EDIT,
            Self::Result { .. } => RESULT,
            Self::Recipes { .. } => RECIPES,
            Self::ActionsQuery { .. } => ACTIONS_QUERY,
            Self::ActionsSheet { .. } => ACTIONS_SHEET,
            Self::ActionRun { .. } => ACTION_RUN,
            Self::ActionResult { .. } => ACTION_RESULT,
            Self::ActionHints { .. } => ACTION_HINTS,
        }
    }

    /// Whether the server answers it with a reply of its own making, so a
    /// player who is no administrator sends it at [`QUERY_RATE`] only.
    #[must_use]
    pub const fn is_rated(&self) -> bool {
        matches!(
            self,
            Self::Query { .. } | Self::Edit { .. } | Self::ActionsQuery { .. }
        )
    }

    /// The message's body, after its kind and the protocol version.
    #[must_use]
    pub fn body(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            Self::Sync { id, values } => {
                put(&mut out, id);
                put(&mut out, values);
            }
            Self::Query { id } => put(&mut out, id),
            Self::Snapshot {
                id,
                admin,
                schema,
                text,
            } => {
                put(&mut out, id);
                out.push(u8::from(*admin));
                put(&mut out, schema);
                put(&mut out, text);
            }
            Self::Edit { id, key, value } => {
                put(&mut out, id);
                put(&mut out, key);
                put(&mut out, value);
            }
            Self::Result { ok, text } | Self::ActionResult { ok, text } => {
                out.push(u8::from(*ok));
                put(&mut out, text);
            }
            Self::Recipes { first, last, ids } => {
                out.push(u8::from(*first) | (u8::from(*last) << 1));
                for id in ids {
                    out.extend_from_slice(&id.to_le_bytes());
                }
            }
            Self::ActionsQuery { language } => out.extend_from_slice(&language.to_le_bytes()),
            Self::ActionsSheet {
                first,
                last,
                id,
                label,
                text,
            } => {
                out.push(u8::from(*first) | (u8::from(*last) << 1));
                put(&mut out, id);
                put(&mut out, label);
                put(&mut out, text);
            }
            Self::ActionRun { id, action, args } => {
                put(&mut out, id);
                put(&mut out, action);
                put(&mut out, args);
            }
            Self::ActionHints { text } => put(&mut out, text),
        }
        out
    }

    /// Reads a message of `kind` from its body.
    #[must_use]
    pub fn read(kind: u8, mut body: &[u8]) -> Option<Self> {
        let bytes = &mut body;
        Some(match kind {
            SYNC => Self::Sync {
                id: take(bytes)?.to_owned(),
                values: take(bytes)?.to_owned(),
            },
            QUERY => Self::Query {
                id: take(bytes)?.to_owned(),
            },
            SNAPSHOT => {
                let id = take(bytes)?.to_owned();
                let (&admin, rest) = bytes.split_first()?;
                *bytes = rest;
                Self::Snapshot {
                    id,
                    admin: admin != 0,
                    schema: take(bytes)?.to_owned(),
                    text: take(bytes)?.to_owned(),
                }
            }
            EDIT => Self::Edit {
                id: take(bytes)?.to_owned(),
                key: take(bytes)?.to_owned(),
                value: take(bytes)?.to_owned(),
            },
            RESULT => {
                let (&ok, rest) = bytes.split_first()?;
                *bytes = rest;
                Self::Result {
                    ok: ok != 0,
                    text: take(bytes)?.to_owned(),
                }
            }
            RECIPES => {
                let (&flags, rest) = bytes.split_first()?;
                let (ids, tail) = rest.as_chunks::<4>();
                if !tail.is_empty() {
                    return None;
                }
                Self::Recipes {
                    first: flags & 1 != 0,
                    last: flags & 2 != 0,
                    ids: ids.iter().map(|id| u32::from_le_bytes(*id)).collect(),
                }
            }
            ACTIONS_QUERY => Self::ActionsQuery {
                language: u32::from_le_bytes(*bytes.first_chunk::<4>()?),
            },
            ACTIONS_SHEET => {
                let (&flags, rest) = bytes.split_first()?;
                *bytes = rest;
                Self::ActionsSheet {
                    first: flags & 1 != 0,
                    last: flags & 2 != 0,
                    id: take(bytes)?.to_owned(),
                    label: take(bytes)?.to_owned(),
                    text: take(bytes)?.to_owned(),
                }
            }
            ACTION_RUN => Self::ActionRun {
                id: take(bytes)?.to_owned(),
                action: take(bytes)?.to_owned(),
                args: take(bytes)?.to_owned(),
            },
            ACTION_RESULT => {
                let (&ok, rest) = bytes.split_first()?;
                *bytes = rest;
                Self::ActionResult {
                    ok: ok != 0,
                    text: take(bytes)?.to_owned(),
                }
            }
            ACTION_HINTS => Self::ActionHints {
                text: take(bytes)?.to_owned(),
            },
            _ => return None,
        })
    }
}

/// The settings file `text` as an administrator sees it remotely: every
/// field the schema marks `secret` taken out.
#[must_use]
pub fn redacted(schema: &Schema, text: &str) -> String {
    let Ok(mut doc) = text.parse::<toml_edit::DocumentMut>() else {
        return String::new();
    };
    for field in schema.fields.iter().filter(|f| f.secret) {
        crate::worlds::remove_at(doc.as_table_mut(), &field.key);
    }
    doc.to_string()
}

/// The join values `values`, an inline table, as a settings text.
#[must_use]
pub fn values_text(values: &str) -> String {
    format!("v = {values}")
        .parse::<toml::Table>()
        .ok()
        .and_then(|mut t| t.remove("v"))
        .and_then(|v| v.as_table().cloned())
        .and_then(|t| toml::to_string(&t).ok())
        .unwrap_or_default()
}

/// Why a remote change is refused before it reaches the settings file.
///
/// # Errors
///
/// The reply for the client: not an administrator, a secret field, or too
/// soon after the last change.
pub fn admit(
    schema: &Schema,
    key: &str,
    admin: bool,
    last: Option<Instant>,
    now: Instant,
) -> Result<(), String> {
    if !admin {
        return Err(texts::fill("remote.admins", &[]));
    }
    if schema.field(key).is_some_and(|f| f.secret) {
        return Err(texts::fill("remote.secret", &[("key", key)]));
    }
    if last.is_some_and(|last| now.duration_since(last) < EDIT_GAP) {
        return Err(texts::fill("remote.busy", &[]));
    }
    Ok(())
}

/// What the server does with a message a client sent, by [`Queries::gate`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Gate {
    /// Serve it.
    Pass,
    /// Send the player this answer instead of serving the message.
    Busy(Message),
    /// Neither serve nor answer it: the client asks again on its own.
    Drop,
}

/// The rates of the [rated](Message::is_rated) messages of players who are
/// no administrators, on the server.
#[derive(Debug, Default)]
pub struct Queries {
    rates: rustc_hash::FxHashMap<u64, Peer>,
}

impl Queries {
    /// Takes one rated message of the player with account key `key` at
    /// `now`; false when the player is over [`QUERY_RATE`]. An
    /// administrator's messages are not rated.
    pub fn admit(&mut self, key: u64, admin: bool, now: Instant) -> bool {
        admin
            || self
                .rates
                .entry(key)
                .or_insert_with(|| Peer::with_tokens(QUERY_BURST))
                .admit_at(now, QUERY_RATE, QUERY_BURST)
    }

    /// Decides whether the server serves `message` of the player with
    /// account key `key` at `now`. Only [rated](Message::is_rated) messages
    /// spend the player's rate; an actions query over it gets no answer.
    pub fn gate(&mut self, key: u64, admin: bool, message: &Message, now: Instant) -> Gate {
        if !message.is_rated() || self.admit(key, admin, now) {
            Gate::Pass
        } else if matches!(message, Message::ActionsQuery { .. }) {
            Gate::Drop
        } else {
            Gate::Busy(Message::Result {
                ok: false,
                text: texts::fill("remote.slow", &[]),
            })
        }
    }

    /// Forgets the player with account key `key`, who left.
    pub fn forget(&mut self, key: u64) {
        self.rates.remove(&key);
    }
}

/// One [`Message::Sync`] per mod of `values`, its join values by mod id.
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "messages go out only in the game")
)]
fn sync_messages(values: std::collections::BTreeMap<String, String>) -> Vec<Message> {
    values
        .into_iter()
        .map(|(id, values)| Message::Sync { id, values })
        .collect()
}

#[cfg(windows)]
pub(crate) use live::{client, forget, forget_player, heard, push_sync, server, sync_to};

#[cfg(windows)]
mod live {
    use crate::locks::Relock;
    use std::sync::Mutex;
    use std::time::Instant;

    use enshwick::abi::PlayerInfo;
    use enshwick::config::Schema;

    use super::{Gate, Message, Queries, admit, redacted, texts, values_text};
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
    pub(crate) fn sync_to(key: u64) {
        for message in super::sync_messages(crate::host::join_values()) {
            crate::net::to_player(key, &message);
        }
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
            .find(|p| p.key == key)
    }

    fn name(player: &PlayerInfo) -> String {
        String::from_utf8_lossy(&player.name[..usize::from(player.name_len).min(64)]).into_owned()
    }

    /// Takes a message a client sent, on the server's game thread.
    pub(crate) fn server(loader: &Loader, key: u64, message: Message) {
        let admin =
            player(loader, key).is_some_and(|p| p.permissions & PlayerInfo::CAN_KICK_BAN != 0);
        match gate(key, admin, &message) {
            Gate::Pass => {}
            Gate::Busy(answer) => return crate::net::to_player(key, &answer),
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
                let text = reply.unwrap_or_else(|why| why);
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
        QUERIES.relock().get_or_insert_with(Queries::default).gate(
            key,
            admin,
            message,
            Instant::now(),
        )
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
    ) -> Result<String, String> {
        let (index, record) = loader
            .find_mod(id)
            .ok_or_else(|| texts::fill("remote.unknown", &[("id", id)]))?;
        let schema = record
            .schema
            .relock()
            .clone()
            .ok_or_else(|| texts::fill("remote.no-schema", &[("id", id)]))?;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_client_that_says_hello_gets_every_mod_s_join_values() {
        let values = std::collections::BTreeMap::from([
            ("you.trading-post".to_owned(), "{cost = 5}".to_owned()),
            ("zones".to_owned(), "{clusters = false}".to_owned()),
        ]);
        let messages = sync_messages(values);
        assert_eq!(messages.len(), 2);
        for message in &messages {
            assert_eq!(
                Message::read(message.kind(), &message.body()).as_ref(),
                Some(message)
            );
        }
        assert!(
            matches!(&messages[0], Message::Sync { id, values } if id == "you.trading-post" && values == "{cost = 5}")
        );
    }
    use enshwick::config::{Field, Stage};

    fn schema() -> Schema {
        Schema::new(
            "",
            "config",
            vec![
                Field::new("limit", Stage::Runtime, &4),
                Field::new("token", Stage::Runtime, "x").with_secret(true),
            ],
        )
    }

    #[test]
    fn messages_round_trip() {
        for message in [
            Message::Sync {
                id: "zones".into(),
                values: "{ a = 1 }".into(),
            },
            Message::Query {
                id: "you.trading-post".into(),
            },
            Message::Snapshot {
                id: "you.trading-post".into(),
                admin: true,
                schema: schema().to_text(),
                text: "limit = 4\n".into(),
            },
            Message::Edit {
                id: "you.trading-post".into(),
                key: "limit".into(),
                value: "6".into(),
            },
            Message::Result {
                ok: false,
                text: "Сейф".into(),
            },
            Message::Recipes {
                first: true,
                last: false,
                ids: vec![1, u32::MAX],
            },
            Message::Recipes {
                first: false,
                last: true,
                ids: Vec::new(),
            },
            Message::ActionsQuery { language: 34 },
            Message::ActionsSheet {
                first: true,
                last: false,
                id: "you.trading-post".into(),
                label: "Trading Post".into(),
                text: "A\taccess\t5\tWho\t\tYour stall\n".into(),
            },
            Message::ActionRun {
                id: "you.trading-post".into(),
                action: "access".into(),
                args: "p0000000000000007\tb1".into(),
            },
            Message::ActionResult {
                ok: true,
                text: "Ann may use it".into(),
            },
            Message::ActionHints {
                text: "you.trading-post\taccess\tWho\tYour stall\n".into(),
            },
        ] {
            assert_eq!(
                Message::read(message.kind(), &message.body()),
                Some(message)
            );
        }
        assert_eq!(Message::read(QUERY, &[5, 0, b'a']), None);
        assert_eq!(Message::read(42, &[]), None);
        assert_eq!(Message::read(RECIPES, &[3, 1, 2]), None);
        let most = Message::Recipes {
            first: true,
            last: true,
            ids: vec![7; 1400],
        };
        // The longest control message the frames carry.
        assert!(most.body().len() + 2 <= 720 * 8 - 2);
    }

    #[test]
    fn secrets_never_leave_and_never_change() {
        let text = "# Limit.\nlimit = 4\ntoken = \"hunter2\"\n";
        let shown = redacted(&schema(), text);
        assert!(
            shown.contains("limit = 4") && !shown.contains("hunter2"),
            "{shown}"
        );
        let now = Instant::now();
        assert!(admit(&schema(), "limit", true, None, now).is_ok());
        assert!(admit(&schema(), "limit", false, None, now).is_err());
        assert!(admit(&schema(), "token", true, None, now).is_err());
        assert!(admit(&schema(), "limit", true, Some(now), now).is_err());
        assert_eq!(values_text("{ a = 1, b = \"x\" }"), "a = 1\nb = \"x\"\n");
    }

    #[test]
    fn secrets_in_inline_tables_never_leave() {
        let schema = Schema::new(
            "",
            "config",
            vec![
                Field::new("auth.token", Stage::Runtime, "x").with_secret(true),
                Field::new("auth.user", Stage::Runtime, "x"),
                Field::new("outer.inner.key", Stage::Runtime, "x").with_secret(true),
                Field::new("outer.keep", Stage::Runtime, &1),
            ],
        );
        for text in [
            "auth = { token = \"hunter2\", user = \"ann\" }\nouter = { inner = { key = \"swordfish\" }, keep = 1 }\n",
            "[auth]\ntoken = \"hunter2\"\nuser = \"ann\"\n[outer]\ninner = { key = \"swordfish\" }\nkeep = 1\n",
            "auth.token = \"hunter2\"\nauth.user = \"ann\"\nouter.inner.key = \"swordfish\"\nouter.keep = 1\n",
        ] {
            let shown = redacted(&schema, text);
            assert!(
                !shown.contains("hunter2") && !shown.contains("swordfish"),
                "{shown}"
            );
            assert!(
                shown.contains("ann") && shown.contains("keep = 1"),
                "{shown}"
            );
        }
    }

    #[test]
    fn a_player_who_is_no_administrator_asks_at_the_query_rate() {
        let mut queries = Queries::default();
        let start = Instant::now();
        let asked = (0..100).filter(|_| queries.admit(7, false, start)).count();
        assert_eq!(f64::from(u32::try_from(asked).unwrap()), QUERY_BURST);
        assert!(!queries.admit(7, false, start));
        assert!(
            queries.admit(8, false, start),
            "each player has a rate of his own"
        );
        let later = start + Duration::from_secs(1);
        let again = (0..100).filter(|_| queries.admit(7, false, later)).count();
        assert_eq!(f64::from(u32::try_from(again).unwrap()), QUERY_RATE);
    }

    #[test]
    fn an_administrator_s_queries_are_not_rated() {
        let mut queries = Queries::default();
        let now = Instant::now();
        assert!((0..1000).all(|_| queries.admit(7, true, now)));
        assert!(queries.rates.is_empty());
    }

    #[test]
    fn a_player_who_left_is_forgotten() {
        let mut queries = Queries::default();
        let now = Instant::now();
        while queries.admit(7, false, now) {}
        queries.forget(7);
        assert!(queries.rates.is_empty());
        assert!(
            queries.admit(7, false, now),
            "a new join starts with a full burst"
        );
    }

    #[test]
    fn queries_and_edits_are_rated_and_answers_are_not() {
        let rated = [
            Message::Query { id: String::new() },
            Message::Edit {
                id: "zones".into(),
                key: "radius".into(),
                value: "5".into(),
            },
            Message::ActionsQuery { language: 0 },
        ];
        assert!(rated.iter().all(Message::is_rated));
        let free = [
            Message::Result {
                ok: true,
                text: String::new(),
            },
            Message::Sync {
                id: "zones".into(),
                values: String::new(),
            },
        ];
        assert!(!free.iter().any(Message::is_rated));
    }

    fn over_the_rate(queries: &mut Queries, key: u64, now: Instant) {
        while queries.admit(key, false, now) {}
    }

    #[test]
    fn a_query_or_edit_over_the_rate_is_answered_busy_and_not_served() {
        let mut queries = Queries::default();
        let now = Instant::now();
        over_the_rate(&mut queries, 7, now);
        let busy = Gate::Busy(Message::Result {
            ok: false,
            text: texts::fill("remote.slow", &[]),
        });
        for message in [
            Message::Query { id: String::new() },
            Message::Edit {
                id: "zones".into(),
                key: "radius".into(),
                value: "5".into(),
            },
        ] {
            assert_eq!(queries.gate(7, false, &message, now), busy);
        }
    }

    #[test]
    fn an_actions_query_over_the_rate_gets_no_answer_at_all() {
        let mut queries = Queries::default();
        let now = Instant::now();
        over_the_rate(&mut queries, 7, now);
        let message = Message::ActionsQuery { language: 0 };
        assert_eq!(queries.gate(7, false, &message, now), Gate::Drop);
    }

    #[test]
    fn rated_messages_pass_until_the_burst_is_spent() {
        let mut queries = Queries::default();
        let now = Instant::now();
        let message = Message::Query { id: String::new() };
        let passed = (0..100)
            .take_while(|_| queries.gate(7, false, &message, now) == Gate::Pass)
            .count();
        assert_eq!(f64::from(u32::try_from(passed).unwrap()), QUERY_BURST);
    }

    #[test]
    fn unrated_messages_and_administrators_always_pass() {
        let mut queries = Queries::default();
        let now = Instant::now();
        over_the_rate(&mut queries, 7, now);
        let run = Message::ActionRun {
            id: "zones".into(),
            action: "clear".into(),
            args: String::new(),
        };
        assert_eq!(queries.gate(7, false, &run, now), Gate::Pass);
        let query = Message::Query { id: String::new() };
        assert!((0..100).all(|_| queries.gate(8, true, &query, now) == Gate::Pass));
    }
}
