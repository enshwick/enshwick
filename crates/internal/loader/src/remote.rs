//! Mod settings across the wire: a server pushes changed join values to the
//! clients joined to it, and an administrator's client reads and changes a
//! dedicated server's settings from its Mods screen.
//!
//! The messages ride the loader's control frames. The server decides
//! everything: it checks the sender's permissions on its own player record,
//! never sends a field marked `secret`, and changes a setting only through
//! the same checks as `/config`.

use std::time::{Duration, Instant};

use enshwick_loader_api::config::Schema;

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

/// The least time between two changes one player sends.
pub const EDIT_GAP: Duration = Duration::from_millis(250);

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
        }
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
            Self::Result { ok, text } => {
                out.push(u8::from(*ok));
                put(&mut out, text);
            }
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
            _ => return None,
        })
    }
}

/// The settings file `text` as an administrator sees it remotely: every
/// field the schema marks `secret` taken out.
#[must_use]
pub fn without_secrets(schema: &Schema, text: &str) -> String {
    let Ok(mut doc) = text.parse::<toml_edit::DocumentMut>() else {
        return String::new();
    };
    for field in schema.fields.iter().filter(|f| f.secret) {
        doc.remove(&field.key);
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
        return Err("Only administrators change the server's settings.".to_owned());
    }
    if schema.field(key).is_some_and(|f| f.secret) {
        return Err(format!("{key} is changed in the server's file only."));
    }
    if last.is_some_and(|last| now.duration_since(last) < EDIT_GAP) {
        return Err("Too many changes at once; try again.".to_owned());
    }
    Ok(())
}

#[cfg(windows)]
pub(crate) use live::{client, forget, heard, push_sync, server};

#[cfg(windows)]
mod live {
    use std::collections::HashMap;
    use std::sync::{Mutex, PoisonError};
    use std::time::Instant;

    use enshwick_loader_api::abi::PlayerInfo;
    use enshwick_loader_api::config::Schema;

    use super::{Message, admit, values_text, without_secrets};
    use crate::host::Loader;

    /// When each player last changed a setting.
    static LAST_EDIT: Mutex<Option<HashMap<u64, Instant>>> = Mutex::new(None);
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
        read(&HEARD.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Forgets what was heard, as when the screen asks again.
    pub(crate) fn forget() {
        let mut heard = HEARD.lock().unwrap_or_else(PoisonError::into_inner);
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
                let mut heard = HEARD.lock().unwrap_or_else(PoisonError::into_inner);
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
                let mut heard = HEARD.lock().unwrap_or_else(PoisonError::into_inner);
                heard.result = Some((ok, text));
                heard.news += 1;
            }
            Message::Query { .. } | Message::Edit { .. } => {}
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
        match message {
            Message::Query { id } if id.is_empty() => {
                let mut text = String::new();
                for record in loader
                    .mods
                    .iter()
                    .filter(|m| m.schema.lock().is_ok_and(|s| s.is_some()))
                    .filter(|m| admin || m.synced.lock().is_ok_and(|s| s.is_some()))
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
            Message::Sync { .. } | Message::Snapshot { .. } | Message::Result { .. } => {}
        }
    }

    /// Sends mod `id`'s settings to the client of `key`: the file without
    /// secrets for an administrator, the join values for anyone else.
    fn snapshot(loader: &Loader, key: u64, id: &str, admin: bool) {
        let Some(record) = loader.mods.iter().find(|m| m.id == id) else {
            return;
        };
        let Some(schema) = record.schema.lock().ok().and_then(|s| s.clone()) else {
            return;
        };
        let text = if admin {
            let file = std::fs::read_to_string(loader.configs_dir.join(format!("{id}.toml")))
                .unwrap_or_default();
            without_secrets(&schema, &file)
        } else {
            record
                .synced
                .lock()
                .ok()
                .and_then(|s| s.clone())
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
                    text: format!("{id}'s settings are too long to send; use /config."),
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
            .mods
            .iter()
            .enumerate()
            .find(|(_, m)| m.id == id)
            .ok_or_else(|| format!("The server runs no mod {id}."))?;
        let schema = record
            .schema
            .lock()
            .ok()
            .and_then(|s| s.clone())
            .ok_or_else(|| format!("{id} does not describe its settings."))?;
        let now = Instant::now();
        {
            let mut last = LAST_EDIT.lock().unwrap_or_else(PoisonError::into_inner);
            let last = last.get_or_insert_with(HashMap::new);
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
    use enshwick_loader_api::config::{Field, Stage};

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
            Message::Query { id: "safes".into() },
            Message::Snapshot {
                id: "safes".into(),
                admin: true,
                schema: schema().to_text(),
                text: "limit = 4\n".into(),
            },
            Message::Edit {
                id: "safes".into(),
                key: "limit".into(),
                value: "6".into(),
            },
            Message::Result {
                ok: false,
                text: "Сейф".into(),
            },
        ] {
            assert_eq!(
                Message::read(message.kind(), &message.body()),
                Some(message)
            );
        }
        assert_eq!(Message::read(QUERY, &[5, 0, b'a']), None);
        assert_eq!(Message::read(42, &[]), None);
    }

    #[test]
    fn secrets_never_leave_and_never_change() {
        let text = "# Limit.\nlimit = 4\ntoken = \"hunter2\"\n";
        let shown = without_secrets(&schema(), text);
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
}
