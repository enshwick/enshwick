//! A stand-in for the game's widgets in tests: it notes each widget a
//! window draws and answers the presses, picks and typing a test sets.

use core::cell::RefCell;

use super::Widgets;
use super::engine::{TextBuffer, Typed};

/// A widget a window drew, its texts as strings.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Drew {
    Label(String),
    Button {
        text: String,
        enabled: bool,
    },
    Toggle {
        label: String,
        on: bool,
        read_only: bool,
    },
    Slider {
        label: String,
        value: f32,
    },
    Dropdown {
        label: String,
        options: Vec<String>,
        index: u32,
    },
    TextField {
        hint: String,
    },
}

/// What the player does to the widget a test names.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Answer {
    /// Presses the button.
    Press,
    /// Flips the switch to this state.
    Flip(bool),
    /// Moves the slider to this value.
    Slide(f32),
    /// Picks this row of the dropdown.
    Pick(u32),
    /// Types this text into the field.
    Type(String),
}

/// Draws nothing: notes each widget, and answers the widget whose text,
/// label or hint a test named with [`Recorder::answer`].
#[derive(Debug, Default)]
pub(super) struct Recorder {
    drew: RefCell<Vec<Drew>>,
    answers: Vec<(String, Answer)>,
}

impl Recorder {
    /// The widget named `name` gets `answer` each time it is drawn.
    pub(super) fn answer(mut self, name: &str, answer: Answer) -> Self {
        self.answers.push((name.to_owned(), answer));
        self
    }

    /// The widgets drawn so far, in order.
    pub(super) fn drew(&self) -> Vec<Drew> {
        self.drew.borrow().clone()
    }

    fn answer_of(&self, name: &str) -> Option<&Answer> {
        self.answers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, answer)| answer)
    }

    fn note(&self, drew: Drew) {
        self.drew.borrow_mut().push(drew);
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

impl Widgets for Recorder {
    fn label(&self, text_bytes: &[u8], _style: u32, _wrap: bool, _center: bool) {
        self.note(Drew::Label(text(text_bytes)));
    }

    fn button(&self, text_bytes: &[u8], enabled: bool) -> bool {
        let text = text(text_bytes);
        let pressed = enabled && self.answer_of(&text) == Some(&Answer::Press);
        self.note(Drew::Button { text, enabled });
        pressed
    }

    fn toggle(&self, label: &[u8], on: bool, read_only: bool) -> Option<bool> {
        let label = text(label);
        let flipped = match self.answer_of(&label) {
            Some(Answer::Flip(now)) if !read_only => Some(*now),
            _ => None,
        };
        self.note(Drew::Toggle {
            label,
            on,
            read_only,
        });
        flipped
    }

    fn slider(
        &self,
        label: &[u8],
        value: f32,
        _range: (f32, f32),
        _steps: u32,
        read_only: bool,
    ) -> Option<f32> {
        let label = text(label);
        let slid = match self.answer_of(&label) {
            Some(Answer::Slide(now)) if !read_only => Some(*now),
            _ => None,
        };
        self.note(Drew::Slider { label, value });
        slid
    }

    fn dropdown(
        &self,
        label: &[u8],
        options: &[&[u8]],
        index: u32,
        read_only: bool,
    ) -> Option<u32> {
        let label = text(label);
        let picked = match self.answer_of(&label) {
            Some(Answer::Pick(row)) if !read_only => Some(*row),
            _ => None,
        };
        self.note(Drew::Dropdown {
            label,
            options: options.iter().map(|option| text(option)).collect(),
            index,
        });
        picked
    }

    fn text_field(&self, buffer: &mut TextBuffer, hint: &[u8], _ending: bool) -> Option<Typed> {
        let hint = text(hint);
        if let Some(Answer::Type(typed)) = self.answer_of(&hint) {
            buffer.set(typed);
        }
        self.note(Drew::TextField { hint });
        Some(Typed {
            entered: false,
            editing: false,
        })
    }
}
