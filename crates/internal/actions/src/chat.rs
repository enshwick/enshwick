//! Chat on the server: hear every line that reached it, and drop or rewrite
//! what players type before anyone reads it.
//!
//! [`Load::on_chat_said`](../struct.Load.html#method.on_chat_said) hears each line after the step: a player's line as
//! the filters left it, an emote, or a notice the server posted. A filter
//! from [`Load::on_chat`](../struct.Load.html#method.on_chat) runs on a player's line inside the game's chat
//! post, after the chat mute, in load order. Chat is for reading: no feature
//! may depend on a typed command. Lines a mod sends raise nothing.
//!
//! # Examples
//!
//! ```
//! use enshwick::chat::ChatVerdict;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     load.on_chat(|line| {
//!         if line.text.contains("spoiler") {
//!             ChatVerdict::Rewrite("[hidden]".to_owned())
//!         } else {
//!             ChatVerdict::Keep
//!         }
//!     })?;
//!     load.on_chat_said(|line| enshwick::info!("{:?}: {} bytes", line.kind, line.text.len()))
//! }
//! ```

use core::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::abi::{self, ChatRewrite, Event, Level};
use crate::script::{Answer, Error, Ty, Value};
use crate::{Entity, Host, Off, PlayerKey};

/// What a chat line is.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum ChatKind {
    /// A line a player typed.
    Player,
    /// An emote of a player; one a client sent passes the filters first.
    Emote,
    /// Another line the server posted, such as a player joining or leaving.
    Notice,
}

/// A chat line on the server, for [`Load::on_chat_said`](../struct.Load.html#method.on_chat_said) and
/// [`Load::on_chat`](../struct.Load.html#method.on_chat).
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct ChatLine {
    /// The sender's account, `None` when the game names none.
    #[script(default)]
    pub player: Option<PlayerKey>,
    /// The sender's character, `None` when it has none in the world.
    #[script(default)]
    pub entity: Option<Entity>,
    /// What the line is.
    pub kind: ChatKind,
    /// The text, at most the game's 1024 bytes; bytes that were not UTF-8
    /// are replaced. Empty for a notice made of text tags.
    pub text: String,
}

impl ChatLine {
    /// The line the loader hands over, its text borrowed for the call.
    fn of(line: &abi::ChatLine) -> Self {
        // SAFETY: the loader's `text` views bytes readable for the call.
        let text = unsafe { line.text.as_str() }.unwrap_or_default();
        Self {
            player: (line.player.0 != 0).then_some(line.player),
            entity: (line.entity.0 != 0).then_some(line.entity),
            kind: match line.kind {
                abi::ChatLine::PLAYER => ChatKind::Player,
                abi::ChatLine::EMOTE => ChatKind::Emote,
                _ => ChatKind::Notice,
            },
            text: text.to_owned(),
        }
    }
}

/// What a chat filter does with a player's line.
///
/// From Luau a filter answers `false` to drop the line, a string to rewrite
/// it, and nil or `true` to keep it.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChatVerdict {
    /// The line goes on as it is.
    Keep,
    /// The line is dropped as the chat mute drops it: nobody reads it, and
    /// the filters after this one do not run.
    Drop,
    /// The line goes on as this text, cut at 1024 bytes on a character
    /// boundary; the filters after this one see it. An empty text drops
    /// the line.
    Rewrite(String),
}

impl Answer for ChatVerdict {
    const TY: &'static Ty = &Ty::Data;

    fn from_answer(answer: Result<Value, Error>) -> Self {
        match answer {
            Ok(Value::Bool(false)) => Self::Drop,
            Ok(Value::String(text)) => Self::Rewrite(text),
            _ => Self::Keep,
        }
    }
}

/// A mod's chat filter and whether it panicked, which turns it off alone.
struct Filter<F> {
    run: F,
    off: AtomicBool,
    host: Host,
}

/// The loader's call of a chat filter `F`.
///
/// # Safety
///
/// - `user` is the leaked `Filter<F>` this function was registered with.
/// - `line` and `rewrite` are null or the loader's, valid for the call,
///   `rewrite.buffer` writable for `rewrite.capacity` bytes.
unsafe extern "system" fn run_chat<F>(
    line: *const abi::ChatLine,
    rewrite: *mut ChatRewrite,
    user: *mut c_void,
) -> u8
where
    F: Fn(&ChatLine) -> ChatVerdict,
{
    // SAFETY: the loader passes a valid line and rewrite for the call, by
    // this function's `# Safety`.
    let (Some(line), Some(rewrite)) = (unsafe { line.as_ref() }, unsafe { rewrite.as_mut() })
    else {
        return ChatRewrite::KEEP;
    };
    // SAFETY: `user` is the leaked `Filter<F>`, by this function's `# Safety`.
    let filter = unsafe { &*user.cast::<Filter<F>>() };
    if filter.off.load(Ordering::Acquire) {
        return ChatRewrite::KEEP;
    }
    let line = ChatLine::of(line);
    match crate::host::caught(|| (filter.run)(&line)) {
        Ok(ChatVerdict::Keep) => ChatRewrite::KEEP,
        Ok(ChatVerdict::Drop) => ChatRewrite::DROP,
        Ok(ChatVerdict::Rewrite(text)) => {
            let text = cut(&text, rewrite.capacity as usize);
            // SAFETY: `buffer` is writable for `capacity` bytes, by this
            // function's `# Safety`, and `text` is at most that long.
            unsafe { core::ptr::copy_nonoverlapping(text.as_ptr(), rewrite.buffer, text.len()) };
            rewrite.len = u32::try_from(text.len()).unwrap_or(0);
            ChatRewrite::REWRITTEN
        }
        Err(panicked) => {
            if !filter.off.swap(true, Ordering::AcqRel) {
                let text = panicked.text();
                filter.host.log(
                    Level::WARN,
                    &format!("a chat filter panicked: {text}; it is off, and lines pass it"),
                );
            }
            ChatRewrite::KEEP
        }
    }
}

/// `text` cut to at most `max` bytes at a character boundary.
fn cut(text: &str, max: usize) -> &str {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::on_chat`.
    #[doc(hidden)]
    fn on_chat<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&ChatLine) -> ChatVerdict + Send + Sync + 'static;

    /// As `Load::on_chat_said`.
    #[doc(hidden)]
    fn on_chat_said<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: Fn(&ChatLine) + Send + Sync + 'static;
}

impl LoadMethods for crate::BaseLoad {
    fn on_chat<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&ChatLine) -> ChatVerdict + Send + Sync + 'static,
    {
        let host = self.host();
        let filter = Filter {
            run: filter,
            off: AtomicBool::new(false),
            host,
        };
        crate::load::hand_over(
            filter,
            |user| (host.api().chat_filter)(host.handle(), run_chat::<F>, user),
            "chat filter",
        )
    }

    fn on_chat_said<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: Fn(&ChatLine) + Send + Sync + 'static,
    {
        self.on_raw_event(Event::CHAT_SAID, move |raw| {
            // SAFETY: subscribed under `Event::CHAT_SAID`, whose data the
            // loader passes as a `ChatLine`, null or valid for the call.
            let line = unsafe {
                abi::read_headed(raw.cast::<abi::ChatLine>(), size_of::<abi::ChatLine>())
            };
            if let Some(line) = line {
                handler(&ChatLine::of(&line));
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_names_its_kind_and_no_sender_or_character_when_the_loader_gives_none() {
        let line = |player, entity, kind| abi::ChatLine {
            header: abi::Header::of::<abi::ChatLine>(),
            player: PlayerKey::new(player),
            entity: Entity(entity),
            kind,
            loca: [0; 2],
            text: abi::Str::new("hi"),
        };
        let emote = ChatLine::of(&line(7, 9, abi::ChatLine::EMOTE));
        assert_eq!(emote.player, Some(PlayerKey::new(7)));
        assert_eq!(emote.entity, Some(Entity(9)));
        assert_eq!(emote.kind, ChatKind::Emote);
        assert_eq!(emote.text, "hi");
        let notice = ChatLine::of(&line(0, 0, 4));
        assert_eq!((notice.player, notice.entity), (None, None));
        assert_eq!(notice.kind, ChatKind::Notice);
        assert_eq!(
            ChatLine::of(&line(7, 0, abi::ChatLine::PLAYER)).kind,
            ChatKind::Player
        );
    }

    #[test]
    fn a_rewrite_is_cut_at_a_character_boundary() {
        assert_eq!(cut("Привіт", 5), "Пр");
        assert_eq!(cut("hi", 1024), "hi");
    }

    #[test]
    fn a_script_answer_of_false_drops_text_rewrites_and_anything_else_keeps() {
        assert_eq!(
            ChatVerdict::from_answer(Ok(Value::Bool(false))),
            ChatVerdict::Drop
        );
        assert_eq!(
            ChatVerdict::from_answer(Ok(Value::String("x".to_owned()))),
            ChatVerdict::Rewrite("x".to_owned())
        );
        assert_eq!(ChatVerdict::from_answer(Ok(Value::Nil)), ChatVerdict::Keep);
        assert_eq!(
            ChatVerdict::from_answer(Ok(Value::Bool(true))),
            ChatVerdict::Keep
        );
    }
}
