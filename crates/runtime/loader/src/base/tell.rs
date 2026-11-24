//! The one way the loader tells a player on the server something: a chat
//! line, or a notice where the client shows one. The chat that carries the
//! lines sits in a layer above and names itself here when it seals.

use std::sync::OnceLock;

use crate::engine::loader::Loader;

/// What the chat gives the loader's lines.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Chat {
    /// Queues a chat line for the player with an account key, or for
    /// everyone with 0; whether it was queued.
    pub(crate) send: fn(to: u64, text: &str) -> bool,
    /// The lines for one player dropped, and those that reached others.
    pub(crate) delivery: fn() -> (u64, u64),
}

/// What telling keeps for the process.
struct State {
    /// The chat, once it sealed.
    chat: OnceLock<Chat>,
}

impl State {
    const fn new() -> Self {
        Self {
            chat: OnceLock::new(),
        }
    }

    /// Names `chat`; the first one named stays.
    fn on_chat(&self, chat: Chat) {
        let _first = self.chat.set(chat);
    }

    /// Queues `text` for `to` in the named chat; false without one.
    fn line(&self, to: u64, text: &str) -> bool {
        self.chat.get().is_some_and(|chat| (chat.send)(to, text))
    }

    /// The named chat's delivery; none without one.
    fn delivery(&self) -> (u64, u64) {
        self.chat.get().map_or((0, 0), |chat| (chat.delivery)())
    }
}

static STATE: State = State::new();

/// Names `chat` as what carries the loader's chat lines.
pub(crate) fn on_chat(chat: Chat) {
    STATE.on_chat(chat);
}

/// Queues `text` in chat for the player with account key `to`, or for
/// everyone with 0; whether it was queued, which the loader's own lines may
/// ignore: false while chat lines are not carried.
pub(crate) fn line(to: u64, text: &str) -> bool {
    STATE.line(to, text)
}

/// Tells player `key` `text` as a notice, or in chat to a client that
/// cannot show one.
pub(crate) fn tell(loader: &Loader, key: u64, text: &str) {
    if crate::base::notices::send(loader, key, text).is_err() {
        let _queued = line(key, text);
    }
}

/// The chat lines for one player dropped, and those that reached other
/// players too; none without a chat.
pub(crate) fn delivery() -> (u64, u64) {
    STATE.delivery()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_goes_through_the_chat_that_named_itself_and_nowhere_before() {
        use std::sync::atomic::{AtomicU64, Ordering};

        static SENT: AtomicU64 = AtomicU64::new(0);
        let state = State::new();
        assert!(!state.line(7, "hello"));
        assert_eq!(state.delivery(), (0, 0));
        state.on_chat(Chat {
            send: |to, text| {
                SENT.fetch_add(to * 100 + text.len() as u64, Ordering::Relaxed);
                false
            },
            delivery: || (0, 0),
        });
        state.on_chat(Chat {
            send: |_, _| panic!("the first chat stays"),
            delivery: || (1, 1),
        });
        assert!(
            !state.line(7, "hello"),
            "the chat queued nothing, as before it links"
        );
        assert_eq!(SENT.load(Ordering::Relaxed), 705);
        assert_eq!(state.delivery(), (0, 0));
    }
}
