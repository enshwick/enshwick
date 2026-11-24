use super::popup::{NodeMap, PopupRoot};
use super::{DialogLayout, MenuLayout, Missing, hint, pass, rows, scroll, text_field};
use crate::engine::fuse::Fuse;
use crate::engine::interface_parts::Part;
use crate::engine::loader::Loader;

/// The Mods screen's lists scroll; off, they page.
static SCROLLING_FUSE: Fuse = Fuse::new("scrolling on the Mods screen (lists page)");
/// The Mods screen edits text settings; off, they are read-only.
static TEXT_FIELDS_FUSE: Fuse = Fuse::new("text settings on the Mods screen (read-only)");
static HINTS_FUSE: Fuse = Fuse::new("input hints");
/// The game's map key opens the map under a carried window with a map; off,
/// only the window's button does.
static MAP_KEYS_FUSE: Fuse = Fuse::new("the map key under carried windows (their button opens it)");
/// The Mods screen lays settings out in rows; off, they stack.
static ROWS_FUSE: Fuse = Fuse::new("rows on the Mods screen (stacked)");
/// Back closes the loader's windows, screens and dialogs; off, their buttons
/// do.
static BACK_FUSE: Fuse = Fuse::new("Back on the loader's windows (their buttons close them)");
/// Back closes a loader dialog; off, its buttons do.
pub(in crate::ui::interface) static DIALOG_BACK_FUSE: Fuse =
    Fuse::new("Back on dialogs (their buttons close them)");

/// The interface's optional parts read from the build's table and
/// reflection, each `None` when it turned off alone.
pub(super) struct Optional {
    pub(super) menu: Option<MenuLayout>,
    pub(super) dialog: Option<DialogLayout>,
    pub(super) nodes: Option<NodeMap>,
    pub(super) popup_root: Option<PopupRoot>,
    pub(super) scroll: Option<scroll::Scrolling>,
    pub(super) text_fields: Option<text_field::TextFields>,
    pub(super) hint: Option<hint::InputHints>,
    pub(super) map_keys: Option<pass::MapKeys>,
    pub(super) rows: Option<rows::Rows>,
}

impl Optional {
    /// Reads each optional layout, adding the menus' entries to `parts_off`
    /// when theirs does not read, then turns off each of `parts_off` and
    /// each other layout that does not read, with its reason.
    pub(super) fn resolve(loader: &Loader, parts_off: &mut Vec<(Part, Missing)>) -> Self {
        let lookup = |id: &str| loader.build.constant(id);
        let menu = MenuLayout::read(&lookup);
        if let Err(why) = menu {
            for part in [Part::MAIN_MENU, Part::PAUSE_MENU] {
                if !parts_off.iter().any(|(p, _)| *p == part) {
                    parts_off.push((part, why));
                }
            }
        }
        let off = |fuse: &'static Fuse| {
            move |why: &Missing| {
                loader.fuse_off(fuse, why.id(), &why.reason());
            }
        };
        for (part, fuse) in Part::ALL.iter().zip(super::super::PART_FUSES) {
            if let Some((_, why)) = parts_off.iter().find(|(off, _)| off == part) {
                off(fuse)(why);
            }
        }
        let dialog = DialogLayout::read(&lookup)
            .inspect_err(off(&super::super::DIALOGS_FUSE))
            .ok();
        // Back is queried through the menus' actions, there while one menu is.
        let menus_off = [Part::MAIN_MENU, Part::PAUSE_MENU].map(|part| {
            parts_off
                .iter()
                .find(|(off, _)| *off == part)
                .map(|(_, why)| *why)
        });
        let actions = match menus_off {
            [Some(why), Some(_)] => Err(why),
            _ => Ok(()),
        };
        let nodes = actions
            .and_then(|()| NodeMap::resolve(loader))
            .inspect_err(off(&BACK_FUSE))
            .ok();
        Self {
            menu: menu.ok(),
            nodes,
            popup_root: dialog.zip(nodes).and_then(|_| {
                PopupRoot::resolve(loader)
                    .inspect_err(off(&DIALOG_BACK_FUSE))
                    .ok()
            }),
            dialog,
            scroll: scroll::Scrolling::resolve(loader)
                .inspect_err(off(&SCROLLING_FUSE))
                .ok(),
            text_fields: text_field::TextFields::resolve(loader)
                .inspect_err(off(&TEXT_FIELDS_FUSE))
                .ok(),
            hint: hint::InputHints::resolve(loader)
                .inspect_err(off(&HINTS_FUSE))
                .ok(),
            map_keys: pass::MapKeys::resolve(loader)
                .inspect_err(off(&MAP_KEYS_FUSE))
                .ok(),
            rows: rows::Rows::resolve(loader)
                .inspect_err(off(&ROWS_FUSE))
                .ok(),
        }
    }
}
