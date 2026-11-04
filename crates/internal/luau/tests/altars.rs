//! `enshwick.altars`: the handle a script asks for at start, as a Rust mod
//! calls `load.altars()` in init, so a mod that never asks arms nothing.

#[cfg(test)]
mod tests {
    use enshwick::test::FakeHost;

    use crate::harness::{logged, run};

    #[test]
    fn a_mod_that_never_asks_for_altars_leaves_them_closed() {
        let loader = run(FakeHost::new(), "enshwick.log.info('hello')");
        assert!(!loader.altars_opened().0);
    }

    #[test]
    fn altars_open_when_asked_at_start_and_their_functions_wait_for_it() {
        let loader = run(
            FakeHost::new(),
            r"
            local ok, why = pcall(enshwick.altars.list)
            enshwick.log.info(`early {not ok} {why}`)
            local altars = enshwick.altars.altars()
            enshwick.log.info(`opened {tostring(altars)} {#enshwick.altars.list()} {#altars:list()}`)
            ",
        );
        assert!(loader.altars_opened().0);
        assert!(logged(&loader, "early true"), "{:?}", loader.logs());
        assert!(logged(
            &loader,
            "call enshwick.altars.altars() at the top level"
        ));
        assert!(logged(&loader, "opened Altars 0 0"), "{:?}", loader.logs());
    }

    #[test]
    fn a_map_snapshot_takes_the_altars_a_script_asked_for() {
        let loader = run(
            FakeHost::new(),
            r"
            local altars = enshwick.altars.altars()
            local ok, why = pcall(enshwick.map.snapshot, altars)
            enshwick.log.info(`snapshot {ok} {why}`)
            ",
        );
        assert!(!logged(&loader, "expected Altars"), "{:?}", loader.logs());
        assert!(logged(&loader, "snapshot"), "{:?}", loader.logs());
    }
}
