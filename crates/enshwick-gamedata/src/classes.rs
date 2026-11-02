//! The audit of every build table entry: whether the loader finds it in the
//! game without a table row, read from `anchors/classes.toml` and held by
//! these tests.

use rustc_hash::FxHashSet;

use crate::{Routes, Side, Table, embedded};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum Class {
    Engine,
    Research,
    Pattern,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    entries: Vec<String>,
    class: Class,
    reason: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ClassesFile {
    class: Vec<Row>,
}

fn rows() -> Vec<Row> {
    let file: ClassesFile = toml::from_str(include_str!("../anchors/classes.toml")).unwrap();
    file.class
}

/// The row of `id`: the one naming it, else the one with its longest prefix.
fn class_of<'r>(rows: &'r [Row], id: &str) -> Option<&'r Row> {
    let named = rows.iter().find(|r| r.entries.iter().any(|e| e == id));
    named.or_else(|| {
        rows.iter()
            .filter_map(|r| {
                r.entries
                    .iter()
                    .filter_map(|e| e.strip_suffix('*'))
                    .filter(|prefix| id.starts_with(prefix))
                    .map(str::len)
                    .max()
                    .map(|len| (len, r))
            })
            .max_by_key(|&(len, _)| len)
            .map(|(_, r)| r)
    })
}

fn tables() -> Vec<Table> {
    embedded()
        .iter()
        .map(|(_, text)| Table::parse(text).unwrap())
        .collect()
}

fn ids(table: &Table) -> impl Iterator<Item = &str> {
    table
        .symbols()
        .iter()
        .map(crate::Symbol::id)
        .chain(table.constants().iter().map(crate::Constant::id))
}

#[test]
fn every_entry_of_every_table_has_a_class_and_a_reason() {
    let rows = rows();
    assert!(rows.iter().all(|r| !r.reason.trim().is_empty()));
    let mut named = FxHashSet::default();
    for entry in rows.iter().flat_map(|r| &r.entries) {
        assert!(named.insert(entry.as_str()), "{entry} is classed twice");
    }
    let unclassed: Vec<String> = tables()
        .iter()
        .flat_map(|t| {
            ids(t)
                .filter(|id| class_of(&rows, id).is_none())
                .map(|id| format!("{}: {id}", t.name()))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(unclassed.is_empty(), "{unclassed:#?}");
}

#[test]
fn every_entry_a_class_names_is_in_a_table() {
    let tables = tables();
    let known: FxHashSet<&str> = tables.iter().flat_map(ids).collect();
    let stale: Vec<String> = rows()
        .into_iter()
        .flat_map(|r| r.entries)
        .filter(|e| !e.ends_with('*') && !known.contains(e.as_str()))
        .collect();
    assert!(stale.is_empty(), "{stale:?}");
}

/// An entry is classed `engine` exactly when the loader finds it without
/// its row: an engine route finds it or its symbol, or it is an
/// attribute's slot the loader reads from the game's archive.
#[test]
fn the_engine_class_is_what_the_loader_finds_without_a_table() {
    let rows = rows();
    let routes = Routes::embedded().unwrap();
    let routed = |id: &str| {
        [Side::Server, Side::Client]
            .iter()
            .any(|&s| routes.routes(id, s))
    };
    let mut wrong = Vec::new();
    for table in tables() {
        let attribute = |id: &str| {
            table
                .constants()
                .iter()
                .any(|c| c.id() == id && c.attribute().is_some())
        };
        for id in ids(&table) {
            let finds =
                routed(id) || id.strip_suffix(".bytes").is_some_and(routed) || attribute(id);
            let engine = class_of(&rows, id).is_some_and(|r| r.class == Class::Engine);
            if finds != engine {
                wrong.push(format!(
                    "{}: {id} found {finds}, classed engine {engine}",
                    table.name()
                ));
            }
        }
    }
    wrong.dedup();
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn an_id_takes_its_own_row_before_the_longest_prefix() {
    let rows = vec![
        Row {
            entries: vec!["ui.*".into()],
            class: Class::Research,
            reason: "r".into(),
        },
        Row {
            entries: vec!["ui.hud.*".into()],
            class: Class::Pattern,
            reason: "p".into(),
        },
        Row {
            entries: vec!["ui.hud.bytes".into()],
            class: Class::Engine,
            reason: "e".into(),
        },
    ];
    let class = |id: &str| class_of(&rows, id).map(|r| r.class);
    assert_eq!(class("ui.hud.bytes"), Some(Class::Engine));
    assert_eq!(class("ui.hud.x"), Some(Class::Pattern));
    assert_eq!(class("ui.slider"), Some(Class::Research));
    assert_eq!(class("map.x"), None);
}
