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

/// What the player did on the mod list.
pub(super) enum Listed {
    Nothing,
    /// Opened mod `index`'s settings and key bindings.
    Settings(usize),
    /// Opened the key bindings of a mod the joined server runs, by id and
    /// name.
    Bindings(String, String),
}

/// Paints a mod's plain name, its version and credits dimmed below it, that
/// it is off or waiting in words and why, and its description.
fn about(paint: &Painter<'_>, record: &crate::host::ModRecord) {
    let off = record.off.load(Ordering::Acquire);
    let waiting = !off && record.waiting.load(Ordering::Acquire);
    let name = record.label();
    paint.label(name.as_bytes(), UiWidget::STYLE_H3, true, false);
    let credits = record.meta.credits();
    let meta = metadata(&record.version, &credits);
    paint.label(meta.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
    if let Some((key, style)) = state_text(off, waiting) {
        let line = texts::fill(key, &[("name", &name), ("version", &record.version)]);
        paint.label(line.as_bytes(), style, true, false);
        let why = if off {
            record.off_reason_shown()
        } else {
            Some(texts::text("mods.waiting.why").to_owned())
        };
        if let Some(why) = why {
            paint.label(why.as_bytes(), style, true, false);
        }
    }
    let description = record.description();
    if !description.is_empty() {
        paint.label(description.as_bytes(), UiWidget::STYLE_BODY, true, false);
    }
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

/// Paints the left cell of mod `index`'s row: what it is, then each of its
/// features that is off in red and the extension points it defines and
/// extends.
fn state(loader: &Loader, paint: &Painter<'_>, index: usize, record: &crate::host::ModRecord) {
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
    let (defines, extends) = crate::points::screen(index, loader);
    for (key, points) in [("mods.points", defines), ("mods.extends", extends)] {
        if !points.is_empty() {
            let line = texts::fill(key, &[("points", &points.join(", "))]);
            paint.label(line.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
        }
    }
}

/// A mod's dimmed line under its name: its version, then its credits, in
/// the separator the credits use between their parts.
fn metadata(version: &str, credits: &str) -> String {
    if credits.is_empty() {
        version.to_owned()
    } else {
        format!("{version}; {credits}")
    }
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
    let listing = listing(paint, MODS_PER_PAGE);
    *first = listing.clamp_first(*first, mods.len());
    let shown = listing.shown(*first, mods.len());
    let settings_button = || paint.button_compact(texts::text("mods.settings").as_bytes(), true);
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
        let opened = paint.row(
            || state(loader, paint, index, record),
            || opens && settings_button(),
        );
        if opened {
            done = Listed::Settings(index);
        }
    }
    for (id, name) in super::controls::carried(loader, &controls) {
        paint.separator();
        let opened = paint.row(
            || {
                paint.label(name.as_bytes(), UiWidget::STYLE_H3, true, false);
                paint.label(
                    texts::text("mods.carried").as_bytes(),
                    UiWidget::STYLE_SECONDARY,
                    true,
                    false,
                );
            },
            settings_button,
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

#[cfg(test)]
mod tests {
    use super::{metadata, state_text, store_off};
    use crate::texts;

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
    fn a_mod_s_dimmed_line_holds_its_version_then_its_credits() {
        assert_eq!(metadata("0.1.1", ""), "0.1.1");
        assert_eq!(
            metadata("0.1.1", "by enshwick; MIT"),
            "0.1.1; by enshwick; MIT"
        );
    }
}
