//! What mods hear and change of the chat: the chat filters run inside the
//! game's chat post on each line a player typed, after the chat mute, and
//! every line that reached the chat waits for the end of the step, where it
//! is raised as `Event::CHAT_SAID`.

use core::ffi::c_void;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use enshwick_base::peer::{Admitted, CHAT_LINE_RATE, Rated};
use enshwick_core::abi::{
    ChatFilterFn, ChatLine, ChatRewrite, Entity, Event, Header, PlayerKey, Status, Str,
};

use crate::base::wire::Drops;
use crate::engine::loader::{Loader, Registered};
use crate::engine::mod_index::ModIndex;
use crate::engine::pending::Pending;
use crate::engine::registry::Registry;
use enshwick_base::Relock;

/// Lines waiting for the end of the step at most; more are dropped.
const MAX_WAITING: usize = 256;

static FILTERS: Registry<Registered<ChatFilterFn>> = Registry::new();

/// A line that reached the chat, waiting to be raised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Said {
    /// The sender's account key, 0 when the game names none.
    pub(crate) player: u64,
    /// `ChatLine::PLAYER`, `EMOTE` or `NOTICE`.
    pub(crate) kind: u32,
    pub(crate) loca: [u32; 2],
    pub(crate) text: String,
}

/// The lines heard while the game runs, and who said them.
struct Heard {
    waiting: Pending<Said>,
    /// Each account's rate of lines raised.
    rate: Mutex<Rated>,
    /// Lines not raised: the queue was full, or their account was over its
    /// rate; a count for the process, which a hosted game's end keeps.
    skipped: Drops,
    /// Whether a mod listens for `Event::CHAT_SAID`; fixed once mods are in,
    /// since they subscribe only during init, so a hosted game's end keeps it.
    wanted: OnceLock<bool>,
}

impl Heard {
    const fn new() -> Self {
        Self {
            waiting: Pending::new(),
            rate: Mutex::new(Rated::new(CHAT_LINE_RATE)),
            skipped: Drops::new(),
            wanted: OnceLock::new(),
        }
    }

    /// Whether a mod listens for lines.
    fn wanted(&self, loader: &Loader) -> bool {
        *self
            .wanted
            .get_or_init(|| loader.subscribed(Event::CHAT_SAID))
    }

    /// Drops the lines waiting and the accounts' rates.
    fn forget(&self) {
        self.waiting.clear();
        self.rate.relock().clear();
    }
}

static HEARD: Heard = Heard::new();

/// Registers mod `owner`'s chat filter.
pub(crate) fn register(
    loader: &Loader,
    owner: ModIndex,
    callback: ChatFilterFn,
    user: *mut c_void,
) -> Status {
    if FILTERS.is_sealed() {
        return Status::TOO_LATE;
    }
    if loader
        .build
        .symbol(crate::engine::pinned::chat::POST.id())
        .is_none()
    {
        return Status::MISSING;
    }
    FILTERS
        .add(Registered::new(owner, callback, user))
        .map_or(Status::TOO_LATE, |()| Status::OK)
}

/// Seals the filters; the chat post links after mods registered theirs.
pub(crate) fn seal() {
    let _filters = FILTERS.seal();
}

/// Whether no mod filters or hears the chat, so a line needs nothing more.
pub(crate) fn idle(loader: &Loader) -> bool {
    FILTERS.sealed().is_none_or(<[_]>::is_empty) && !HEARD.wanted(loader)
}

/// Runs every live filter on `text`, a line the player of account `player`
/// typed, in load order: `None` when one dropped it, else the text as the
/// last filter left it, at most `max_text` bytes.
pub(crate) fn filter(player: u64, loca: [u32; 2], text: String, max_text: usize) -> Option<String> {
    let filters = FILTERS.sealed().filter(|filters| !filters.is_empty());
    let Some(filters) = filters else {
        return Some(text);
    };
    let entity = character(player);
    let mut text = text;
    let mut buffer = vec![0u8; max_text];
    for f in filters.iter().filter(|f| f.live()) {
        let line = ChatLine {
            header: Header::of::<ChatLine>(),
            player: PlayerKey::new(player),
            entity,
            kind: ChatLine::PLAYER,
            loca,
            text: Str::new(&text),
        };
        let mut rewrite = ChatRewrite {
            header: Header::of::<ChatRewrite>(),
            buffer: buffer.as_mut_ptr(),
            capacity: u32::try_from(buffer.len()).unwrap_or(u32::MAX),
            len: 0,
        };
        // SAFETY: `callback` is a `ChatFilterFn` the mod registered, in its
        // DLL, which stays loaded (`Registered`); `user` is the pointer it
        // registered with it; `line`, the text it views and `rewrite` with
        // its `capacity` writable bytes live for the call.
        let answer = crate::engine::profile::call(
            f.owner,
            crate::engine::profile::Feature::Chat,
            || unsafe { (f.callback)(&raw const line, &raw mut rewrite, f.user()) },
        );
        match answer {
            ChatRewrite::DROP => return None,
            ChatRewrite::REWRITTEN => match rewritten(&buffer, rewrite.len) {
                // The game would post an empty line.
                Some("") => return None,
                Some(new) => new.clone_into(&mut text),
                None => {}
            },
            _ => {}
        }
    }
    Some(text)
}

/// The text a filter wrote: its first `len` bytes when they fit and are
/// UTF-8.
fn rewritten(buffer: &[u8], len: u32) -> Option<&str> {
    let bytes = buffer.get(..usize::try_from(len).ok()?)?;
    core::str::from_utf8(bytes).ok()
}

/// The character of the connected player with account key `player`.
fn character(player: u64) -> Entity {
    crate::engine::loader::loader()
        .and_then(|loader| crate::engine::world::players(loader).ok())
        .and_then(|players| players.into_iter().find(|p| p.key.0 == player))
        .map_or(Entity(0), |p| p.entity)
}

/// Keeps a line that reached the chat for the end of the step, when a mod
/// listens; a player's line counts against its account's rate.
pub(crate) fn heard(loader: &Loader, said: Said) {
    if !HEARD.wanted(loader) {
        return;
    }
    let over =
        rated(said.kind) && HEARD.rate.relock().admit(said.player, Instant::now()) != Admitted::Yes;
    if over
        || !HEARD
            .waiting
            .push_within(MAX_WAITING, core::iter::once(said))
    {
        skipped();
    }
}

/// Whether lines of `kind` count against their account's rate: those a
/// player's client causes, typed lines and emotes.
const fn rated(kind: u32) -> bool {
    kind != ChatLine::NOTICE
}

/// Counts a line not raised and logs the first and every hundredth.
#[cold]
fn skipped() {
    if let Some(total) = HEARD.skipped.add(1) {
        crate::log!(
            WARN,
            "chat lines: {total} not raised to mods, over a player's {} a second or a full queue",
            CHAT_LINE_RATE.per_second
        );
    }
}

/// Raises the lines that reached the chat during the step; on the game
/// thread, outside the game's systems.
pub(crate) fn raise(loader: &Loader) {
    let lines = HEARD.waiting.take();
    if lines.is_empty() {
        return;
    }
    let players = crate::engine::world::players(loader).unwrap_or_default();
    for said in lines {
        let entity = players
            .iter()
            .find(|p| p.key.0 == said.player)
            .map_or(Entity(0), |p| p.entity);
        let line = ChatLine {
            header: Header::of::<ChatLine>(),
            player: PlayerKey::new(said.player),
            entity,
            kind: said.kind,
            loca: said.loca,
            text: Str::new(&said.text),
        };
        loader.emit(Event::CHAT_SAID, None, (&raw const line).cast());
    }
}

/// Drops the lines waiting for a hosted game that stops.
pub(crate) fn forget() {
    HEARD.forget();
}

/// The kind of a line the game posted itself, by its chat type: 1 is an
/// emote (server build 387, the loca tag `Emote.chatMessage`), anything
/// else a notice such as a join or leave line.
pub(crate) const fn kind_of(chat_type: u8) -> u32 {
    if chat_type == super::EMOTE {
        ChatLine::EMOTE
    } else {
        ChatLine::NOTICE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hosted_game_s_end_drops_the_lines_it_heard_and_its_rates() {
        let heard = Heard::new();
        let said = Said {
            player: 7,
            kind: ChatLine::PLAYER,
            loca: [0; 2],
            text: "hello".to_owned(),
        };
        assert!(
            heard
                .waiting
                .push_within(MAX_WAITING, core::iter::once(said))
        );
        let now = Instant::now();
        let spent = (0..20)
            .take_while(|_| heard.rate.relock().admit(7, now) == Admitted::Yes)
            .count();
        assert_eq!(
            f64::from(u32::try_from(spent).unwrap()),
            CHAT_LINE_RATE.burst
        );
        heard.forget();
        assert!(heard.waiting.take().is_empty());
        assert_eq!(
            heard.rate.relock().admit(7, now),
            Admitted::Yes,
            "the next game starts with a full rate"
        );
    }

    #[test]
    fn whether_a_mod_hears_lines_is_asked_once_and_outlives_a_hosted_game() {
        let heard = Heard::new();
        let loader = crate::start::tests::bare_loader();
        assert!(!heard.wanted(&loader), "no mod subscribed");
        heard.forget();
        assert_eq!(heard.wanted.get(), Some(&false), "kept for the process");
    }

    #[test]
    fn a_rewrite_counts_only_when_it_fits_and_is_utf8() {
        let buffer = *b"hi \xd0\x9f there";
        assert_eq!(rewritten(&buffer, 2), Some("hi"));
        assert_eq!(rewritten(&buffer, 5), Some("hi П"));
        assert_eq!(rewritten(&buffer, 4), None, "a cut character");
        assert_eq!(rewritten(&buffer, 99), None, "past the buffer");
    }

    #[test]
    fn emotes_count_against_the_rate_as_typed_lines_do_and_notices_do_not() {
        assert!(rated(ChatLine::PLAYER));
        assert!(rated(ChatLine::EMOTE));
        assert!(!rated(ChatLine::NOTICE));
    }

    #[test]
    fn the_game_s_own_lines_are_emotes_or_notices() {
        assert_eq!(kind_of(1), ChatLine::EMOTE);
        assert_eq!(kind_of(4), ChatLine::NOTICE);
        assert_eq!(kind_of(0), ChatLine::NOTICE);
    }
}
