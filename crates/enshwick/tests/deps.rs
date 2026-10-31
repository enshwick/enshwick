//! `deps!` and local names: what a mod names by the aliases of its
//! `Cargo.toml`, which requires `lockers` (`you.lockers`) and recommends
//! `lanterns` (`you.lanterns`) among others, and by its own id, `you.hello`.

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
    fn a_required_dependency_names_its_content_in_a_constant() {
        const LOCKER: Id = deps::lockers::id("locker");
        assert_eq!(deps::lockers::ID, "you.lockers");
        assert_eq!(LOCKER, Id::new("you.lockers:locker"));
        assert_eq!(LOCKER.guid(), Guid::named("you.lockers:locker"));
        assert_eq!(LOCKER.namespace(), "you.lockers");
        assert_eq!(LOCKER.part("name").to_string(), "you.lockers:locker.name");
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
