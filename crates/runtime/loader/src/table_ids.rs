//! Which build table ids each part of the loader names, declared beside the
//! code that names them, so `enshwick-compat` can tell which features
//! a build lacks. Ids built at run time, such as `survivors.{name}`, only
//! reach compat through these lists.

/// Each part of the loader with the build table ids it names.
pub static TABLE_IDS: &[(&str, &[&str])] = &[
    ("admission", crate::base::admission::TABLE_IDS),
    ("altars", crate::world::altars::TABLE_IDS),
    ("bases", crate::world::bases::TABLE_IDS),
    ("buffs", crate::world::buffs::TABLE_IDS),
    ("carrier", crate::base::carrier::TABLE_IDS),
    ("chat", crate::actions::chat::TABLE_IDS),
    ("code", crate::engine::code::TABLE_IDS),
    ("console", crate::actions::console::TABLE_IDS),
    ("content", crate::content::served::TABLE_IDS),
    ("damage", crate::inventory::damage::TABLE_IDS),
    ("daytime", crate::world::daytime::TABLE_IDS),
    ("deaths", crate::inventory::deaths::TABLE_IDS),
    ("ecs", crate::engine::ecs::TABLE_IDS),
    ("emits", crate::base::emits::TABLE_IDS),
    ("entities", crate::world::entities::TABLE_IDS),
    ("events", crate::events::TABLE_IDS),
    ("fog", crate::world::fog::TABLE_IDS),
    ("game_events", crate::base::game_events::TABLE_IDS),
    ("game_settings", crate::world::game_settings::TABLE_IDS),
    ("interactions", crate::actions::interactions::TABLE_IDS),
    ("item_data", crate::inventory::item_data::TABLE_IDS),
    ("inventory", crate::inventory::transactions::TABLE_IDS),
    ("knowledge", crate::knowledge::store::TABLE_IDS),
    ("label", crate::world::label::TABLE_IDS),
    ("local_held", crate::actions::local_held::TABLE_IDS),
    ("notices", crate::ui::notices::TABLE_IDS),
    ("pacing", crate::engine::pacing::TABLE_IDS),
    ("progress", crate::knowledge::progress::TABLE_IDS),
    ("rate_links", crate::content::rate_links::TABLE_IDS),
    ("rights", crate::world::rights::TABLE_IDS),
    ("saves", crate::base::saves::TABLE_IDS),
    ("scene", crate::world::scene::TABLE_IDS),
    ("spawns", crate::inventory::spawns::TABLE_IDS),
    ("start", crate::start::TABLE_IDS),
    ("static_halves", crate::engine::static_halves::TABLE_IDS),
    ("stats", crate::inventory::stats::TABLE_IDS),
    ("survivors", crate::inventory::survivors::TABLE_IDS),
    ("terrain", crate::world::terrain::TABLE_IDS),
    ("textures", crate::content::textures::TABLE_IDS),
    ("transactions", crate::world::transactions::TABLE_IDS),
    ("ui", crate::ui::interface::TABLE_IDS),
    ("vetoes", crate::actions::vetoes::TABLE_IDS),
    ("voxels", crate::world::voxels::TABLE_IDS),
    ("weather", crate::world::weather::TABLE_IDS),
    ("world", crate::engine::world::TABLE_IDS),
    ("writes", crate::world::writes::TABLE_IDS),
];

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::{Path, PathBuf};

    use enshwick_gamedata::{self as tables, Table};

    use super::TABLE_IDS;

    /// Entries the tables pin that no part of the loader names yet.
    const UNNAMED: &[&str] = &[
        "actor.state_in_shroud",
        "attribute.experience",
        "attribute.experience_needed_slot",
        "attribute.fuel",
        "attribute.fuel_max_slot",
        "attribute.level",
        "attribute.shroud",
        "attribute.shroud_max_slot",
        "inventory.skip_entity",
        "loot.request_labels",
        "loot.request_node",
        "loot.roll",
        "loot.roll_one",
        "loot.txn_context",
        "loot.txn_entity",
        "skill.airborne_architect",
        "skill.airborne_architect_cell",
        "ui_image.content",
        "ui_image.flags",
        "ui_image.format",
        "ui_image.height",
        "ui_image.levels",
        "ui_image.width",
    ];

    /// Every symbol and constant id of the embedded tables, without the
    /// expected bytes the tables keep beside a symbol.
    fn pinned() -> BTreeSet<String> {
        tables::kept()
            .unwrap()
            .iter()
            .map(|(file, text)| Table::parse(text).unwrap_or_else(|e| panic!("{file}: {e}")))
            .flat_map(|t| {
                let symbols = t.symbols().iter().map(|s| s.id().to_owned());
                let constants = t.constants().iter().map(|c| c.id().to_owned());
                symbols.chain(constants).collect::<Vec<_>>()
            })
            .filter(|id| id.rsplit('.').next() != Some("bytes"))
            .collect()
    }

    /// Every id the loader finds on a build: by an anchor's pattern on
    /// either side, or by an engine route; the loader never reads the kept
    /// tables, so they count for nothing here.
    fn anchored_or_routed() -> BTreeSet<String> {
        let anchored = [tables::Side::Server, tables::Side::Client]
            .into_iter()
            .flat_map(|side| {
                let anchors = tables::anchors(side).unwrap();
                let symbols = anchors.symbols().iter().map(|s| s.id().to_owned());
                let constants = anchors.constants().iter().map(|c| c.id().to_owned());
                symbols.chain(constants).collect::<Vec<_>>()
            });
        let routed = tables::Routes::embedded()
            .unwrap()
            .ids()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        anchored.chain(routed).collect()
    }

    fn listed() -> BTreeSet<&'static str> {
        TABLE_IDS
            .iter()
            .flat_map(|(_, ids)| ids.iter().copied())
            .collect()
    }

    fn module_of(name: &str) -> String {
        // Without whitespace, so a row rustfmt wraps reads the same.
        let this: String = include_str!("table_ids.rs")
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let row = format!("(\"{name}\",crate::");
        let start = this.find(&row).unwrap_or_else(|| panic!("no row {name}")) + row.len();
        let rest = &this[start..];
        let end = rest
            .find("::TABLE_IDS")
            .unwrap_or_else(|| panic!("row {name} names no TABLE_IDS"));
        rest[..end].replace("::", "/")
    }

    /// The code of the module of row `name` and its submodules, without test
    /// modules.
    fn sources(name: &str) -> String {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let module = module_of(name);
        let mut files = vec![src.join(format!("{module}.rs"))];
        let mut dirs = vec![src.join(module)];
        while let Some(dir) = dirs.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for path in entries.filter_map(|e| e.ok().map(|e| e.path())) {
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    files.push(path);
                }
            }
        }
        files
            .iter()
            .filter(|p| {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default();
                name != "tests.rs" && !name.ends_with("_tests.rs")
            })
            .map(|p: &PathBuf| {
                let text = std::fs::read_to_string(p).unwrap();
                match text.find("#[cfg(test)]\nmod tests") {
                    Some(at) => text[..at].to_owned(),
                    None => text,
                }
            })
            .collect()
    }

    #[test]
    fn every_listed_id_is_anchored_or_routed() {
        let found = anchored_or_routed();
        let stale: Vec<_> = listed()
            .into_iter()
            .filter(|id| !found.contains(*id))
            .collect();
        assert!(
            stale.is_empty(),
            "listed but in no anchor or route: {stale:?}"
        );
    }

    #[test]
    fn every_pinned_id_is_listed_or_known_unnamed() {
        let listed = listed();
        let unlisted: Vec<_> = pinned()
            .into_iter()
            .filter(|id| !listed.contains(id.as_str()) && !UNNAMED.contains(&id.as_str()))
            .collect();
        assert!(
            unlisted.is_empty(),
            "pinned but listed by no part of the loader: {unlisted:?}"
        );
        let named: Vec<_> = UNNAMED.iter().filter(|id| listed.contains(*id)).collect();
        assert!(named.is_empty(), "listed, so no longer unnamed: {named:?}");
    }

    /// Every typed id of `crate::engine::pinned`, by its path below it, such as
    /// `world::TIME`, read from the catalogue's sources.
    fn catalogue() -> BTreeMap<String, String> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/engine/pinned");
        let mut ids = BTreeMap::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let part = path.file_stem().unwrap().to_string_lossy().into_owned();
            let text = std::fs::read_to_string(&path).unwrap();
            for item in text.split("pub(crate) const ").skip(1) {
                let (name, rest) = item.split_once(':').unwrap();
                let id = rest.split('"').nth(1).unwrap();
                ids.insert(format!("{part}::{name}"), id.to_owned());
            }
        }
        ids
    }

    #[test]
    fn every_catalogued_id_is_anchored_or_routed() {
        let found = anchored_or_routed();
        let stale: Vec<_> = catalogue()
            .into_values()
            .filter(|id| !found.contains(id))
            .collect();
        assert!(
            stale.is_empty(),
            "catalogued but in no anchor or route: {stale:?}"
        );
    }

    #[test]
    fn every_id_a_module_names_is_in_its_list() {
        let catalogue = catalogue();
        for (name, ids) in TABLE_IDS {
            let text = sources(name);
            let missing: Vec<_> = text
                .split("pinned::")
                .skip(1)
                .filter_map(|rest| {
                    let end = rest
                        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == ':'))
                        .unwrap_or(rest.len());
                    catalogue.get(&rest[..end])
                })
                .filter(|id| !ids.contains(&id.as_str()))
                .collect();
            assert!(missing.is_empty(), "{name} names {missing:?} unlisted");
        }
    }

    #[test]
    fn no_module_spells_a_pinned_id_as_text() {
        let pinned = pinned();
        for (name, _) in TABLE_IDS {
            let text = sources(name);
            let spelled: Vec<_> = text
                .split('"')
                .skip(1)
                .step_by(2)
                .filter(|literal| pinned.contains(*literal))
                .collect();
            assert!(
                spelled.is_empty(),
                "{name} spells {spelled:?}; name them through crate::pinned"
            );
        }
    }
}
