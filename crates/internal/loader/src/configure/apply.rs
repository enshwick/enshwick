//! Writes one setting of a mod: checked against its schema and by the mod's
//! own check, written atomically with the file's comments kept, and the
//! mod told at once. `/config` and the Mods screen both change settings
//! through here.

use enshwick_loader_api::abi::Event;
use enshwick_loader_api::config::Stage;

use super::Change;
use crate::host::Loader;

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
        .ok_or_else(|| "No such mod.".to_owned())?;
    let id = record.id.as_str();
    let schema = record
        .schema
        .lock()
        .ok()
        .and_then(|s| s.clone())
        .ok_or_else(|| {
            format!("{id} does not describe its settings; edit mod-configs/{id}.toml.")
        })?;
    if loader.side == enshwick_loader_api::abi::Side::SERVER
        && schema.field(key).is_some_and(|f| f.stage == Stage::Client)
    {
        return Err(format!(
            "{id} {key} is each player's own: they set it in their game."
        ));
    }
    let path = loader.configs_dir.join(format!("{id}.toml"));
    let text = std::fs::read_to_string(&path)
        .map_err(|_| format!("mod-configs/{id}.toml is missing; restart to write it."))?;
    let change = super::set(id, &schema, &text, key, value)?;
    crate::host::check_settings(index, &change.text)
        .map_err(|why| format!("Not changed: {why}."))?;
    let staged = path.with_extension("toml.new");
    std::fs::write(&staged, &change.text)
        .and_then(|()| std::fs::rename(&staged, &path))
        .map_err(|e| format!("Not changed: mod-configs/{id}.toml: {e}"))?;
    crate::watch::wrote(&path);
    crate::log!(INFO, "{who} set {id} {key} = {}", change.value);
    loader.emit(Event::SETTINGS_CHANGED, Some(index), core::ptr::null());
    Ok(change)
}
