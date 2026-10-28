//! Entity data: kinds opened by a mod, values set on entities, read back and
//! removed, through the fake loader.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::Entity;
    use enshwick::abi::Status;
    use enshwick::entity_data::{EntityData, Replication, SetError};
    use enshwick::{Load, Mod, Off};

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Owner {
        player: u64,
        name: String,
    }

    static OWNERS: OnceLock<EntityData<Owner>> = OnceLock::new();
    static NICKNAMES: OnceLock<EntityData<String>> = OnceLock::new();

    struct Chests;

    impl Mod for Chests {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.chests", side: BOTH)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = OWNERS.set(load.entity_data("owner", Replication::Server)?);
            let _first = NICKNAMES
                .set(load.entity_data("you.market-stalls:nickname", Replication::Clients)?);
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Chests);

    #[test]
    fn values_stay_with_their_entity() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Chests)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let (owners, nicknames) = (OWNERS.get().unwrap(), NICKNAMES.get().unwrap());
        let chest = Entity(4_200);
        assert_eq!(owners.get(chest), None);
        let ann = Owner {
            player: 7,
            name: "Ann".into(),
        };
        owners.set(chest, &ann).unwrap();
        assert_eq!(owners.get(chest), Some(ann));
        assert_eq!(owners.get(Entity(4_201)), None);
        nicknames.set(Entity(77), &"Old Tom".to_owned()).unwrap();
        assert_eq!(nicknames.get(Entity(77)).as_deref(), Some("Old Tom"));
        let kinds: Vec<String> = loader.item_data().into_iter().map(|v| v.0).collect();
        assert_eq!(kinds, ["test.fake:owner", "you.market-stalls:nickname"]);
        let long = "x".repeat(600);
        assert_eq!(
            nicknames.set(Entity(77), &long),
            Err(SetError::TooLarge(602))
        );
        owners.remove(chest).unwrap();
        assert_eq!(owners.get(chest), None);
    }

    #[test]
    fn a_player_s_character_takes_no_value_since_no_load_finds_it_again() {
        use enshwick::abi::{Header, PlayerInfo};

        let ann = Entity(2);
        let player = PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: ann,
            key: enshwick::PlayerKey::new(7),
            permissions: 0,
            name_len: 0,
            name: [0; 64],
        };
        let loader = enshwick::test::FakeHost::new().player(player).install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Chests)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let owners = OWNERS.get().unwrap();
        let owner = Owner {
            player: 7,
            name: "Ann".into(),
        };
        assert_eq!(owners.set(ann, &owner), Err(SetError::NotKept));
        assert_eq!(owners.get(ann), None);
        assert_eq!(owners.remove(ann), Ok(()), "nothing to remove is no error");
    }
}
