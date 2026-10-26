//! The world's containers by kind, found without an engine name.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::test::FakeHost;
    use enshwick::world::{ContainerKinds, World};
    use enshwick::{Load, Mod, Off};

    const SETUP: u32 = 7;
    const USED_ITEM: u32 = 2;
    const PLAYER: u32 = 8;
    const DROP: u32 = 9;
    const ANIMAL: u32 = 10;
    const LOOT_FILL: u32 = 11;

    struct Holders {
        world: World,
    }

    impl Mod for Holders {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.holders", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            Ok(Self {
                world: load.world()?,
            })
        }
    }

    enshwick::declare_mod!(Holders);

    fn world() -> FakeHost {
        FakeHost::new()
            .type_hash("current_transform", 1)
            .type_hash("used_item", USED_ITEM)
    }

    /// A chest a player built, with the loot fill world chests have too, a
    /// player's character, a corpse, a container no marker names, a tamed
    /// animal that keeps the item it came from, and a world chest.
    fn holding() -> FakeHost {
        world()
            .type_hash("inventory_setup", SETUP)
            .type_hash("player_input", PLAYER)
            .type_hash("destroy_on_loot", DROP)
            .type_hash("animal", ANIMAL)
            .type_hash("loot_fill", LOOT_FILL)
            .placed(SETUP, 10, [0.0; 3])
            .component(10, USED_ITEM, &5u32.to_le_bytes())
            .component(10, LOOT_FILL, &[0; 4])
            .placed(SETUP, 11, [1.0; 3])
            .component(11, PLAYER, &[0; 4])
            .placed(SETUP, 12, [2.0; 3])
            .component(12, DROP, &[0; 4])
            .placed(SETUP, 13, [3.0; 3])
            .placed(SETUP, 14, [4.0; 3])
            .component(14, ANIMAL, &[0; 4])
            .component(14, USED_ITEM, &6u32.to_le_bytes())
            .placed(SETUP, 15, [5.0; 3])
            .component(15, LOOT_FILL, &[0; 4])
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
        assert_eq!(found(holding(), ContainerKinds::NPCS), Vec::<u32>::new());
        let both = ContainerKinds::PLACED | ContainerKinds::CHARACTERS;
        assert_eq!(found(holding(), both), [10, 11]);
        assert_eq!(
            found(holding(), ContainerKinds::ALL),
            [10, 11, 12, 13, 14, 15]
        );
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
