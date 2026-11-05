//! What mods hear and change of the chat: the chat filters run inside the
//! game's chat post on each line a player typed, after the chat mute, and
//! every line that reached the chat waits for the end of the step, where it
//! is raised as `Event::CHAT_SAID`.

use core::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use enshwick::abi::{
    ChatFilterFn, ChatLine, ChatRewrite, Entity, Event, Header, PlayerKey, Status, Str,
};

use crate::host::{Loader, ModIndex, Registered};
use crate::locks::Relock;
use crate::registry::Registry;
use crate::wire::Drops;

/// Lines waiting for the end of the step at most; more are dropped.
const MAX_WAITING: usize = 256;
/// Lines of one account raised in a window at most; the filters still run
/// on every line.
const PER_WINDOW: u32 = 8;
/// The rate window.
const WINDOW: Duration = Duration::from_secs(1);
/// Accounts the rate keeps before it forgets those of past windows.
const RATE_ACCOUNTS: usize = 64;

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

/// One account's rate: its key, the start of its window, lines in it.
type Rate = (u64, Instant, u32);

/// The waiting lines and the rate of each account; at most
/// [`RATE_ACCOUNTS`] accounts, scanned, so no peer key is hashed.
#[derive(Debug)]
struct Heard {
    waiting: Vec<Said>,
    rate: Vec<Rate>,
}

static HEARD: Mutex<Heard> = Mutex::new(Heard {
    waiting: Vec::new(),
    rate: Vec::new(),
});
/// Set while lines wait, so an idle step takes no lock.
static WAITING: AtomicBool = AtomicBool::new(false);
/// Whether a mod listens for `Event::CHAT_SAID`; fixed once mods are in,
/// since they subscribe only during init.
static WANTED: OnceLock<bool> = OnceLock::new();
/// Lines not raised: the queue was full, or their account was over its rate.
static SKIPPED: Drops = Drops::new();

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
        .symbol(crate::pinned::chat::POST.id())
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

/// Whether a mod listens for lines.
fn wanted(loader: &Loader) -> bool {
    *WANTED.get_or_init(|| loader.subscribed(Event::CHAT_SAID))
}

/// Whether no mod filters or hears the chat, so a line needs nothing more.
pub(crate) fn idle(loader: &Loader) -> bool {
    FILTERS.sealed().is_none_or(<[_]>::is_empty) && !wanted(loader)
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
        let answer = crate::profile::call(f.owner, crate::profile::Feature::Chat, || unsafe {
            (f.callback)(&raw const line, &raw mut rewrite, f.user())
        });
        match answer {
            ChatRewrite::DROP => return None,
            ChatRewrite::REWRITTEN => {
                if let Some(new) = rewritten(&buffer, rewrite.len) {
                    text = new.to_owned();
                }
            }
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
    crate::host::loader()
        .and_then(|loader| crate::world::players(loader).ok())
        .and_then(|players| players.into_iter().find(|p| p.key.0 == player))
        .map_or(Entity::NONE, |p| p.entity)
}

/// Keeps a line that reached the chat for the end of the step, when a mod
/// listens; a player's line counts against its account's rate.
pub(crate) fn heard(loader: &Loader, said: Said) {
    if !wanted(loader) {
        return;
    }
    let mut heard = HEARD.relock();
    let now = Instant::now();
    let over = said.kind == ChatLine::PLAYER && !admit(&mut heard.rate, said.player, now);
    if over || heard.waiting.len() >= MAX_WAITING {
        drop(heard);
        skipped();
        return;
    }
    heard.waiting.push(said);
    WAITING.store(true, Ordering::Release);
}

/// Whether one more line of `player` fits its rate at `now`, counting it.
fn admit(rate: &mut Vec<Rate>, player: u64, now: Instant) -> bool {
    let index = match rate.iter().position(|&(key, ..)| key == player) {
        Some(index) => index,
        None => {
            if rate.len() >= RATE_ACCOUNTS {
                rate.retain(|&(_, start, _)| now.duration_since(start) < WINDOW);
            }
            if rate.len() >= RATE_ACCOUNTS {
                // Every account is in this window: the line counts as over.
                return false;
            }
            rate.push((player, now, 0));
            rate.len() - 1
        }
    };
    let (_, start, count) = &mut rate[index];
    if now.duration_since(*start) >= WINDOW {
        *start = now;
        *count = 0;
    }
    *count += 1;
    *count <= PER_WINDOW
}

/// Counts a line not raised and logs the first and every hundredth.
#[cold]
fn skipped() {
    if let Some(total) = SKIPPED.add(1) {
        crate::log!(
            WARN,
            "chat lines: {total} not raised to mods, over a player's {PER_WINDOW} a second or a full queue"
        );
    }
}

/// Raises the lines that reached the chat during the step; on the game
/// thread, outside the game's systems.
pub(crate) fn raise(loader: &Loader) {
    if !WAITING.load(Ordering::Acquire) {
        return;
    }
    let lines = {
        let mut heard = HEARD.relock();
        WAITING.store(false, Ordering::Release);
        core::mem::take(&mut heard.waiting)
    };
    let players = crate::world::players(loader).unwrap_or_default();
    for said in lines {
        let entity = players
            .iter()
            .find(|p| p.key.0 == said.player)
            .map_or(Entity::NONE, |p| p.entity);
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
    let mut heard = HEARD.relock();
    heard.waiting.clear();
    heard.rate.clear();
    WAITING.store(false, Ordering::Release);
}

/// The kind of a line the game posted itself, by its chat type: 1 is an
/// emote (server build 387, the loca tag `Emote.chatMessage`), anything
/// else a notice such as a join or leave line.
pub(crate) const fn kind_of(chat_type: u8) -> u32 {
    if chat_type == 1 {
        ChatLine::EMOTE
    } else {
        ChatLine::NOTICE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_account_gets_eight_lines_a_second_and_a_new_window_starts_afresh() {
        let mut rate = Vec::new();
        let start = Instant::now();
        let admitted = (0..12).filter(|_| admit(&mut rate, 7, start)).count();
        assert_eq!(admitted, PER_WINDOW as usize);
        assert!(
            admit(&mut rate, 8, start),
            "another account has its own rate"
        );
        assert!(admit(&mut rate, 7, start + WINDOW), "a new window");
    }

    #[test]
    fn the_rate_forgets_accounts_of_past_windows_once_it_holds_many() {
        let mut rate = Vec::new();
        let start = Instant::now();
        for player in 0..RATE_ACCOUNTS as u64 {
            assert!(admit(&mut rate, player, start));
        }
        assert!(!admit(&mut rate, 999, start), "no room in this window");
        assert!(admit(&mut rate, 1000, start + WINDOW));
        assert_eq!(
            rate.len(),
            1,
            "only the account of the current window stays"
        );
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
    fn the_game_s_own_lines_are_emotes_or_notices() {
        assert_eq!(kind_of(1), ChatLine::EMOTE);
        assert_eq!(kind_of(4), ChatLine::NOTICE);
        assert_eq!(kind_of(0), ChatLine::NOTICE);
    }
}
