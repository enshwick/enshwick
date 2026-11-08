//! The mod's interface: what it draws, its markers, dialogs and the player's input.

use crate::abi::Header;

use super::table;
use super::{FakeHost, Installed};

/// The prompt and the screen's host the fake's frames hear, as a test set
/// them.
#[derive(Clone, Debug, Default)]
struct Reads {
    title: String,
    verb: u32,
    host: u32,
    host_read: Option<([u8; 16], [i64; 3])>,
}

fn reads() -> Reads {
    table::with(Reads::default(), |s| Reads {
        title: s
            .prompt
            .as_ref()
            .map_or_else(String::new, |(t, _)| t.clone()),
        verb: s.prompt.as_ref().map_or(0, |&(_, v)| v),
        host: s.screen_host,
        host_read: s.screen_host_read.filter(|_| s.screen_host != 0),
    })
}

/// An in-game frame `token` with `open` the screen and `reads` its prompt
/// and host, which it borrows.
fn in_game(token: u64, open: &str, reads: &Reads) -> crate::abi::UiFrame {
    crate::abi::UiFrame {
        header: Header::of::<crate::abi::UiFrame>(),
        token,
        screen: crate::abi::UiFrame::IN_GAME,
        reserved: 0,
        hud_screen: crate::abi::Str::new(open),
        prompt_title: crate::abi::Str::new(&reads.title),
        prompt_verb: reads.verb,
        screen_host: crate::Entity(reads.host),
        screen_host_template: reads.host_read.map_or([0; 16], |(t, _)| t),
        screen_host_position: reads.host_read.map_or([0; 3], |(_, p)| p),
    }
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// Runs the mod's frame callbacks once, in game, and returns what they
    /// drew, in order; inputs given with [`Installed::input`] reach their
    /// widgets in this frame.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Side;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// assert!(loader.draw_frame().is_empty());
    /// ```
    #[must_use]
    pub fn draw_frame(&self) -> Vec<Drawn> {
        let (frames, open) = table::with((Vec::new(), ""), |s| (s.frames.clone(), s.open_screen));
        let mut drawn = Vec::new();
        for (token, handler) in (1u64..).zip(frames) {
            table::with((), |s| {
                s.frame = Some((token, false, Vec::new()));
                s.drawing = handler.owner();
            });
            let reads = reads();
            let frame = in_game(token, open, &reads);
            // SAFETY: a callback the mod registered with its own pointer.
            handler.run(|callback, user| unsafe { callback(&raw const frame, user) });
            let list = table::with(None, |s| {
                s.drawing = None;
                s.frame.take()
            });
            if let Some((_, _, list)) = list {
                drawn.extend(list);
            }
        }
        drawn
    }

    /// Runs the mod's HUD callbacks once and returns what they drew, each
    /// item with its anchor (`UiWidget::TOP_LEFT` and on), in order.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Side;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// assert!(loader.draw_hud().is_empty());
    /// ```
    #[must_use]
    pub fn draw_hud(&self) -> Vec<(u32, Drawn)> {
        let (huds, open) = table::with((Vec::new(), ""), |s| (s.huds.clone(), s.open_screen));
        let mut drawn = Vec::new();
        for (token, handler) in (1u64..).zip(huds) {
            table::with((), |s| {
                s.hud = Some((token, Vec::new()));
                s.drawing = handler.owner();
            });
            let reads = reads();
            let frame = in_game(token, open, &reads);
            // SAFETY: a callback the mod registered with its own pointer.
            handler.run(|callback, user| unsafe { callback(&raw const frame, user) });
            let list = table::with(None, |s| {
                s.drawing = None;
                s.hud.take()
            });
            if let Some((_, list)) = list {
                drawn.extend(list);
            }
        }
        drawn
    }

    /// Runs the callbacks of the mod's extensions of the game screen
    /// `screen` once, as when the player has that screen open, and returns
    /// what each drew, with its edge (`UiExtension::LEFT` and on), edge by
    /// edge and in registration order within an edge. The loader opens each
    /// edge's window for its panels, so a window widget is refused there,
    /// and a panel that drew nothing is left out, as it shows no window.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Side;
    /// use enshwick::ui::HudScreenType;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// assert!(loader.draw_screen(HudScreenType::StorageChest).is_empty());
    /// ```
    #[cfg(feature = "unstable")]
    #[must_use]
    pub fn draw_screen(&self, screen: crate::ui::HudScreenType) -> Vec<(u32, Vec<Drawn>)> {
        let mut extensions = table::with(Vec::new(), |s| s.extensions.clone());
        extensions.sort_by_key(|&(_, edge, ..)| edge);
        let open = screen.name();
        let template = reads().host_read.map(|(t, _)| t);
        let mut drawn = Vec::new();
        for (token, (name, edge, handler, hosts)) in (1u64..).zip(extensions) {
            let for_host = hosts.is_empty() || template.is_some_and(|t| hosts.contains(&t));
            if name != screen.name() || !for_host {
                continue;
            }
            table::with((), |s| {
                s.frame = Some((token, true, Vec::new()));
                s.extending = Some(token);
                s.drawing = handler.owner();
            });
            let reads = reads();
            let frame = in_game(token, open, &reads);
            // SAFETY: a callback the mod registered with its own pointer.
            handler.run(|callback, user| unsafe { callback(&raw const frame, user) });
            let list = table::with(None, |s| {
                s.extending = None;
                s.drawing = None;
                s.frame.take()
            });
            if let Some((_, _, list)) = list.filter(|(_, _, list)| !list.is_empty()) {
                drawn.push((edge, list));
            }
        }
        drawn
    }

    /// Opens the game screen `screen` for the frames and HUDs drawn from
    /// now on, as when the player opens it; `None` closes it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Side;
    /// use enshwick::ui::HudScreenType;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// loader.open_screen(Some(HudScreenType::Map));
    /// assert!(loader.draw_frame().is_empty());
    /// ```
    #[cfg(feature = "unstable")]
    pub fn open_screen(&self, screen: Option<crate::ui::HudScreenType>) {
        table::with((), |s| {
            s.open_screen = screen.map_or("", crate::ui::HudScreenType::name);
        });
    }

    /// Shows the interaction prompt `title` with the verb `LocaTagId`
    /// `verb` in the frames drawn from now on, as when the player looks at
    /// an object; `None` shows none.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Side;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// loader.show_prompt(Some(("Chest", 7)));
    /// assert!(loader.draw_frame().is_empty());
    /// ```
    pub fn show_prompt(&self, prompt: Option<(&str, u32)>) {
        table::with((), |s| {
            s.prompt = prompt.map(|(title, verb)| (title.to_owned(), verb));
        });
    }

    /// Makes `host`, an entity of the client's world, the object the open
    /// screen belongs to in the frames drawn from now on; `None` for none.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::Entity;
    /// use enshwick::abi::Side;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// loader.host_screen(Some(Entity(512)));
    /// assert!(loader.draw_frame().is_empty());
    /// ```
    pub fn host_screen(&self, host: Option<crate::Entity>) {
        table::with((), |s| s.screen_host = host.map_or(0, |e| e.0));
    }

    /// Makes the open screen's host one made from `template`, standing at
    /// `position` in metres, as the client's last step read it, for the
    /// frames and panels drawn from now on while a host is set with
    /// [`Installed::host_screen`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::Entity;
    /// use enshwick::abi::Side;
    /// use enshwick::content::Guid;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// loader.host_screen(Some(Entity(512)));
    /// loader.read_screen_host(Guid::named("you.safes:safe"), [1.0, 2.0, 3.0]);
    /// assert!(loader.draw_frame().is_empty());
    /// ```
    pub fn read_screen_host(&self, template: crate::content::Guid, position: [f64; 3]) {
        let fixed = crate::ecs::fixed(position);
        table::with((), |s| s.screen_host_read = Some((template.0, fixed)));
    }

    /// Names the game action `action` on the fake build, its reflected
    /// name, with a key `bound` to it or none, for the input hints drawn
    /// from now on; an action never named is one the build lacks, which
    /// the loader refuses.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Side;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// loader.game_action("ContextualAction", true);
    /// assert!(loader.draw_frame().is_empty());
    /// ```
    pub fn game_action(&self, action: &str, bound: bool) {
        table::with((), |s| {
            s.game_actions.retain(|(name, _)| name != action);
            s.game_actions.push((action.to_owned(), bound));
        });
    }

    /// Makes `held`, a `Bitmask64` of the game's `PlayerInputType`, the
    /// controls the local player holds from now on, as the client's last
    /// step read them; `None` as outside a game.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Side;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// loader.hold_locally(Some(1 << 30));
    /// ```
    pub fn hold_locally(&self, held: Option<u64>) {
        table::with((), |s| s.local_held = held);
    }

    /// The mod's markers on the map, as it set them last.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.markers().is_empty());
    /// ```
    #[must_use]
    pub fn markers(&self) -> Vec<FakeMarker> {
        table::with(Vec::new(), |s| s.markers.clone())
    }

    /// Gives the widget whose text or label is `text` an input from the
    /// player in the next frame drawn.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::test::Input;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.input("Close", Input::Click);
    /// ```
    pub fn input(&self, text: &str, input: Input) {
        table::with((), |s| s.inputs.push((text.to_owned(), input)));
    }

    /// The dialogs waiting for an answer, oldest first.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.dialogs().is_empty());
    /// ```
    #[must_use]
    pub fn dialogs(&self) -> Vec<ShownDialog> {
        table::with(Vec::new(), |s| {
            s.dialogs.iter().map(|(d, _)| d.clone()).collect()
        })
    }

    /// Answers the oldest waiting dialog with `answer`, `None` for closed
    /// without one; false when none waits.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ui::Response;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(!loader.answer(Some(Response::Ok)));
    /// ```
    pub fn answer(&self, answer: Option<crate::ui::Response>) -> bool {
        let first = table::with(None, |s| {
            (!s.dialogs.is_empty()).then(|| s.dialogs.remove(0))
        });
        let Some((_, answered)) = first else {
            return false;
        };
        let raw = answer.map_or(crate::abi::UiDialog::CLOSED, crate::ui::Response::to_raw);
        // SAFETY: the mod's answer callback with its own pointer, once.
        answered.run(|callback, user| unsafe { callback(raw, user) });
        true
    }
}

/// A widget a frame callback drew, as [`Installed::draw_frame`] reports it.
///
/// # Examples
///
/// ```
/// use enshwick::test::Drawn;
///
/// let drawn = Drawn::Button { text: "Close".into(), enabled: true };
/// assert!(matches!(drawn, Drawn::Button { .. }));
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Drawn {
    /// A window started.
    Window {
        /// Its id.
        id: String,
        /// Its title.
        title: String,
    },
    /// The window ended.
    End,
    /// A label.
    Label {
        /// Its text.
        text: String,
        /// Its style, `UiWidget::STYLE_*`.
        style: u32,
    },
    /// A button.
    Button {
        /// Its text.
        text: String,
        /// Whether it takes clicks.
        enabled: bool,
    },
    /// A toggle row.
    Toggle {
        /// Its label.
        label: String,
        /// Its state after the frame.
        on: bool,
        /// Whether it takes changes.
        read_only: bool,
    },
    /// A slider row.
    Slider {
        /// Its label.
        label: String,
        /// Its value after the frame.
        value: f32,
        /// Its least value.
        min: f32,
        /// Its greatest value.
        max: f32,
        /// Its stops, 0 for continuous.
        steps: u32,
        /// Whether it takes changes.
        read_only: bool,
    },
    /// A dropdown row.
    Dropdown {
        /// Its label.
        label: String,
        /// Its options.
        options: Vec<String>,
        /// Its option after the frame.
        index: usize,
        /// Whether it takes changes.
        read_only: bool,
    },
    /// A progress bar.
    Progress {
        /// How full, from 0 to 1.
        fraction: f32,
    },
    /// A sound, `UiWidget::SOUND_*`.
    Sound(u32),
    /// A sound container played with
    /// [`Frame::play`](crate::ui::Frame::play), by its GUID.
    Play(crate::content::Guid),
    /// The key hint of a game action.
    InputHint {
        /// The action's reflected name.
        action: String,
        /// The label beside the key, empty for none.
        label: String,
        /// Whether a key is bound to the action, so the game drew one.
        bound: bool,
    },
}

/// A marker the mod set on the map, as [`Installed::markers`] reports it.
///
/// # Examples
///
/// See [`Installed::markers`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakeMarker {
    /// Where, in 32.32 fixed-point metres.
    pub position: [i64; 3],
    /// The label.
    pub label: String,
    /// The colour, `UiMarker::WHITE` and on.
    pub color: u32,
    /// The registry key of its marker type; 0 for the loader's choice.
    pub marker_type: u32,
}

/// What the player does to a widget, given with [`Installed::input`].
///
/// # Examples
///
/// See [`Installed::input`].
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Input {
    /// Clicks a button.
    Click,
    /// Switches a toggle row.
    Toggle(bool),
    /// Moves a slider row.
    Value(f32),
    /// Picks a dropdown row's option.
    Pick(usize),
}

/// A dialog the mod queued, as [`Installed::dialogs`] reports it.
///
/// # Examples
///
/// See [`Installed::dialogs`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShownDialog {
    /// The title.
    pub title: String,
    /// The message.
    pub message: String,
    /// The detail.
    pub detail: String,
    /// The buttons, bit `i` for answer `i`.
    pub buttons: u32,
}

impl FakeHost {
    /// The client's fog of war as the loader keeps it: `info` and its
    /// `info.width` × `info.height` pixels, row by row.
    ///
    /// # Panics
    ///
    /// When `pixels` does not hold the image `info` describes.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{FogInfo, Side};
    ///
    /// let info = FogInfo { width: 2, height: 2, metres_per_pixel: 8.0, size_z: 16.0, ..FogInfo::EMPTY };
    /// let loader = enshwick::test::FakeHost::new()
    ///     .side(Side::CLIENT)
    ///     .fog(info, vec![0, 255, 0, 0])
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn fog(mut self, info: crate::abi::FogInfo, pixels: Vec<u8>) -> Self {
        assert_eq!(
            pixels.len(),
            info.width as usize * info.height as usize,
            "the pixels of the image the info describes"
        );
        self.fog = Some((info, pixels));
        self
    }
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// Replaces the fog of war, as the loader does after a reveal.
    ///
    /// # Panics
    ///
    /// As [`FakeHost::fog`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{FogInfo, Side};
    ///
    /// let info = FogInfo { width: 1, height: 1, ..FogInfo::EMPTY };
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// loader.set_fog(info, vec![255]);
    /// ```
    pub fn set_fog(&self, info: crate::abi::FogInfo, pixels: Vec<u8>) {
        assert_eq!(pixels.len(), info.width as usize * info.height as usize);
        table::with((), |s| s.host.fog = Some((info, pixels)));
    }
}
