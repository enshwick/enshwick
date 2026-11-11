//! `enshwick.sound`: a script changes a sound container and adds a sound,
//! with the bounds and refusals of a Rust mod's.

#[cfg(test)]
mod tests {
    use enshwick::abi::Side;
    use enshwick::test::FakeHost;

    use crate::harness::{logged, run};

    const MAIN: &str = r#"
        local chest = "6943053a-32c1-4718-ae22-e61427ddf54d"
        enshwick.sound.change(chest, { volume = 0.5, looped = true })
        local loud = pcall(enshwick.sound.change, chest, { volume = 9 })
        enshwick.log.info(`loud refused {not loud}`)
        local ok, why = pcall(enshwick.sound.add, { id = "you.horns:horn", file = "../horn.wav" })
        enshwick.log.info(`outside refused {not ok}: {why}`)
    "#;

    #[test]
    fn a_script_s_sound_changes_and_files_are_checked_as_a_rust_mod_s() {
        let loader = run(FakeHost::new().side(Side::SERVER), MAIN);
        assert!(logged(&loader, "loud refused true"), "{:?}", loader.logs());
        assert!(
            logged(&loader, "outside refused true") && logged(&loader, "inside the mod's folder"),
            "{:?}",
            loader.logs()
        );
    }
}
