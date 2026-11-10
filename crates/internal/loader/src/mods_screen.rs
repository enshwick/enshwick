//! What the Mods screen decides apart from drawing: where Back goes, when
//! the screen closes with its menu, which settings a mod's page shows under
//! which heading and how a settings row reads.

use enshwick::config::Schema;

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
    /// The joined server's settings of a mod, opened from the server's list.
    ServerSettings,
    /// The joined server's settings of a mod, opened from that mod on the
    /// list of mods.
    ModServerSettings,
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
        Kind::ServerList | Kind::ModServerSettings => Back::List,
        Kind::ServerSettings => Back::ServerList,
        // A key binding on the page may wait for a key.
        Kind::Settings => Back::Inside,
    }
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

/// The rows of a mod's settings page in another's world: `schema` without
/// its secrets over `values`, what the server sent, every row marked as set
/// by the server, which takes no changes here.
#[must_use]
pub fn server_rows(
    mut schema: Schema,
    values: &str,
    words: Option<crate::configure::rows::Words<'_>>,
) -> Vec<Row> {
    schema.fields.retain(|f| !f.secret);
    let mut rows = crate::configure::rows::rows(&schema, values, words);
    lock_to_server(&mut rows);
    rows
}

/// Marks every row as set by the server, which takes no changes here.
pub fn lock_to_server(rows: &mut [Row]) {
    for row in rows {
        row.lock = Lock::Server;
    }
}

/// The label of `row`, marked with the server that sets it.
#[must_use]
pub fn row_label(row: &Row) -> String {
    if row.lock == Lock::Server {
        texts::fill("row.server", &[("label", &row.label)])
    } else {
        row.label.clone()
    }
}

/// Whether a mod's gear shows: in a world this process runs, or in
/// another's world whose server sent the mod's settings. In the main menu
/// settings belong to no world, and the screen says so once.
#[must_use]
pub const fn gear(world: bool, sent: bool) -> bool {
    world || sent
}

#[cfg(test)]
mod tests;
