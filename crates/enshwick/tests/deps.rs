//! `deps!` and local names: what a mod names by the aliases of its
//! `Cargo.toml` and by its own id, `you.hello`.
//!
//! The `Cargo.toml` requires `lockers` (`you.lockers`) and recommends
//! `lanterns` (`you.lanterns`) and `trading_post` (`you.trading-post`).

enshwick::deps!();

/// A locker's price, a point of `lockers`.
#[enshwick::point(of = lockers, name = "price", mode = Override, output = u32)]
#[derive(Clone, Copy, Debug)]
pub struct Price {
    /// The locker.
    pub locker: u32,
}

/// A bell rang, an event of this mod.
#[enshwick::event(name = "rang")]
#[derive(Clone, Copy, Debug)]
pub struct Rang {
    /// The bell.
    pub bell: u32,
}

/// A lantern lit, an event of `lanterns`.
#[enshwick::event(of = lanterns, name = "lit")]
#[derive(Clone, Copy, Debug)]
pub struct Lit {
    /// The lantern.
    pub lantern: u32,
}

#[cfg(test)]
mod tests {
    use enshwick::abi::{ModInfo, Status, Str};
    use enshwick::content::{Guid, Id};
    use enshwick::points::Point;
    use enshwick::services::ModEvent;
    use enshwick::{Load, Mod, Off};

    use super::{Lit, Price, Rang, deps};

    #[test]
    fn a_required_dependency_names_its_content_outside_the_game() {
        let locker = deps::lockers::id("locker").unwrap();
        assert_eq!(deps::lockers::ID, "you.lockers");
        assert_eq!(locker, Id::new("you.lockers:locker"));
        assert_eq!(locker.guid(), Guid::named("you.lockers:locker"));
        assert_eq!(locker.namespace(), "you.lockers");
        assert_eq!(locker.part("name").to_string(), "you.lockers:locker.name");
    }

    #[test]
    #[should_panic(expected = "a content id is <mod>:<name>")]
    fn a_required_dependency_refuses_a_malformed_name() {
        drop(deps::lockers::id("Locker"));
    }

    /// What a mod's init got for the names it asked `lockers` and
    /// `lanterns` for, and the fake's warnings.
    struct Named {
        status: Status,
        required: Vec<Result<Id, String>>,
        optional: Vec<Option<Id>>,
        warnings: Vec<String>,
    }

    /// Starts a mod whose init names `names` of `lockers` and `lanterns`,
    /// beside a running `lanterns`, where `lockers` exports `locker` and
    /// `lanterns` exports `lantern`.
    fn named(names: &'static [&'static str]) -> Named {
        use std::sync::Mutex;

        static REQUIRED: Mutex<Vec<Result<Id, String>>> = Mutex::new(Vec::new());
        static OPTIONAL: Mutex<Vec<Option<Id>>> = Mutex::new(Vec::new());
        static NAMES: Mutex<&[&str]> = Mutex::new(&[]);

        struct Namer;

        impl Mod for Namer {
            fn init(_load: &mut Load<'_>) -> Result<Self, Off> {
                for &name in *NAMES.lock().unwrap() {
                    let required = deps::lockers::id(name).map_err(|off| off.to_string());
                    REQUIRED.lock().unwrap().push(required);
                    OPTIONAL.lock().unwrap().push(deps::lanterns::id(name));
                }
                Ok(Self)
            }
        }

        enshwick::test::declare_mod!(
            Namer,
            r#"
            id = "you.hello"
            side = "server"
            "#
        );

        *NAMES.lock().unwrap() = names;
        let loader = enshwick::test::FakeHost::new()
            .loaded(ModInfo {
                id: Str::new("you.lanterns"),
                ..ModInfo::EMPTY
            })
            .exports("you.lockers", &["content:locker", "point:key"])
            .exports("you.lanterns", &["content:lantern"])
            .install();
        // SAFETY: both functions are the exports of this function's `declare_mod!(Namer)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        let warnings = loader
            .logs()
            .into_iter()
            .filter(|(level, _)| *level == enshwick::abi::Level::WARN)
            .map(|(_, line)| line)
            .collect();
        Named {
            status,
            required: std::mem::take(&mut *REQUIRED.lock().unwrap()),
            optional: std::mem::take(&mut *OPTIONAL.lock().unwrap()),
            warnings,
        }
    }

    #[test]
    fn a_dependency_s_content_passes_where_it_exports_the_name_or_the_name_it_is_part_of() {
        let Named {
            status,
            required,
            optional,
            warnings,
        } = named(&["locker", "locker.recipe", "lantern.icon"]);
        assert_eq!(status, Status::OK);
        assert_eq!(
            required[..2],
            [
                Ok(Id::new("you.lockers:locker")),
                Ok(Id::new("you.lockers:locker.recipe"))
            ]
        );
        assert_eq!(
            optional,
            [None, None, Some(Id::new("you.lanterns:lantern.icon"))]
        );
        assert_eq!(
            warnings,
            [
                "content id you.lanterns:locker: you.lanterns does not export locker; it exports content:lantern",
                "content id you.lanterns:locker.recipe: you.lanterns does not export locker.recipe; it exports content:lantern",
                "content id you.lockers:lantern.icon: you.lockers does not export lantern.icon; it exports content:locker, point:key",
            ]
        );
        assert!(required[2].is_err());
    }

    /// `you.lockers` exports a point `key`, which opens no content `key`.
    #[test]
    fn a_required_dependency_s_content_it_does_not_export_is_off() {
        let Named {
            required,
            optional,
            warnings,
            ..
        } = named(&["key"]);
        let refused = required[0].as_ref().unwrap_err();
        assert!(refused.contains("content id you.lockers:key"), "{refused}");
        assert_eq!(optional, [None]);
        assert!(
            warnings.contains(
                &"content id you.lockers:key: you.lockers does not export key; it exports content:locker, point:key"
                    .to_owned()
            )
        );
    }

    #[test]
    fn an_optional_dependency_names_nothing_while_it_does_not_run() {
        assert_eq!(deps::lanterns::ID, "you.lanterns");
        assert_eq!(deps::lanterns::id("lantern"), None);
    }

    #[test]
    #[should_panic(expected = "a content id is <mod>:<name>")]
    fn an_optional_dependency_refuses_a_malformed_name_while_it_does_not_run() {
        let _ = deps::lanterns::id("Lantern");
    }

    #[test]
    fn an_optional_dependency_names_its_content_while_it_runs() {
        struct Hello;

        impl Mod for Hello {
            fn init(_load: &mut Load<'_>) -> Result<Self, Off> {
                Ok(Self)
            }
        }

        enshwick::test::declare_mod!(
            Hello,
            r#"
            id = "you.hello"
            side = "server"
            "#
        );

        let loader = enshwick::test::FakeHost::new()
            .loaded(ModInfo {
                id: Str::new("you.hello"),
                ..ModInfo::EMPTY
            })
            .loaded(ModInfo {
                id: Str::new("you.lanterns"),
                ..ModInfo::EMPTY
            })
            .install();
        // SAFETY: both functions are the exports of this test's `declare_mod!(Hello)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);

        assert_eq!(
            deps::lanterns::id("lantern"),
            Some(Id::new("you.lanterns:lantern"))
        );
    }

    #[test]
    fn a_local_name_takes_the_dependency_it_is_of_or_this_mod() {
        assert_eq!(Price::NAME, "you.lockers:price");
        assert_eq!(Rang::NAME, "you.hello:rang");
        assert_eq!(Lit::NAME, "you.lanterns:lit");
        assert!(Price::SCHEMA.contains("you.lockers:price"));
    }
}
