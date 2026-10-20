//! Craft and build filters, as the loader asks them.

#[cfg(test)]
mod tests {
    use enshwick_loader_api::abi::{BuildCheck, CraftCheck, Header, Status};
    use enshwick_loader_api::{Load, Mod, Off};

    struct Picky;

    impl Mod for Picky {
        fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
            enshwick_loader_api::manifest!(id: "picky", side: BOTH)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_craft(|craft| craft.station.is_some() || craft.amount < 10)?;
            load.on_build(|placement| placement.position[1] >= 0.0 && placement.item != 13)?;
            Ok(Self)
        }
    }

    enshwick_loader_api::declare_mod!(Picky);

    #[test]
    fn filters_see_typed_requests_and_refuse_by_returning_false() {
        let loader = enshwick_loader_api::test::FakeHost::new().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let craft = |station, amount| CraftCheck {
            header: Header::of::<CraftCheck>(),
            player: 2,
            station,
            recipe: 5,
            amount,
        };
        assert!(loader.craft(&craft(0, 3)));
        assert!(!loader.craft(&craft(0, 12)));
        assert!(loader.craft(&craft(40, 12)));
        let build = |item, y: i64| BuildCheck {
            header: Header::of::<BuildCheck>(),
            player: 2,
            item,
            position: [0, y << 32, 0],
        };
        assert!(loader.build(&build(7, 5)));
        assert!(!loader.build(&build(7, -1)));
        assert!(!loader.build(&build(13, 5)));
    }
}
