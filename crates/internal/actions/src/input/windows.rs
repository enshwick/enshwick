//! The items of the windows the server's mods show this client, by the ids
//! a [`CommandAction::PressWindowItem`](super::CommandAction::PressWindowItem)
//! and its kin name them.

use super::Input;
use crate::WorldError;
use crate::abi::{ClientWindowItem as Raw, Status, Str};
use enshwick_world::__internal::world_status;

/// What an item of a window is, from [`ClientWindowItem::kind`].
///
/// # Examples
///
/// ```
/// use enshwick::input::WindowItemKind;
///
/// assert_eq!(WindowItemKind::TextField.name(), "text_field");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WindowItemKind {
    /// A button: a row's, an action's Run, a form's Cancel, the window's
    /// Close or its map's; a press clicks it.
    Button,
    /// A list's switch for one player; a press flips it.
    Switch,
    /// A field naming one of the server's players: a pick takes one of the
    /// list, a text types a name.
    PlayerField,
    /// A field of a few options; a pick takes one.
    ChoiceField,
    /// A field of a number in a range, which no command sets.
    NumberField,
    /// An on-or-off field; a press flips it.
    ToggleField,
    /// A field of typed text; a text sets it.
    TextField,
    /// A field of one of the game's items: a text sets its search, a pick
    /// takes an item by the name the game shows.
    ItemField,
    /// A field of one of the game's templates, as
    /// [`WindowItemKind::ItemField`].
    TemplateField,
    /// A kind a newer loader lists that this build of the API does not
    /// name, as its raw kind and field.
    Other(u32, u32),
}

impl WindowItemKind {
    /// Every kind this build names, with its raw kind and field and its
    /// name as the Luau API gives it.
    const ALL: [(u32, u32, Self, &'static str); 9] = [
        (Raw::BUTTON, 0, Self::Button, "button"),
        (Raw::SWITCH, 0, Self::Switch, "switch"),
        (Raw::FIELD, Raw::PLAYER, Self::PlayerField, "player_field"),
        (Raw::FIELD, Raw::CHOICE, Self::ChoiceField, "choice_field"),
        (Raw::FIELD, Raw::NUMBER, Self::NumberField, "number_field"),
        (Raw::FIELD, Raw::TOGGLE, Self::ToggleField, "toggle_field"),
        (Raw::FIELD, Raw::TEXT, Self::TextField, "text_field"),
        (Raw::FIELD, Raw::ITEM, Self::ItemField, "item_field"),
        (
            Raw::FIELD,
            Raw::TEMPLATE,
            Self::TemplateField,
            "template_field",
        ),
    ];

    fn from_raw(kind: u32, field: u32) -> Self {
        Self::ALL
            .iter()
            .find(|(k, f, ..)| *k == kind && *f == field)
            .map_or(Self::Other(kind, field), |(.., this, _)| *this)
    }

    /// The kind's name as the Luau API gives it; `"other"` for every
    /// [`WindowItemKind::Other`].
    ///
    /// # Examples
    ///
    /// See [`WindowItemKind`].
    #[must_use]
    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(.., this, _)| *this == self)
            .map_or("other", |(.., name)| name)
    }
}

/// An item of a window a server's mod shows this client, from
/// [`Input::client_windows`].
///
/// The item's id comes from the action ids the window's mod declared:
/// `<action>` is the button that runs the action, drawn whole or in its
/// form; `<action>/<n>` the field of its argument `n`, from 0;
/// `<action>@<account>` a list's switch for one player, by the account's
/// key; `<action>#<n>` the window's `n`th row button that runs the action,
/// from 0; `@close`, `@cancel` and `@map` the window's Close, a form's
/// Cancel and the map's button.
///
/// # Examples
///
/// ```
/// use enshwick::input::{Input, ClientWindowItem};
///
/// fn ban_of(input: Input, name: &str) -> Option<ClientWindowItem> {
///     let items = input.client_windows().ok()?;
///     items.into_iter().find(|item| item.shown && item.item.starts_with("ban#") && item.line == name)
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ClientWindowItem {
    /// The window, as `mod:window`.
    pub window: String,
    /// The item's id in it.
    pub item: String,
    /// What the item is.
    pub kind: WindowItemKind,
    /// What it shows: a button's or a field's label, a switch's player.
    pub label: String,
    /// The line it stands under: its row's text or its action's label;
    /// empty for none.
    pub line: String,
    /// Whether its window shows and takes commands: only the newest does.
    /// While it steps aside for the Map screen, or the game's own popup
    /// covers it, a command waits until it draws again.
    pub shown: bool,
    /// Whether a switch is on.
    pub on: bool,
    /// Whether the window draws it off: an action the player may not run
    /// now, or the map's button on a build that cannot open the map. A Run
    /// whose fields do not all hold a value is drawn off too, though this
    /// reads false; a press of it is cancelled with
    /// [`CancelReason::ItemOff`](super::CancelReason::ItemOff) all the same.
    pub off: bool,
}

/// The text of a string the loader lent, owned.
fn owned(text: Str) -> String {
    // SAFETY: the loader keeps the entry's strings until the next
    // `HostApi::client_windows` call on this thread, and this copies them
    // before that call.
    unsafe { text.as_str() }.unwrap_or_default().to_owned()
}

impl ClientWindowItem {
    fn from_raw(raw: &Raw) -> Self {
        Self {
            window: owned(raw.window),
            item: owned(raw.item),
            kind: WindowItemKind::from_raw(raw.kind, raw.field),
            label: owned(raw.label),
            line: owned(raw.line),
            shown: raw.flags & Raw::SHOWN != 0,
            on: raw.flags & Raw::ON != 0,
            off: raw.flags & Raw::OFF != 0,
        }
    }
}

#[crate::script(client_windows)]
impl Input {
    /// The items of the windows the server's mods show this client, the
    /// newest window last, by the ids client commands name them; the items
    /// of a form while one is open. Any thread.
    ///
    /// # Errors
    ///
    /// [`WorldError::Unavailable`] on a server.
    ///
    /// # Examples
    ///
    /// See [`ClientWindowItem`].
    #[script]
    pub fn client_windows(self) -> Result<Vec<ClientWindowItem>, WorldError> {
        let host = self.0.host;
        let entry = u32::try_from(size_of::<Raw>()).unwrap_or(0);
        let mut count = 0u32;
        // Count first, then read: the strings of the second call stay until
        // the next one on this thread.
        world_status((host.api().client_windows)(
            host.handle(),
            core::ptr::null_mut(),
            entry,
            0,
            &raw mut count,
        ))?;
        let mut raw = vec![Raw::EMPTY; usize::try_from(count).unwrap_or(0)];
        let capacity = u32::try_from(raw.len()).unwrap_or(0);
        let status = (host.api().client_windows)(
            host.handle(),
            raw.as_mut_ptr(),
            entry,
            capacity,
            &raw mut count,
        );
        if status != Status::OK {
            world_status(status)?;
        }
        // Items listed between the two calls past the room are left out.
        let read = usize::try_from(count).unwrap_or(0).min(raw.len());
        Ok(raw[..read].iter().map(ClientWindowItem::from_raw).collect())
    }
}

/// [`ClientWindowItem`] as Luau gives it.
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
#[script(name = "ClientWindowItem")]
struct LuauItem {
    /// The window, as `mod:window`.
    pub window: String,
    /// The item's id in it.
    pub item: String,
    /// `"button"`, `"switch"`, `"player_field"`, `"choice_field"`,
    /// `"number_field"`, `"toggle_field"`, `"text_field"`, `"item_field"`,
    /// `"template_field"` or `"other"`.
    pub kind: String,
    /// What it shows.
    pub label: String,
    /// The line it stands under.
    pub line: String,
    /// Whether its window shows and takes commands.
    pub shown: bool,
    /// Whether a switch is on.
    pub on: bool,
    /// Whether the window draws it off.
    pub off: bool,
}

impl From<ClientWindowItem> for LuauItem {
    fn from(item: ClientWindowItem) -> Self {
        Self {
            kind: item.kind.name().to_owned(),
            window: item.window,
            item: item.item,
            label: item.label,
            line: item.line,
            shown: item.shown,
            on: item.on,
            off: item.off,
        }
    }
}

impl crate::script::Typed for ClientWindowItem {
    const TY: &'static crate::script::Ty = <LuauItem as crate::script::Typed>::TY;
}

impl crate::script::IntoScript for ClientWindowItem {
    fn into_value(self) -> crate::script::Value {
        LuauItem::from(self).into_value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_reads_its_kind_and_flags_from_the_loader_s_entry() {
        let raw = enshwick_abi::headed!(Raw {
            kind: Raw::FIELD,
            field: Raw::TEXT,
            flags: Raw::SHOWN | Raw::OFF,
            window: Str::new("you.admin:panel"),
            item: Str::new("say/0"),
            label: Str::new("Message"),
            line: Str::new("Announce"),
            ..Raw::EMPTY
        });
        let item = ClientWindowItem::from_raw(&raw);
        assert_eq!(item.kind, WindowItemKind::TextField);
        assert_eq!((item.shown, item.on, item.off), (true, false, true));
        assert_eq!(
            (item.item.as_str(), item.line.as_str()),
            ("say/0", "Announce")
        );
        let newer = WindowItemKind::from_raw(9, 0);
        assert_eq!(
            (newer, newer.name()),
            (WindowItemKind::Other(9, 0), "other")
        );
        assert_eq!(
            LuauItem::from(item).kind,
            "text_field",
            "Luau names the kind"
        );
    }
}
