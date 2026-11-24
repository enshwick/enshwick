//! The one way a run's answer reaches a player: in chat for a run typed
//! there, else as one notice.

/// How a run's reply reaches the player who ran it.
#[derive(Debug, PartialEq, Eq)]
pub enum Reply<'a> {
    /// No text: nothing goes.
    None,
    /// One notice, its lines joined.
    Notice(std::borrow::Cow<'a, str>),
    /// Every line in chat, to that player alone.
    Chat(&'a str),
}

/// How the reply `text` of a run goes to its player.
///
/// The whole reply goes in chat when `in_chat` (the player typed the run
/// there and lines to one player work), else its lines joined into one
/// notice. A notice is the game's "cannot do that" message: on client
/// 26430625 it draws in a warning colour, on the line of an interaction
/// prompt when one shows, and the game's own notices replace it.
#[must_use]
pub fn reply_by(text: &str, in_chat: bool) -> Reply<'_> {
    if text.is_empty() {
        Reply::None
    } else if in_chat {
        Reply::Chat(text)
    } else if text.lines().nth(1).is_none() {
        Reply::Notice(text.trim_end_matches(['\r', '\n']).into())
    } else {
        Reply::Notice(text.lines().collect::<Vec<_>>().join(" / ").into())
    }
}

#[cfg(windows)]
pub(crate) use live::{answer, tell_text};

#[cfg(windows)]
mod live {
    use super::{Reply, reply_by};
    use crate::actions::admission::Path;
    use crate::base::tell::tell;
    use crate::engine::loader::Loader;

    /// Tells player `key` the loader's text `text_key` in the language
    /// they read.
    pub(crate) fn tell_text(loader: &Loader, key: u64, text_key: &'static str) {
        let text = crate::engine::texts::for_player(key, || crate::engine::texts::text(text_key));
        tell(loader, key, text);
    }

    /// Gives player `key` the reply `text` to a run from `path`, done or
    /// refused, as [`reply_by`] sends it; a run from the console or a remote
    /// caller answers its caller instead.
    pub(crate) fn answer(loader: &Loader, path: Path, key: u64, text: &str) {
        let in_chat = match path {
            Path::Chat => crate::actions::chat::to_one_player(),
            Path::Interface => false,
            Path::Console | Path::Remote => return,
        };
        match reply_by(text, in_chat) {
            Reply::Chat(chat_text) => {
                if !crate::actions::chat::reply(key, chat_text)
                    && let Reply::Notice(notice) = reply_by(text, false)
                {
                    tell(loader, key, &notice);
                }
            }
            Reply::Notice(text) => tell(loader, key, &text),
            Reply::None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_of_several_lines_goes_as_one_notice_outside_chat() {
        let help = "online: players online\nkick: kick a player";
        assert_eq!(reply_by(help, true), Reply::Chat(help));
        assert_eq!(
            reply_by(help, false),
            Reply::Notice("online: players online / kick: kick a player".into())
        );
        assert_eq!(
            reply_by("3 players\nAnn\r\nBob", false),
            Reply::Notice("3 players / Ann / Bob".into()),
            "a line ending of the client's kind is no part of a line"
        );
        assert_eq!(reply_by("", false), Reply::None);
        assert_eq!(reply_by("", true), Reply::None);
    }

    #[test]
    fn a_one_line_notice_leaves_its_line_ending_behind() {
        for text in ["Gate opened\n", "Gate opened\r\n", "Gate opened\r"] {
            assert_eq!(reply_by(text, false), Reply::Notice("Gate opened".into()));
        }
    }

    #[test]
    fn a_one_line_notice_borrows_its_text() {
        assert!(matches!(
            reply_by("Gate opened\n", false),
            Reply::Notice(std::borrow::Cow::Borrowed("Gate opened"))
        ));
    }

    #[test]
    fn a_one_line_reply_to_a_typed_command_goes_into_the_chat_of_who_typed_it() {
        let done = "Chest with Wood Log × 10 appears near 3736, 844, 1437.";
        assert_eq!(
            reply_by(done, true),
            Reply::Chat(done),
            "a notice draws beside an interaction prompt and the game's own notices replace it"
        );
        let unknown = "No action dance. /adm help lists them.";
        assert_eq!(reply_by(unknown, true), Reply::Chat(unknown));
        assert_eq!(
            reply_by(unknown, false),
            Reply::Notice(unknown.into()),
            "as a notice outside chat"
        );
    }
}
