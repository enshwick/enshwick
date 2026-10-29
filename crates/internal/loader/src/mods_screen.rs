//! What the Mods screen and the world settings' Mods rows decide apart from
//! drawing: where Back goes, when the screen closes with its menu, which
//! settings each place shows and how a settings row reads.

use enshwick::config::{Schema, Scope, Stage};

use crate::configure::rows::{Lock, Row};
use crate::texts;

/// A menu hook saw Back while the screen showed.
pub const BACK_PRESSED: u8 = 1;
/// The pause menu opened its world settings, as the screen asked.
pub const WORLD_SETTINGS_OPENED: u8 = 2;

/// Whether something last drawn in frame `seen` missed the frame before
/// `frame`: its menu or view went away.
#[must_use]
pub const fn missed_frame(seen: u64, frame: u64) -> bool {
    seen.saturating_add(1) < frame
}

/// What the screen does at the start of a frame with what its menu hooks
/// heard since the last one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heard {
    /// Nothing: the page draws.
    Nothing,
    /// The world settings opened over the pause menu: the screen goes.
    WorldSettings,
    /// Back while a text field has the caret: the typing ends, the page
    /// stays.
    EndTyping,
    /// Back: one step back.
    Back,
}

/// What the screen does with `back`, the hooks' last word, while a text
/// field has the caret or not (`editing`).
#[must_use]
pub const fn heard(back: u8, editing: bool) -> Heard {
    match back {
        WORLD_SETTINGS_OPENED => Heard::WorldSettings,
        BACK_PRESSED if editing => Heard::EndTyping,
        BACK_PRESSED => Heard::Back,
        _ => Heard::Nothing,
    }
}

/// The screen's pages, as far as Back cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The mods.
    List,
    /// A mod's settings.
    Settings,
    /// The joined server's mods with settings.
    ServerList,
    /// The joined server's settings of a mod.
    ServerSettings,
    /// The actions the server offers.
    Actions,
}

/// Where one step back from a page goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Back {
    /// The screen closes.
    Close,
    /// The list of mods.
    List,
    /// The joined server's list of mods, asked for again.
    ServerList,
    /// A step inside the page, and to the list of mods from its start.
    Inside,
}

/// One step back from a page of `kind`.
#[must_use]
pub const fn back(kind: Kind) -> Back {
    match kind {
        Kind::List => Back::Close,
        Kind::Settings | Kind::ServerList => Back::List,
        Kind::ServerSettings => Back::ServerList,
        Kind::Actions => Back::Inside,
    }
}

/// The settings of `schema` each player keeps, the ones the Mods screen
/// edits; `None` when it has none.
#[must_use]
pub fn user_settings(mut schema: Schema) -> Option<Schema> {
    schema.fields.retain(|f| f.scope == Scope::User);
    (!schema.fields.is_empty()).then_some(schema)
}

/// The settings of `schema` the world settings show: the world's, and of
/// those only the ones a server sends at join while the client is `joined`
/// to another's game; `None` when none are left.
#[must_use]
pub fn world_settings(mut schema: Schema, joined: bool) -> Option<Schema> {
    schema.fields.retain(|f| f.scope == Scope::World);
    if joined {
        schema.fields.retain(|f| f.stage == Stage::Sync);
    }
    (!schema.fields.is_empty()).then_some(schema)
}

/// Marks every row as set by the server, which takes no changes here.
pub fn lock_to_server(rows: &mut [Row]) {
    for row in rows {
        row.lock = Lock::Server;
    }
}

/// `row` as a player who may not change the world reads it: a row open to
/// changes shows read-only.
#[must_use]
pub fn read_only(mut row: Row) -> Row {
    if row.lock == Lock::Open {
        row.lock = Lock::File;
    }
    row
}

/// The label of `row`, marked with when a change takes effect and with the
/// server that sets it.
#[must_use]
pub fn row_label(row: &Row) -> String {
    let mut label = row.label.clone();
    match row.stage {
        Stage::Startup => label = texts::fill("row.startup", &[("label", &label)]),
        Stage::Restart => label = texts::fill("row.restart", &[("label", &label)]),
        _ => {}
    }
    if row.lock == Lock::Server {
        label = texts::fill("row.server", &[("label", &label)]);
    }
    label
}

/// A joined server's join values, an inline table, as a table.
#[must_use]
pub fn server_table(values: &str) -> Option<toml::Table> {
    format!("v = {values}")
        .parse::<toml::Table>()
        .ok()?
        .remove("v")?
        .as_table()
        .cloned()
}

#[cfg(test)]
mod tests;
