//! `enshwick.i18n`: a script's texts as the object `Texts`, read, filled and
//! merged as a Rust mod's are, and handed to the loader while it starts.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::test::FakeHost;

    use crate::harness::{logged, run, start};

    #[test]
    fn a_script_fills_its_texts_in_a_language_with_english_as_the_fallback() {
        let loader = run(
            FakeHost::new(),
            r#"
            local texts = enshwick.i18n.texts_from('[en]\ngreeting = "Hello {name}"\nbye = "Bye"\n[uk]\ngreeting = "Привіт {name}"\n')
            enshwick.log.info(`uk {texts:fill("greeting", "uk", { { "name", "Ash" } })}`)
            enshwick.log.info(`fallback {texts:get("bye", "uk")}`)
            enshwick.log.info(`unknown {texts:fill("nothing", "en", {})}`)
            enshwick.log.info(`languages {table.concat(texts:languages(), ",")}`)
            "#,
        );
        assert!(logged(&loader, "uk Привіт Ash"), "{:?}", loader.logs());
        assert!(logged(&loader, "fallback Bye"));
        assert!(logged(&loader, "unknown nothing"));
        assert!(logged(&loader, "languages en,uk"));
    }

    #[test]
    fn a_script_reads_a_player_s_language_and_the_fallback_for_others() {
        use enshwick::PlayerKey;
        use enshwick::i18n::Language;

        let loader = run(
            FakeHost::new()
                .world()
                .player_language(PlayerKey::new(7), Language::UK_UA),
            r#"
            enshwick.log.info(`seven {enshwick.world.language_of("7")}`)
            enshwick.log.info(`eight {enshwick.world.language_of("8")}`)
            "#,
        );
        assert!(logged(&loader, "seven uk"), "{:?}", loader.logs());
        assert!(logged(&loader, "eight en"), "{:?}", loader.logs());
    }

    #[test]
    fn merging_returns_new_texts_and_names_each_key_a_language_lacks() {
        let loader = run(
            FakeHost::new(),
            r#"
            local texts = enshwick.i18n.parse('[en]\na = "A"\n[uk]\na = "А"\n')
            local merged = texts:merge(enshwick.i18n.parse('[en]\nb = "B"\n'))
            local missing = merged:missing_in({ "uk" })
            enshwick.log.info(`missing {#missing} {missing[1][1]} {missing[1][2]}`)
            local untranslated = merged:untranslated()
            enshwick.log.info(`untranslated {#untranslated} {untranslated[1][1]} {untranslated[1][2]}`)
            enshwick.log.info(`toml {enshwick.i18n.parse(merged:to_toml()):get("b", "en")}`)
            enshwick.log.info(`kept {texts:get("b", "en")}`)
            "#,
        );
        assert!(logged(&loader, "missing 1 uk b"), "{:?}", loader.logs());
        assert!(
            logged(&loader, "untranslated 1 uk b"),
            "{:?}",
            loader.logs()
        );
        assert!(logged(&loader, "toml B"));
        assert!(logged(&loader, "kept nil"));
    }

    #[test]
    fn texts_that_do_not_parse_raise_an_error_naming_the_call() {
        let loader = run(
            FakeHost::new(),
            r#"
            local ok, why = pcall(enshwick.i18n.parse, '[xx]\na = "b"\n')
            enshwick.log.info(`refused {not ok} {why}`)
            "#,
        );
        assert!(logged(&loader, "refused true"), "{:?}", loader.logs());
        assert!(logged(&loader, "enshwick.i18n.parse"));
        assert!(logged(&loader, "unknown language"));
    }

    #[test]
    fn a_text_key_without_any_text_refuses_the_start() {
        let (loader, status) = start(
            FakeHost::new(),
            r#"
            local texts = enshwick.i18n.texts_from('[en]\na = "A"\n')
            enshwick.i18n.add_texts(texts, { { "missing", "you.luau:tag" } })
            "#,
        );
        assert_ne!(status, Status::OK);
        assert!(logged(&loader, "no text for"), "{:?}", loader.logs());
    }

    #[test]
    fn a_script_reads_the_player_s_language_as_its_code() {
        let loader = run(
            FakeHost::new(),
            "enshwick.log.info(`language {enshwick.i18n.language()}`)",
        );
        assert!(logged(&loader, "language en"), "{:?}", loader.logs());
    }

    #[test]
    fn a_script_s_texts_are_the_texts_toml_of_its_folder() {
        let dir = std::env::temp_dir().join(format!("enshwick-luau-texts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("texts.toml"), "[en]\nhello = \"Hello\"\n").unwrap();
        let loader = run(
            FakeHost::new().mod_dir(dir.to_str().unwrap()),
            r#"enshwick.log.info(`text {enshwick.i18n.texts():get("hello", "uk")}`)"#,
        );
        let _cleaned = std::fs::remove_dir_all(&dir);
        assert!(logged(&loader, "text Hello"), "{:?}", loader.logs());
    }
}
