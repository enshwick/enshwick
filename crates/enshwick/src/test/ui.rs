//! The mod's interface: what it draws, its markers, dialogs and the player's input.

use super::Installed;
use super::table;
use crate::abi::Header;

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
        let frames = table::with(Vec::new(), |s| s.frames.clone());
        let mut drawn = Vec::new();
        for (token, (callback, user)) in (1u64..).zip(frames) {
            table::with((), |s| s.frame = Some((token, false, Vec::new())));
            let frame = crate::abi::UiFrame {
                header: Header::of::<crate::abi::UiFrame>(),
                token,
                screen: crate::abi::UiFrame::IN_GAME,
                reserved: 0,
            };
            // SAFETY: a callback the mod registered with its own pointer.
            unsafe {
                callback(
                    &raw const frame,
                    core::ptr::with_exposed_provenance_mut(user),
                );
            };
            if let Some((_, _, list)) = table::with(None, |s| s.frame.take()) {
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
        let huds = table::with(Vec::new(), |s| s.huds.clone());
        let mut drawn = Vec::new();
        for (token, (callback, user)) in (1u64..).zip(huds) {
            table::with((), |s| s.hud = Some((token, Vec::new())));
            let frame = crate::abi::UiFrame {
                header: Header::of::<crate::abi::UiFrame>(),
                token,
                screen: crate::abi::UiFrame::IN_GAME,
                reserved: 0,
            };
            // SAFETY: a callback the mod registered with its own pointer.
            unsafe {
                callback(
                    &raw const frame,
                    core::ptr::with_exposed_provenance_mut(user),
                );
            }
            if let Some((_, list)) = table::with(None, |s| s.hud.take()) {
                drawn.extend(list);
            }
        }
        drawn
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
            s.dialogs.iter().map(|(d, _, _)| d.clone()).collect()
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
        let Some((_, callback, user)) = first else {
            return false;
        };
        let raw = answer.map_or(crate::abi::UiDialog::CLOSED, crate::ui::Response::to_raw);
        // SAFETY: the mod's answer callback with its own pointer, once.
        unsafe { callback(raw, core::ptr::with_exposed_provenance_mut(user)) };
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
