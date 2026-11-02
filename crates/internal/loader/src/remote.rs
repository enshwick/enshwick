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
/// [`AdmitEditError`]: not an administrator, a secret field, or too soon
/// after the last change.
pub fn admit(
    schema: &Schema,
    key: &str,
    admin: bool,
    last: Option<Instant>,
    now: Instant,
) -> Result<(), AdmitEditError> {
    if !admin {
        return Err(AdmitEditError::NotAdmin);
    }
    if schema.field(key).is_some_and(|f| f.secret) {
        return Err(AdmitEditError::Secret {
            key: key.to_owned(),
        });
    }
    if last.is_some_and(|last| now.duration_since(last) < EDIT_GAP) {
        return Err(AdmitEditError::Busy);
    }
    Ok(())
}

/// Why the server refuses a client's change of a setting before it reaches
/// the settings file.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AdmitEditError {
    /// The player is no administrator.
    #[error("only administrators change settings")]
    NotAdmin,
    /// The setting is secret.
    #[error("{key} is secret, changed in the server's file only")]
    Secret {
        /// The setting's key.
        key: String,
    },
    /// The player changed a setting a moment ago.
    #[error("too soon after the last change")]
    Busy,
}

impl AdmitEditError {
    /// The reply for the client, in its player's language.
    #[must_use]
    pub fn reply(&self) -> String {
        match self {
            Self::NotAdmin => texts::fill("remote.admins", &[]),
            Self::Secret { key } => texts::fill("remote.secret", &[("key", key)]),
            Self::Busy => texts::fill("remote.busy", &[]),
        }
    }
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
mod live;

#[cfg(test)]
mod tests;
