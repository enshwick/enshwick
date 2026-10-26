//! Cloned resources outlive the mods that clone them: the loader records each
//! cloned template and item with its vanilla source in
//! `mod-data/enshwick/clones.toml`, and when no mod serves a cloned template
//! any more, the game reads the vanilla template under the clone's GUID, so a
//! saved station loads as the vanilla station.
//!
//! Items are recorded too: an item stack keeps only its item id, and the
//! game drops ids it does not know, so [`crate::standins`] serves a
//! stand-in for each recorded item no mod serves.

use std::collections::BTreeMap;
use std::path::Path;

use enshwick::abi::ResourceKey;
use enshwick::content::Guid;
use rustc_hash::FxHashMap;

use crate::catalogue::types;

/// The record file, under the `mod-data` folder.
pub(crate) const FILE: &str = "enshwick/clones.toml";

/// Every clone ever served: clone GUID to vanilla GUID, as text.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub(crate) struct Clones {
    /// Cloned templates.
    pub(crate) templates: BTreeMap<String, String>,
    /// Cloned items.
    pub(crate) items: BTreeMap<String, String>,
}

impl Clones {
    /// Adds every served key that clones another resource of its type;
    /// whether anything was new.
    pub(crate) fn add(
        &mut self,
        served: impl IntoIterator<Item = (ResourceKey, ResourceKey)>,
    ) -> bool {
        let mut changed = false;
        for (key, base) in served {
            if key.guid == base.guid || key.type_hash != base.type_hash {
                continue;
            }
            let list = match key.type_hash {
                types::TEMPLATE => &mut self.templates,
                types::ITEM_INFO => &mut self.items,
                _ => continue,
            };
            let vanilla = Guid(base.guid).to_string();
            let previous = list.insert(Guid(key.guid).to_string(), vanilla.clone());
            changed |= previous.as_ref() != Some(&vanilla);
        }
        changed
    }

    /// The template clones: clone GUID to vanilla GUID, in memory order.
    pub(crate) fn template_aliases(&self) -> FxHashMap<[u8; 16], [u8; 16]> {
        self.templates
            .iter()
            .filter_map(|(clone, vanilla)| Some((guid(clone)?.0, guid(vanilla)?.0)))
            .collect()
    }
}

/// `text` as a GUID; `None` unless it is one.
fn guid(text: &str) -> Option<Guid> {
    let bytes = text.as_bytes();
    let well_formed = bytes.len() == 36
        && bytes.iter().enumerate().all(|(i, &c)| match i {
            8 | 13 | 18 | 23 => c == b'-',
            _ => c.is_ascii_hexdigit(),
        });
    // Checked above, so the parse cannot fail.
    well_formed.then(|| Guid::parse(text))
}

/// The cloned items of the record in `data_dir`: item GUID and vanilla
/// GUID, in memory order; none without a record.
pub(crate) fn items(data_dir: &Path) -> Vec<([u8; 16], [u8; 16])> {
    std::fs::read_to_string(data_dir.join(FILE))
        .ok()
        .and_then(|text| toml::from_str::<Clones>(&text).ok())
        .map(|clones| {
            clones
                .items
                .iter()
                .filter_map(|(item, vanilla)| Some((guid(item)?.0, guid(vanilla)?.0)))
                .collect()
        })
        .unwrap_or_default()
}

/// Reads the record in `data_dir`, adds `served` and writes it back when it
/// changed; returns the template aliases.
pub(crate) fn update(
    data_dir: &Path,
    served: impl IntoIterator<Item = (ResourceKey, ResourceKey)>,
) -> FxHashMap<[u8; 16], [u8; 16]> {
    let path = data_dir.join(FILE);
    let mut clones = match std::fs::read_to_string(&path) {
        Ok(text) => toml::from_str::<Clones>(&text).unwrap_or_else(|error| {
            crate::log!(
                WARN,
                "{} unreadable, started again: {error}",
                path.display()
            );
            Clones::default()
        }),
        Err(_) => Clones::default(),
    };
    if clones.add(served) {
        let written = toml::to_string(&clones)
            .map_err(|error| error.to_string())
            .and_then(|text| write(&path, &text).map_err(|error| error.to_string()));
        if let Err(error) = written {
            crate::log!(
                WARN,
                "clone record not written to {}: {error}",
                path.display()
            );
        }
    }
    clones.template_aliases()
}

/// Writes `text` to a temporary file beside `path`, then renames it over.
fn write(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temporary = path.with_extension("toml.tmp");
    let header = "# Written by the Enshwick loader: every resource a mod cloned and the\n\
                  # vanilla one it came from. Keep it: saved stations fall back to these.\n\n";
    std::fs::write(&temporary, format!("{header}{text}"))?;
    std::fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLONE: Guid = Guid::parse("77ff6155-1365-4a4a-8987-507a1bdd32a2");
    const BENCH: Guid = Guid::parse("ebf450c3-fb96-4240-a3be-8ebe0713eccd");
    const ITEM: Guid = Guid::parse("0851e93d-8883-42e4-a807-c30fd644e7c3");
    const BENCH_ITEM: Guid = Guid::parse("2f04a05f-697d-489b-a32f-76bb3439f5bc");

    #[test]
    fn clones_are_recorded_and_patches_are_not() {
        let mut clones = Clones::default();
        let registry = BENCH.key_by_hash(types::ITEM_REGISTRY);
        let served = [
            (
                CLONE.key_by_hash(types::TEMPLATE),
                BENCH.key_by_hash(types::TEMPLATE),
            ),
            (
                ITEM.key_by_hash(types::ITEM_INFO),
                BENCH_ITEM.key_by_hash(types::ITEM_INFO),
            ),
            (registry, registry),
        ];
        assert!(clones.add(served));
        assert!(!clones.add(served), "nothing new the second time");
        assert_eq!(clones.templates.len(), 1);
        assert_eq!(clones.items[&ITEM.to_string()], BENCH_ITEM.to_string());
        let aliases = clones.template_aliases();
        assert_eq!(aliases.get(&CLONE.0), Some(&BENCH.0));
    }

    #[test]
    fn the_record_round_trips_and_skips_what_is_not_a_guid() {
        let mut clones = Clones::default();
        clones.add([(
            CLONE.key_by_hash(types::TEMPLATE),
            BENCH.key_by_hash(types::TEMPLATE),
        )]);
        clones
            .templates
            .insert("nonsense".into(), BENCH.to_string());
        let text = toml::to_string(&clones).unwrap();
        let back: Clones = toml::from_str(&text).unwrap();
        assert_eq!(back, clones);
        assert_eq!(back.template_aliases().len(), 1);
        assert_eq!(toml::from_str::<Clones>("").unwrap(), Clones::default());
    }

    #[test]
    fn a_removed_mod_leaves_its_clones_in_the_file() {
        let dir = std::env::temp_dir().join(format!("enshwick-clones-{}", std::process::id()));
        let served = [(
            CLONE.key_by_hash(types::TEMPLATE),
            BENCH.key_by_hash(types::TEMPLATE),
        )];
        assert_eq!(update(&dir, served).len(), 1);
        let aliases = update(&dir, []);
        assert_eq!(aliases.get(&CLONE.0), Some(&BENCH.0));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
