//! What a queued command does, its arguments checked, and what follows
//! from its kind.

use enshwick_core::RecipeId;
use enshwick_core::abi::{ClientCommand as Spec, ClientCommandStatus as Status};

use super::play::Host;
use super::{FrameChange, ModsAsk, dialogs, edits, windows};
use crate::actions::client_control::{Act, Mark, MenuTab};

/// What a queued command does, its arguments checked.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Kind {
    /// Opens the screen byte.
    Open(u8),
    /// Closes the open screen, to the world's screen byte.
    Close(u8),
    /// Shows the tab.
    Tab(MenuTab),
    /// Turns the camera by yaw and pitch radians once.
    Look([f32; 2]),
    /// Holds the direction for the milliseconds.
    Move([f32; 2], u32),
    /// Highlights the recipe in the open crafting menu.
    Highlight(RecipeId),
    /// Joins a server from the main menu; its target and password live in
    /// [`play`] from the queue on.
    Join,
    /// Leaves play back to the main menu.
    Leave,
    /// Holds the game action of the digital input's bit for the
    /// milliseconds; `true` for a form of the interact key, whose screen of
    /// what the player faces is the press's own effect.
    Press(u8, u32, bool),
    /// Sets, moves or clears the local player's waypoint, or pings a
    /// position, through its input's waypoint request.
    Mark(Mark),
    /// Writes a crafting, item transfer, equipment or item request into the
    /// local player's input.
    Act(Act),
    /// Hosts a world from the main menu; [`play`] runs it from the queue
    /// on, as a join.
    Host(Host),
    /// Shows a page of the loader's own Mods screen.
    Mods(ModsAsk),
    /// Acts on an item of a carried window; its ask lives in [`windows`]
    /// from the queue on.
    Window(windows::Act),
    /// Answers the loader's dialog of one number with the raw answer,
    /// through [`dialogs`].
    Answer(dialogs::Ask),
    /// Sends a chat line as the local player's own; its line lives in
    /// [`chat`] from the queue on.
    Chat,
    /// Binds a key or sets a setting on a mod's page of the loader's own
    /// Mods screen; its change lives in [`edits`] from the queue on.
    Edit(edits::Edit),
    /// Quits the game from the main menu through its own quit command.
    Quit,
}

impl Kind {
    pub(super) const fn raw(self) -> u32 {
        match self {
            Self::Open(_) => Spec::OPEN_SCREEN,
            Self::Close(_) => Spec::CLOSE_SCREEN,
            Self::Tab(_) => Spec::CHOOSE_TAB,
            Self::Look(_) => Spec::LOOK,
            Self::Move(..) => Spec::MOVE,
            Self::Highlight(_) => Spec::HIGHLIGHT_RECIPE,
            Self::Join => Spec::JOIN,
            Self::Host(_) => Spec::HOST,
            Self::Mods(ModsAsk {
                mod_index: None, ..
            }) => Spec::OPEN_MODS,
            Self::Mods(ModsAsk { gear: false, .. }) => Spec::OPEN_MOD,
            Self::Mods(ModsAsk { gear: true, .. }) => Spec::OPEN_MOD_SETTINGS,
            Self::Leave => Spec::LEAVE,
            Self::Press(..) => Spec::PRESS,
            Self::Mark(Mark::Set(_)) => Spec::SET_WAYPOINT,
            Self::Mark(Mark::Clear) => Spec::CLEAR_WAYPOINT,
            Self::Mark(Mark::Ping(_)) => Spec::PING,
            Self::Act(Act::Craft { .. }) => Spec::CRAFT,
            Self::Act(Act::Move { .. }) => Spec::MOVE_ITEM,
            Self::Act(Act::MoveAll { to: 0, .. }) => Spec::TAKE_ALL,
            Self::Act(Act::MoveAll { .. }) => Spec::DEPOSIT_ALL,
            Self::Act(Act::Equip { .. }) => Spec::EQUIP,
            Self::Act(Act::Trash { .. }) => Spec::TRASH,
            Self::Window(windows::Act::Press) => Spec::WINDOW_PRESS,
            Self::Window(windows::Act::Pick) => Spec::WINDOW_PICK,
            Self::Window(windows::Act::Text) => Spec::WINDOW_TEXT,
            Self::Answer(_) => Spec::ANSWER_DIALOG,
            Self::Chat => Spec::CHAT,
            Self::Edit(edits::Edit::Rebind) => Spec::REBIND,
            Self::Edit(edits::Edit::Setting) => Spec::SET_SETTING,
            Self::Quit => Spec::QUIT,
        }
    }

    /// Whether a command of this kind waits beside one of `other` from the
    /// same mod: one of each kind, one press of each action, and one
    /// waypoint or ping.
    pub(super) fn waits_beside(self, other: Self) -> bool {
        match (self, other) {
            (Self::Press(bit, ..), Self::Press(other, ..)) => bit != other,
            (Self::Mark(_), Self::Mark(_)) | (Self::Act(_), Self::Act(_)) => false,
            _ => self.raw() != other.raw(),
        }
    }

    /// The longest hold of its kind, which its timeout waits past once its
    /// detour took it, so a move or a press holds its whole hold.
    pub(super) const fn longest_hold(self) -> u64 {
        match self {
            Self::Move(..) => Spec::MAX_HOLD_MS as u64,
            Self::Press(..) => Spec::MAX_PRESS_MS as u64,
            _ => 0,
        }
    }

    /// Whether the player's own input cancels it before the game takes it:
    /// a waypoint, a ping, a crafting or item request, a chat line, a change
    /// on the Mods screen and a quit drive none of the player's input to
    /// give way.
    pub(super) const fn gives_way(self) -> bool {
        !matches!(
            self,
            Self::Mark(_) | Self::Act(_) | Self::Chat | Self::Edit(_) | Self::Quit
        )
    }

    /// Whether it drives the player's input, which only the world's own
    /// screen takes, from the queue to its end.
    pub(super) const fn drives(self) -> bool {
        matches!(self, Self::Look(_) | Self::Move(..) | Self::Press(..))
    }

    /// The change the interface frame applies for it; `None` for a kind a
    /// detour of the game's own code runs through a [`Handoff`], and for a
    /// chat, whose line the frame borrows from [`chat`].
    pub(super) const fn frame_change(self) -> Option<FrameChange<'static>> {
        match self {
            Self::Open(screen) => Some(FrameChange::Open(screen)),
            Self::Close(screen) => Some(FrameChange::Close(screen)),
            Self::Tab(tab) => Some(FrameChange::Tab(tab)),
            Self::Mods(ask) => Some(FrameChange::Mods(ask)),
            Self::Quit => Some(FrameChange::Quit),
            _ => None,
        }
    }

    /// Whether a detour of the game's own code runs it, through a
    /// [`Handoff`], rather than the interface frame.
    pub(super) const fn handed(self) -> bool {
        self.frame_change().is_none() && !matches!(self, Self::Chat)
    }

    /// Whether it joins or leaves play, at the game's own pace once handed:
    /// no session, screen or two-second check ends it then.
    pub(super) const fn plays(self) -> bool {
        matches!(self, Self::Join | Self::Leave | Self::Host(_))
    }

    /// Its phase when queued: a join waits for the server's answer, a leave
    /// starts in play; 0 for kinds without phases.
    pub(super) const fn queued_phase(self) -> u32 {
        match self {
            Self::Join => Status::PHASE_QUEUED,
            Self::Host(_) => Status::PHASE_MENU,
            Self::Leave => Status::PHASE_IN_WORLD,
            _ => 0,
        }
    }
}
