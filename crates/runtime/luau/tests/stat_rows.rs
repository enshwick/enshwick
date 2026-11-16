//! `enshwick.stats`: a script's stat kind keeps the group of the Character
//! screen it shows in, as a Rust mod's does, and a kind without one shows
//! no row.

#[cfg(test)]
mod tests {
    use enshwick::test::FakeHost;

    use crate::harness::{logged, run};

    #[test]
    fn a_script_s_kind_keeps_its_group_and_a_kind_without_one_has_none() {
        let loader = run(
            FakeHost::new(),
            r#"
            local luck = enshwick.stats.define_stat("you.luau:luck", {
                ty = "Additive", default = 0, min = 0, max = 1, format = "Percent", group = "Primary",
            })
            local plain = enshwick.stats.define_stat("you.luau:plain", {
                ty = "Additive", default = 0, min = 0, max = 1, format = "Number",
            })
            local stats = enshwick.stats.stats()
            local shown = stats:info(luck).def
            local unshown = stats:info(plain).def
            enshwick.log.info(`luck {shown.group} {shown.hidden}, plain {unshown.group}`)
            enshwick.log.info(`described by {enshwick.stats.description_key("you.luau:luck")}`)
            "#,
        );
        assert!(
            logged(&loader, "luck Primary false, plain nil"),
            "{:?}",
            loader.logs()
        );
        assert!(logged(&loader, "described by luck.description"));
    }

    #[test]
    fn a_script_s_folder_texts_name_its_kinds_without_a_call_for_them() {
        use enshwick::i18n::{Language, Texts};

        let dir = std::env::temp_dir().join(format!("enshwick-luau-rows-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("texts.toml"), "[en]\n\"luck.name\" = \"Luck\"\n").unwrap();
        let loader = run(
            FakeHost::new().mod_dir(dir.to_str().unwrap()),
            r#"enshwick.stats.define_stat("you.luau:luck", {
                ty = "Additive", default = 0, min = 0, max = 1, format = "Percent", group = "Primary",
            })"#,
        );
        let _cleaned = std::fs::remove_dir_all(&dir);
        let handed = Texts::parse(&loader.published_texts().unwrap()).unwrap();
        assert_eq!(handed.get("luck.name", Language::EN_US), Some("Luck"));
    }
}
