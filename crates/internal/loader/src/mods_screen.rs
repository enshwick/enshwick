//! What the Mods screen decides apart from drawing: where Back goes, when
//! the screen closes with its menu, which settings a mod's page shows under
//! which heading and how a settings row reads.

use enshwick::config::{Schema, Scope, Stage};

use crate::configure::rows::{Lock, Row};
use crate::texts;

/// A menu hook saw Back while the screen showed.
pub const BACK_PRESSED: u8 = 1;

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

/// The settings of `schema` a mod's page shows, each player's own first.
///
/// The world's follow; in another's world (`away`) only those the server
/// sends at join, which the page shows read-only. `None` when none are left.
#[must_use]
pub fn page_settings(mut schema: Schema, away: bool) -> Option<Schema> {
    if away {
        schema
            .fields
            .retain(|f| f.scope == Scope::User || f.stage == Stage::Sync);
    }
    schema.fields.sort_by_key(|f| f.scope != Scope::User);
    (!schema.fields.is_empty()).then_some(schema)
}

/// Where the Mods screen shows a mod's settings, from the game's state as
/// the page opens: the page is built again when it changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    /// This process runs a world, single player or hosted: the world's
    /// settings are that world's.
    pub world: bool,
    /// The player is in another's world, which keeps its settings: they
    /// show read-only.
    pub away: bool,
    /// The joined server takes messages, so its administrators change its
    /// settings on its own page.
    pub joined: bool,
}

impl Place {
    /// The place of a screen open over the pause menu (`paused`) or the
    /// main menu, while this process runs a world (`world`) or not, joined
    /// to a server that takes messages (`joined`) or not.
    #[must_use]
    pub const fn new(paused: bool, world: bool, joined: bool) -> Self {
        Self {
            world,
            away: paused && !world,
            joined: joined && !world,
        }
    }
}

/// Marks the world's rows of `rows`, whose fields `schema` describes, as
/// set by the server, which takes no changes from here.
pub fn lock_world_rows(schema: &Schema, rows: &mut [Row]) {
    for row in rows {
        let key = crate::configure::element(&row.key).0;
        if schema.field(key).is_some_and(|f| f.scope == Scope::World) {
            row.lock = Lock::Server;
        }
    }
}

/// Whether `schema` has settings of the world, which a server keeps.
#[must_use]
pub fn has_world_settings(schema: &Schema) -> bool {
    schema.fields.iter().any(|f| f.scope == Scope::World)
}

/// The text keys of the heading over a mod's settings of `scope` and of the
/// note under it.
///
/// While this process runs a world (`world`) its settings are that world's;
/// with none they are the defaults of new worlds.
#[must_use]
pub fn section(scope: Scope, world: bool) -> (&'static str, Option<&'static str>) {
    match scope {
        Scope::World if world => ("mods.settings.world", Some("world.belongs")),
        Scope::World => ("mods.settings.new-worlds", Some("world.new")),
        _ => ("mods.settings.yours", None),
    }
}

/// Marks every row as set by the server, which takes no changes here.
pub fn lock_to_server(rows: &mut [Row]) {
    for row in rows {
        row.lock = Lock::Server;
    }
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
