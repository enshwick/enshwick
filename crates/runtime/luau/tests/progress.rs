//! `enshwick.progress`: the experience filter a script links at start, with
//! the level table of the pass.

#[cfg(test)]
mod tests {
    use enshwick::Entity;
    use enshwick::progress::Experience;
    use enshwick::test::FakeHost;

    use crate::harness::{logged, run};

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
