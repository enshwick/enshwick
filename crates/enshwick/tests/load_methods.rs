//! `Load` methods no other test calls, each through the fake host: a code
//! search, a status source, a text, a client game event, an icon and a
//! craft affix rule.

/// A mod that searches the fake's code for a pattern found once, none and
/// twice, and for a malformed one.
#[cfg(test)]
mod find_code {
    use std::sync::Mutex;

    use enshwick::abi::Status;
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    static CODE: [u8; 8] = [0x90, 0xb9, 0x00, 0x02, 0x90, 0xc3, 0x90, 0xc3];

    /// Each search's answer, an address or the refusal's text.
    static FOUND: Mutex<Vec<Result<u64, String>>> = Mutex::new(Vec::new());

    struct Search;

    impl Mod for Search {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let mut found = FOUND.lock().unwrap();
            for pattern in ["B9 ?? 02", "CC CC", "90 C3", "B9 0"] {
                found.push(load.find_code(pattern).map_err(|off| off.to_string()));
            }
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Search,
        r#"
        id = "you.search"
        side = "server"
        "#
    );

    #[test]
    fn a_pattern_found_once_gives_its_address_and_none_two_or_a_malformed_one_refuse() {
        let loader = FakeHost::new().code(&CODE).install();
        // SAFETY: the export of this module's `declare_mod!`.
        let status = unsafe { loader.init(enshwick_mod_init) };
        assert_eq!(status, Status::OK);
        let found = FOUND.lock().unwrap();
        assert_eq!(found[0], Ok(CODE.as_ptr().addr() as u64 + 1));
        assert!(found[1..].iter().all(Result::is_err), "{found:?}");
    }
}

/// A mod with a status source, a text and a client game event handler.
#[cfg(test)]
#[cfg(feature = "unstable")]
mod status_text_and_client_event {
    use std::sync::atomic::{AtomicU32, Ordering};

    use enshwick::abi::{Side, Status};
    use enshwick::content::Guid;
    use enshwick::i18n::Language;
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    const NAME: Guid = Guid::parse("b7c1d2e3-4f50-4a61-8b72-c3d4e5f60718");
    const OPENED: &str = "ecs::OpenHudScreenEvent";

    static MERGED: AtomicU32 = AtomicU32::new(0);
    static SEEN: AtomicU32 = AtomicU32::new(0);

    struct Reports;

    impl Mod for Reports {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.add_status(|| format!("{} zones merged", MERGED.load(Ordering::Relaxed)))?;
            load.add_text(NAME, Language::FALLBACK, "Locker")?;
            let opened = load.game_event_type(OPENED)?;
            load.on_client_raw_game_event(opened, |event| {
                SEEN.fetch_add(
                    u32::try_from(event.bytes().len()).unwrap(),
                    Ordering::Relaxed,
                );
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Reports,
        r#"
        id = "you.reports"
        side = "both"
        "#
    );

    #[test]
    fn the_status_line_reads_now_the_text_is_kept_and_only_client_events_reach_the_handler() {
        let loader = FakeHost::new()
            .side(Side::CLIENT)
            .game_event_type(OPENED, 0x0042, 16)
            .install();
        // SAFETY: the export of this module's `declare_mod!`.
        let status = unsafe { loader.init(enshwick_mod_init) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());

        MERGED.store(3, Ordering::Relaxed);
        assert_eq!(loader.status(), ["3 zones merged"]);

        assert_eq!(
            loader.texts(),
            [(NAME.fnv1a(), Language::FALLBACK.0, "Locker".to_owned())]
        );

        assert_eq!(
            loader.raise_game_event(0x0042, &[0; 16]),
            0,
            "a server event"
        );
        assert_eq!(loader.raise_client_game_event(0x0042, &[0; 16]), 1);
        assert_eq!(
            loader.raise_client_game_event(0x0043, &[0; 8]),
            0,
            "another type"
        );
        assert_eq!(SEEN.load(Ordering::Relaxed), 16);
    }
}

/// A mod that adds an icon and a craft affix rule.
#[cfg(test)]
#[cfg(feature = "unstable")]
mod icon_and_craft_affixes {
    use enshwick::abi::Status;
    use enshwick::affixes::{AffixCraft, RollRules};
    use enshwick::content::icons::{Icon, Pixels, Tint};
    use enshwick::content::{Guid, id};
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    const VANILLA: Guid = Guid::parse("505ccfbc-440d-4d63-a3d6-d689d3d55e78");

    struct Adds;

    impl Mod for Adds {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.add_icon(Icon::new(
                id!("ghostly-staff.icon"),
                VANILLA,
                Pixels::Tint(Tint::new([120, 170, 255], 60)),
            ))?;
            let refused = load
                .add_craft_affixes(AffixCraft::new(1.5, RollRules::new(1, 1)))
                .expect_err("a chance above 1");
            assert!(refused.to_string().contains("chance 1.5"), "{refused}");
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Adds,
        r#"
        id = "you.adds"
        side = "both"
        "#
    );

    #[test]
    fn an_icon_is_declared_and_a_craft_rule_with_a_chance_above_one_is_refused() {
        let loader = FakeHost::new().icons_ready().install();
        // SAFETY: the export of this module's `declare_mod!`.
        let status = unsafe { loader.init(enshwick_mod_init) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert!(!loader.is_off());
    }
}
