//! The world's containers by kind, found without an engine name.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::EngineFacts;
    use enshwick::test::{FakeHost, facts};
    use enshwick::world::{ContainerKinds, World};
    use enshwick::{Load, Mod, Off};

    const SETUP: u32 = 7;
    const PLAYER: u32 = 8;
    const DROP: u32 = 9;
    const ANIMAL: u32 = 10;
    const LOOT_FILL: u32 = 11;
    const NPC: u32 = 12;
    const RESOURCE: u32 = 13;

    /// Every marker the classification reads, by id.
    const MARKERS: [(&str, u32); 6] = [
        ("player_input", PLAYER),
        ("npc", NPC),
        ("animal", ANIMAL),
        ("destroy_on_loot", DROP),
        ("loot_fill", LOOT_FILL),
        ("resource_node", RESOURCE),
    ];

    struct Holders {
        world: World,
    }

    impl Mod for Holders {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            Ok(Self {
                world: load.world()?,
            })
        }
    }

    enshwick::test::declare_mod!(
        Holders,
        r#"
        id = "you.holders"
        side = "server"
        "#
    );

    fn world() -> FakeHost {
        FakeHost::new().world()
    }

    /// One holder of each kind, with the markers several kinds share: a
    /// built chest with the loot fill world chests have too, a player's
    /// character, an enemy's drop with the loot fill, a holder no marker
    /// names, a tamed animal and a summoned NPC that keep the item they came
    /// from, a world chest and a plant. Every marker but `missing` is on the
    /// build.
    fn holding_without(missing: &str) -> FakeHost {
        let host = MARKERS
            .iter()
            .filter(|(id, _)| *id != missing)
            .fold(world(), |host, &(id, hash)| host.type_hash(id, hash));
        let used_item = host.hash(facts::USED_ITEM.id()).unwrap();
        [
            (10, &[used_item, LOOT_FILL][..]),
            (11, &[PLAYER]),
            (12, &[DROP, LOOT_FILL]),
            (13, &[]),
            (14, &[ANIMAL, used_item]),
            (15, &[LOOT_FILL]),
            (16, &[NPC, used_item]),
            (17, &[RESOURCE]),
        ]
        .into_iter()
        .fold(
            host.type_hash("inventory_setup", SETUP),
            |host, (entity, markers)| {
                markers.iter().fold(
                    host.placed(SETUP, enshwick::Entity(entity), [0.0; 3]),
                    |host, &marker| host.component(enshwick::Entity(entity), marker, &[0; 4]),
                )
            },
        )
    }

    fn holding() -> FakeHost {
        holding_without("")
    }

    fn found(host: FakeHost, kinds: ContainerKinds) -> Vec<u32> {
        let loader = host.install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Holders)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let world = Holders::instance().unwrap().world;
        world
            .containers(kinds)
            .iter()
            .map(|placed| placed.entity.0)
            .collect()
    }

    #[test]
    fn the_default_kinds_are_the_containers_players_build() {
        assert_eq!(ContainerKinds::default(), ContainerKinds::PLACED);
        assert_eq!(found(holding(), ContainerKinds::default()), [10]);
    }

    #[test]
    fn each_kind_finds_its_own_holders_and_the_rest_are_other() {
        assert_eq!(found(holding(), ContainerKinds::CHARACTERS), [11]);
        assert_eq!(found(holding(), ContainerKinds::DROPS), [12]);
        assert_eq!(found(holding(), ContainerKinds::OTHER), [13]);
        assert_eq!(found(holding(), ContainerKinds::ANIMALS), [14]);
        assert_eq!(found(holding(), ContainerKinds::LOOT), [15]);
        assert_eq!(found(holding(), ContainerKinds::NPCS), [16]);
        assert_eq!(found(holding(), ContainerKinds::PLANTS), [17]);
        let both = ContainerKinds::PLACED | ContainerKinds::CHARACTERS;
        assert_eq!(found(holding(), both), [10, 11]);
        assert_eq!(
            found(holding(), ContainerKinds::ALL),
            [10, 11, 12, 13, 14, 15, 16, 17]
        );
    }

    #[test]
    fn a_build_missing_any_marker_finds_no_containers_rather_than_wrong_ones() {
        for (id, _) in MARKERS {
            let found = found(holding_without(id), ContainerKinds::ALL);
            assert_eq!(found, Vec::<u32>::new(), "without {id}");
        }
    }

    #[test]
    fn a_build_without_containers_finds_none() {
        assert_eq!(found(world(), ContainerKinds::ALL), Vec::<u32>::new());
    }

    #[test]
    fn kinds_combine_and_contain() {
        let both = ContainerKinds::PLACED | ContainerKinds::DROPS;
        assert!(both.contains(ContainerKinds::DROPS));
        assert!(!both.contains(ContainerKinds::CHARACTERS));
        assert!(ContainerKinds::ALL.contains(both));
    }
}
