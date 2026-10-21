//! Which build table ids each part of the loader names, declared beside the
//! code that names them, so `cargo enshwick compat` can tell which features
//! a build lacks. Ids built at run time, such as `survivors.{name}`, only
//! reach compat through these lists.

/// Each part of the loader with the build table ids it names.
pub static TABLE_IDS: &[(&str, &[&str])] = &[
    ("altars", crate::altars::TABLE_IDS),
    ("bases", crate::bases::TABLE_IDS),
    ("buffs", crate::buffs::TABLE_IDS),
    ("chat", crate::chat::TABLE_IDS),
    ("content", crate::content::TABLE_IDS),
    ("damage", crate::damage::TABLE_IDS),
    ("daytime", crate::daytime::TABLE_IDS),
    ("deaths", crate::deaths::TABLE_IDS),
    ("ecs", crate::ecs::TABLE_IDS),
    ("emits", crate::emits::TABLE_IDS),
    ("entities", crate::entities::TABLE_IDS),
    ("events", crate::events::TABLE_IDS),
    ("game_events", crate::game_events::TABLE_IDS),
    ("game_settings", crate::game_settings::TABLE_IDS),
    ("interactions", crate::interactions::TABLE_IDS),
    ("inventory", crate::inventory::TABLE_IDS),
    ("knowledge", crate::knowledge::TABLE_IDS),
    ("label", crate::label::TABLE_IDS),
    ("notices", crate::notices::TABLE_IDS),
    ("progress", crate::progress::TABLE_IDS),
    ("saves", crate::saves::TABLE_IDS),
    ("spawns", crate::spawns::TABLE_IDS),
    ("start", crate::start::TABLE_IDS),
    ("stats", crate::stats::TABLE_IDS),
    ("survivors", crate::survivors::TABLE_IDS),
    ("terrain", crate::terrain::TABLE_IDS),
    ("textures", crate::textures::TABLE_IDS),
    ("ui", crate::ui::TABLE_IDS),
    ("vetoes", crate::vetoes::TABLE_IDS),
    ("weather", crate::weather::TABLE_IDS),
    ("world", crate::world::TABLE_IDS),
    ("writes", crate::writes::TABLE_IDS),
];

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    use enshwick_loader_builds::{self as tables, Table};

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
        tables::embedded()
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

    fn listed() -> BTreeSet<&'static str> {
        TABLE_IDS
            .iter()
            .flat_map(|(_, ids)| ids.iter().copied())
            .collect()
    }

    /// The code of module `name` and its submodules, without test modules.
    fn sources(name: &str) -> String {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = vec![src.join(format!("{name}.rs"))];
        let mut dirs = vec![src.join(name)];
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
            .filter(|p| !p.to_string_lossy().ends_with("_tests.rs"))
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
    fn every_listed_id_is_pinned() {
        let pinned = pinned();
        let stale: Vec<_> = listed()
            .into_iter()
            .filter(|id| !pinned.contains(*id))
            .collect();
        assert!(stale.is_empty(), "listed but in no table: {stale:?}");
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

    #[test]
    fn every_id_a_module_spells_out_is_in_its_list() {
        let pinned = pinned();
        for (name, ids) in TABLE_IDS {
            let text = sources(name);
            let missing: Vec<_> = text
                .split('"')
                .skip(1)
                .step_by(2)
                .filter(|literal| pinned.contains(*literal) && !ids.contains(literal))
                .collect();
            assert!(missing.is_empty(), "{name} names {missing:?} unlisted");
        }
    }
}
