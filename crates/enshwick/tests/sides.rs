//! A mod's two sides through the fake loader: server or both, never the
//! client alone, and what a refused join lists to mods.

#[cfg(test)]
mod tests {
    use enshwick::abi::{Header, Manifest, ModInfo, Side, Status, Str};
    use enshwick::net::Channel;
    use enshwick::{Load, Mod, Off};

    mod both {
        use super::*;

        pub(super) struct Lanterns;

        impl Mod for Lanterns {
            fn init(load: &mut Load<'_>) -> Result<Self, Off> {
                let _channel: Channel<u8> = load.channel("lit", 1, |_, _| {})?;
                Ok(Self)
            }
        }

        enshwick::test::declare_mod!(
            Lanterns,
            r#"
            id = "you.lanterns"
            side = "both"
            "#
        );

        /// This mod's manifest with the client as its side, which no mod
        /// declares any more.
        pub(super) extern "system" fn client_query() -> *const Manifest {
            // SAFETY: `enshwick_mod_query` returns the manifest of
            // `declare_mod!`, valid for the process.
            let declared = unsafe { *enshwick_mod_query() };
            Box::leak(Box::new(Manifest {
                side: Side::CLIENT,
                ..declared
            }))
        }
    }

    mod server {
        use super::*;

        pub(super) struct Loot;

        impl Mod for Loot {
            fn init(load: &mut Load<'_>) -> Result<Self, Off> {
                let opened: Result<Channel<u8>, Off> = load.channel("drops", 1, |_, _| {});
                if opened.is_ok() {
                    return Err(Off::new("a server mod opened a channel to clients"));
                }
                Ok(Self)
            }
        }

        enshwick::test::declare_mod!(
            Loot,
            r#"
            id = "you.loot"
            side = "server"
            "#
        );
    }

    #[test]
    fn a_mod_of_side_both_talks_to_clients_and_a_server_mod_does_not() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: the three functions are the exports of `both`'s `declare_mod!`.
        let status = unsafe {
            loader.start(
                both::enshwick_mod_query,
                both::enshwick_mod_init,
                both::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK);
        drop(loader);
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: the three functions are the exports of `server`'s `declare_mod!`.
        let status = unsafe {
            loader.start(
                server::enshwick_mod_query,
                server::enshwick_mod_init,
                server::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.faults());
    }

    #[test]
    fn a_mod_for_the_client_alone_is_refused_before_its_init() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: init and ready are `both`'s exports; the query hands the
        // same manifest with another side, valid for the process.
        let status = unsafe {
            loader.start(
                both::client_query,
                both::enshwick_mod_init,
                both::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::REFUSED);
        assert!(
            loader.faults()[0].contains("not a mod's side"),
            "{:?}",
            loader.faults()
        );
    }

    #[test]
    fn a_refused_join_lists_ids_and_versions_and_nothing_a_server_chose() {
        let loader = enshwick::test::FakeHost::new()
            .join_missing(ModInfo {
                header: Header::of::<ModInfo>(),
                id: Str::new("you.trading-post"),
                version: Str::new("0.3.0"),
                side: Side::BOTH,
                off: 1,
                ..ModInfo::EMPTY
            })
            .install();
        // SAFETY: the three functions are the exports of `both`'s `declare_mod!`.
        let status = unsafe {
            loader.start(
                both::enshwick_mod_query,
                both::enshwick_mod_init,
                both::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK);
        let missing = enshwick::mods::join_missing();
        assert_eq!(missing.len(), 1);
        assert_eq!(
            (missing[0].id.as_str(), missing[0].version.as_str()),
            ("you.trading-post", "0.3.0")
        );
        assert_eq!(missing[0].side, Side::BOTH);
        assert!(!missing[0].running);
        assert!(missing[0].name.is_empty() && missing[0].homepage.is_empty());
        assert_eq!(missing[0].label(), "you.trading-post");
    }
}
