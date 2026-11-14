//! The checks a mod's widget passes before the loader draws it: the order
//! of its window's start and end, and the numbers and options it hands in.

use enshwick_core::abi::UiWidget;

/// Why a mod's widget was not drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum WidgetError {
    /// A window started while one is open, an end without one, or another
    /// widget outside a window.
    #[error("the widget is out of its window's order")]
    OutOfOrder,
    /// A widget kind the loader has none of.
    #[error("no such widget kind")]
    Unknown,
    /// The interface's engine is not made yet.
    #[error("the interface does not run yet")]
    NoEngine,
    /// Called outside the mod's own frame callback.
    #[error("called outside the mod's frame callback")]
    WrongThread,
    /// A text, number or option the widget does not take.
    #[error("the widget is malformed")]
    Malformed,
    /// A HUD widget of a kind the HUD does not show, or a HUD full.
    #[error("the HUD does not take the widget")]
    HudRefused,
    /// What the widget shows is not there: a sound or a key hint.
    #[error("what the widget shows is not there")]
    Unavailable,
}

/// Whether a widget of `kind` may draw now, or why not.
///
/// A window starts only while none is open (`window_open`) and ends only
/// an open one; a sound plays anywhere, and every other widget draws inside
/// the window.
///
/// # Errors
///
/// [`WidgetError::OutOfOrder`] out of the window's order,
/// [`WidgetError::Unknown`] for a kind the loader has none of.
pub const fn order(kind: u32, window_open: bool) -> Result<(), WidgetError> {
    match kind {
        UiWidget::WINDOW if window_open => Err(WidgetError::OutOfOrder),
        UiWidget::END if !window_open => Err(WidgetError::OutOfOrder),
        UiWidget::WINDOW | UiWidget::END | UiWidget::SOUND | UiWidget::PLAY => Ok(()),
        _ if !window_open => Err(WidgetError::OutOfOrder),
        kind if needs_window(kind) => Ok(()),
        _ => Err(WidgetError::Unknown),
    }
}

/// Whether a widget of `kind` draws inside a window, so a screen
/// extension's first such widget opens its edge's window.
#[must_use]
pub const fn needs_window(kind: u32) -> bool {
    matches!(
        kind,
        UiWidget::LABEL
            | UiWidget::BUTTON
            | UiWidget::TOGGLE
            | UiWidget::SLIDER
            | UiWidget::DROPDOWN
            | UiWidget::PROGRESS
            | UiWidget::INPUT_HINT
    )
}

/// A progress bar's value, kept between `min` and `max`; `None` when a
/// number is not finite or the range is empty.
#[must_use]
pub fn progress(value: f32, min: f32, max: f32) -> Option<f32> {
    let finite = min.is_finite() && max.is_finite() && value.is_finite();
    (finite && max > min).then(|| value.clamp(min, max))
}

/// A slider's range, its ends in order, and its value kept inside it;
/// `None` when a number is not finite.
#[must_use]
pub fn slider(value: f32, min: f32, max: f32) -> Option<((f32, f32), f32)> {
    if !(min.is_finite() && max.is_finite() && value.is_finite()) {
        return None;
    }
    let range = (min.min(max), max.max(min));
    Some((range, value.clamp(range.0, range.1)))
}

/// How many of a dropdown's `count` options show, at most
/// [`enshwick_core::ui::MAX_OPTIONS`]; `None` when it hands in none (`given`
/// false for a null list).
#[must_use]
pub fn options(count: u32, given: bool) -> Option<usize> {
    let count = usize::try_from(count)
        .unwrap_or(usize::MAX)
        .min(enshwick_core::ui::MAX_OPTIONS);
    (given && count > 0).then_some(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_starts_once_and_ends_only_when_open() {
        assert_eq!(order(UiWidget::WINDOW, false), Ok(()));
        assert_eq!(order(UiWidget::WINDOW, true), Err(WidgetError::OutOfOrder));
        assert_eq!(order(UiWidget::END, true), Ok(()));
        assert_eq!(order(UiWidget::END, false), Err(WidgetError::OutOfOrder));
    }

    #[test]
    fn widgets_draw_only_inside_the_window_and_sounds_anywhere() {
        for kind in [
            UiWidget::LABEL,
            UiWidget::BUTTON,
            UiWidget::TOGGLE,
            UiWidget::SLIDER,
            UiWidget::DROPDOWN,
            UiWidget::PROGRESS,
            UiWidget::INPUT_HINT,
        ] {
            assert_eq!(order(kind, true), Ok(()), "{kind}");
            assert_eq!(order(kind, false), Err(WidgetError::OutOfOrder), "{kind}");
        }
        for sound in [UiWidget::SOUND, UiWidget::PLAY] {
            assert_eq!(order(sound, false), Ok(()), "{sound}");
            assert_eq!(order(sound, true), Ok(()), "{sound}");
        }
    }

    #[test]
    fn only_widgets_drawn_inside_a_window_need_one() {
        for kind in [UiWidget::LABEL, UiWidget::BUTTON, UiWidget::PROGRESS] {
            assert!(needs_window(kind), "{kind}");
        }
        for kind in [
            UiWidget::WINDOW,
            UiWidget::END,
            UiWidget::SOUND,
            UiWidget::PLAY,
            u32::MAX,
        ] {
            assert!(!needs_window(kind), "{kind}");
        }
    }

    #[test]
    fn an_unknown_widget_is_missing_inside_the_window_and_refused_outside() {
        assert_eq!(order(0, true), Err(WidgetError::Unknown));
        assert_eq!(order(u32::MAX, true), Err(WidgetError::Unknown));
        assert_eq!(order(u32::MAX, false), Err(WidgetError::OutOfOrder));
    }

    #[test]
    fn a_progress_bar_needs_finite_numbers_and_a_range() {
        assert_eq!(progress(5.0, 0.0, 10.0), Some(5.0));
        assert_eq!(progress(15.0, 0.0, 10.0), Some(10.0));
        assert_eq!(progress(-1.0, 0.0, 10.0), Some(0.0));
        assert_eq!(progress(1.0, 1.0, 1.0), None);
        assert_eq!(progress(1.0, 2.0, 1.0), None);
        assert_eq!(progress(f32::NAN, 0.0, 1.0), None);
        assert_eq!(progress(0.5, 0.0, f32::INFINITY), None);
    }

    #[test]
    fn a_slider_orders_its_range_and_keeps_its_value_inside() {
        assert_eq!(slider(5.0, 0.0, 10.0), Some(((0.0, 10.0), 5.0)));
        assert_eq!(slider(15.0, 10.0, 0.0), Some(((0.0, 10.0), 10.0)));
        assert_eq!(slider(3.0, 2.0, 2.0), Some(((2.0, 2.0), 2.0)));
        assert_eq!(slider(f32::NAN, 0.0, 1.0), None);
        assert_eq!(slider(0.0, f32::NEG_INFINITY, 1.0), None);
    }

    #[test]
    fn a_dropdown_shows_its_options_up_to_the_most_a_row_holds() {
        assert_eq!(options(3, true), Some(3));
        assert_eq!(options(0, true), None);
        assert_eq!(options(3, false), None);
        assert_eq!(
            options(u32::MAX, true),
            Some(enshwick_core::ui::MAX_OPTIONS)
        );
    }
}
