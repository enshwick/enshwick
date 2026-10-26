//! Item data: a kind opened by a mod, values set, read back and removed,
//! and a value that reads as missing once its entity holds another item.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::Status;
    use enshwick::ecs::Entity;
    use enshwick::item_data::{ItemData, ItemInstance, SetError};
    use enshwick::{Load, Mod, Off};

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Bonus {
        fire: i16,
        by: String,
    }

    static BONUS: OnceLock<ItemData<Bonus>> = OnceLock::new();
    static SHARED: OnceLock<ItemData<u16>> = OnceLock::new();

    struct Rings;

    impl Mod for Rings {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.rings", side: BOTH)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = BONUS.set(load.item_data("bonus")?);
            let _first = SHARED.set(load.item_data("survivors-per-altar:charges")?);
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Rings);

    #[test]
    fn values_stay_with_their_item() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let (bonus, shared) = (BONUS.get().unwrap(), SHARED.get().unwrap());
        let ring = ItemInstance {
            pide: Entity(900),
            item: 0x5e01,
        };
        assert_eq!(bonus.get(ring), None);
        let value = Bonus {
            fire: 12,
            by: "Balthazar".into(),
        };
        bonus.set(ring, &value).unwrap();
        assert_eq!(bonus.get(ring), Some(value));
        let reused = ItemInstance {
            item: 0x0c01,
            ..ring
        };
        assert_eq!(bonus.get(reused), None, "the entity holds another item");
        shared.set(ring, &3).unwrap();
        assert_eq!(shared.get(ring), Some(3));
        let kinds: Vec<String> = loader.item_data().into_iter().map(|v| v.0).collect();
        assert_eq!(kinds, ["test.fake:bonus", "survivors-per-altar:charges"]);
        let long = Bonus {
            fire: 0,
            by: "x".repeat(600),
        };
        assert_eq!(bonus.set(ring, &long), Err(SetError::TooLarge(603)));
        bonus.remove(ring).unwrap();
        assert_eq!(bonus.get(ring), None);
        assert_eq!(loader.item_data().len(), 1);
    }
}
