//! `enshwick.deps`: the mods a Luau mod's `pack.toml` names by alias.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{ModInfo, Status, Str};
    use enshwick::packs::PackManifest;
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Load, Mod, Off};
    use enshwick_luau::{Limits, Scripts};

    const MANIFEST: &str = r#"
        id = "you.luau"
        version = "1.0.0"

        [requires]
        lockers = "you.lockers"

        [recommends]
        lanterns = { id = "you.lanterns", version = ">=0.2" }

        [breaks]
        old = "old.luau"
    "#;

    static MAIN: Mutex<Option<String>> = Mutex::new(None);

    struct Luau;

    impl Mod for Luau {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let main = MAIN.lock().unwrap().take().unwrap();
            let manifest = PackManifest::parse(MANIFEST).unwrap();
            let scripts = Scripts::new().with("main.luau", main);
            let _runtime = enshwick_luau::start_pack(load, &manifest, scripts, Limits::default())?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Luau,
        r#"
        id = "you.luau"
        side = "server"
        "#
    );

    fn loaded(id: &'static str) -> ModInfo {
        ModInfo {
            id: Str::new(id),
            ..ModInfo::EMPTY
        }
    }

    fn run(main: &str, host: FakeHost) -> Installed {
        *MAIN.lock().unwrap() = Some(main.to_owned());
        let loader = host.loaded(loaded("you.luau")).install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Luau)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    fn logged(loader: &Installed, text: &str) -> bool {
        loader.logs().iter().any(|(_, line)| line.contains(text))
    }

    #[test]
    fn deps_names_each_dependency_by_its_alias_but_those_it_breaks() {
        let loader = run(
            r"print(`lockers {enshwick.deps.lockers.id}`)
            print(`lanterns {enshwick.deps.lanterns.id}`)
            print(`old {enshwick.deps.old}`)",
            FakeHost::new(),
        );
        assert!(
            logged(&loader, "lockers you.lockers"),
            "{:?}",
            loader.logs()
        );
        assert!(
            logged(&loader, "lanterns you.lanterns"),
            "{:?}",
            loader.logs()
        );
        assert!(logged(&loader, "old nil"), "{:?}", loader.logs());
    }

    #[test]
    fn running_says_whether_an_optional_dependency_runs() {
        let main = r"print(`lanterns run {enshwick.deps.lanterns.running()}`)";
        let alone = run(main, FakeHost::new());
        assert!(logged(&alone, "lanterns run false"), "{:?}", alone.logs());
    }

    #[test]
    fn running_is_true_beside_a_running_optional_dependency() {
        let main = r"print(`lanterns run {enshwick.deps.lanterns.running()}`)";
        let beside = run(main, FakeHost::new().loaded(loaded("you.lanterns")));
        assert!(logged(&beside, "lanterns run true"), "{:?}", beside.logs());
    }

    #[test]
    fn deps_is_read_only() {
        let loader = run(
            r"local ok = pcall(function() enshwick.deps.lockers = nil end)
            print(`changed {ok}`)",
            FakeHost::new(),
        );
        assert!(logged(&loader, "changed false"), "{:?}", loader.logs());
    }
}
