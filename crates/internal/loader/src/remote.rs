//! Mod settings across the wire, between a server and its joined clients.
//!
//! A server sends its world's settings to each client that joins it and
//! again after a change, and an administrator's client reads and changes a
//! dedicated server's settings from its Mods screen.
//!
//! The messages ride the loader's control frames. The server decides
//! everything: it checks the sender's permissions on its own player record,
//! never sends a field marked `secret`, and changes a setting only through
//! the same checks as `enshwick config` on the console.

use std::time::{Duration, Instant};

use enshwick::abi::ActionTarget;
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
/// A client names the language its player reads action replies in.
pub const ACTIONS_LANGUAGE: u8 = 12;
/// A client runs an action.
pub const ACTION_RUN: u8 = 14;
/// The server's controls of one of its mods of side server; 13 and 15
/// stay unused, since loaders before them read those as action sheets.
pub const CONTROLS: u8 = 16;
/// A window of one of the server's mods, as the client's player sees it.
pub const WINDOW: u8 = 17;
/// The server closes a window of one of its mods.
pub const WINDOW_CLOSE: u8 = 18;

/// The controls of one mod a [`Message::Controls`] carries at most.
pub const MAX_CARRIED: usize = 24;
/// The longest text a [`Message::Controls`] carries, a label or the mod's
/// name, in bytes; the server cuts a longer one.
pub const MAX_CARRIED_TEXT: usize = 160;

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
/// The action runs a second a player who is no administrator sends, after
/// a burst of [`RUN_BURST`].
///
/// A control sends one a key press, so faster than queries, and each costs
/// the server one handler call and one notice.
pub const RUN_RATE: f64 = 10.0;
/// See [`RUN_RATE`].
pub const RUN_BURST: f64 = 20.0;

/// A message between a client's loader and its server's.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// The server's settings of mod `id`, every file seen as one, as TOML,
    /// without a secret.
    Sync {
        /// The mod.
        id: String,
        /// Its settings.
        values: String,
    },
    /// A client asks for mod `id`'s settings.
    Query {
        /// The mod.
        id: String,
    },
    /// The server's settings of mod `id`, every file seen as one, with its
    /// schema, both without secret fields. For an empty `id`, the mods with
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
    /// A client names the language its player reads action replies in.
    ActionsLanguage {
        /// The client's language.
        language: u32,
    },
    /// A client runs mod `id`'s action `action` with `args`.
    ActionRun {
        /// The mod.
        id: String,
        /// The action.
        action: String,
        /// The arguments, in the sheet's encoding.
        args: String,
        /// The object the run names, if any; a loader before it sends none,
        /// and one before it reads past the arguments.
        target: Option<ActionTarget>,
    },
    /// The controls of the server's mod `id` of side server that run an
    /// action, which the client lists and reads keys for without the mod.
    Controls {
        /// The mod.
        id: String,
        /// Its name, in the client's language.
        name: String,
        /// Its controls, at most [`MAX_CARRIED`].
        controls: Vec<Carried>,
    },
    /// The window `window` of the server's mod `id`, as the client's
    /// player sees it, which the client draws without the mod.
    Window {
        /// The mod.
        id: String,
        /// The window's id in the mod.
        window: String,
        /// Whether it opens: the mod showed it. Otherwise it only updates a
        /// window the client still holds, as after the player's run.
        open: bool,
        /// Its text, at most [`enshwick::actions::window::MAX_BYTES`].
        text: String,
    },
    /// The server closes the window `window` of its mod `id`.
    WindowClose {
        /// The mod.
        id: String,
        /// The window's id in the mod.
        window: String,
    },
}

/// A control of a server's mod, as [`Message::Controls`] carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Carried {
    /// Its name in its mod.
    pub name: String,
    /// What the mod's key bindings show, in the client's language.
    pub label: String,
    /// The mod's action a firing runs on the server.
    pub action: String,
    /// The mod's default keys, 0 for none.
    pub keys: [u32; 2],
    /// The phases it fires on.
    pub fires: u32,
    /// How long a key stays down before a hold fires, in milliseconds.
    pub hold_ms: u32,
}

impl Carried {
    /// Whether everything a client reads of it is in bounds: a name and an
    /// action as controls name them, a label of at most
    /// [`MAX_CARRIED_TEXT`] bytes, keys that bind, phases that fire.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        use enshwick::controls::{Key, is_valid_name, valid_phases};
        is_valid_name(&self.name)
            && is_valid_name(&self.action)
            && self.label.len() <= MAX_CARRIED_TEXT
            && self
                .keys
                .iter()
                .all(|&bits| bits == 0 || Key::from_bits(bits).is_some())
            && valid_phases(self.fires)
    }
}

/// The server mods a client keeps carried controls of at most.
pub const MAX_CARRIED_MODS: usize = 32;

/// A server's control as its client keeps it, with its mod.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heard {
    /// The mod's id.
    pub mod_id: String,
    /// The mod's name, in the client's language.
    pub mod_name: String,
    /// The control.
    pub control: Carried,
}

impl Heard {
    /// `<mod id>:<name>`, as `mod-controls.toml` names it.
    #[must_use]
    pub fn id(&self) -> String {
        format!("{}:{}", self.mod_id, self.control.name)
    }
}

/// The carried controls once the server sent mod `id`'s `controls`.
///
/// They take the place of the mod's earlier ones, where those stood, so a
/// control sent again keeps its place; a new mod goes after the others. A
/// new mod past [`MAX_CARRIED_MODS`] is left out, so a server cannot grow the
/// list without end.
///
/// # Examples
///
/// ```
/// use enshwick_loader::remote::{Carried, carried_after};
///
/// let horn = Carried { name: "horn".into(), label: "Horn".into(), action: "honk".into(), keys: [0x23, 0], fires: 1, hold_ms: 500 };
/// let kept = carried_after(&[], "you.cars", "Cars", &[horn.clone()]);
/// let again = carried_after(&kept, "you.cars", "Cars", &[horn]);
/// assert_eq!(again.len(), 1);
/// assert_eq!(again[0].id(), "you.cars:horn");
/// ```
#[must_use]
pub fn carried_after(kept: &[Heard], id: &str, name: &str, controls: &[Carried]) -> Vec<Heard> {
    let known = kept.iter().position(|h| h.mod_id == id);
    let mut list: Vec<Heard> = kept.iter().filter(|h| h.mod_id != id).cloned().collect();
    let mut mods: Vec<&str> = list.iter().map(|h| h.mod_id.as_str()).collect();
    mods.dedup();
    if known.is_none() && mods.len() >= MAX_CARRIED_MODS {
        return list;
    }
    // A mod's controls stand together, so its first one's place is free.
    let at = known.unwrap_or(list.len());
    let heard = controls.iter().map(|control| Heard {
        mod_id: id.to_owned(),
        mod_name: name.to_owned(),
        control: control.clone(),
    });
    drop(list.splice(at..at, heard));
    list
}

/// `text` cut to at most `max` bytes at a character's edge.
#[must_use]
pub fn cut(text: &str, max: usize) -> &str {
    &text[..text.floor_char_boundary(max)]
}

/// The bytes of a run's object after its arguments: flags, entity and the
/// position's three numbers.
const TARGET_BYTES: usize = 4 + 4 + 24;

fn put_target(out: &mut Vec<u8>, target: &ActionTarget) {
    out.extend_from_slice(&target.flags.to_le_bytes());
    out.extend_from_slice(&target.entity.0.to_le_bytes());
    for coordinate in target.position {
        out.extend_from_slice(&coordinate.to_le_bytes());
    }
}

/// A run's object from the bytes after its arguments: `None` for none, or
/// for flags this loader does not know or a position that is not finite,
/// which no loader sends.
fn take_target(bytes: &[u8]) -> Option<ActionTarget> {
    let bytes = bytes.first_chunk::<TARGET_BYTES>()?;
    let word =
        |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    let number = |at: usize| {
        let mut eight = [0; 8];
        eight.copy_from_slice(&bytes[at..at + 8]);
        f64::from_le_bytes(eight)
    };
    let flags = word(0);
    let target = ActionTarget {
        flags,
        entity: enshwick::Entity(word(4)),
        // Bytes the flags leave unset read as zero, so the message writes back the same.
        position: if flags & ActionTarget::POSITION == 0 {
            [0.0; 3]
        } else {
            [number(8), number(16), number(24)]
        },
        ..ActionTarget::NONE
    };
    (target.flags != 0 && target.is_valid()).then_some(target)
}

/// Whether a window message names a mod `id` and a window `window` as ids
/// read.
fn window_named(id: &str, window: &str) -> bool {
    enshwick::abi::is_valid_id(id) && enshwick::actions::is_valid_id(window)
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
            Self::ActionsLanguage { .. } => ACTIONS_LANGUAGE,
            Self::ActionRun { .. } => ACTION_RUN,
            Self::Controls { .. } => CONTROLS,
            Self::Window { .. } => WINDOW,
            Self::WindowClose { .. } => WINDOW_CLOSE,
        }
    }

    /// Whether the server answers it with a reply of its own making or runs
    /// a mod's handler for it, so a player who is no administrator sends it
    /// at [`QUERY_RATE`] only, or an action run at [`RUN_RATE`].
    #[must_use]
    pub const fn is_rated(&self) -> bool {
        matches!(
            self,
            Self::Query { .. }
                | Self::Edit { .. }
                | Self::ActionsLanguage { .. }
                | Self::ActionRun { .. }
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
            Self::Result { ok, text } => {
                out.push(u8::from(*ok));
                put(&mut out, text);
            }
            Self::Recipes { first, last, ids } => {
                out.push(u8::from(*first) | (u8::from(*last) << 1));
                for id in ids {
                    out.extend_from_slice(&id.to_le_bytes());
                }
            }
            Self::ActionsLanguage { language } => out.extend_from_slice(&language.to_le_bytes()),
            Self::ActionRun {
                id,
                action,
                args,
                target,
            } => {
                put(&mut out, id);
                put(&mut out, action);
                put(&mut out, args);
                if let Some(target) = target {
                    put_target(&mut out, target);
                }
            }
            Self::Controls { id, name, controls } => {
                put(&mut out, id);
                put(&mut out, name);
                out.push(u8::try_from(controls.len()).unwrap_or(u8::MAX));
                for control in controls.iter().take(usize::from(u8::MAX)) {
                    put(&mut out, &control.name);
                    put(&mut out, &control.label);
                    put(&mut out, &control.action);
                    for word in [
                        control.keys[0],
                        control.keys[1],
                        control.fires,
                        control.hold_ms,
                    ] {
                        out.extend_from_slice(&word.to_le_bytes());
                    }
                }
            }
            Self::Window {
                id,
                window,
                open,
                text,
            } => {
                put(&mut out, id);
                put(&mut out, window);
                out.push(u8::from(*open));
                put(&mut out, text);
            }
            Self::WindowClose { id, window } => {
                put(&mut out, id);
                put(&mut out, window);
            }
        }
        out
    }

    /// Reads a [`Message::Window`] or [`Message::WindowClose`] from the
    /// rest of its body: `None` for ids that do not read, an open flag past
    /// 1 or a text past the window's bound.
    fn read_window(kind: u8, bytes: &mut &[u8]) -> Option<Self> {
        let (id, window) = (take(bytes)?, take(bytes)?);
        if !window_named(id, window) {
            return None;
        }
        let (id, window) = (id.to_owned(), window.to_owned());
        if kind == WINDOW_CLOSE {
            return Some(Self::WindowClose { id, window });
        }
        let (&open, rest) = bytes.split_first()?;
        *bytes = rest;
        let text = take(bytes)?;
        if open > 1 || text.len() > enshwick::actions::window::MAX_BYTES {
            return None;
        }
        Some(Self::Window {
            id,
            window,
            open: open == 1,
            text: text.to_owned(),
        })
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
            ACTIONS_LANGUAGE => Self::ActionsLanguage {
                language: u32::from_le_bytes(*bytes.first_chunk::<4>()?),
            },
            ACTION_RUN => Self::ActionRun {
                id: take(bytes)?.to_owned(),
                action: take(bytes)?.to_owned(),
                args: take(bytes)?.to_owned(),
                target: take_target(bytes),
            },
            CONTROLS => {
                let id = take(bytes)?.to_owned();
                let name = take(bytes)?.to_owned();
                let (&count, rest) = bytes.split_first()?;
                *bytes = rest;
                if !enshwick::abi::is_valid_id(&id)
                    || name.len() > MAX_CARRIED_TEXT
                    || usize::from(count) > MAX_CARRIED
                {
                    return None;
                }
                let mut controls: Vec<Carried> = Vec::with_capacity(usize::from(count));
                for _ in 0..count {
                    let (name, label, action) = (take(bytes)?, take(bytes)?, take(bytes)?);
                    let mut words = [0u32; 4];
                    for word in &mut words {
                        let (four, rest) = bytes.split_first_chunk::<4>()?;
                        *word = u32::from_le_bytes(*four);
                        *bytes = rest;
                    }
                    let control = Carried {
                        name: name.to_owned(),
                        label: label.to_owned(),
                        action: action.to_owned(),
                        keys: [words[0], words[1]],
                        fires: words[2],
                        hold_ms: words[3],
                    };
                    if !control.is_valid() || controls.iter().any(|c| c.name == control.name) {
                        return None;
                    }
                    controls.push(control);
                }
                Self::Controls { id, name, controls }
            }
            WINDOW | WINDOW_CLOSE => Self::read_window(kind, bytes)?,
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
#[derive(Clone, Debug, PartialEq)]
pub enum Gate {
    /// Serve it.
    Pass,
    /// Send the player this answer instead of serving the message.
    Busy(Message),
    /// Neither serve nor answer it: a run still takes the language of the
    /// client's hello.
    Drop,
}

/// The rates of the [rated](Message::is_rated) messages of players who are
/// no administrators, on the server.
#[derive(Debug, Default)]
pub struct Queries {
    rates: rustc_hash::FxHashMap<u64, Peer>,
    /// Each player's own rate of action runs, apart from the queries'.
    runs: rustc_hash::FxHashMap<u64, Peer>,
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
    /// spend the player's rate; a language message over it gets no answer.
    pub fn gate(&mut self, key: u64, admin: bool, message: &Message, now: Instant) -> Gate {
        let admitted = match message {
            Message::ActionRun { .. } => {
                admin
                    || self
                        .runs
                        .entry(key)
                        .or_insert_with(|| Peer::with_tokens(RUN_BURST))
                        .admit_at(now, RUN_RATE, RUN_BURST)
            }
            _ => !message.is_rated() || self.admit(key, admin, now),
        };
        if admitted {
            Gate::Pass
        } else if matches!(message, Message::ActionsLanguage { .. }) {
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
        self.runs.remove(&key);
    }
}

/// One [`Message::Sync`] per mod of `values`, its settings by mod id.
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
