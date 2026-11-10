//! Chat lines on the server, for [`Event::CHAT_SAID`](super::Event::CHAT_SAID)
//! and [`HostApi::chat_filter`](super::HostApi::chat_filter).

use core::ffi::c_void;

use crate::abi::{Entity, Header, PlayerKey, Str};

/// A chat line on the server, borrowed for the call that hands it over.
///
/// Raised with [`Event::CHAT_SAID`](super::Event::CHAT_SAID) after the step
/// for every line that reached the chat, and handed to each
/// [`ChatFilterFn`] for a player's line before anyone reads it. `text` is
/// valid UTF-8 (bytes that are not are replaced), at most the game's own
/// limit of 1024 bytes, and lives only for the call. Layout grows only by
/// appending; read a later field after [`Header::covers`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ChatLine, Header, Str};
///
/// let line = ChatLine {
///     header: Header::of::<ChatLine>(),
///     player: enshwick::PlayerKey::new(7),
///     entity: enshwick::Entity(2),
///     kind: ChatLine::PLAYER,
///     loca: [0; 2],
///     text: Str::new("hello"),
/// };
/// assert_eq!(line.text.len(), 5);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct ChatLine {
    /// Size and version of this struct.
    pub header: Header,
    /// The sender's account key; zero when the game names none.
    pub player: PlayerKey,
    /// The sender's character, `Entity(0)` when it has none in the world.
    pub entity: Entity,
    /// [`ChatLine::PLAYER`], [`ChatLine::EMOTE`] or [`ChatLine::NOTICE`].
    pub kind: u32,
    /// The two text tags the game posts with the line, zero when unused.
    pub loca: [u32; 2],
    /// The text, UTF-8, borrowed for the call; empty for a notice made of
    /// text tags.
    pub text: Str,
}

impl_headed!(ChatLine);

impl ChatLine {
    /// A line a player typed: the game posted it for what their client
    /// sent, whatever chat type the client named.
    pub const PLAYER: u32 = 0;
    /// An emote the server posted for a player.
    pub const EMOTE: u32 = 1;
    /// Any other line the server posted itself, such as a player joining
    /// or leaving.
    pub const NOTICE: u32 = 2;
}

/// The buffer a [`ChatFilterFn`] writes a rewritten line into.
///
/// The loader owns `buffer`, writable for `capacity` bytes during the call
/// only; the filter writes UTF-8 into it, sets `len` and answers
/// [`ChatRewrite::REWRITTEN`]. A `len` over `capacity`, or bytes that are
/// not UTF-8, keep the line as it was. The loader owns the struct too and
/// hands it to one filter at a time on the game thread, never null; the
/// filter keeps no pointer past the call. Layout grows only by appending;
/// read a later field after [`Header::covers`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ChatRewrite, Header};
///
/// let mut bytes = [0u8; 16];
/// let rewrite = ChatRewrite {
///     header: Header::of::<ChatRewrite>(),
///     buffer: bytes.as_mut_ptr(),
///     capacity: 16,
///     len: 0,
/// };
/// assert_eq!(rewrite.len, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct ChatRewrite {
    /// Size and version of this struct.
    pub header: Header,
    /// Where the filter writes the new text; never null.
    pub buffer: *mut u8,
    /// Bytes `buffer` holds: the game's limit for a line.
    pub capacity: u32,
    /// Bytes the filter wrote.
    pub len: u32,
}

impl_headed!(ChatRewrite);

impl ChatRewrite {
    /// The line goes on as it is.
    pub const KEEP: u8 = 0;
    /// The line is dropped as the chat mute drops it: nobody reads it, and
    /// the filters after this one do not run.
    pub const DROP: u8 = 1;
    /// The line goes on as the text written into the buffer; an empty text
    /// drops it as [`ChatRewrite::DROP`] does.
    pub const REWRITTEN: u8 = 2;
}

/// A mod's chat filter, registered with
/// [`HostApi::chat_filter`](super::HostApi::chat_filter).
///
/// It is called on the server's game thread, inside the game's chat post,
/// for a player's line after the chat mute; it answers [`ChatRewrite::KEEP`],
/// [`ChatRewrite::DROP`] or [`ChatRewrite::REWRITTEN`], any other answer
/// keeping the line. `line` and `rewrite` are borrowed for the call and
/// never null; `line.text` is the text after the filters before this one.
/// Never re-entered; it must not unwind.
pub type ChatFilterFn = unsafe extern "system" fn(
    line: *const ChatLine,
    rewrite: *mut ChatRewrite,
    user: *mut c_void,
) -> u8;
