//! A mod finds a loaded mod by its place in load order.

#[cfg(test)]
mod tests {
    use enshwick::abi::{Header, ManifestFlags, ModInfo, Side, Status, Str};
    use enshwick::mods::ModIndex;
    use enshwick::{Load, Mod, Off};

    struct Finder;

    impl Mod for Finder {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.finder", side: SERVER)
        }

        fn init(_load: &mut Load<'_>) -> Result<Self, Off> {
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Finder);

    fn info(id: &'static str) -> ModInfo {
        ModInfo {
            header: Header::of::<ModInfo>(),
            id: Str::new(id),
            version: Str::new("1.0.0"),
            side: Side::BOTH,
            flags: ManifestFlags::NONE,
            off: 0,
            file: Str::new("mods/any.dll"),
            ..ModInfo::EMPTY
        }
    }

    #[test]
    fn mods_after_the_loader_count_from_zero_and_resolve_back() {
        let loader = enshwick::test::FakeHost::new()
            .loaded(info(enshwick::mods::LOADER_ID))
            .loaded(info("you.finder"))
            .loaded(info("you.second"))
            .join_missing(info("you.absent"))
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Finder)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);

        let indices: Vec<Option<ModIndex>> = enshwick::mods::all()
            .iter()
            .map(|entry| entry.index)
            .collect();
        assert_eq!(indices, [None, Some(ModIndex(0)), Some(ModIndex(1))]);
        assert_eq!(
            ModIndex(1).entry().map(|entry| entry.id),
            Some("you.second".to_owned())
        );
        assert_eq!(ModIndex(2).entry(), None);
        assert_eq!(ModIndex(1).to_string(), "1");
        assert!(
            enshwick::mods::join_missing()
                .iter()
                .all(|entry| entry.index.is_none())
        );
    }
}
