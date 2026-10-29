//! The checks a mod's widget passes before the loader draws it: the order
//! of its window's start and end, and the numbers and options it hands in.

use enshwick::abi::{Status, UiWidget};

/// Whether a widget of `kind` may draw now: `Status::OK`, or why not.
///
/// A window starts only while none is open (`window_open`) and ends only
/// an open one; a sound plays anywhere, and every other widget draws inside
/// the window.
#[must_use]
pub const fn order(kind: u32, window_open: bool) -> Status {
    match kind {
        UiWidget::WINDOW if window_open => Status::REFUSED,
        UiWidget::END if !window_open => Status::REFUSED,
        UiWidget::WINDOW | UiWidget::END | UiWidget::SOUND => Status::OK,
        _ if !window_open => Status::REFUSED,
        UiWidget::LABEL
        | UiWidget::BUTTON
        | UiWidget::TOGGLE
        | UiWidget::SLIDER
        | UiWidget::DROPDOWN
        | UiWidget::PROGRESS => Status::OK,
        _ => Status::MISSING,
    }
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
/// [`enshwick::ui::MAX_OPTIONS`]; `None` when it hands in none (`given`
/// false for a null list).
#[must_use]
pub fn options(count: u32, given: bool) -> Option<usize> {
    let count = usize::try_from(count)
        .unwrap_or(usize::MAX)
        .min(enshwick::ui::MAX_OPTIONS);
    (given && count > 0).then_some(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_starts_once_and_ends_only_when_open() {
        assert_eq!(order(UiWidget::WINDOW, false), Status::OK);
        assert_eq!(order(UiWidget::WINDOW, true), Status::REFUSED);
        assert_eq!(order(UiWidget::END, true), Status::OK);
        assert_eq!(order(UiWidget::END, false), Status::REFUSED);
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
        ] {
            assert_eq!(order(kind, true), Status::OK, "{kind}");
            assert_eq!(order(kind, false), Status::REFUSED, "{kind}");
        }
        assert_eq!(order(UiWidget::SOUND, false), Status::OK);
        assert_eq!(order(UiWidget::SOUND, true), Status::OK);
    }

    #[test]
    fn an_unknown_widget_is_missing_inside_the_window_and_refused_outside() {
        assert_eq!(order(0, true), Status::MISSING);
        assert_eq!(order(u32::MAX, true), Status::MISSING);
        assert_eq!(order(u32::MAX, false), Status::REFUSED);
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
        assert_eq!(options(u32::MAX, true), Some(enshwick::ui::MAX_OPTIONS));
    }
}
