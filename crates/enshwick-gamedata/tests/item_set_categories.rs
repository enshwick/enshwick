//! The item set categories `enshwick::content::journal::item_set_category`
//! names are the ones the real journal lists its item sets under, on both
//! sides. The archives sit beside the executables `ENSH_SERVER_EXE` and
//! `ENSH_CLIENT_EXE` name; the tests are ignored by default.

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use enshwick::content::journal::item_set_category;
    use enshwick::resources::Value;
    use enshwick_gamedata::archive::Archive;
    use enshwick_gamedata::reflect::{Image, Layout, Reflection};

    const JOURNAL: &str = "keen::JournalRegistryResource";

    fn key(category: &Value) -> Option<u32> {
        match category.field("categoryName")?.field("value")? {
            Value::Uint(key) => u32::try_from(*key).ok(),
            _ => None,
        }
    }

    /// The categories of the journal beside `var`'s executable; `None`
    /// without the executable.
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
        let Some(Value::List(categories)) = journal.field("itemSetCategories") else {
            panic!("no item set categories")
        };
        let held: BTreeSet<u32> = categories.iter().filter_map(key).collect();
        assert_eq!(held.len(), categories.len(), "every category has a key");
        held
    }

    fn named() -> BTreeSet<u32> {
        item_set_category::ALL
            .iter()
            .map(|category| category.get())
            .collect()
    }

    #[test]
    fn every_named_item_set_category_is_a_distinct_one() {
        assert_eq!(named().len(), item_set_category::ALL.len());
    }

    #[test]
    #[ignore = "needs ENSH_SERVER_EXE"]
    fn the_server_s_journal_lists_its_item_sets_under_the_named_categories() {
        assert_eq!(held("ENSH_SERVER_EXE"), named());
    }

    #[test]
    #[ignore = "needs ENSH_CLIENT_EXE"]
    fn the_client_s_journal_lists_its_item_sets_under_the_named_categories() {
        assert_eq!(held("ENSH_CLIENT_EXE"), named());
    }
}
