//! How the loader's screens show a long list: all of it inside the game's
//! scroll area, or one page at a time on a build whose scroll area the
//! loader cannot draw.

use core::ops::Range;

/// How a screen shows its lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Listing {
    /// Every item, inside a scroll area.
    Scrolled,
    /// `per_page` items at a time, with buttons for the other pages.
    Paged {
        /// Items a page shows; at least one.
        per_page: usize,
    },
}

/// The page buttons and line a paged list shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Controls {
    /// The page shown, from 1.
    pub page: usize,
    /// How many pages the list has.
    pub pages: usize,
    /// The list goes on after this page.
    pub next: bool,
    /// The list has items before this page.
    pub previous: bool,
}

impl Listing {
    /// How a list shows: inside the screen's scroll area when it has one
    /// this frame (`scrolled`), else `per_page` items at a time.
    #[must_use]
    pub const fn new(scrolled: bool, per_page: usize) -> Self {
        if scrolled {
            Self::Scrolled
        } else {
            Self::Paged { per_page }
        }
    }

    /// The first item of the page showing `first` in a list that now holds
    /// `count` items: the last page when the list shrank below it, and 0
    /// for a scrolled list.
    #[must_use]
    pub fn clamp_first(self, first: usize, count: usize) -> usize {
        match self {
            Self::Scrolled => 0,
            Self::Paged { .. } if first < count => first,
            Self::Paged { per_page } => {
                let per_page = per_page.max(1);
                count.saturating_sub(1) / per_page * per_page
            }
        }
    }

    /// The items of a list of `count` that show from `first`, which
    /// [`Listing::clamp_first`] has kept in the list.
    #[must_use]
    pub fn shown(self, first: usize, count: usize) -> Range<usize> {
        match self {
            Self::Scrolled => 0..count,
            Self::Paged { per_page } => {
                let first = first.min(count);
                first..first.saturating_add(per_page.max(1)).min(count)
            }
        }
    }

    /// The page line and buttons of a list of `count` from `first`; `None`
    /// when the whole list shows at once.
    #[must_use]
    pub fn controls(self, first: usize, count: usize) -> Option<Controls> {
        let Self::Paged { per_page } = self else {
            return None;
        };
        let per_page = per_page.max(1);
        (count > per_page).then(|| Controls {
            page: first / per_page + 1,
            pages: count.div_ceil(per_page),
            next: first.saturating_add(per_page) < count,
            previous: first > 0,
        })
    }

    /// The first item of the next page after `first`.
    #[must_use]
    pub fn next(self, first: usize) -> usize {
        match self {
            Self::Scrolled => 0,
            Self::Paged { per_page } => first.saturating_add(per_page.max(1)),
        }
    }

    /// The first item of the page before `first`.
    #[must_use]
    pub fn previous(self, first: usize) -> usize {
        match self {
            Self::Scrolled => 0,
            Self::Paged { per_page } => first.saturating_sub(per_page.max(1)),
        }
    }
}

/// The height the game lays its interface out in, whatever the window's
/// resolution: the interface scales to it.
pub const SCREEN_HEIGHT: f32 = 1080.0;
/// Room kept free above and below the screen's window.
const MARGIN: f32 = 60.0;
/// The window's own height besides its list: padding above and below, the
/// title, the Back button and the gaps between them.
const CHROME: f32 = 300.0;
/// The room a message above the list takes: two lines.
const MESSAGE: f32 = 70.0;
/// The least height a scroll area keeps, so a few rows always show.
const LEAST: f32 = 240.0;

/// The height of a screen's scroll area, so that the whole window fits on
/// the screen with a message above the list or without one.
#[must_use]
pub fn scroll_height(message: bool) -> f32 {
    let message = if message { MESSAGE } else { 0.0 };
    (SCREEN_HEIGHT - 2.0 * MARGIN - CHROME - message).max(LEAST)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGED: Listing = Listing::Paged { per_page: 8 };

    #[test]
    fn a_page_past_a_shrunk_list_moves_to_its_last_page() {
        assert_eq!(PAGED.clamp_first(8, 20), 8);
        assert_eq!(PAGED.clamp_first(16, 12), 8);
        assert_eq!(PAGED.clamp_first(8, 5), 0);
        assert_eq!(PAGED.clamp_first(8, 8), 0);
        assert_eq!(PAGED.clamp_first(8, 0), 0);
    }

    #[test]
    fn a_scrolled_list_shows_every_item_without_page_buttons() {
        assert_eq!(Listing::Scrolled.clamp_first(8, 20), 0);
        assert_eq!(Listing::Scrolled.shown(0, 20), 0..20);
        assert_eq!(Listing::Scrolled.controls(0, 20), None);
        assert_eq!(Listing::Scrolled.next(0), 0);
    }

    #[test]
    fn a_paged_list_shows_one_page_and_its_buttons() {
        assert_eq!(PAGED.shown(8, 20), 8..16);
        assert_eq!(PAGED.shown(16, 20), 16..20);
        assert_eq!(PAGED.shown(30, 20), 20..20);
        assert_eq!(
            PAGED.controls(8, 20),
            Some(Controls {
                page: 2,
                pages: 3,
                next: true,
                previous: true
            })
        );
        assert_eq!(PAGED.controls(0, 8), None, "one page needs no buttons");
        assert_eq!(PAGED.next(8), 16);
        assert_eq!(PAGED.previous(8), 0);
        assert_eq!(PAGED.previous(3), 0);
    }

    #[test]
    fn a_list_pages_only_without_a_scroll_area() {
        assert_eq!(Listing::new(true, 4), Listing::Scrolled);
        assert_eq!(Listing::new(false, 4), Listing::Paged { per_page: 4 });
    }

    #[test]
    fn paging_forward_and_back_walks_every_page_once() {
        let count = 11;
        let mut first = 0;
        let mut seen = Vec::new();
        while let Some(controls) = PAGED.controls(first, count) {
            seen.push(PAGED.shown(first, count));
            if !controls.next {
                break;
            }
            first = PAGED.next(first);
        }
        assert_eq!(seen, [0..8, 8..11]);
        assert!(!PAGED.controls(first, count).unwrap().next);
        first = PAGED.previous(first);
        assert_eq!(first, 0);
        assert!(!PAGED.controls(first, count).unwrap().previous);
    }

    #[test]
    fn a_page_of_nothing_still_moves() {
        let empty = Listing::Paged { per_page: 0 };
        assert_eq!(empty.shown(0, 3), 0..1);
        assert_eq!(empty.next(0), 1);
        assert_eq!(empty.controls(0, 3).map(|c| c.pages), Some(3));
    }

    #[test]
    fn the_window_fits_the_screen_with_or_without_a_message() {
        for message in [false, true] {
            let extra = if message { MESSAGE } else { 0.0 };
            let window = scroll_height(message) + CHROME + extra;
            assert!(window + 2.0 * MARGIN <= SCREEN_HEIGHT, "{window}");
            assert!(scroll_height(message) >= LEAST);
        }
        assert!(scroll_height(true) < scroll_height(false));
    }
}
