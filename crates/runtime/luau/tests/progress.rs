//! `enshwick.progress`: the comfort and experience filters a script links at
//! start, the latter with the level table of the pass.

#[cfg(test)]
mod tests {
    use enshwick::Entity;
    use enshwick::abi::{Comfort, Header};
    use enshwick::progress::Experience;
    use enshwick::test::FakeHost;

    use crate::harness::{logged, run};

    fn comfort(category: u32, value: i32) -> Comfort {
        Comfort {
            header: Header::of::<Comfort>(),
            category,
            value,
            position: [0.0; 3],
        }
    }

    #[test]
    fn a_comfort_filter_replaces_the_comfort_and_nil_keeps_it() {
        let loader = run(
            FakeHost::new(),
            r"
            enshwick.progress.on_comfort(function(comfort)
                if comfort.category == 1 then
                    return comfort.value * 10
                end
                return nil
            end)
            ",
        );
        assert_eq!(loader.comfort(&comfort(1, 2)), 20);
        assert_eq!(loader.comfort(&comfort(2, 3)), 3, "nil keeps the comfort");
    }

    #[test]
    fn an_experience_filter_reads_the_level_table_and_changes_the_gain() {
        let loader = run(
            FakeHost::new(),
            r"
            enshwick.progress.on_experience(function(all, levels)
                enshwick.log.info(`levels {levels:cap()} {levels:need(2)} {levels:need(0)} {levels:need(26)}`)
                all[1].gained = levels:need(1)
                return all
            end)
            ",
        );
        let mut rows = [Experience {
            entity: Entity(1),
            level: 1,
            progress: 0,
            need: 100,
            gained: 5,
        }];
        loader.experience(&mut rows, 25, |level| level * 100);
        assert!(logged(&loader, "levels 25 200 0 0"), "{:?}", loader.logs());
        assert_eq!(rows[0].gained, 100);
    }
}
