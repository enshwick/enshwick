//! Cloned resources outlive the mods that clone them: the loader records each
//! cloned template and item with its vanilla source in
//! `mods/enshwick/clones.toml`, and when no mod serves a cloned template
//! any more, the game reads the vanilla template under the clone's GUID, so a
//! saved station loads as the vanilla station.
//!
//! Items are recorded too: an item stack keeps only its item id, and the
//! game drops ids it does not know, so [`crate::content::standins`] serves a
//! stand-in for each recorded item no mod serves.

use std::collections::BTreeMap;
use std::path::Path;

use enshwick_core::abi::ResourceKey;
use enshwick_core::content::Guid;
use rustc_hash::FxHashMap;

use crate::engine::catalogue::types;

/// The record file, under the `mods` folder.
pub(crate) const FILE: &str = "enshwick/clones.toml";

/// Every clone ever served: clone GUID to vanilla GUID, as text.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub(crate) struct Clones {
    /// Cloned templates.
    pub(crate) templates: BTreeMap<String, String>,
    /// Cloned items.
    pub(crate) items: BTreeMap<String, String>,
    /// Terrain materials mods added: each GUID to the GUID of the vanilla
    /// material its ground loads as without it.
    pub(crate) terrain: BTreeMap<String, String>,
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
            let vanilla = base.guid.to_string();
            let previous = list.insert(key.guid.to_string(), vanilla.clone());
            changed |= previous.as_ref() != Some(&vanilla);
        }
        changed
    }

    /// Adds each terrain material of `added`, a GUID and its fallback's;
    /// whether anything was new.
    pub(crate) fn add_terrain(&mut self, added: impl IntoIterator<Item = (Guid, Guid)>) -> bool {
        let mut changed = false;
        for (material, fallback) in added {
            let fallback = fallback.to_string();
            let previous = self.terrain.insert(material.to_string(), fallback.clone());
            changed |= previous.as_ref() != Some(&fallback);
        }
        changed
    }

    /// The template clones: clone GUID to vanilla GUID.
    pub(crate) fn template_aliases(&self) -> FxHashMap<Guid, Guid> {
        self.templates
            .iter()
            .filter_map(|(clone, vanilla)| Some((guid(clone)?, guid(vanilla)?)))
            .collect()
    }

    /// The terrain materials: material GUID and its fallback's GUID, in the
    /// record's order.
    pub(crate) fn terrain(&self) -> Vec<(Guid, Guid)> {
        self.terrain
            .iter()
            .filter_map(|(material, fallback)| Some((guid(material)?, guid(fallback)?)))
            .collect()
    }
}

/// What the record holds once [`update`] added this run's.
#[derive(Debug, Default)]
pub(crate) struct Recorded {
    /// The template clones: clone GUID to vanilla GUID.
    pub(crate) aliases: FxHashMap<Guid, Guid>,
    /// The terrain materials: material GUID and its fallback's GUID.
    pub(crate) terrain: Vec<(Guid, Guid)>,
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
/// GUID; none without a record.
pub(crate) fn items(data_dir: &Path) -> Vec<(Guid, Guid)> {
    std::fs::read_to_string(data_dir.join(FILE))
        .ok()
        .and_then(|text| toml::from_str::<Clones>(&text).ok())
        .map_or_default(|clones| {
            clones
                .items
                .iter()
                .filter_map(|(item, vanilla)| Some((guid(item)?, guid(vanilla)?)))
                .collect()
        })
}

/// Reads the record in `data_dir`, adds `served` and the terrain materials
/// `terrain` and writes it back when it changed; returns what it holds.
pub(crate) fn update(
    data_dir: &Path,
    served: impl IntoIterator<Item = (ResourceKey, ResourceKey)>,
    terrain: impl IntoIterator<Item = (Guid, Guid)>,
) -> Recorded {
    let path = data_dir.join(FILE);
    let read = enshwick_core::files::read_to_string_if_present(&path)
        .map_err(|error| error.to_string())
        .and_then(|text| {
            text.map_or_else(
                || Ok(Clones::default()),
                |text| toml::from_str::<Clones>(&text).map_err(|error| error.to_string()),
            )
        });
    let (mut clones, readable) = match read {
        Ok(clones) => (clones, true),
        // Saved stations fall back to what the file names: it stays for a
        // fix by hand, and this session records nothing.
        Err(error) => {
            crate::log!(
                WARN,
                "{} does not read and stays as it is: {error}",
                path.display()
            );
            (Clones::default(), false)
        }
    };
    let new_clone = clones.add(served);
    if (clones.add_terrain(terrain) || new_clone) && readable {
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
    Recorded {
        aliases: clones.template_aliases(),
        terrain: clones.terrain(),
    }
}

/// Writes `text` to `path` under the file's header.
fn write(path: &Path, text: &str) -> std::io::Result<()> {
    let header = "# Written by the Enshwick loader: every resource a mod cloned and every\n\
                  # terrain material a mod added, with the vanilla one each falls back to.\n\
                  # Keep it: saved stations and ground fall back to these.\n\n";
    enshwick_core::files::write_atomically(path, format!("{header}{text}"))
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
        assert_eq!(aliases.get(&CLONE), Some(&BENCH));
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
        assert_eq!(update(&dir, served, []).aliases.len(), 1);
        let aliases = update(&dir, [], []).aliases;
        assert_eq!(aliases.get(&CLONE), Some(&BENCH));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_removed_mod_leaves_its_terrain_materials_with_their_fallbacks() {
        let dir =
            std::env::temp_dir().join(format!("enshwick-clones-terrain-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&dir);
        let (tar, sand, rock) = (Guid([1; 16]), Guid([9; 16]), Guid([24; 16]));
        assert_eq!(update(&dir, [], [(tar, sand)]).terrain, [(tar, sand)]);
        assert_eq!(
            update(&dir, [], []).terrain,
            [(tar, sand)],
            "kept once the mod is gone"
        );
        assert_eq!(
            update(&dir, [], [(tar, rock)]).terrain,
            [(tar, rock)],
            "a changed fallback is the one kept"
        );
        let mut clones = Clones::default();
        assert!(clones.add_terrain([(tar, sand)]));
        assert!(
            !clones.add_terrain([(tar, sand)]),
            "nothing new the second time"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_record_that_does_not_read_stays_as_it_is() {
        let dir =
            std::env::temp_dir().join(format!("enshwick-clones-unread-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(FILE).parent().unwrap()).unwrap();
        let served = || {
            [(
                CLONE.key_by_hash(types::TEMPLATE),
                BENCH.key_by_hash(types::TEMPLATE),
            )]
        };
        let damaged = "[items\nnot toml";
        std::fs::write(dir.join(FILE), damaged).unwrap();
        let _recorded = update(&dir, served(), []);
        assert_eq!(std::fs::read_to_string(dir.join(FILE)).unwrap(), damaged);
        std::fs::remove_file(dir.join(FILE)).unwrap();
        std::fs::create_dir(dir.join(FILE)).unwrap();
        let _recorded = update(&dir, served(), []);
        assert!(dir.join(FILE).is_dir(), "nothing written in its place");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
