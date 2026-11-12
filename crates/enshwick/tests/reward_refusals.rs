//! A reward its build refuses stays undeclared: a retry names the real
//! reason and code cannot grant it.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::Status;
    use enshwick::rewards::{Action, Give, Reward, Rewards};
    use enshwick::test::FakeHost;
    use enshwick::{Entity, ItemId, Load, Mod, Off};

    const TONIC: ItemId = ItemId::named("you.refusals:tonic");

    /// Both refusals of the tonic reward and the mod's rewards handle.
    static SEEN: OnceLock<(Off, Off, Rewards)> = OnceLock::new();

    struct Refused;

    impl Mod for Refused {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let tonic = || Reward::new("tonic", Action::Consume(TONIC)).with(Give::Experience(5));
            let first = load.add_reward(tonic()).err();
            let again = load.add_reward(tonic()).err();
            let rewards = load.rewards()?;
            if let (Some(first), Some(again)) = (first, again) {
                let _first = SEEN.set((first, again, rewards));
            }
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Refused,
        r#"
        id = "you.refusals"
        side = "server"
        "#
    );

    #[test]
    fn a_reward_whose_action_the_build_lacks_stays_undeclared() {
        let data = std::env::temp_dir().join(format!("enshwick-refusals-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&data);
        let loader = FakeHost::new()
            .world()
            .data_dir(data.to_str().unwrap())
            .world_key("world0")
            .without_item_filters()
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Refused)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.faults());
        let (first, again, rewards) = SEEN.get().expect("both declarations were refused");
        assert_eq!(
            again, first,
            "the retry names the build's refusal, not a twin"
        );
        assert!(!rewards.grant(Entity(2), "tonic"), "code cannot grant it");
        let _gone = std::fs::remove_dir_all(&data);
    }
}
