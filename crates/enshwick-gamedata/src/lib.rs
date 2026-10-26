//! The loader's build tables: per game build, the engine functions and
//! constants that reflection does not describe, by symbolic id.
//!
//! A table is a TOML file named after the build's fingerprint (see
//! [`file_name`]). The loader embeds the tables of `tables/` ([`embedded`]).
//! For the running build it takes the table
//! made for it ([`exact`], [`Table::resolve`]); on a build without one, the
//! newest table of the same side ([`newest`]) finds each entry by its pattern
//! ([`Table::fall_back`]), and what no pattern finds stays missing. The
//! game's own archive reads with [`archive::Archive`].

pub mod archive;
mod entries;
mod resolve;
mod table;
mod version;

pub use entries::{Constant, Locate, Operand, Symbol};
pub use resolve::{Kind, Resolved, embedded, exact, newest};
pub use table::{Table, TableError, file_name};
pub use version::{GameVersion, SteamBuild};

pub use enshwick_reflect::Fingerprint;

/// The binary a table is for.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::Side;
///
/// assert_eq!(Side::Server.as_str(), "server");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    /// The dedicated server.
    Server,
    /// The game client.
    Client,
}

impl Side {
    /// The side as table files spell it.
    ///
    /// # Examples
    ///
    /// See [`Side`].
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Client => "client",
        }
    }
}

impl std::fmt::Display for SteamBuild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Display for GameVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::{expected_bytes, search};
    use enshwick::code::Pattern;

    #[test]
    fn a_panicking_search_keeps_the_other_entries_in_place() {
        let entries: Vec<u32> = (0..100).collect();
        let found = search(&entries, |&n| {
            assert!(n != 3, "a broken pattern");
            Some(n)
        });
        assert_eq!(found.len(), entries.len());
        assert_eq!(found[3], None);
        assert_eq!(found[99], Some(99));
    }

    fn tables() -> Vec<Table> {
        embedded()
            .iter()
            .map(|(file, text)| {
                let table = Table::parse(text).unwrap_or_else(|e| panic!("{file}: {e}"));
                assert_eq!(table.file_name(), *file);
                table
            })
            .collect()
    }

    #[test]
    fn every_embedded_table_reads_and_is_named_after_its_build() {
        let tables = tables();
        assert!(tables.iter().any(|t| t.side() == Side::Server));
        assert!(tables.iter().any(|t| t.side() == Side::Client));
        for table in &tables {
            assert!(table.symbols().iter().all(|s| s.rva() != 0));
            assert!(!table.checks().is_empty(), "{}", table.name());
        }
    }

    #[test]
    fn the_client_table_carries_every_server_constant_and_symbol() {
        let tables = tables();
        let server = newest(&tables, Side::Server).unwrap().resolve();
        let client = newest(&tables, Side::Client).unwrap().resolve();
        for (id, value) in server.constants() {
            let expected = match id.as_str() {
                // The client's weather instances carry 8 bytes more.
                "weather.entry_size" => 0x50,
                // The client's session holds more before its host machine.
                "session.host_machine" => 0x1_1a98,
                _ => *value,
            };
            assert_eq!(client.constant(id), Some(expected), "{id}");
        }
        for (id, _) in server.symbols() {
            assert!(client.symbol(id).is_some(), "{id}");
        }
        assert_eq!(server.symbol("npc.lookup"), Some(0x20_3d30));
        assert_eq!(client.symbol("npc.lookup"), Some(0x46_9c30));
    }

    const TABLE: &str = r#"
        name = "test (2)"
        side = "server"
        guid = "000102030405060708090a0b0c0d0e0f"
        age = 7
        steam_build = 2
        game_version = 9
        [checks]
        "a_system" = 0x10
        [symbols]
        "f.plain" = 0x1000
        "f.found" = { rva = 0x1004, pattern = "11 22 ?? 44", at = 1 }
        "f.twice" = { rva = 0x1010, pattern = "AA" }
        "f.changed" = { rva = 0x1020, pattern = "11 22 ?? 44", at = 1 }
        "f.called" = { rva = 0x1030, pattern = "E8 ?? ?? ?? ?? C3", rel = 1 }
        [constants]
        "c.plain" = 8
        "c.used" = { value = 0x48, pattern = "8B 48 ?? 48" }
        "c.read" = { value = 0x48, pattern = "8B 48 ?? 48", at = 2, size = 1 }
        "c.wide" = { value = 0x1234, pattern = "B9 ?? ?? ?? ?? 8B 41 07", at = 1, size = 4 }
        "c.gone" = { value = 0x50, pattern = "8B 50" }
        "c.below" = { value = 0x60, pattern = "48 8D 55 ??", at = 3, size = 1, neg = true }
        "c.after" = { value = 0x50, pattern = "8B 48 ?? 48", at = 2, size = 1, plus = 8 }
        "f.found.bytes" = 0x0807_0605_3344_7722
        "f.changed.bytes" = "0x9111_1111_1111_1111"
    "#;

    #[test]
    fn a_table_parses_with_plain_and_patterned_entries() {
        let table = Table::parse(TABLE).unwrap();
        assert_eq!(table.fingerprint().age(), 7);
        assert_eq!(table.fingerprint().guid()[15], 0x0f);
        assert_eq!(table.steam_build(), Some(SteamBuild::new(2)));
        assert_eq!(table.game_version(), Some(GameVersion::new(9)));
        assert_eq!(table.revision(), 1);
        let resolved = table.resolve();
        assert_eq!(resolved.kind(), Kind::Exact);
        assert_eq!(resolved.symbol("f.plain"), Some(0x1000));
        assert_eq!(resolved.constant("c.used"), Some(0x48));
        assert_eq!(resolved.checks(), [("a_system".to_owned(), 0x10)]);
        assert!(resolved.missing().is_empty());
    }

    #[test]
    fn a_fallback_keeps_only_what_its_patterns_find() {
        let table = Table::parse(TABLE).unwrap();
        let mut code = vec![0u8; 64];
        // f.found: 11 22 ?? 44 at 8, then the bytes it pins from at = 1 on.
        code[8..17].copy_from_slice(&[0x11, 0x22, 0x99, 0x44, 0x33, 0x05, 0x06, 0x07, 0x08]);
        code[20] = 0xaa;
        code[30] = 0xaa;
        code[40..44].copy_from_slice(&[0x8b, 0x48, 0x01, 0x48]);
        let resolved = table.fall_back(&code, 0x5000);
        assert_eq!(resolved.kind(), Kind::Fallback);
        assert!(resolved.checks().is_empty());
        assert_eq!(resolved.symbol("f.found"), Some(0x5000 + 9));
        assert_eq!(resolved.symbol("f.plain"), None);
        assert_eq!(resolved.symbol("f.twice"), None);
        assert_eq!(resolved.constant("c.used"), Some(0x48));
        assert_eq!(resolved.constant("c.read"), Some(0x01), "read, not pinned");
        assert_eq!(resolved.constant("c.after"), Some(0x09));
        assert_eq!(resolved.constant("c.plain"), None);
        assert_eq!(resolved.constant("c.gone"), None);
        assert_eq!(
            resolved.missing(),
            [
                "c.below",
                "c.gone",
                "c.plain",
                "c.wide",
                "f.called",
                "f.changed",
                "f.changed.bytes",
                "f.found.bytes",
                "f.plain",
                "f.twice"
            ]
        );
    }

    /// A constant read `within` a symbol's code counts where its pattern
    /// matches once in those bytes, wherever the code before it moved, and
    /// not where it matches twice or only past them.
    #[test]
    fn constants_within_a_symbol_are_read_where_they_match_once() {
        let table = Table::parse(
            r#"
            name = "1.0 server (1)"
            side = "server"
            guid = "00000000000000000000000000000001"
            age = 2
            [symbols]
            "g.body" = { rva = 0x5010, pattern = "55 48 89 E5" }
            [constants]
            "g.field" = { value = 0x48, via = "g.body", within = 0x20, pattern = "8B 41 ?? 90", at = 2, size = 1 }
            "g.twice" = { value = 0x10, via = "g.body", within = 0x20, pattern = "0F B6 ?? 0C", at = 2, size = 1 }
            "g.far" = { value = 0x10, via = "g.body", within = 0x8, pattern = "8B 41 ?? 90", at = 2, size = 1 }
            "#,
        )
        .unwrap();
        let mut code = vec![0xccu8; 64];
        code[0x10..0x14].copy_from_slice(&[0x55, 0x48, 0x89, 0xe5]);
        code[0x1a..0x1e].copy_from_slice(&[0x8b, 0x41, 0x58, 0x90]);
        code[0x20..0x24].copy_from_slice(&[0x0f, 0xb6, 0x01, 0x0c]);
        code[0x28..0x2c].copy_from_slice(&[0x0f, 0xb6, 0x02, 0x0c]);
        // Past the 0x20 bytes: a copy of the field's code in the next function.
        code[0x32..0x36].copy_from_slice(&[0x8b, 0x41, 0x60, 0x90]);
        let resolved = table.fall_back(&code, 0x5000);
        assert_eq!(resolved.constant("g.field"), Some(0x58));
        assert_eq!(resolved.missing(), ["g.far", "g.twice"]);
        assert!(
            Table::parse(
                r#"
                name = "x"
                side = "server"
                guid = "00000000000000000000000000000001"
                age = 2
                [constants]
                "x" = { value = 1, within = 8, pattern = "90" }
                "#
            )
            .is_err(),
            "`within` needs `via`"
        );
    }

    /// A constant read via a symbol counts where the symbol is found and
    /// its code matches; a slot of an attribute waits for the loader.
    #[test]
    fn constants_via_a_symbol_or_from_data_fall_back_as_they_say() {
        let table = Table::parse(
            r#"
            name = "1.0 server (1)"
            side = "server"
            guid = "00000000000000000000000000000001"
            age = 2
            [symbols]
            "g.get" = { rva = 0x5020, pattern = "E8 ?? ?? ?? ?? 0F 0B", rel = 1 }
            "g.lost" = { rva = 0x5030, pattern = "77 66 55" }
            [constants]
            "g.blob" = { value = 0x48, via = "g.get", pattern = "48 8B 41 ?? C3", at = 3, size = 1 }
            "g.other" = { value = 0x48, via = "g.lost", pattern = "48 8B 41 ?? C3", at = 3, size = 1 }
            "g.slot" = { value = 7, attribute = "Health_Max" }
            "#,
        )
        .unwrap();
        let mut code = vec![0xccu8; 64];
        // A call at 0 to 0x20, where the getter reads +0x50 now.
        code[..7].copy_from_slice(&[0xe8, 0x1b, 0, 0, 0, 0x0f, 0x0b]);
        code[0x20..0x25].copy_from_slice(&[0x48, 0x8b, 0x41, 0x50, 0xc3]);
        let mut resolved = table.fall_back(&code, 0x5000);
        assert_eq!(resolved.symbol("g.get"), Some(0x5020));
        assert_eq!(resolved.constant("g.blob"), Some(0x50));
        assert_eq!(resolved.missing(), ["g.lost", "g.other", "g.slot"]);
        assert_eq!(
            resolved.attributes(),
            [("g.slot".to_owned(), "Health_Max".to_owned())]
        );
        assert_eq!(resolved.supply("g.slot", 7), None);
        assert_eq!(resolved.supply("g.blob", 0x50), Some(0x50));
        assert_eq!(resolved.missing(), ["g.lost", "g.other"]);
        code[0x23] = 0x48;
        code[0x24] = 0x90;
        assert_eq!(table.fall_back(&code, 0x5000).constant("g.blob"), None);
        assert_eq!(table.resolve().constant("g.slot"), Some(7));
        for bad in [
            r#""x" = { value = 1, via = "g.get" }"#,
            r#""x" = { value = 1, attribute = "A", pattern = "11" }"#,
        ] {
            let text = format!(
                "name = \"n\"
side = \"server\"
guid = \"{}\"
age = 1
[constants]
{bad}
",
                "0".repeat(32)
            );
            assert!(Table::parse(&text).is_err(), "{bad}");
        }
    }

    /// A constant only its own build holds counts there, and a build the
    /// table is carried to neither finds nor misses it.
    #[test]
    fn a_constant_kept_from_carrying_is_neither_found_nor_missing() {
        let text = r#"
            name = "1.0 client (1)"
            side = "client"
            guid = "00000000000000000000000000000001"
            age = 2
            [constants]
            "g.slot" = { value = 16, carried = false }
            "g.plain" = 8
            "#;
        let table = Table::parse(text).unwrap();
        let carried = table.fall_back(&[0xcc; 16], 0x1000);
        assert_eq!(carried.constant("g.slot"), None);
        assert_eq!(carried.missing(), ["g.plain"]);
        assert_eq!(carried.uncarried(), ["g.slot"]);
        let exact = table.resolve();
        assert_eq!(exact.constant("g.slot"), Some(16));
        assert!(exact.uncarried().is_empty());
        for bad in [
            r#"{ value = 1, carried = false, pattern = "11" }"#,
            r#"{ value = 1, carried = false, attribute = "A" }"#,
        ] {
            let text = text.replace("{ value = 16, carried = false }", bad);
            assert!(Table::parse(&text).is_err(), "{bad}");
        }
    }

    /// The join's slot in the online provider's table is per build: a
    /// client table carried to any code leaves it out without missing it.
    #[test]
    fn a_carried_client_table_never_misses_the_join_slot() {
        let tables = tables();
        let clients: Vec<&Table> = tables.iter().filter(|t| t.side() == Side::Client).collect();
        assert!(!clients.is_empty());
        for table in clients {
            let carried = table.fall_back(&[0xcc; 64], 0x1000);
            let slot = "net.provider_join_slot";
            assert!(
                !carried.missing().iter().any(|id| id == slot),
                "{}",
                table.name()
            );
            assert!(
                carried.uncarried().iter().any(|id| id == slot),
                "{}",
                table.name()
            );
            assert_eq!(table.resolve().constant(slot), Some(16), "{}", table.name());
        }
    }

    #[test]
    fn expected_bytes_count_only_where_the_pattern_fixes_them() {
        let mut symbol = Symbol {
            id: "s".into(),
            rva: 0x10,
            pattern: Some(Pattern::parse("11 ?? 33").unwrap()),
            locate: Locate::At(0),
        };
        let code = [0x11, 0x99, 0x33, 4, 5, 6, 7, 8, 9];
        let want = u64::from_le_bytes([0x11, 0x22, 0x33, 4, 5, 6, 7, 8]);
        assert!(expected_bytes(&code, 0x10, 0x10, want, &symbol));
        let wrong = u64::from_le_bytes([0x11, 0x22, 0x33, 4, 5, 6, 7, 0]);
        assert!(!expected_bytes(&code, 0x10, 0x10, wrong, &symbol));
        symbol.locate = Locate::Rel { at: 0, end: 4 };
        assert!(
            !expected_bytes(&code, 0x10, 0x10, want, &symbol),
            "a target is compared outright"
        );
    }

    #[test]
    fn operands_read_the_value_from_the_code() {
        let table = Table::parse(TABLE).unwrap();
        // c.read: 8B 48 ?? 48, the disp8 read; c.wide: the imm32.
        let mut code = vec![0u8; 80];
        code[40..44].copy_from_slice(&[0x8b, 0x48, 0x52, 0x48]);
        code[48..56].copy_from_slice(&[0xb9, 0x34, 0x12, 0, 0, 0x8b, 0x41, 0x07]);
        // f.called: E8 rel32 at 56, calling 0x5000 + 61 - 0x10.
        code[56..61].copy_from_slice(&[0xe8, 0xf0, 0xff, 0xff, 0xff]);
        code[61] = 0xc3;
        let resolved = table.fall_back(&code, 0x5000);
        assert_eq!(resolved.constant("c.read"), Some(0x52));
        assert_eq!(resolved.constant("c.after"), Some(0x5a));
        assert_eq!(resolved.constant("c.wide"), Some(0x1234));
        assert_eq!(resolved.symbol("f.called"), Some(0x5000 + 61 - 0x10));
        code[62..66].copy_from_slice(&[0x48, 0x8d, 0x55, 0xa0]);
        let resolved = table.fall_back(&code, 0x5000);
        assert_eq!(
            resolved.constant("c.below"),
            Some(0x60),
            "lea rdx, [rbp - 0x60]"
        );
        let read = table
            .constants()
            .iter()
            .find(|c| c.id() == "c.read")
            .unwrap();
        assert_eq!(read.operand(), Operand::new(2, 1));
    }

    #[test]
    fn malformed_operands_are_refused() {
        for bad in [
            "{ value = 1, pattern = \"8B\", at = 1 }",
            "{ value = 1, pattern = \"8B\", at = 1, size = 3 }",
            "{ value = 1, pattern = \"8B\", neg = true }",
            "{ value = 1, at = 1, size = 1 }",
            "{ value = 1, pattern = \"8B\", plus = 1 }",
        ] {
            let text = TABLE.replace("\"c.plain\" = 8", &format!("\"c.plain\" = {bad}"));
            assert!(Table::parse(&text).is_err(), "{bad}");
        }
        let text = TABLE.replace("pattern = \"AA\" }", "pattern = \"AA\", end = 4 }");
        assert!(Table::parse(&text).is_err());
    }

    #[test]
    fn a_table_with_a_steam_build_is_newer_than_one_without() {
        let mut known = Table::parse(TABLE).unwrap();
        known.game_version = None;
        let mut unknown = known.clone();
        unknown.steam_build = None;
        unknown.game_version = Some(GameVersion::new(u32::MAX));
        unknown.fingerprint = Fingerprint::new([1; 16], 1);
        let tables = [unknown.clone(), known.clone()];
        assert_eq!(newest(&tables, Side::Server), Some(&known));
    }

    #[test]
    fn the_single_build_key_of_old_tables_is_refused() {
        let text = TABLE.replace("steam_build = 2", "build = 2");
        assert!(Table::parse(&text).is_err());
    }

    #[test]
    fn the_newest_embedded_tables_name_their_steam_builds() {
        let tables = tables();
        let steam = |side| newest(&tables, side).and_then(Table::steam_build);
        assert_eq!(steam(Side::Client), Some(SteamBuild::new(26_082_781)));
        assert_eq!(steam(Side::Server), Some(SteamBuild::new(26_083_076)));
    }

    #[test]
    fn the_newest_table_of_a_side_is_the_highest_build() {
        let old = Table::parse(TABLE).unwrap();
        let mut new = old.clone();
        new.steam_build = Some(SteamBuild::new(3));
        new.game_version = Some(GameVersion::new(8));
        new.fingerprint = Fingerprint::new([1; 16], 1);
        let tables = [old.clone(), new.clone()];
        assert_eq!(newest(&tables, Side::Server), Some(&new));
        assert_eq!(newest(&tables, Side::Client), None);
        assert_eq!(exact(&tables, Side::Server, old.fingerprint()), Some(&old));
        let mut revised = old.clone();
        revised.revision = 2;
        let tables = [old.clone(), revised.clone()];
        assert_eq!(
            exact(&tables, Side::Server, old.fingerprint()),
            Some(&revised)
        );
    }

    #[test]
    fn malformed_tables_are_refused() {
        assert!(matches!(Table::parse("name = 1"), Err(TableError::Toml(_))));
        let bad_guid = TABLE.replace("000102030405060708090a0b0c0d0e0f", "XYZ");
        assert!(matches!(Table::parse(&bad_guid), Err(TableError::Guid(_))));
        let bad_pattern = TABLE.replace("\"AA\"", "\"A\"");
        assert!(matches!(
            Table::parse(&bad_pattern),
            Err(TableError::Pattern { .. })
        ));
        let unknown = format!("extra = 1\n{TABLE}");
        assert!(Table::parse(&unknown).is_err());
    }

    /// On the executables `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` name (each
    /// skipped without it), the patterns of their own table find every
    /// patterned entry where the table pins it.
    #[test]
    fn patterns_find_their_entries_on_their_own_build() {
        for var in ["ENSH_SERVER_EXE", "ENSH_CLIENT_EXE"] {
            let Ok(path) = std::env::var(var) else {
                continue;
            };
            let bytes = std::fs::read(path).unwrap();
            let image =
                enshwick_reflect::Image::parse(&bytes, enshwick_reflect::Layout::File, None)
                    .unwrap();
            let fingerprint = image.fingerprint().unwrap();
            let tables = tables();
            let side = if var == "ENSH_SERVER_EXE" {
                Side::Server
            } else {
                Side::Client
            };
            let table = exact(&tables, side, fingerprint).unwrap();
            let text = image.section(".text").unwrap();
            let range = image.range(text);
            let code = image.read(range.start, range.end - range.start).unwrap();
            let code_rva = range.start - image.base();
            let mut wrong = Vec::new();
            let mut unique = |id: &str, pattern: &Pattern| {
                let count = pattern.find_all(code).take(2).count();
                if count != 1 {
                    wrong.push(format!("{id}: {count} matches"));
                }
            };
            for symbol in table.symbols() {
                if let Some((pattern, _)) = symbol.pattern() {
                    unique(symbol.id(), pattern);
                }
            }
            for constant in table.constants() {
                if let Some(pattern) = constant.pattern()
                    && constant.via().is_none()
                {
                    unique(constant.id(), pattern);
                }
            }
            for symbol in table.symbols() {
                let found = symbol.find(code, code_rva);
                if symbol.pattern().is_some() && found != Some(symbol.rva()) {
                    wrong.push(format!("{}: found at {found:x?}", symbol.id()));
                }
            }
            for constant in table.constants() {
                let found = table.find_pinned(constant, code, code_rva);
                if constant.pattern().is_some() && found != Some(constant.value()) {
                    wrong.push(format!("{}: reads {found:x?}", constant.id()));
                }
            }
            let resolved = table.fall_back(code, code_rva);
            assert!(wrong.is_empty(), "{var}:\n{}", wrong.join("\n"));
            assert!(
                !resolved
                    .missing()
                    .iter()
                    .any(|id| id == "net.provider_join_slot"),
                "{var}"
            );
            let (symbols, constants) = (resolved.symbols().len(), resolved.constants().len());
            let floor = match side {
                Side::Server => COVERED_SERVER,
                Side::Client => COVERED_CLIENT,
            };
            assert!(
                symbols >= floor.0 && constants >= floor.1,
                "{var}: patterns cover {symbols} symbols and {constants} constants, fewer than {floor:?}; missing {:?}",
                resolved.missing()
            );
        }
    }

    /// How many symbols and constants the patterns of each side's table find
    /// on its own build, at least; raised as patterns are added.
    const COVERED_SERVER: (usize, usize) = (121, 271);
    const COVERED_CLIENT: (usize, usize) = (183, 427);
}
