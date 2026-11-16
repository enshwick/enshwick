//! How stat kinds show through the fake loader: a kind's group of the
//! Character screen, a hidden kind and a description text cross the ABI and
//! read back as they were defined, and the mod's texts, which name its
//! rows, reach the loader as it starts, beneath the texts it hands itself.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::Status;
    use enshwick::content::id;
    use enshwick::content::items::StatFormat;
    use enshwick::i18n::{Language, Texts};
    use enshwick::stats::{Stat, StatDef, StatGroup, Stats, description_key};
    use enshwick::{Load, Mod, Off};

    static KIT: OnceLock<(Stats, Stat, Stat, Stat)> = OnceLock::new();

    struct Rpg;

    impl Mod for Rpg {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _renamed = load.texts_from("[en]\n\"luck.name\" = \"Fortune\"\n")?;
            let luck = StatDef::additive()
                .range(0.0, 1.0)
                .format(StatFormat::Percent)
                .in_group(StatGroup::Primary);
            let luck = load.define_stat(id!("luck"), &luck)?;
            let curse = load.define_stat(id!("curse"), &StatDef::flag().hidden())?;
            let plain = load.define_stat(id!("plain"), &StatDef::additive())?;
            let _first = KIT.set((load.stats()?, luck, curse, plain));
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Rpg,
        r#"
        id = "you.hello"
        side = "both"
        "#,
        texts = "tests/texts/stat_rows.toml",
    );

    #[test]
    fn a_kind_s_group_hidden_flag_and_description_read_back_as_defined() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Rpg)`.
        let started =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(started, Status::OK);
        let (stats, luck, curse, plain) = *KIT.get().unwrap();

        let luck = stats.info(luck).unwrap();
        assert_eq!(luck.def.group, Some(StatGroup::Primary));
        assert_eq!(luck.def.format, StatFormat::Percent);
        assert!(!luck.def.hidden);
        let described = description_key(&luck.id).unwrap();
        let handed = Texts::parse(&loader.published_texts().unwrap()).unwrap();
        assert_eq!(
            handed.get(&described, Language::UK_UA),
            Some("Щастить у бою й під час здобичі")
        );
        assert_eq!(
            handed.get("luck.name", Language::EN_US),
            Some("Fortune"),
            "a text the mod handed itself stays over its file's"
        );

        let curse = stats.info(curse).unwrap();
        assert!(curse.def.hidden, "shown nowhere");
        assert_eq!(curse.def.group, None);
        assert_eq!(
            stats.info(plain).unwrap().def.group,
            None,
            "no row by default"
        );
    }
}
