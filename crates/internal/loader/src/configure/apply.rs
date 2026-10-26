//! Writes one setting of a mod: checked against its schema and by the mod's
//! own check, written atomically with the file's comments kept, and the
//! mod told at once. Each player's own settings go to the settings file,
//! the others to the running world's settings, or with no world running to
//! the changes waiting for the next world. The console's `enshwick config` and the Mods screen both change settings
//! through here.

use enshwick::abi::Event;
use enshwick::config::Stage;

use crate::host::Loader;
use crate::locks::Relock;
use crate::texts;
use crate::worlds::Destination;

use super::{Change, refused};

/// Sets mod `index`'s setting `key` to `value`, TOML or for a text setting
/// the words as typed; `who` names the player for the log.
///
/// # Errors
///
/// A reply for the player: the mod, setting or value refused, or the file
/// not written.
pub(crate) fn apply(
    loader: &Loader,
    index: usize,
    key: &str,
    value: &str,
    who: &str,
) -> Result<Change, String> {
    let record = loader
        .mods
        .get(index)
        .ok_or_else(|| texts::fill("refused.no-mod", &[]))?;
    let id = record.id.as_str();
    let schema = record
        .schema
        .relock()
        .clone()
        .ok_or_else(|| texts::fill("config.no-schema", &[("id", id)]))?;
    let field = schema.field(super::element(key).0);
    if loader.side == enshwick::abi::Side::SERVER && field.is_some_and(|f| f.stage == Stage::Client)
    {
        return Err(texts::fill("refused.client", &[("id", id), ("key", key)]));
    }
    let stage = field.map_or(Stage::Runtime, |f| f.stage);
    let destination = crate::worlds::destination(stage, crate::worlds::current().as_deref());
    let global_path = loader.configs_dir.join(format!("{id}.toml"));
    let global = std::fs::read_to_string(&global_path)
        .map_err(|_| texts::fill("config.missing", &[("id", id)]))?;
    let (path, text) = crate::worlds::target_text(loader, record, &destination);
    let mut change = super::set(id, &schema, &text, key, value)?;
    // The mod checks what it would read with the change.
    let read = match &destination {
        Destination::Global => crate::worlds::effective(loader, record, &change.text),
        Destination::World(_) => crate::worlds::compose(&change.text, &global, &schema),
        Destination::Next => crate::worlds::overlay(&global, &change.text, None),
    };
    crate::host::check_settings(index, &read).map_err(|why| refused(&why))?;
    crate::watch::write_noted(&path, || crate::worlds::write_target(&path, &change.text)).map_err(
        |e| {
            texts::fill(
                "refused.write",
                &[
                    ("path", &path.display().to_string()),
                    ("error", &e.to_string()),
                ],
            )
        },
    )?;
    crate::log!(INFO, "{who} set {id} {key} = {}", change.value);
    change.destination = destination.clone();
    if destination != Destination::Next {
        loader.emit(Event::SETTINGS_CHANGED, Some(index), core::ptr::null());
    }
    Ok(change)
}
