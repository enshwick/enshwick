//! The `check` shapes of the engine routes, refreshed for a new build: each
//! route that finds a symbol, but for a start shape of two steps or more
//! over eight bytes, keeps a check that
//! still holds at the symbol on every build, or takes one made from their
//! code, so a hook needs no table row on the new build either.

use anyhow::Context as _;
use enshwick_gamedata::reflect::Image;
use enshwick_gamedata::{Resolved, check_holds, make_check, shape_checks};
use toml_edit::{Array, DocumentMut, Item, Table, Value};

use super::rows::Side;

/// The routes file, relative to the repository.
pub(crate) const ROUTES: &str = "crates/enshwick-gamedata/anchors/routes.toml";

/// The takes of a route that finds a constant, which has no code to check.
const CONSTANT_TAKES: [&str; 2] = ["value", "cell"];

/// Refreshes the checks in the routes file `text` for `side`, over `builds`
/// as each build's image and table: a check that holds at the symbol on
/// every build that pins it stays; otherwise a row of `side` alone takes the
/// check made from their code, or loses its check when none holds. The new
/// text and a note per route left without a check.
pub(crate) fn refresh(
    text: &str,
    side: Side,
    builds: &[(&Image<'_>, &Resolved)],
) -> anyhow::Result<(String, Vec<String>)> {
    let mut doc: DocumentMut = text.parse().context("routes file")?;
    let mut notes = Vec::new();
    let rows = doc
        .get_mut("route")
        .and_then(Item::as_array_of_tables_mut)
        .context("routes file has no [[route]]")?;
    for row in rows.iter_mut() {
        let Some(id) = row.get("id").and_then(Item::as_str).map(str::to_owned) else {
            continue;
        };
        let Some(alone) = sides(row, side) else {
            continue;
        };
        if !finds_symbol(row, &id) {
            continue;
        }
        let code: Vec<(&Image<'_>, u64)> = builds
            .iter()
            .filter_map(|&(image, table)| Some((image, table.symbol(&id)?)))
            .filter(|&(image, rva)| in_code(image, rva))
            .collect();
        if code.is_empty() {
            continue;
        }
        let shape: Option<Vec<String>> = row.get("shape").and_then(Item::as_array).map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        });
        let take = row.get("take").and_then(Item::as_str).unwrap_or("start");
        if take == "start"
            && shape.is_some_and(|shape| {
                code.iter()
                    .all(|&(image, rva)| shape_checks(&shape, image, rva))
            })
        {
            row.remove("check");
            continue;
        }
        let current: Option<Vec<String>> = row.get("check").and_then(Item::as_array).map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        });
        if current.as_ref().is_some_and(|check| {
            code.iter()
                .all(|&(image, rva)| check_holds(check, image, rva))
        }) {
            continue;
        }
        if !alone {
            if current.is_some() {
                notes.push(format!(
                    "{id}: its check fails and its row serves both sides; split the row by hand"
                ));
            }
            continue;
        }
        if let Some(check) = make_check(&code) {
            let check: Array = check.iter().collect();
            row.insert("check", Item::Value(Value::Array(check)));
        } else {
            row.remove("check");
            notes.push(format!(
                "{id}: no check shape holds on every build; its table's checks stay its hook's"
            ));
        }
    }
    Ok((doc.to_string(), notes))
}

/// Whether `rva` lies in `image`'s code section; a data symbol has no code
/// to check.
fn in_code(image: &Image<'_>, rva: u64) -> bool {
    image
        .section(".text")
        .is_some_and(|text| image.range(text).contains(&(image.base() + rva)))
}

/// Whether the row serves `side`, and whether it serves it alone; `None`
/// when it does not serve it.
fn sides(row: &Table, side: Side) -> Option<bool> {
    let name = match side {
        Side::Server => "server",
        Side::Client => "client",
    };
    let Some(sides) = row.get("sides").and_then(Item::as_array) else {
        return Some(false);
    };
    let names: Vec<&str> = sides.iter().filter_map(Value::as_str).collect();
    names.contains(&name).then_some(names.len() == 1)
}

/// Whether the row finds an entry's symbol, not a constant or a waypoint.
fn finds_symbol(row: &Table, id: &str) -> bool {
    let take = row.get("take").and_then(Item::as_str).unwrap_or("start");
    let waypoint = id.starts_with('~') || !id.contains('.');
    !waypoint && !CONSTANT_TAKES.contains(&take)
}

#[cfg(test)]
mod tests {
    use enshwick_gamedata::Table;
    use enshwick_gamedata::reflect::{Image, Layout};

    use super::{Side, refresh};

    /// A PE32+ file of one `.text` section at 0x1000 holding `code`.
    fn image_bytes(code: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0xccu8; 0x2000];
        bytes[..0x200].fill(0);
        bytes[..2].copy_from_slice(b"MZ");
        bytes[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
        bytes[0x86..0x88].copy_from_slice(&1u16.to_le_bytes());
        bytes[0x94..0x96].copy_from_slice(&0xf0u16.to_le_bytes());
        bytes[0x98..0x9a].copy_from_slice(&0x20bu16.to_le_bytes());
        bytes[0x98 + 24..0x98 + 32].copy_from_slice(&0x1_4000_0000u64.to_le_bytes());
        let header = 0x98 + 0xf0;
        bytes[header..header + 8].copy_from_slice(b".text\0\0\0");
        for (field, value) in [(8, 0x1000u32), (12, 0x1000), (16, 0x1000), (20, 0x1000)] {
            bytes[header + field..header + field + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[0x1000..0x1000 + code.len()].copy_from_slice(code);
        bytes
    }

    const ROUTES: &str = r#"
[[route]]
id = "a.kept"
sides = ["server"]
loads = ["k"]
check = ["mov [rsp+?],rbx", "push rdi", "sub rsp,?"]

[[route]]
id = "a.stale"
sides = ["server"]
loads = ["s"]
check = ["push rbx"]

[[route]]
id = "a.new"
sides = ["server"]
loads = ["n"]

[[route]]
id = "a.shared"
loads = ["h"]
check = ["push rbx"]

[[route]]
id = "a.start"
sides = ["server"]
in = "x"
shape = ["push rdi"]

[[route]]
id = "a.client"
sides = ["client"]
loads = ["c"]

[[route]]
id = "a.data"
sides = ["server"]
loads = ["d"]
"#;

    #[test]
    fn a_check_that_still_holds_stays_a_stale_one_is_made_again_and_data_gets_none() {
        // `mov [rsp+0x8],rbx; push rdi; sub rsp,0x20`, then 0x30 on the new build.
        let old = image_bytes(&[0x48, 0x89, 0x5c, 0x24, 0x08, 0x57, 0x48, 0x83, 0xec, 0x20]);
        let new = image_bytes(&[0x48, 0x89, 0x5c, 0x24, 0x08, 0x57, 0x48, 0x83, 0xec, 0x30]);
        let old = Image::parse(&old, Layout::File, None).unwrap();
        let new = Image::parse(&new, Layout::File, None).unwrap();
        let table = Table::parse(
            "name = \"t\"\nside = \"server\"\nguid = \"00000000000000000000000000000001\"\nage = 1\n[symbols]\n\"a.kept\" = 0x1000\n\"a.stale\" = 0x1000\n\"a.new\" = 0x1000\n\"a.shared\" = 0x1000\n\"a.start\" = 0x1000\n\"a.client\" = 0x1000\n\"a.data\" = 0x3000",
        )
        .unwrap()
        .resolve();
        let (text, notes) =
            refresh(ROUTES, Side::Server, &[(&old, &table), (&new, &table)]).unwrap();
        let routes = enshwick_gamedata::Routes::parse(&text).unwrap();
        assert_eq!(routes.ids().count(), 7);
        let doc: toml_edit::DocumentMut = text.parse().unwrap();
        let check = |i: usize| doc["route"][i].get("check").map(ToString::to_string);
        let made = Some(r#" ["mov [rsp+0x8],rbx", "push rdi", "sub rsp,?"]"#.to_owned());
        assert_eq!(
            check(0).as_deref(),
            Some(r#" ["mov [rsp+?],rbx", "push rdi", "sub rsp,?"]"#)
        );
        assert_eq!(check(1), made);
        assert_eq!(check(2), made);
        assert_eq!(check(3).as_deref(), Some(r#" ["push rbx"]"#));
        assert_eq!(check(4), made);
        assert_eq!(check(5), None);
        assert_eq!(check(6), None);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].starts_with("a.shared"));
    }
}
