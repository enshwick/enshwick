//! The one way a run's answer reaches a player: one policy for a reply of
//! several lines, and a notice, or chat for a client that cannot show one.

/// How a run's reply reaches the player who ran it.
#[derive(Debug, PartialEq, Eq)]
pub enum Reply<'a> {
    /// No text: nothing goes.
    None,
    /// One notice.
    Notice(std::borrow::Cow<'a, str>),
    /// Several lines in chat, to that player alone, however many.
    Chat(&'a str),
}

/// How the reply `text` of a run goes to its player.
///
/// One line of an action that was `done` goes as a notice. Several lines,
/// and every refusal, go in chat when `in_chat` (the player typed the run
/// there and lines to one player work), else joined into one notice.
#[must_use]
pub fn reply_by(text: &str, done: bool, in_chat: bool) -> Reply<'_> {
    if text.is_empty() {
        Reply::None
    } else if done && text.lines().nth(1).is_none() {
        Reply::Notice(text.trim_end_matches(['\r', '\n']).into())
    } else if in_chat {
        Reply::Chat(text)
    } else {
        Reply::Notice(text.lines().collect::<Vec<_>>().join(" / ").into())
    }
}

#[cfg(windows)]
pub(crate) use live::{answer, tell, tell_text};

#[cfg(windows)]
mod live {
    use super::{Reply, reply_by};
    use crate::actions::admission::Path;
    use crate::engine::loader::Loader;

    /// Tells player `key` `text` as a notice, or in chat to a client that
    /// cannot show one.
    pub(crate) fn tell(loader: &Loader, key: u64, text: &str) {
        if crate::base::notices::send(loader, key, text).is_err() {
            let _queued = crate::actions::chat::send(key, text);
        }
    }

    /// Tells player `key` the loader's text `text_key` in the language
    /// they read.
    pub(crate) fn tell_text(loader: &Loader, key: u64, text_key: &'static str) {
        let text = crate::engine::texts::for_player(key, || crate::engine::texts::text(text_key));
        tell(loader, key, text);
    }

    /// Gives player `key` the reply `text` to a run from `path` that was
    /// `done` or refused, as [`reply_by`] sends it; a run from the console or
    /// a remote caller answers its caller instead.
    pub(crate) fn answer(loader: &Loader, path: Path, key: u64, done: bool, text: &str) {
        let in_chat = match path {
            Path::Chat => crate::actions::chat::to_one_player(),
            Path::Interface => false,
            Path::Console | Path::Remote => return,
        };
        match reply_by(text, done, in_chat) {
            Reply::Chat(text) => {
                let _queued = crate::actions::chat::reply(key, text);
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
        assert_eq!(reply_by(help, true, true), Reply::Chat(help));
        assert_eq!(
            reply_by(help, true, false),
            Reply::Notice("online: players online / kick: kick a player".into())
        );
        assert_eq!(
            reply_by("kicked Ann", true, true),
            Reply::Notice("kicked Ann".into())
        );
        assert_eq!(
            reply_by("3 players\nAnn\r\nBob", true, false),
            Reply::Notice("3 players / Ann / Bob".into()),
            "a line ending of the client's kind is no part of a line"
        );
        assert_eq!(reply_by("", true, false), Reply::None);
        assert_eq!(reply_by("", false, true), Reply::None);
    }

    #[test]
    fn a_one_line_notice_leaves_its_line_ending_behind() {
        for text in ["Gate opened\n", "Gate opened\r\n"] {
            assert_eq!(
                reply_by(text, true, false),
                Reply::Notice("Gate opened".into())
            );
        }
    }

    #[test]
    fn a_refusal_of_a_typed_command_goes_into_the_chat_of_who_typed_it() {
        let unknown = "No action dance. /adm help lists them.";
        assert_eq!(reply_by(unknown, false, true), Reply::Chat(unknown));
        assert_eq!(
            reply_by(unknown, false, false),
            Reply::Notice(unknown.into()),
            "as a notice outside chat"
        );
    }
}
