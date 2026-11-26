//! The lore categories `enshwick::content::journal::lore_category` names are
//! the ones the pages of the real journal are listed under, on both sides.
//! The archives sit beside the executables `ENSH_SERVER_EXE` and
//! `ENSH_CLIENT_EXE` name; the tests are ignored by default.

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use enshwick::content::journal::lore_category;
    use enshwick::resources::Value;
    use enshwick_gamedata::archive::Archive;
    use enshwick_gamedata::reflect::{Image, Layout, Reflection};

    const JOURNAL: &str = "keen::JournalRegistryResource";

    fn key(page: &Value) -> Option<u32> {
        match page.field("loreCategory")?.field("value")? {
            Value::Uint(key) => u32::try_from(*key).ok(),
            _ => None,
        }
    }

    /// The categories the pages of the journal beside `var`'s executable
    /// name, the tutorial pages' 0 left out.
    fn held(var: &str) -> BTreeSet<u32> {
        let exe = std::env::var(var).unwrap_or_else(|_| panic!("{var} names the game executable"));
        let exe = Path::new(&exe);
        let bytes = std::fs::read(exe).unwrap();
        let image = Image::parse(&bytes, Layout::File, None).unwrap();
        let registry = Reflection::scan(&image).registry();
        let stem = exe.file_stem().unwrap().to_str().unwrap();
        let archive = Archive::open(exe.parent().unwrap(), stem).unwrap();
        let ty = registry.named(JOURNAL).unwrap();
        let journals = archive.keys_of(ty.qualified_hash());
        assert_eq!(journals.len(), 1, "one journal");
        let journal = registry
            .decode(JOURNAL, &archive.resource(&journals[0]).unwrap())
            .unwrap();
        let Some(Value::List(pages)) = journal.field("collections") else {
            panic!("no collections")
        };
        pages.iter().filter_map(key).filter(|&k| k != 0).collect()
    }

    fn named() -> BTreeSet<u32> {
        lore_category::ALL.iter().map(|tag| tag.fnv1a()).collect()
    }

    #[test]
    fn every_named_category_is_a_distinct_one() {
        assert_eq!(named().len(), lore_category::ALL.len());
    }

    #[test]
    #[ignore = "needs ENSH_SERVER_EXE"]
    fn the_server_s_journal_lists_its_pages_under_the_named_categories() {
        assert_eq!(held("ENSH_SERVER_EXE"), named());
    }

    #[test]
    #[ignore = "needs ENSH_CLIENT_EXE"]
    fn the_client_s_journal_lists_its_pages_under_the_named_categories() {
        assert_eq!(held("ENSH_CLIENT_EXE"), named());
    }
}
