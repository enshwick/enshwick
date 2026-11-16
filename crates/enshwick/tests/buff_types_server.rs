//! A server-only mod may not add a buff type: clients would lack it.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::buffs::{BuffTypeId, NewBuff};
    use enshwick::content::Id;
    use enshwick::{Load, Mod, Off};

    struct Lonely;

    impl Mod for Lonely {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.add_buff(NewBuff::new(
                Id::new("you.lonely:lonely"),
                BuffTypeId(0xf9b3_64f6),
            ))?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Lonely,
        r#"
        id = "you.lonely"
        side = "server"
        "#
    );

    #[test]
    fn a_server_only_mod_with_a_buff_type_turns_off_with_the_reason() {
        let loader = enshwick::test::FakeHost::new()
            .buffs(&[BuffTypeId(0xf9b3_64f6)])
            .install();
        // SAFETY: the exports of this file's `declare_mod!(Lonely)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_ne!(status, Status::OK);
        assert!(
            loader
                .logs()
                .iter()
                .any(|(_, line)| line.contains("runs on both sides")),
            "{:?}",
            loader.logs()
        );
        assert!(loader.resources().is_empty(), "nothing is served");
    }
}
