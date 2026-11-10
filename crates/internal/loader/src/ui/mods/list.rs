//! The list of mods: what each is and what is off, the loader's own notes, and paging.

use std::sync::atomic::Ordering;

use enshwick::abi::UiWidget;

use super::MODS_PER_PAGE;
use crate::host::Loader;
use crate::listing::Listing;
use crate::locks::Relock;
use crate::mods_screen;
use crate::texts;
use crate::ui::Painter;
use crate::ui::engine::Icon;

/// What the player did on the mod list.
pub(super) enum Listed {
    Nothing,
    /// Opened mod `index`'s settings and key bindings with its gear, which
    /// for a mod with nothing to set here goes straight to the joined
    /// server's settings.
    Settings(usize),
    /// Opened mod `index`'s own page with its Details button.
    Details(usize),
    /// Opened the key bindings of a mod the joined server runs, by id and
    /// name.
    Bindings(String, String),
}

/// What a click on mod `index`'s Details button or gear opens: the Details
/// button the mod's own page, the gear its settings.
const fn clicked(details: bool, gear: bool, index: usize) -> Option<Listed> {
    if gear {
        Some(Listed::Settings(index))
    } else if details {
        Some(Listed::Details(index))
    } else {
        None
    }
}

/// Paints a mod's plain name, its version and authors dimmed below it, that
/// it is off or waiting in words and why, and its description.
fn about(paint: &Painter<'_>, record: &crate::host::ModRecord) {
    let off = record.off.load(Ordering::Acquire);
    let waiting = !off && record.waiting.load(Ordering::Acquire);
    let name = record.label();
    let why = if off {
        record.off_reason_shown()
    } else {
        waiting.then(|| texts::text("mods.waiting.why").to_owned())
    };
    heading(
        paint,
        &name,
        &metadata(&record.version, &record.meta.authors),
        state_text(off, waiting).map(|state| (state, record.version.as_str(), why)),
    );
    let description = record.description();
    if !description.is_empty() {
        paint.label(description.as_bytes(), UiWidget::STYLE_BODY, true, false);
    }
}

/// The top of a mod's row: its name, the dimmed line under it and, for a
/// mod off or waiting, that state in words with its version, then why.
fn heading(
    paint: &Painter<'_>,
    name: &str,
    dimmed: &str,
    state: Option<((&str, u32), &str, Option<String>)>,
) {
    paint.name(name.as_bytes());
    paint.label(dimmed.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
    if let Some(((key, style), version, why)) = state {
        let line = texts::fill(key, &[("name", name), ("version", version)]);
        paint.label(line.as_bytes(), style, true, false);
        if let Some(why) = why {
            paint.label(why.as_bytes(), style, true, false);
        }
    }
}

/// Paints a mod the loader skipped at start, as off with its reason; its
/// dimmed line names its file, so a player finds the copy the reason means.
fn skipped_row(paint: &Painter<'_>, skipped: &crate::resolve::Skipped) {
    heading(
        paint,
        &skipped.label,
        &metadata(&skipped.version, core::slice::from_ref(&skipped.file)),
        state_text(true, false).map(|state| {
            (
                state,
                skipped.version.as_str(),
                Some(skipped.reason.shown()),
            )
        }),
    );
}

/// The text that says in words that a mod is off or waiting, with its
/// style, so the state never reads from the colour alone; `None` for a mod
/// that runs.
const fn state_text(off: bool, waiting: bool) -> Option<(&'static str, u32)> {
    if off {
        Some(("mods.off", UiWidget::STYLE_NEGATIVE))
    } else if waiting {
        Some(("mods.waiting", UiWidget::STYLE_WARNING))
    } else {
        None
    }
}

/// Paints the left cell of a mod's row: what it is, then each of its
/// features that is off in red.
fn state(paint: &Painter<'_>, record: &crate::host::ModRecord) {
    about(paint, record);
    let features = record.features.relock().clone();
    let features = crate::features::shown(&features, record.texts.relock().as_ref());
    for feature in features {
        if let Some(reason) = &feature.off {
            let line = store_off(&record.label(), &feature.name, reason).unwrap_or_else(|| {
                texts::fill(
                    "mods.feature.off",
                    &[("feature", &feature.name), ("reason", reason)],
                )
            });
            paint.label(line.as_bytes(), UiWidget::STYLE_NEGATIVE, true, false);
        }
    }
}

/// Mod `index`'s technical details, dimmed lines for the top of its page:
/// its licence, its homepage and the extension points it defines and
/// extends.
fn detail_lines(loader: &Loader, index: usize, record: &crate::host::ModRecord) -> Vec<String> {
    let meta = &record.meta;
    let (defines, extends) = crate::points::screen(index, loader);
    [
        ("mods.license", "license", meta.license.clone()),
        ("mods.homepage", "address", meta.homepage.clone()),
        ("mods.points", "points", defines.join(", ")),
        ("mods.extends", "points", extends.join(", ")),
    ]
    .into_iter()
    .filter(|(_, _, value)| !value.is_empty())
    .map(|(key, name, value)| texts::fill(key, &[(name, &value)]))
    .collect()
}

/// Draws mod `index`'s version and authors, then its technical details, at
/// the top of its page.
pub(super) fn details(loader: &Loader, paint: &Painter<'_>, index: usize) {
    let Some(record) = loader.mods.get(index) else {
        return;
    };
    let credits = metadata(&record.version, &record.meta.authors);
    for line in core::iter::once(credits).chain(detail_lines(loader, index, record)) {
        paint.label(line.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
    }
}

/// A mod's dimmed line under its name: its version, then its authors.
fn metadata(version: &str, authors: &[String]) -> String {
    core::iter::once(version)
        .chain(authors.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(texts::text("mods.metadata.gap"))
}

/// The name the API gives the feature of a world store, before the store's
/// own name.
const STORE_FEATURE: &str = "world store ";
/// What the API's store error puts between a file and the parser's words;
/// the API's own tests hold the wording.
const STORE_UNPARSED: &str = " does not parse: ";

/// The line for mod `label`'s world store that is off, in the reader's
/// language: the mod and the store, then the file, the parser's words last.
/// `None` for a feature that is no world store.
fn store_off(label: &str, feature: &str, reason: &str) -> Option<String> {
    let store = feature.strip_prefix(STORE_FEATURE)?;
    let named = [("mod", label), ("store", store)];
    Some(match reason.split_once(STORE_UNPARSED) {
        Some((path, detail)) => texts::fill(
            "mods.store.unparsed",
            &[named[0], named[1], ("path", path), ("detail", detail)],
        ),
        None => texts::fill("mods.store.off", &[named[0], named[1], ("reason", reason)]),
    })
}

/// What the loader itself has on, off or wrong, above the mods: developer
/// tools on, with the interface demo's button where this game has them,
/// features this build lacks, parts a panic turned off, a newer release, a
/// broken settings file and discovery off.
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
        paint.label(line.as_bytes(), UiWidget::STYLE_NEGATIVE, true, false);
    }
    parts_off(paint);
    if let Some(release) = loader.newer_release.get() {
        let line = texts::fill(
            "mods.newer",
            &[
                ("version", &release.version().to_string()),
                ("address", &release.page()),
            ],
        );
        paint.label(line.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
    }
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
        paint.label(line.as_bytes(), UiWidget::STYLE_NEGATIVE, true, false);
    }
}

/// Draws a page of the mod list; what the player did.
pub(super) fn list(loader: &Loader, paint: &Painter<'_>, first: &mut usize) -> Listed {
    let mods = &loader.mods;
    let mut done = Listed::Nothing;
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
    let count = mods.len() + loader.skipped.len();
    let listing = listing(paint, MODS_PER_PAGE);
    *first = listing.clamp_first(*first, count);
    let shown = listing.shown(*first, count);
    let button = |icon: Icon| {
        let fallback = match icon {
            Icon::Settings => "mods.settings",
            Icon::Info => "mods.details",
        };
        paint.icon_button(icon, texts::text(fallback).as_bytes(), true)
    };
    let controls = crate::controls::rows();
    for (index, record) in mods.iter().enumerate().take(shown.end).skip(shown.start) {
        if index > shown.start {
            paint.separator();
        }
        let settings = record
            .settings_schema()
            .is_some_and(|s| !s.fields.is_empty());
        let sent = record.server_values.relock().is_some();
        let opens = settings && mods_screen::gear(world, sent)
            || super::controls::any_in(&controls, &record.id);
        // The Details button on every mod, at the same place; the gear under
        // it where the mod has something to set here, else its place kept.
        let clicked = paint.row(
            || state(paint, record),
            || {
                let details = button(Icon::Info);
                let settings = opens && button(Icon::Settings);
                if !opens {
                    paint.icon_slot();
                }
                clicked(details, settings, index)
            },
        );
        if let Some(clicked) = clicked {
            done = clicked;
        }
    }
    for (index, skipped) in (mods.len()..).zip(&loader.skipped) {
        if shown.contains(&index) {
            if index > shown.start {
                paint.separator();
            }
            skipped_row(paint, skipped);
        }
    }
    for (id, name) in super::controls::carried(loader, &controls) {
        paint.separator();
        let opened = paint.row(
            || {
                paint.name(name.as_bytes());
                paint.label(
                    texts::text("mods.carried").as_bytes(),
                    UiWidget::STYLE_SECONDARY,
                    true,
                    false,
                );
            },
            || button(Icon::Settings),
        );
        if opened {
            done = Listed::Bindings(id, name);
        }
    }
    let beside = crate::others::loaded(&loader.others);
    if !beside.is_empty() {
        let line = texts::fill("mods.others.beside", &[("files", &beside.join(", "))]);
        paint.label(line.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
    }
    pages(paint, listing, first, count);
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

#[cfg(test)]
mod tests {
    use super::{Listed, clicked, metadata, state_text, store_off};
    use crate::texts;

    #[test]
    fn the_details_button_opens_the_mod_s_page_and_only_the_gear_its_settings() {
        assert!(matches!(clicked(true, false, 3), Some(Listed::Details(3))));
        assert!(matches!(clicked(false, true, 3), Some(Listed::Settings(3))));
        assert!(clicked(false, false, 3).is_none());
    }

    #[test]
    fn a_mod_that_is_off_or_waiting_says_so_in_words_not_by_colour_alone() {
        let line = |off, waiting| {
            state_text(off, waiting)
                .map(|(key, _)| texts::fill(key, &[("name", "Safes"), ("version", "0.1.1")]))
        };
        assert_eq!(line(true, false).as_deref(), Some("Safes 0.1.1 (off)"));
        assert_eq!(line(true, true).as_deref(), Some("Safes 0.1.1 (off)"));
        assert_eq!(line(false, true).as_deref(), Some("Safes 0.1.1 (waiting)"));
        assert_eq!(line(false, false), None, "a mod that runs");
    }

    #[test]
    fn a_world_store_that_is_off_reads_in_the_player_s_language_with_its_file() {
        let reason = r"C:\saves\w-mod-data\you.safes\owners.toml does not parse: expected a string";
        let uk = Some(enshwick::i18n::Language::UK_UA);
        let line = texts::for_reader(uk, || {
            store_off("Personal Safes", "world store owners", reason)
        })
        .unwrap();
        assert_eq!(
            line,
            "Personal Safes: збережені дані «owners» вимкнено в цьому світі. Файл не читається: \
             C:\\saves\\w-mod-data\\you.safes\\owners.toml. expected a string"
        );
        let unread = store_off(
            "Safes",
            "world store owners",
            "cannot read or write x: denied",
        );
        assert!(
            unread.is_some_and(|line| line.ends_with(": cannot read or write x: denied")),
            "a reason with no parser's words follows whole"
        );
        assert_eq!(store_off("Safes", "glow", reason), None, "no world store");
    }

    #[test]
    fn a_mod_s_dimmed_line_holds_its_version_then_its_authors_and_nothing_else() {
        assert_eq!(metadata("0.1.1", &[]), "0.1.1");
        let authors = ["themaiby".to_owned(), "enshwick".to_owned()];
        assert_eq!(metadata("0.2.0", &authors), "0.2.0 · themaiby · enshwick");
    }
}
