//! The messages in the running game: what a client heard, and how its server answers.

use enshwick_base::Relock;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use enshwick_core::config::Schema;

use enshwick_base::peer::Rated;

use super::{AdmitEditError, EDIT_RATE, Gate, Message, Queries, admit, redacted, texts};
use crate::engine::loader::Loader;
use crate::engine::world::player_with_key as player;

/// The server's rates of its clients' messages.
struct Rates {
    /// The queries of players who are no administrators.
    queries: Queries,
    /// Each administrator's settings changes.
    edits: Rated,
}

impl Rates {
    const fn new() -> Self {
        Self {
            queries: Queries::new(),
            edits: Rated::new(EDIT_RATE),
        }
    }
}

/// What takes a message of one kind a client heard from its server.
pub(crate) type TakeOnClient = fn(Message);

/// What answers a message of one kind the client of the player with an
/// account key sent its server.
pub(crate) type TakeOnServer = fn(&Loader, u64, Message);

/// The message kinds a part may take, every kind [`Message::kind`] names
/// below it.
const KINDS: usize = 32;

/// What the settings messages keep while a game runs.
struct State {
    rates: Mutex<Rates>,
    /// What the client last heard from its server, for the Mods screen.
    heard: Mutex<Heard>,
    /// What takes each kind a part above base owns on a client, by kind.
    on_client: [OnceLock<TakeOnClient>; KINDS],
    /// What answers each kind a part above base owns on a server, by kind.
    on_server: [OnceLock<TakeOnServer>; KINDS],
}

impl State {
    const fn new() -> Self {
        Self {
            rates: Mutex::new(Rates::new()),
            heard: Mutex::new(Heard::new()),
            on_client: [const { OnceLock::new() }; KINDS],
            on_server: [const { OnceLock::new() }; KINDS],
        }
    }

    /// Forgets the rates of the hosted game that went. What was heard
    /// stays: only a client joined to another's game hears, and the Mods
    /// screen forgets it before each query.
    fn forget(&self) {
        *self.rates.relock() = Rates::new();
    }

    /// Hands `message`, which a client heard, to the take its kind names.
    fn take_on_client(&self, message: Message) {
        let kind = message.kind();
        match named(&self.on_client, kind) {
            Some(take) => take(message),
            None => untaken("message", kind),
        }
    }

    /// Hands `message`, which the client of the player with account key
    /// `key` sent, to the take that answers its kind.
    fn take_on_server(&self, loader: &Loader, key: u64, message: Message) {
        let kind = message.kind();
        match named(&self.on_server, kind) {
            Some(take) => take(loader, key, message),
            None => untaken("message", kind),
        }
    }
}

static STATE: State = State::new();

/// How often the log says again that a message arrived no part takes.
const UNTAKEN_LOGGED_EVERY: Duration = Duration::from_secs(10);

/// Names `take` as what takes the messages of `kind` a client hears from
/// its server; the first one named stays.
pub(crate) fn on_client(kind: u8, take: TakeOnClient) {
    name(&STATE.on_client, kind, take);
}

/// Names `take` as what answers the messages of `kind` a client sends its
/// server; the first one named stays.
pub(crate) fn on_server(kind: u8, take: TakeOnServer) {
    name(&STATE.on_server, kind, take);
}

/// Names `take` for `kind` in `takes`, where the first one named stays; a
/// kind past them is a loader bug, which the log names.
pub(crate) fn name<T, const N: usize>(takes: &[OnceLock<T>; N], kind: u8, take: T) {
    if let Some(slot) = takes.get(usize::from(kind)) {
        let _first = slot.set(take);
    } else {
        crate::log!(
            ERROR,
            "loader messages: a part named kind {kind}, past the {N} kinds a part may take"
        );
    }
}

/// The take named for `kind` in `takes`.
pub(crate) fn named<T: Copy, const N: usize>(takes: &[OnceLock<T>; N], kind: u8) -> Option<T> {
    takes
        .get(usize::from(kind))
        .and_then(OnceLock::get)
        .copied()
}

/// Notes that a `what` of `kind` arrived that no part takes, so it was
/// dropped: its part is off here, or it never named itself. A peer sends
/// them, so the line is rated.
pub(crate) fn untaken(what: &str, kind: u8) {
    crate::log_every!(
        INFO,
        UNTAKEN_LOGGED_EVERY,
        "loader messages: no part takes {what} kind {kind}, so it was dropped"
    );
}

/// Forgets the players' rates when a client's hosted game ends.
pub(crate) fn forget_game() {
    STATE.forget();
}

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
    read(&STATE.heard.relock())
}

/// Forgets what was heard, as when the screen asks again.
pub(crate) fn forget() {
    let mut heard = STATE.heard.relock();
    heard.mods = None;
    heard.settings = None;
    heard.result = None;
}

/// Pushes mod `id`'s settings, every file seen as one without a secret, to
/// every client that runs Enshwick.
pub(crate) fn push_sync(id: &str, values: &str) {
    crate::base::net::broadcast_control(&Message::Sync {
        id: id.to_owned(),
        values: values.to_owned(),
    });
}

/// Sends the client of `key`, which just said hello, the settings of every
/// running join mod, without a secret: the client plays this world with
/// them.
pub(crate) fn sync_to(key: u64) -> bool {
    let Some(loader) = crate::engine::loader::loader() else {
        return true;
    };
    let values = loader
        .mods
        .iter()
        .filter(|m| m.joins())
        .filter_map(|m| Some((m.id.clone(), crate::base::worlds::sent_view(m)?)))
        .collect();
    super::sync_messages(values)
        .iter()
        .all(|message| crate::base::net::to_player(key, message))
}

/// Takes a message the server sent, on the client.
pub(crate) fn client(message: Message) {
    match message {
        Message::Sync { id, values } => crate::base::join::client::apply_server_value(&id, &values),
        Message::Snapshot {
            id,
            admin,
            schema,
            text,
        } => {
            let mut heard = STATE.heard.relock();
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
            let mut heard = STATE.heard.relock();
            heard.result = Some((ok, text));
            heard.news += 1;
        }
        message => STATE.take_on_client(message),
    }
}

/// Takes a message a client sent, on the server's game thread; the
/// answer is in the language the client's player reads.
pub(crate) fn server(loader: &Loader, key: u64, message: Message) {
    crate::base::net::for_player(key, || answer(loader, key, message));
}

/// Answers a message the client `key` sent.
fn answer(loader: &Loader, key: u64, message: Message) {
    let is_admin = || player(loader, key).is_some_and(|p| p.is_admin());
    match gate(key, is_admin, &message) {
        Gate::Pass => {}
        Gate::Busy(answer) => {
            crate::base::net::to_player(key, &answer);
            return;
        }
        Gate::Drop => return,
    }
    match message {
        Message::Query { id } if id.is_empty() => {
            let admin = is_admin();
            let mut text = String::new();
            for record in loader
                .mods
                .iter()
                .filter(|m| !crate::base::worlds::settings_files(m).is_empty())
                .filter(|m| admin || m.joins())
            {
                text.push_str(&record.id);
                text.push('\t');
                text.push_str(&crate::base::manifest::label(record));
                text.push('\n');
            }
            crate::base::net::to_player(
                key,
                &Message::Snapshot {
                    id: String::new(),
                    admin,
                    schema: String::new(),
                    text,
                },
            );
        }
        Message::Query { id } => snapshot(loader, key, &id, is_admin()),
        Message::Edit {
            id,
            key: setting,
            value,
        } => {
            let admin = is_admin();
            let reply = edit(loader, key, &id, &setting, &value, admin);
            let ok = reply.is_ok();
            let text = reply.unwrap_or_else(|why| why.reply());
            crate::base::net::to_player(key, &Message::Result { ok, text });
            snapshot(loader, key, &id, admin);
        }
        Message::ActionsLanguage { language } => {
            crate::base::net::language_named(key, language);
        }
        message => STATE.take_on_server(loader, key, message),
    }
}

/// What the server does with `message` of the player with account key
/// `key` now.
fn gate(key: u64, admin: impl FnOnce() -> bool, message: &Message) -> Gate {
    STATE
        .rates
        .relock()
        .queries
        .gate(key, admin, message, Instant::now())
}

/// Forgets the player with account key `key`, who left the server.
pub(crate) fn forget_player(key: u64) {
    let mut rates = STATE.rates.relock();
    rates.queries.forget(key);
    rates.edits.forget(key);
}

/// Sends mod `id`'s settings to the client of `key`, without secrets: to
/// change for an administrator, to read for anyone else.
fn snapshot(loader: &Loader, key: u64, id: &str, admin: bool) {
    let Some((_, record)) = loader.find_mod(id) else {
        return;
    };
    let Some(schema) = crate::base::worlds::schema_of(record) else {
        return;
    };
    let text = redacted(&schema, &crate::base::worlds::show_text(record));
    let mut shown = schema.clone();
    shown.fields.retain(|f| !f.secret);
    let mut message = Message::Snapshot {
        id: id.to_owned(),
        admin,
        schema: shown.to_text(),
        text: text.clone(),
    };
    if message.body().len() > crate::base::net::MAX_CONTROL {
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
    if message.body().len() > crate::base::net::MAX_CONTROL {
        crate::base::net::to_player(
            key,
            &Message::Result {
                ok: false,
                text: texts::fill(
                    "remote.too-long",
                    &[("id", id), ("file", &format!("{id}/settings.toml"))],
                ),
            },
        );
        return;
    }
    crate::base::net::to_player(key, &message);
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
        .settings_schema()
        .ok_or_else(|| EditSettingError::NoSchema { id: id.to_owned() })?;
    let now = Instant::now();
    admit(
        &schema,
        setting,
        admin,
        (key, &mut STATE.rates.relock().edits),
        now,
    )?;
    let who =
        player(loader, key).map_or_else(|| format!("{key:016x}"), |p| p.name_text().into_owned());
    let change = crate::base::configure::apply(loader, index, setting, value, &who)?;
    Ok(crate::base::configure::changed(id, setting, &change))
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
    Apply(#[from] crate::base::configure::ApplySettingError),
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

#[cfg(test)]
mod tests {
    use super::*;
    use enshwick_base::peer::Admitted;

    #[test]
    fn a_heard_message_reaches_the_take_its_kind_names_and_only_that_one() {
        use std::sync::atomic::{AtomicU32, Ordering};

        static UNCOVERED: AtomicU32 = AtomicU32::new(0);
        let state = State::new();
        let fog = |radius: f32| Message::FogUncover {
            at: [1.0, 2.0],
            radius,
        };
        state.take_on_client(fog(3.0));
        name(&state.on_client, super::super::FOG_UNCOVER, |message| {
            if let Message::FogUncover { radius, .. } = message {
                UNCOVERED.store(radius.to_bits(), Ordering::Relaxed);
            }
        });
        name(&state.on_client, super::super::FOG_UNCOVER, |_| {
            panic!("the first take stays")
        });
        state.take_on_client(fog(5.0));
        state.take_on_client(Message::Recipes {
            first: true,
            last: true,
            ids: vec![7],
        });
        assert_eq!(f32::from_bits(UNCOVERED.load(Ordering::Relaxed)), 5.0);
        assert!(
            named(&state.on_server, super::super::FOG_UNCOVER).is_none(),
            "a client's take answers nothing on a server"
        );
        name(&state.on_client, u8::MAX, |_| panic!("no such kind"));
        assert!(named(&state.on_client, u8::MAX).is_none());
    }

    #[test]
    fn a_hosted_game_s_end_forgets_its_players_rates() {
        let state = State::new();
        let now = Instant::now();
        let spent = core::iter::repeat_with(|| state.rates.relock().edits.admit(7, now))
            .take(1000)
            .any(|admitted| admitted != Admitted::Yes);
        assert!(spent, "the edit rate ran out");
        state.forget();
        assert_eq!(state.rates.relock().edits.admit(7, now), Admitted::Yes);
    }

    #[test]
    fn a_client_s_window_close_is_answered_by_letting_the_window_go_for_its_mod_to_hear() {
        use crate::ui::windows::harness::{KEY, MOD, Serving, WINDOW};
        let serving = Serving::new();
        let windows = crate::ui::PARTS
            .iter()
            .find(|part| part.name == "windows")
            .and_then(|part| part.seal);
        (windows.unwrap())(serving.loader);
        serving.greet();
        serving.show();
        crate::ui::windows::after_step(serving.loader);
        assert_eq!(serving.sent(), [Serving::opened()]);
        let close = |id: &str| Message::WindowClose {
            id: id.to_owned(),
            window: WINDOW.to_owned(),
        };

        answer(serving.loader, KEY + 1, close(MOD));
        answer(serving.loader, KEY, close("you.unknown"));
        assert_eq!(
            serving.windows().0.len(),
            1,
            "not this client's, not this mod"
        );
        answer(serving.loader, KEY, close(MOD));
        assert_eq!(serving.windows(), (Vec::new(), Vec::new()));
        crate::ui::windows::after_step(serving.loader);
        assert_eq!(serving.heard(), [WINDOW]);
        assert!(
            serving.sent().is_empty(),
            "nothing is answered to the client"
        );
    }
}
