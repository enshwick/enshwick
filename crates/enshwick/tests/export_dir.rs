//! A mod's export folder takes its id, whatever its folder in `mods/` is called.

#[cfg(test)]
mod tests {
    use std::path::Path;

    use enshwick::abi::{Header, ModInfo, Side, Status, Str};
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    struct Exporter;

    impl Mod for Exporter {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            match load.export_dir() {
                Ok(dir) => enshwick::info!("export {}", dir.display()),
                Err(why) => enshwick::info!("no export: {why}"),
            }
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Exporter,
        r#"
        id = "you.exporter"
        side = "server"
        "#
    );

    fn info(id: &'static str, file: &'static str) -> ModInfo {
        ModInfo {
            header: Header::of::<ModInfo>(),
            id: Str::new(id),
            version: Str::new("1.0.0"),
            side: Side::SERVER,
            file: Str::new(file),
            ..ModInfo::EMPTY
        }
    }

    /// What the mod logged about its export folder at init.
    fn answer(host: FakeHost) -> String {
        let loader = host
            .loaded(info(enshwick::mods::LOADER_ID, enshwick::mods::LOADER_FILE))
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Exporter)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
            .logs()
            .into_iter()
            .map(|(_, line)| line)
            .find(|line| line.starts_with("export ") || line.starts_with("no export: "))
            .unwrap_or_default()
    }

    #[test]
    fn the_export_folder_takes_the_mod_id_not_the_name_of_its_folder() {
        let answer = answer(
            FakeHost::new()
                .mod_dir("game/mods/Exporter 1.2")
                .loaded(info("you.other", "mods/Other/other.dll"))
                .loaded(info("you.exporter", "mods/Exporter 1.2/exporter.dll")),
        );
        let named = Path::new("game").join("export").join("you.exporter");
        assert_eq!(answer, format!("export {}", named.display()));
    }

    #[test]
    fn a_folder_the_loader_lists_no_mod_in_has_no_export_folder() {
        let answer = answer(
            FakeHost::new()
                .mod_dir("game/mods/Exporter")
                .loaded(info("you.exporter", "mods/Exporter-old/exporter.dll")),
        );
        assert_eq!(
            answer,
            "no export: export folder: the loader lists no mod in mods/Exporter"
        );
    }

    #[test]
    fn a_mod_folder_outside_mods_has_no_export_folder() {
        let answer = answer(
            FakeHost::new()
                .mod_dir("dev/you.exporter")
                .loaded(info("you.exporter", "mods/you.exporter/exporter.dll")),
        );
        assert!(answer.starts_with("no export: export folder: "), "{answer}");
        assert!(answer.ends_with("is no folder in mods/"), "{answer}");
    }
}
