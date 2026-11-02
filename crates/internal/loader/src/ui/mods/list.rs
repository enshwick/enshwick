//! The list of mods: what each is and what is off, the loader's own notes, and paging.

use std::sync::atomic::Ordering;

use enshwick::abi::UiWidget;
use enshwick::config::Scope;

use super::settings::client_schema;
use super::{JUMP, JUMP_SHOWN, MODS_PER_PAGE};
use crate::host::Loader;
use crate::listing::Listing;
use crate::locks::Relock;
use crate::others::Origin;
use crate::texts;
use crate::ui::{Painter, actions};

/// What the player did on the mod list.
pub(super) enum Listed {
    Nothing,
    /// Opened mod `index`'s settings.
    Settings(usize),
    /// Opened the actions of the mod with this id, or of every mod.
    Actions(Option<String>),
}

/// Paints a mod's heading, why it is off if it is, its description and
/// credits.
fn about(paint: &Painter<'_>, record: &crate::host::ModRecord) {
    let heading = if record.off.load(Ordering::Acquire) {
        let name = record.label();
        texts::fill("mods.off", &[("name", &name), ("version", &record.version)])
    } else {
        format!("{} {}", record.label(), record.version)
    };
    paint.label(heading.as_bytes(), UiWidget::STYLE_H3, false, false);
    if let Some(reason) = record.off_reason_shown() {
        paint.label(reason.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
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
/// tools on, features this build lacks, parts a panic turned off, a broken
/// settings file and discovery off.
fn loader_notes(loader: &Loader, paint: &Painter<'_>) {
    if crate::developer_tools::warned(loader) {
        paint.label(
            crate::developer_tools::warning().as_bytes(),
            UiWidget::STYLE_NEGATIVE,
            true,
            false,
        );
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
    if crate::world::hosting() && JUMP_SHOWN.load(Ordering::Relaxed) != 0 {
        paint.label(
            texts::text("mods.world.here").as_bytes(),
            UiWidget::STYLE_SECONDARY,
            true,
            false,
        );
        if paint.button(texts::text("mods.world.open").as_bytes(), true) {
            JUMP.store(1, Ordering::Relaxed);
        }
    } else {
        paint.label(
            texts::text("mods.world.elsewhere").as_bytes(),
            UiWidget::STYLE_SECONDARY,
            true,
            false,
        );
    }
    loader_notes(loader, paint);
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
        let world = record
            .schema
            .relock()
            .as_ref()
            .is_some_and(|s| s.fields.iter().any(|f| f.scope == Scope::World));
        if client_schema(record).is_some() {
            if paint.button(texts::text("mods.settings.yours").as_bytes(), true) {
                done = Listed::Settings(index);
            }
        } else if !world {
            let note = texts::fill("mods.settings.file", &[("id", &record.id)]);
            paint.label(note.as_bytes(), UiWidget::STYLE_SECONDARY, false, false);
        }
    }
    for (origin, loaded, key) in [
        (Origin::Mods, true, "mods.others.native"),
        (Origin::Chain, true, "mods.others.chain"),
        (Origin::Game, true, "mods.others.beside"),
        (Origin::Chain, false, "mods.others.chain-failed"),
    ] {
        let files = crate::others::files(&loader.others, origin, loaded);
        if !files.is_empty() {
            let line = texts::fill(key, &[("files", &files.join(", "))]);
            paint.label(line.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
        }
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
