//! Stat kinds and modifiers through the fake loader: a kind defined and a
//! built-in one opened, modifiers from a mod, a buff and a worn item summed
//! in source order, changes raised once, and what is refused.

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use enshwick::Entity;
    use enshwick::abi::{BuffTypeInfo, Header, Status};
    use enshwick::buffs::{Apply, BuffTypeId, Buffs};
    use enshwick::content::{Id, id};
    use enshwick::i18n::{Language, Texts};
    use enshwick::inventory::ItemInstance;
    use enshwick::stats::{
        ItemStats, Lasting, Modifier, Op, Origin, Stat, StatChange, StatDef, Stats, builtin,
    };
    use enshwick::{Load, Mod, Off};

    const BLESSED: BuffTypeId = BuffTypeId::new(0xb1e5);

    struct Kit {
        buffs: Buffs,
        stats: Stats,
        texts: Texts,
        luck: Stat,
        fire: Stat,
    }

    static KIT: OnceLock<Kit> = OnceLock::new();
    static CHANGES: Mutex<Vec<StatChange>> = Mutex::new(Vec::new());

    struct Rpg;

    impl Mod for Rpg {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let luck = load.define_stat(id!("luck"), &StatDef::additive().range(0.0, 1.0))?;
            let fire = load.open_stat(builtin::RESISTANCE_FIRE)?;
            assert!(
                load.define_stat(Id::new("other:luck"), &StatDef::additive())
                    .is_err(),
                "another mod's namespace"
            );
            assert!(
                load.define_stat(id!("luck"), &StatDef::additive()).is_err(),
                "defined twice"
            );
            load.add_buff_modifiers(BLESSED, &[Modifier::new(luck, Op::Add, 0.25)])?;
            load.on_stats_changed(|change| {
                CHANGES
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(*change);
            })?;
            let stats = load.stats()?;
            let buffs = load.buffs();
            let texts = load.texts()?;
            let _first = KIT.set(Kit {
                buffs,
                stats,
                texts,
                luck,
                fire,
            });
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Rpg,
        r#"
        id = "you.hello"
        side = "server"
        "#,
        texts = "tests/texts/stat_kinds.toml",
    );

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn sources_add_up_in_order_and_changes_are_raised_once() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Rpg)`.
        let started = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(started, Status::OK);
        let Kit {
            buffs,
            stats,
            texts,
            luck,
            fire,
        } = KIT.get().unwrap();
        let (buffs, stats, luck, fire) = (*buffs, *stats, *luck, *fire);
        let player = Entity(2);

        assert!(close(stats.value(player, luck), 0.0));
        let info = stats.info(fire).unwrap();
        assert_eq!(info.id, builtin::RESISTANCE_FIRE.to_string());
        assert_eq!(info.text, "resistance_fire.name");
        let own = stats.info(luck).unwrap();
        assert_eq!(own.id, "you.hello:luck");
        assert_eq!(texts.fill(&own.text, Language::FALLBACK, &[]), "Luck");
        let kept = Texts::parse(&loader.published_texts().unwrap()).unwrap();
        assert_eq!(kept.get(&own.text, Language::UK_UA), Some("Удача"));

        stats
            .set(
                player,
                "bless",
                Modifier::new(luck, Op::Add, 0.1),
                Lasting::UntilRemoved,
            )
            .unwrap();
        stats
            .set(
                player,
                "bless",
                Modifier::new(luck, Op::Add, 0.2),
                Lasting::UntilRemoved,
            )
            .unwrap();
        assert!(
            close(stats.value(player, luck), 0.2),
            "the same key replaces"
        );

        let blessed = BuffTypeInfo {
            header: Header::of::<BuffTypeInfo>(),
            id: BLESSED,
            slot: BuffTypeInfo::BUFF,
            flags: 0,
            name: 0,
            duration: 60_000_000_000,
        };
        loader.set_buffs(&[blessed], &[player]);
        buffs.apply(player, BLESSED, Apply::new()).unwrap();
        let ring = ItemInstance {
            pide: Entity(900),
            item: enshwick::ItemId::new(0x5e01),
        };
        loader.wear(Entity(player.0), ring);
        stats
            .set_item(
                ring,
                &ItemStats {
                    modifiers: vec![
                        Modifier::new(luck, Op::Multiply, 2.0).to_item(),
                        Modifier::new(fire, Op::Add, 0.3).to_item(),
                    ],
                    ..ItemStats::default()
                },
            )
            .unwrap();
        // (0.25 + 0.2) * 2, kept within the range.
        assert!(close(stats.value(player, luck), 0.9));
        assert!(close(stats.value(player, fire), 0.3));

        let order: Vec<Origin> = stats
            .sources(player, Some(luck))
            .iter()
            .map(|s| s.origin)
            .collect();
        assert!(matches!(
            order.as_slice(),
            [
                Origin::Item(_),
                Origin::Buff(BLESSED),
                Origin::Mod {
                    index: enshwick::mods::ModIndex(0),
                    ..
                }
            ]
        ));

        assert_eq!(loader.settle_stats(), 2);
        assert_eq!(loader.settle_stats(), 0, "nothing moved since");
        let changes = CHANGES.lock().unwrap().clone();
        assert!(changes.iter().any(|c| c.stat == luck && close(c.new, 0.9)));

        stats.remove(player, "bless", luck).unwrap();
        assert!(close(stats.value(player, luck), 0.5));
        assert!(loader.stat_modifiers().is_empty());
    }
}

/// A dependency's kind: kept when its owner exports its name or the name it
/// is a part of, refused with the owner's exports when it does not.
#[cfg(test)]
mod dependency {
    use std::sync::OnceLock;

    use enshwick::abi::{Level, Status};
    use enshwick::content::Id;
    use enshwick::stats::{Stat, Stats};
    use enshwick::{Load, Mod, Off};

    enshwick::deps!();

    /// The kind the mod opens: a name through `deps!`, or a full id past
    /// its export check.
    #[derive(Clone, Copy)]
    enum Kind {
        Dependency(&'static str),
        Full(&'static str),
    }

    static KIND: OnceLock<Kind> = OnceLock::new();
    static OPENED: OnceLock<(Stats, Stat)> = OnceLock::new();

    struct Reader;

    impl Mod for Reader {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let id = match KIND.get().copied().unwrap_or(Kind::Dependency("luck")) {
                Kind::Dependency(name) => Id::of(deps::lockers::ID, name),
                Kind::Full(id) => Id::new(id),
            };
            let opened = load.open_stat(id)?;
            let _first = OPENED.set((load.stats()?, opened));
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Reader,
        r#"
        id = "you.hello"
        side = "server"

        [requires]
        lockers = "you.lockers"
        "#
    );

    /// Starts the mod opening `kind` of `you.lockers`, which exports
    /// `exports`; its status and the warnings logged.
    fn start(kind: Kind, exports: &[&str]) -> (Status, Vec<String>) {
        let _first = KIND.set(kind);
        let loader = enshwick::test::FakeHost::new()
            .exports("you.lockers", exports)
            .install();
        // SAFETY: both functions are the exports of this module's `declare_mod!(Reader)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        let warnings = loader
            .logs()
            .into_iter()
            .filter(|(level, _)| *level == Level::WARN)
            .map(|(_, line)| line)
            .collect();
        (status, warnings)
    }

    #[test]
    fn a_kind_its_owner_exports_opens_by_its_dependency_id() {
        assert_eq!(
            start(Kind::Dependency("luck"), &["stat:luck"]).0,
            Status::OK
        );
        let (stats, luck) = OPENED.get().unwrap();
        assert_eq!(luck.key(), enshwick::stats::key_of("you.lockers:luck"));
        assert!(
            stats.info(*luck).is_none(),
            "opened, defined by no running mod"
        );
    }

    #[test]
    fn a_kind_opens_when_its_owner_exports_the_name_it_is_a_part_of() {
        let (status, warnings) = start(Kind::Dependency("charm.fire"), &["stat:charm"]);
        assert_eq!(status, Status::OK);
        assert!(warnings.is_empty(), "{warnings:?}");
        let (_, fire) = OPENED.get().unwrap();
        assert_eq!(
            fire.key(),
            enshwick::stats::key_of("you.lockers:charm.fire")
        );
    }

    #[test]
    fn a_kind_its_owner_does_not_export_is_refused_with_the_owner_s_exports() {
        let (status, warnings) = start(
            Kind::Full("you.lockers:luck"),
            &["point:luck", "content:luck"],
        );
        assert_eq!(
            status,
            Status::REFUSED,
            "a point or content of the name opens no kind"
        );
        assert!(OPENED.get().is_none());
        assert_eq!(
            warnings,
            [
                "stat you.lockers:luck: you.lockers does not export luck; it exports point:luck, content:luck"
            ]
        );
    }
}
