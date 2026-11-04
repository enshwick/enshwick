//! The list of mods: what each is and what is off, the loader's own notes, and paging.

use std::sync::atomic::Ordering;

use enshwick::abi::UiWidget;

use super::MODS_PER_PAGE;
use crate::host::Loader;
use crate::listing::Listing;
use crate::locks::Relock;
use crate::mods_screen;
use crate::texts;
use crate::ui::{Painter, actions};

/// What the player did on the mod list.
pub(super) enum Listed {
    Nothing,
    /// Opened mod `index`'s settings.
    Settings(usize),
    /// Opened the actions of the mod with this id, or of every mod.
    Actions(Option<String>),
    /// Opened the controls page.
    Controls,
}

/// Paints a mod's heading, why it is off if it is, its description and
/// credits.
fn about(paint: &Painter<'_>, record: &crate::host::ModRecord) {
    let off = record.off.load(Ordering::Acquire);
    let waiting = !off && record.waiting.load(Ordering::Acquire);
    let heading = if off || waiting {
        let name = record.label();
        let key = if off { "mods.off" } else { "mods.waiting" };
        texts::fill(key, &[("name", &name), ("version", &record.version)])
    } else {
        format!("{} {}", record.label(), record.version)
    };
    paint.label(heading.as_bytes(), UiWidget::STYLE_H3, false, false);
    if let Some(reason) = record.off_reason_shown() {
        paint.label(reason.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
    } else if waiting {
        let why = texts::text("mods.waiting.why");
        paint.label(why.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
    }
    let description = record.description();
    if !description.is_empty() {
        paint.label(description.as_bytes(), UiWidget::STYLE_BODY, true, false);
    }
    let credits = record.meta.credits();
    if !credits.is_empty() {
        paint.label(credits.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
    }
}

/// What the loader itself has on, off or wrong, above the mods: developer
/// tools on, with the interface demo's button where this game has them,
/// features this build lacks, parts a panic turned off, a broken settings
/// file and discovery off.
fn loader_notes(loader: &Loader, paint: &Painter<'_>) {
    if loader.developer_tools {
        paint.label(
            crate::developer_tools::warning().as_bytes(),
            UiWidget::STYLE_NEGATIVE,
            true,
            false,
        );
    }
    if loader.developer_tools && paint.button(texts::text("mods.demo").as_bytes(), true) {
        crate::ui::open_demo(None);
    }
    let lacking = loader.build.lacking(crate::TABLE_IDS);
    if !lacking.is_empty() {
        let line = texts::fill("mods.features.off", &[("features", lacking)]);
        paint.label(line.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
    }
    parts_off(paint);
    for (shown, key) in [
        (!loader.settings_problems.is_empty(), "mods.settings.broken"),
        (!crate::lobby::discovery_on(), "mods.discovery.off"),
    ] {
        if shown {
            paint.label(
                texts::text(key).as_bytes(),
                UiWidget::STYLE_SECONDARY,
                true,
                false,
            );
        }
    }
}

fn parts_off(paint: &Painter<'_>) {
    if !crate::diag::any_part_off() {
        return;
    }
    for part in crate::diag::parts_off() {
        let line = texts::fill(
            "mods.part.off",
            &[("part", &part.part), ("reason", &part.reason)],
        );
        paint.label(line.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
    }
}

/// Draws a page of the mod list; what the player did.
pub(super) fn list(loader: &Loader, paint: &Painter<'_>, first: &mut usize) -> Listed {
    let mods = &loader.mods;
    let mut done = Listed::Nothing;
    if actions::any_offered() && paint.button(texts::text("actions").as_bytes(), true) {
        done = Listed::Actions(None);
    }
    if super::controls::any() && paint.button(texts::text("controls").as_bytes(), true) {
        done = Listed::Controls;
    }
    loader_notes(loader, paint);
    let world = crate::worlds::running();
    let away = mods.iter().any(|m| m.server_values.relock().is_some());
    if !world && !away && mods.iter().any(|m| !m.settings_files.relock().is_empty()) {
        paint.label(
            texts::text("mods.settings.in-world").as_bytes(),
            UiWidget::STYLE_SECONDARY,
            true,
            false,
        );
    }
    let listing = listing(paint, MODS_PER_PAGE);
    *first = listing.clamp_first(*first, mods.len());
    let shown = listing.shown(*first, mods.len());
    for (index, record) in mods.iter().enumerate().take(shown.end).skip(shown.start) {
        about(paint, record);
        let features = record.features.relock().clone();
        let features = crate::features::shown(&features, record.texts.relock().as_ref());
        for feature in features {
            if let Some(reason) = &feature.off {
                let line = texts::fill(
                    "mods.feature.off",
                    &[("feature", &feature.name), ("reason", reason)],
                );
                paint.label(line.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
            }
        }
        let (defines, extends) = crate::points::screen(index, loader);
        for (key, points) in [("mods.points", defines), ("mods.extends", extends)] {
            if !points.is_empty() {
                let line = texts::fill(key, &[("points", &points.join(", "))]);
                paint.label(line.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
            }
        }
        if actions::offers(&record.id) && paint.button(texts::text("actions").as_bytes(), true) {
            done = Listed::Actions(Some(record.id.clone()));
        }
        let settings = record
            .settings_schema()
            .is_some_and(|s| !s.fields.is_empty());
        let sent = record.server_values.relock().is_some();
        if settings
            && mods_screen::gear(world, sent)
            && paint.button(texts::text("mods.settings").as_bytes(), true)
        {
            done = Listed::Settings(index);
        }
    }
    let beside = crate::others::loaded(&loader.others);
    if !beside.is_empty() {
        let line = texts::fill("mods.others.beside", &[("files", &beside.join(", "))]);
        paint.label(line.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
    }
    pages(paint, listing, first, mods.len());
    done
}

pub(super) fn listing(paint: &Painter<'_>, per_page: usize) -> Listing {
    Listing::new(paint.scrolled(), per_page)
}

/// The page line and the Previous and Next buttons of a list of `count`
/// items, when it pages.
pub(super) fn pages(paint: &Painter<'_>, listing: Listing, first: &mut usize, count: usize) {
    let Some(controls) = listing.controls(*first, count) else {
        return;
    };
    let (page, pages) = (controls.page.to_string(), controls.pages.to_string());
    let page = texts::fill("mods.page", &[("page", &page), ("pages", &pages)]);
    paint.label(page.as_bytes(), UiWidget::STYLE_SECONDARY, false, true);
    if controls.next && paint.button(texts::text("mods.page.next").as_bytes(), true) {
        *first = listing.next(*first);
    }
    if controls.previous && paint.button(texts::text("mods.page.previous").as_bytes(), true) {
        *first = listing.previous(*first);
    }
}
