use std::sync::atomic::Ordering;

use enshwick::abi::{Registers, UiWidget};
use enshwick::config::{Scope, Stage};

use crate::configure::rows;
use crate::configure::rows::{Lock, Row};
use crate::host::Loader;
use crate::locks::Relock;
use crate::texts;
use crate::ui::{ENGINE, Painter};

use super::{FRAME, LAST_TAB, WORLD, draw_row, joined_elsewhere, server_table};

/// The mods' rows in the world settings, kept while the view shows.
pub(super) struct WorldRows {
    /// The frame they last showed in.
    pub(super) frame: u64,
    /// Each mod with settings: index, label, schema and rows.
    pub(super) mods: Vec<(usize, String, enshwick::config::Schema, Vec<Row>)>,
    /// The last change's outcome.
    pub(super) message: Option<(String, bool)>,
}

/// Every mod's world rows: its settings but each player's own.
pub(super) fn world_rows(
    loader: &Loader,
) -> Vec<(usize, String, enshwick::config::Schema, Vec<Row>)> {
    loader
        .mods
        .iter()
        .enumerate()
        .filter_map(|(index, record)| {
            let mut schema = record.schema.relock().clone()?;
            schema.fields.retain(|f| f.scope == Scope::World);
            if schema.fields.is_empty() {
                return None;
            }
            // Joined to another's game: its join values, shown, not changed.
            let joined = joined_elsewhere();
            let server = record
                .server_synced
                .relock()
                .clone()
                .and_then(|values| server_table(&values));
            if joined {
                schema.fields.retain(|f| f.stage == Stage::Sync);
                if schema.fields.is_empty() {
                    return None;
                }
            }
            let file = crate::worlds::show_text(loader, record);
            let texts = record.texts.relock().clone();
            let words = texts.as_ref().map(|texts| rows::Words {
                texts,
                language: texts::language(),
            });
            let mut rows = rows::rows(&schema, &file, server.as_ref().filter(|_| joined), words);
            if joined {
                for row in &mut rows {
                    row.lock = Lock::Server;
                }
            }
            Some((index, record.label(), schema, rows))
        })
        .collect()
}

/// The hook in the world settings' scroll area: draws the mods' rows under
/// its last tab.
///
/// # Safety
///
/// - Called only by the mid-hook stub, on the hooked thread, with null or
///   the registers it saved, valid and unaliased for the call.
pub(super) unsafe extern "system" fn world_settings(
    registers: *mut Registers,
    _user: *mut core::ffi::c_void,
) {
    // SAFETY: the mid-hook stub passes either null or the registers it saved
    // on its stack, valid and unaliased for this call.
    let Some(registers) = (unsafe { registers.as_ref() }) else {
        return;
    };
    let (Ok(view), Ok(tab_state)) = (
        usize::try_from(registers.rsi),
        usize::try_from(registers.r15),
    ) else {
        return;
    };
    let Some(layout) = ENGINE.get().and_then(|engine| engine.view) else {
        return;
    };
    if view == 0 || tab_state == 0 {
        return;
    }
    // SAFETY: `tab_state` is non-zero, checked by the guard before it; at the
    // hook at the table's `ui.world_settings_site`, `r15` holds the view's tab
    // state, whose shown tab sits at the table's `ui.view_tab`.
    let tab = unsafe { crate::memory::read::<u8>(tab_state, layout.tab) };
    if tab != LAST_TAB {
        return;
    }
    // SAFETY: `view` is non-zero, checked by the guard before it; at the hook
    // at the table's `ui.world_settings_site`, `rsi` holds the view's config,
    // whose byte at the table's `ui.view_read_only` makes every row read-only,
    // as for a player who is no host.
    let read_only = unsafe { crate::memory::read::<u8>(view, layout.read_only) } != 0;
    let _drawn = WORLD_ROWS_FUSE.run("the world settings' draw", || world(read_only));
}

/// Turns the mods' world settings rows off at their first panic.
static WORLD_ROWS_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("the world settings rows");

/// Draws the mods' rows under the world settings' last tab, in its scroll
/// area; read-only for a player who may not change the world.
pub(super) fn world(read_only: bool) {
    let (Some(engine), Some(loader)) = (ENGINE.get(), crate::host::loader()) else {
        return;
    };
    let context = engine.context();
    if context == 0 {
        return;
    }
    let frame = FRAME.load(Ordering::Relaxed);
    // SAFETY: `world` runs only from `world_settings`, the hook inside the
    // world settings' scroll area, where the game's own window is open in the
    // frame on its thread; `context` is non-zero, checked by the guard before
    // it, and `paint` stays inside this call.
    let paint = unsafe { Painter::new(engine, context) };
    WORLD.with_borrow_mut(|slot| {
        let fresh = slot.as_ref().is_none_or(|w| w.frame + 1 < frame);
        if fresh {
            *slot = Some(WorldRows {
                frame,
                mods: world_rows(loader),
                message: None,
            });
        }
        let Some(world) = slot.as_mut() else {
            return;
        };
        world.frame = frame;
        if world.mods.is_empty() {
            return;
        }
        paint.label(
            texts::text("mods.title").as_bytes(),
            UiWidget::STYLE_H2,
            false,
            false,
        );
        let note = if crate::worlds::current().is_some() {
            texts::text("world.belongs")
        } else {
            texts::text("world.next")
        };
        paint.label(note.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
        if let Some((text, ok)) = &world.message {
            let style = if *ok {
                UiWidget::STYLE_POSITIVE
            } else {
                UiWidget::STYLE_NEGATIVE
            };
            paint.label(text.as_bytes(), style, true, false);
        }
        let mut changed = None;
        for (index, label, schema, rows) in &world.mods {
            paint.label(label.as_bytes(), UiWidget::STYLE_H3, false, false);
            for row in rows {
                let mut row = row.clone();
                if read_only && row.lock == Lock::Open {
                    row.lock = Lock::File;
                }
                let id = loader.mods.get(*index).map_or("", |m| m.id.as_str());
                if let Some(value) = draw_row(&paint, id, schema, &row, None) {
                    changed = Some((*index, row.key.clone(), value));
                }
            }
        }
        let Some((index, key, value)) = changed else {
            return;
        };
        let id = loader
            .mods
            .get(index)
            .map_or("", |m| m.id.as_str())
            .to_owned();
        world.message = Some(
            match crate::configure::apply(loader, index, &key, &value, "the world settings") {
                Ok(change) => (crate::configure::changed(&id, &key, &change), true),
                Err(reply) => (reply, false),
            },
        );
        world.mods = world_rows(loader);
    });
}
