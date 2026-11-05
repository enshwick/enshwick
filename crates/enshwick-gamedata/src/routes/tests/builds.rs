//! The routes and the anchors on the kept game executables, each test
//! skipped without the variables that name them.

use super::super::{Routed, Routes};
use super::empty;
use crate::reflect::{Image, Layout, Reflection};
use crate::{Side, Table, embedded, exact};

/// Entries a route finds on a table build that its own table does not pin
/// and no pattern of the newest table finds there either, by table name.
const UNPINNED_UNCHECKED: &[(&str, &[&str])] = &[
    (
        "1.0 client (1269579)",
        &[
            "bases.cursor",
            "bases.entity_query",
            "bases.set_insert",
            "building.block_box",
            "building.box_extent",
            "building.box_min",
            "building.box_size",
            "recipe.limit_stop",
            "ui.no_construct",
            "ui.scroll_focus",
            "ui.scroll_state_size",
            "ui.text_field",
            "ui.text_field_extent",
            "ui.text_field_text",
        ],
    ),
    (
        "1.0 server (1269579)",
        &[
            "bases.cursor",
            "bases.entity_query",
            "bases.set_insert",
            "building.block_box",
            "building.box_extent",
            "building.box_min",
            "building.box_size",
            "recipe.limit_stop",
        ],
    ),
];

/// Entries a kept build's table pins that the loader needs on no build of
/// that side, by side: the server keeps the client's base set-up code but
/// never runs it (`anchors/classes.toml`).
const NOT_NEEDED: &[(Side, &[&str])] = &[(
    Side::Server,
    &[
        "bases.base_ids",
        "bases.client_create",
        "bases.client_destroy",
        "bases.client_size",
        "bases.record_base_id",
    ],
)];

/// The executables `ENSH_SERVER_EXE`, `ENSH_CLIENT_EXE` and the opt-in
/// `ENSH_EXES` (a path list, such as every kept build) name, each a server
/// when its file is `enshrouded_server.exe`.
fn executables() -> Vec<(std::path::PathBuf, Side)> {
    let mut paths: Vec<(std::path::PathBuf, Side)> = [
        ("ENSH_SERVER_EXE", Side::Server),
        ("ENSH_CLIENT_EXE", Side::Client),
    ]
    .into_iter()
    .filter_map(|(var, side)| Some((std::env::var_os(var)?.into(), side)))
    .collect();
    if let Some(list) = std::env::var_os("ENSH_EXES") {
        for path in std::env::split_paths(&list) {
            let server = path
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("enshrouded_server.exe"));
            paths.push((path, if server { Side::Server } else { Side::Client }));
        }
    }
    paths
}

fn tables() -> Vec<Table> {
    embedded()
        .iter()
        .map(|(_, text)| Table::parse(text).unwrap())
        .collect()
}

/// On every [`executables`] entry, every route of its side finds, with no
/// table behind it, the value its own table pins; an entry the table does
/// not pin takes the value the newest table's patterns find there, or is
/// one of [`UNPINNED_UNCHECKED`].
#[test]
#[cfg_attr(miri, ignore = "reading a game executable takes hours under Miri")]
fn every_route_holds_on_every_kept_build() {
    let tables = tables();
    let routes = Routes::embedded().unwrap();
    for (path, side) in executables() {
        let bytes = std::fs::read(&path).unwrap();
        let image = Image::parse(&bytes, Layout::File, None).unwrap();
        let reflection = Reflection::scan(&image);
        let own = exact(&tables, side, image.fingerprint().unwrap())
            .unwrap()
            .resolve();
        let text = image.range(image.section(".text").unwrap());
        let code = image.read(text.start, text.end - text.start).unwrap();
        let carried = crate::newest(&tables, side).unwrap().fall_back(
            code,
            text.start - image.base(),
            |at| image.u32(image.base() + at),
        );
        let unchecked: &[&str] = UNPINNED_UNCHECKED
            .iter()
            .find(|(name, _)| *name == own.name())
            .map_or(&[], |(_, ids)| ids);
        let wrong: Vec<String> = routes
            .find(&image, &reflection, side, &empty())
            .into_iter()
            .filter_map(|(id, routed)| {
                let (value, constant) = match routed {
                    Some(Routed::Symbol(v) | Routed::Reached(v)) => (Some(v), false),
                    Some(Routed::Constant(v)) => (Some(v), true),
                    None => (None, false),
                };
                let entry = |table: &crate::Resolved| {
                    if constant {
                        table.constant(id)
                    } else {
                        table.symbol(id)
                    }
                };
                let pinned = own.symbol(id).is_some() || own.constant(id).is_some();
                let want = if pinned {
                    entry(&own)
                } else if value.is_none() || unchecked.contains(&id) {
                    return None;
                } else {
                    entry(&carried)
                };
                (value != want).then(|| format!("{id}: route {routed:x?}, table {want:x?}"))
            })
            .collect();
        assert!(
            wrong.is_empty(),
            "{} ({}), {} wrong:\n{}",
            path.display(),
            own.name(),
            wrong.len(),
            wrong.join("\n")
        );
    }
}

/// On every [`executables`] entry, the anchors of its side carried by their
/// patterns and then every engine route, with no table behind them, give
/// each entry its own table pins the value it pins; the attribute slots,
/// which the loader reads from the archive, the `.bytes` of a symbol whose
/// route checks its code and the [`NOT_NEEDED`] entries are left out.
#[test]
#[cfg_attr(miri, ignore = "reading a game executable takes hours under Miri")]
fn the_anchors_and_routes_find_every_pinned_entry_without_a_table() {
    let tables = tables();
    let routes = Routes::embedded().unwrap();
    for (path, side) in executables() {
        let bytes = std::fs::read(&path).unwrap();
        let image = Image::parse(&bytes, Layout::File, None).unwrap();
        let reflection = Reflection::scan(&image);
        let own = exact(&tables, side, image.fingerprint().unwrap()).unwrap();
        let text = image.range(image.section(".text").unwrap());
        let code = image.read(text.start, text.end - text.start).unwrap();
        let base = image.base();
        let mut found = crate::anchors(side)
            .unwrap()
            .fall_back(code, text.start - base, |at| image.u32(base + at));
        for (id, routed) in routes.find(&image, &reflection, side, &found.clone()) {
            found.route(id, routed);
        }
        let not_needed: &[&str] = NOT_NEEDED
            .iter()
            .find(|(s, _)| *s == side)
            .map_or(&[], |(_, ids)| ids);
        let pinned = own.resolve();
        let wrong: Vec<String> = pinned
            .symbols()
            .iter()
            .map(|(id, rva)| (id, *rva, found.symbol(id)))
            .chain(
                pinned
                    .constants()
                    .iter()
                    .filter(|(id, _)| pinned.attributes().iter().all(|(a, _)| a != id))
                    .filter(|(id, _)| id.strip_suffix(".bytes").is_none_or(|s| !found.routed(s)))
                    .map(|(id, value)| (id, *value, found.constant(id))),
            )
            .filter(|(id, want, have)| *have != Some(*want) && !not_needed.contains(&id.as_str()))
            .map(|(id, want, have)| format!("{id}: found {have:x?}, table {want:#x}"))
            .collect();
        assert!(
            wrong.is_empty(),
            "{} ({}), {} wrong:\n{}",
            path.display(),
            own.name(),
            wrong.len(),
            wrong.join("\n")
        );
    }
}
