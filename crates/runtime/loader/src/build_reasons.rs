//! Why a build filter refused a placement, told to the builder. A client
//! that runs the refusing mod shows the reason under the build cursor
//! through the HUD; the server tells a player whose client cannot as one
//! notice, or in chat to a client without Enshwick.
//!
//! The cursor check runs every frame a player aims, so the check's own path
//! only copies the reason into a fixed slot, never waiting for the lock; the
//! server's step tells each player at most once a second, and the same
//! reason again only after that player's refusals stopped for a second.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use enshwick_core::Entity;
use enshwick_core::abi::Side;

use crate::host::Loader;
use crate::mod_index::ModIndex;
use enshwick_base::Relock;

/// The longest reason kept, in bytes; a longer one is cut on a character
/// boundary.
const MAX_LEN: usize = 120;
/// Players whose refusals one step keeps at most.
const SLOTS: usize = 32;
/// The least time between two notices to one player, and the pause after
/// which the same reason counts as new.
const GAP_MS: u64 = 1000;
/// How long the client shows a reason after the last refusal.
const SHOWN_MS: u64 = 250;
/// How long the server remembers a player it told.
const FORGET_MS: u64 = 60_000;

/// A refusing mod's reason, copied out of its thread's text.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Reason {
    owner: ModIndex,
    len: u8,
    bytes: [u8; MAX_LEN],
}

impl Reason {
    fn new(owner: ModIndex, text: &str) -> Self {
        let end = text.floor_char_boundary(MAX_LEN);
        let mut bytes = [0; MAX_LEN];
        bytes[..end].copy_from_slice(&text.as_bytes()[..end]);
        Self {
            owner,
            len: u8::try_from(end).unwrap_or(0),
            bytes,
        }
    }

    fn text(&self) -> &str {
        core::str::from_utf8(&self.bytes[..usize::from(self.len)]).unwrap_or_default()
    }
}

/// A player's latest refusal: their character, the reason and when.
#[derive(Clone, Copy)]
struct Seen {
    player: Entity,
    reason: Reason,
    at_ms: u64,
}

/// What the server last told a player; `sent_ms` is `None` before the
/// first notice.
#[derive(Clone, Copy)]
struct Told {
    player: Entity,
    reason: Reason,
    sent_ms: Option<u64>,
    seen_ms: u64,
}

struct State {
    start: OnceLock<Instant>,
    /// Set once `seen` may hold a refusal, so an idle step takes no lock.
    any: AtomicBool,
    /// The step's refusals by player, filled on the cursor threads.
    seen: Mutex<[Option<Seen>; SLOTS]>,
    /// When `local` last changed, in ms since `start` plus one, 0 before any
    /// refusal, so a HUD frame with no recent one takes no lock.
    local_at: AtomicU64,
    /// The client's own latest refusal, for its HUD.
    local: Mutex<Option<Seen>>,
    /// What the server told each player lately, on its game thread.
    told: Mutex<Vec<Told>>,
}

static STATE: State = State {
    start: OnceLock::new(),
    any: AtomicBool::new(false),
    local_at: AtomicU64::new(0),
    seen: Mutex::new([None; SLOTS]),
    local: Mutex::new(None),
    told: Mutex::new(Vec::new()),
};

fn now_ms() -> u64 {
    let start = STATE.start.get_or_init(Instant::now);
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Keeps mod `owner`'s refusal of `player`'s placement for `text`, on the
/// cursor check's thread right after the filter returned: a check in the
/// `server` world for the notices, any other for the client's own HUD, so a
/// hosting client shows only its own player's refusals.
#[cold]
pub(crate) fn refused(owner: ModIndex, player: Entity, text: &str, server: bool) {
    if text.is_empty() {
        return;
    }
    let seen = Seen {
        player,
        reason: Reason::new(owner, text),
        at_ms: now_ms(),
    };
    if !server {
        if let Some(mut local) = STATE.local.try_relock() {
            *local = Some(seen);
            STATE
                .local_at
                .store(seen.at_ms.saturating_add(1), Ordering::Release);
        }
        return;
    }
    let Some(mut slots) = STATE.seen.try_relock() else {
        return;
    };
    let slot = slots
        .iter()
        .position(|s| s.is_some_and(|s| s.player == player))
        .or_else(|| slots.iter().position(Option::is_none));
    if let Some(slot) = slot {
        slots[slot] = Some(seen);
        STATE.any.store(true, Ordering::Release);
    }
}

/// Whether the server tells `seen` now, given what it `told` that player:
/// a new reason, or the same one after a pause, and never twice in a second.
fn due(told: &Told, seen: &Seen) -> bool {
    let new = seen.reason != told.reason || seen.at_ms.saturating_sub(told.seen_ms) > GAP_MS;
    new && told
        .sent_ms
        .is_none_or(|sent| seen.at_ms.saturating_sub(sent) >= GAP_MS)
}

/// Tells the builders the step's refusals, on the server's game thread after
/// its step.
pub(crate) fn after_step(loader: &Loader) {
    if !STATE.any.swap(false, Ordering::Acquire) {
        return;
    }
    let seen = core::mem::replace(&mut *STATE.seen.relock(), [None; SLOTS]);
    let mut told = STATE.told.relock();
    let now = now_ms();
    told.retain(|t| now.saturating_sub(t.seen_ms) < FORGET_MS);
    if seen.iter().all(Option::is_none) {
        return;
    }
    let players = crate::world::players(loader).unwrap_or_default();
    for seen in seen.iter().flatten() {
        let Some(player) = players.iter().find(|p| p.entity == seen.player) else {
            continue;
        };
        let index = told
            .iter()
            .position(|t| t.player == seen.player)
            .unwrap_or_else(|| {
                told.push(Told {
                    player: seen.player,
                    reason: Reason::new(seen.reason.owner, ""),
                    sent_ms: None,
                    seen_ms: 0,
                });
                told.len() - 1
            });
        let tell = due(&told[index], seen);
        told[index].seen_ms = seen.at_ms;
        if !tell {
            continue;
        }
        told[index].reason = seen.reason;
        told[index].sent_ms = Some(seen.at_ms);
        let key = player.key.get();
        if shown_by_client(loader, seen.reason.owner, key) {
            continue;
        }
        let text = shown(loader, &seen.reason, crate::net::language_of(key));
        if crate::notices::send(loader, key, &text).is_err() {
            let _sent = crate::chat::send(key, &text);
        }
    }
}

/// Whether the client of player `key` shows mod `owner`'s reasons itself:
/// it hosts this game, whose own cursor check runs every mod, or it runs
/// Enshwick and the mod runs there too.
fn shown_by_client(loader: &Loader, owner: ModIndex, key: u64) -> bool {
    crate::carrier::is_host(key)
        || crate::net::ready(key)
            && loader
                .mods
                .get(owner.get())
                .is_some_and(|m| m.manifest.side == Side::BOTH)
}

/// `reason` in `language`, the game's for `None`, from its mod's texts,
/// else as the mod gave it.
fn shown(
    loader: &Loader,
    reason: &Reason,
    language: Option<enshwick_core::i18n::Language>,
) -> String {
    let language = language.unwrap_or_else(crate::texts::language);
    loader
        .mods
        .get(reason.owner.get())
        .and_then(|m| m.texts.relock().as_ref()?.reason(reason.text(), language))
        .unwrap_or_else(|| reason.text().replace('\t', " "))
}

thread_local! {
    /// The client's last reason as shown, so a frame that shows the same
    /// one reads no texts.
    static SHOWN: core::cell::RefCell<Option<(Reason, String)>> = const { core::cell::RefCell::new(None) };
}

/// Hands `show` the client's latest reason while the player still aims at
/// the refused placement, on the interface thread inside the HUD's frame.
pub(crate) fn hud_line(show: impl FnOnce(&str)) {
    let at = STATE.local_at.load(Ordering::Acquire);
    if at == 0 || now_ms().saturating_sub(at - 1) > SHOWN_MS {
        return;
    }
    let Some(seen) = STATE.local.try_relock().and_then(|local| *local) else {
        return;
    };
    if now_ms().saturating_sub(seen.at_ms) > SHOWN_MS {
        return;
    }
    let Some(loader) = crate::host::loader() else {
        return;
    };
    SHOWN.with_borrow_mut(|kept| {
        if kept.as_ref().is_none_or(|(r, _)| *r != seen.reason) {
            *kept = Some((seen.reason, shown(loader, &seen.reason, None)));
        }
        if let Some((_, text)) = kept {
            show(text);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(text: &str, at_ms: u64) -> Seen {
        Seen {
            player: Entity(7),
            reason: Reason::new(ModIndex::new(1), text),
            at_ms,
        }
    }

    fn told(text: &str, sent_ms: Option<u64>, seen_ms: u64) -> Told {
        Told {
            player: Entity(7),
            reason: Reason::new(ModIndex::new(1), text),
            sent_ms,
            seen_ms,
        }
    }

    #[test]
    fn a_player_hears_a_new_reason_at_most_once_a_second() {
        assert!(due(&told("", None, 0), &seen("low", 5000)));
        assert!(!due(&told("low", Some(5000), 5000), &seen("high", 5400)));
        assert!(due(&told("low", Some(5000), 5900), &seen("high", 6000)));
    }

    #[test]
    fn a_player_refused_in_the_clock_s_first_second_hears_it() {
        assert!(due(&told("", None, 0), &seen("low", 300)));
    }

    #[test]
    fn the_same_reason_is_told_again_only_after_the_refusals_paused() {
        assert!(!due(&told("low", Some(5000), 9950), &seen("low", 9966)));
        assert!(due(&told("low", Some(5000), 9000), &seen("low", 10_500)));
        assert!(
            !due(&told("low", Some(5000), 9000), &seen("low", 9000 + GAP_MS)),
            "a pause of exactly the gap is no pause"
        );
    }

    #[test]
    fn a_hosting_client_shows_only_its_own_worlds_refusals_and_tells_only_the_servers() {
        assert!(!STATE.any.load(Ordering::Acquire), "an idle step skips");
        hud_line(|_| panic!("no refusal to show"));
        assert_eq!(
            STATE.local_at.load(Ordering::Acquire),
            0,
            "an idle HUD frame skips"
        );
        refused(ModIndex::new(1), Entity(40), "far player", true);
        assert!(
            STATE.local.relock().is_none(),
            "the server world's check is not the HUD's"
        );
        assert_eq!(STATE.seen.relock().iter().flatten().count(), 1);
        assert!(STATE.any.load(Ordering::Acquire), "the step looks");
        refused(ModIndex::new(1), Entity(40), "nearer", true);
        let kept: Vec<_> = STATE
            .seen
            .relock()
            .iter()
            .flatten()
            .map(|s| s.reason.text().to_owned())
            .collect();
        assert_eq!(
            kept,
            ["nearer"],
            "a player's later refusal takes their slot"
        );
        refused(ModIndex::new(1), Entity(3), "own cursor", false);
        assert_ne!(STATE.local_at.load(Ordering::Acquire), 0, "the HUD looks");
        let local = STATE
            .local
            .relock()
            .map(|s| (s.player, s.reason.text().to_owned()));
        assert_eq!(local, Some((Entity(3), "own cursor".to_owned())));
        assert_eq!(
            STATE.seen.relock().iter().flatten().count(),
            1,
            "a client world's entity never reaches the server's notices"
        );
    }

    #[test]
    fn a_long_reason_is_cut_on_a_character_boundary() {
        let long = "ї".repeat(100);
        let reason = Reason::new(ModIndex::new(1), &long);
        assert_eq!(reason.text().len(), MAX_LEN);
        assert!(long.starts_with(reason.text()));
    }
}
