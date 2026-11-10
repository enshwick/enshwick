//! Building zones: a mod's zone check accepts a placement the game refused,
//! and the game's last grouping of altar zones into bases reads back.

#[cfg(test)]
mod tests {
    use enshwick::Load;
    use enshwick::abi::{Bounds, Status};
    use enshwick::altars::{Grouping, grouping};
    use enshwick::{Mod, Off};

    struct Joined;

    impl Mod for Joined {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            // An object whose box lies in two zones' bounding box.
            load.on_building_zone_check(|check| {
                check.zones.len() >= 2
                    && (0..3).all(|a| {
                        let low = check.zones.iter().map(|z| z.min[a]).min().unwrap_or(0);
                        let high = check.zones.iter().map(|z| z.max[a]).max().unwrap_or(0);
                        low <= check.object.min[a] - check.margin
                            && check.object.max[a] + check.margin < high
                    })
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Joined,
        r#"
        id = "you.joined"
        side = "server"
        "#
    );

    fn bounds(min: [i64; 3], max: [i64; 3]) -> Bounds {
        Bounds { min, max }
    }

    fn started() -> enshwick::test::Installed {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Joined)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.faults());
        loader
    }

    #[test]
    fn a_zone_check_accepts_an_object_across_two_zones_and_nothing_past_them() {
        let loader = started();
        let zones = [
            bounds([0, 0, 0], [10, 10, 10]),
            bounds([10, 0, 0], [20, 10, 10]),
        ];
        assert!(loader.building_zone_check(&zones, bounds([8, 1, 1], [12, 2, 2]), 0));
        assert!(
            !loader.building_zone_check(&zones, bounds([8, 1, 1], [12, 2, 2]), 9),
            "the margin reaches past the zones"
        );
        assert!(!loader.building_zone_check(&zones, bounds([18, 1, 1], [22, 2, 2]), 0));
        assert!(!loader.building_zone_check(&zones[..1], bounds([1, 1, 1], [2, 2, 2]), 0));
    }

    #[test]
    fn the_grouping_reads_none_until_the_game_groups_then_its_counts() {
        let loader = started();
        assert_eq!(grouping(), None);
        loader.set_grouping(17, 16);
        assert_eq!(
            grouping(),
            Some(Grouping {
                zones: 17,
                bases: 16
            })
        );
    }
}
