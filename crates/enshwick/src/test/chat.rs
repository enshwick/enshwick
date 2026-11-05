//! Chat lines, as the loader hands them to chat filters and to the mods
//! that hear the chat.

use crate::PlayerKey;
use crate::abi::{ChatLine, ChatRewrite, Entity, Event, Header, Str};

use super::Installed;
use super::table;

/// The game's limit for a chat line, in bytes.
const MAX_TEXT: usize = 0x400;
/// The lines of one account the loader raises as heard in a window.
const PER_WINDOW: u32 = 8;
/// The loader's chat rate window, in nanoseconds.
const WINDOW_NANOS: u64 = 1_000_000_000;

impl Installed {
    /// Runs a line the player of account `player` typed through the fake
    /// server's chat as the loader does: a muted account's line reaches no
    /// filter and no handler; the filters run in load order until one drops
    /// it, each seeing the text the one before left; the line that goes on
    /// is raised as [`Event::CHAT_SAID`], for at most eight lines of an
    /// account a second of fake server time ([`Installed::tick`]), while the
    /// filters run on every line. The text the chat posts, `None` when the
    /// line was dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let key = enshwick::PlayerKey::new(7);
    /// assert_eq!(loader.say(key, "hello").as_deref(), Some("hello"));
    /// ```
    #[must_use]
    pub fn say(&self, player: PlayerKey, text: &str) -> Option<String> {
        let (muted, filters) = table::with((false, Vec::new()), |s| {
            (s.muted.contains(&player), s.chat_filters.clone())
        });
        if muted {
            return None;
        }
        let mut text = text.to_owned();
        let mut buffer = vec![0u8; MAX_TEXT];
        for (callback, user) in filters {
            let line = line(player, ChatLine::PLAYER, &text);
            let mut rewrite = ChatRewrite {
                header: Header::of::<ChatRewrite>(),
                buffer: buffer.as_mut_ptr(),
                capacity: u32::try_from(MAX_TEXT).unwrap_or(u32::MAX),
                len: 0,
            };
            let user = core::ptr::with_exposed_provenance_mut(user);
            // SAFETY: a valid line and rewrite buffer for the call; `user` is
            // the mod's pointer.
            match unsafe { callback(&raw const line, &raw mut rewrite, user) } {
                ChatRewrite::DROP => return None,
                ChatRewrite::REWRITTEN => {
                    let written = buffer.get(..rewrite.len as usize);
                    match written.and_then(|b| core::str::from_utf8(b).ok()) {
                        Some("") => return None,
                        Some(new) => new.clone_into(&mut text),
                        None => {}
                    }
                }
                _ => {}
            }
        }
        if table::with(true, |s| admit(&mut s.chat_rate, player, s.ticks.nanos())) {
            let said = line(player, ChatLine::PLAYER, &text);
            // SAFETY: `said` is a `ChatLine`, the data of `CHAT_SAID`, and
            // lives for the call.
            let _ran = unsafe { self.raise_raw(Event::CHAT_SAID, (&raw const said).cast()) };
        }
        Some(text)
    }
}

/// Whether one more line of `player` is heard at `now`, counting it: the
/// loader raises [`Event::CHAT_SAID`] for [`PER_WINDOW`] lines of an account
/// in a window of [`WINDOW_NANOS`] from its first.
fn admit(rate: &mut Vec<(PlayerKey, u64, u32)>, player: PlayerKey, now: u64) -> bool {
    let index = rate.iter().position(|r| r.0 == player).unwrap_or_else(|| {
        rate.push((player, now, 0));
        rate.len() - 1
    });
    let Some((_, start, count)) = rate.get_mut(index) else {
        return true;
    };
    if now.saturating_sub(*start) >= WINDOW_NANOS {
        *start = now;
        *count = 0;
    }
    *count += 1;
    *count <= PER_WINDOW
}

/// A line of `kind` from `player`, viewing `text`.
fn line(player: PlayerKey, kind: u32, text: &str) -> ChatLine {
    ChatLine {
        header: Header::of::<ChatLine>(),
        player,
        entity: Entity(0),
        kind,
        loca: [0; 2],
        text: Str::new(text),
    }
}
