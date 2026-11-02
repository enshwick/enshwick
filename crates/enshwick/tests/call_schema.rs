//! A call's schema: what `cargo enshwick deps` reads from its server,
//! `you.lockers`, which this crate's `Cargo.toml` requires as `lockers`.

use enshwick::Entity;

/// What a player does at a stall.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, enshwick::Shape)]
#[shape(values(BUY, SELL))]
pub struct Deal(pub u32);

impl Deal {
    /// Buy from it.
    pub const BUY: Self = Self(0);
    /// Sell to it.
    pub const SELL: Self = Self(1);
}

/// What a stall sells.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, enshwick::Shape)]
pub enum Stall {
    /// Food.
    Food = 1,
    /// Tools.
    Tools = 2,
}

/// The rent of a stall, in coins.
#[enshwick::call(name = "you.lockers:rent", export, output = u64, version = 2)]
#[derive(Clone, Copy, Debug)]
pub struct Rent {
    /// The player's character.
    pub character: Entity,
    /// What the player does.
    pub deal: Deal,
    /// Whether the stall is the player's own.
    pub own: bool,
    /// What the stall sells.
    pub stall: Stall,
    /// The stall's place in its row.
    pub place: u16,
}

/// The fee for holding a stall, in coins.
#[enshwick::call(name = "you.lockers:fee", export, output = u64, version = 1)]
#[derive(Clone, Copy, Debug)]
pub struct Fee {
    /// The player's character.
    pub character: Entity,
    /// How many days the stall is held.
    pub days: u32,
    /// The stall's place in its row.
    pub place: u16,
    /// Whether the stall is the player's own.
    pub own: bool,
    /// The stall's row.
    pub row: u8,
}

/// The module `cargo enshwick deps lockers` writes from that schema.
#[path = "../../cargo-enshwick/tests/fixtures/lockers_calls.rs"]
pub mod generated;

/// The mod that serves the calls.
mod lockers {
    #[expect(
        dead_code,
        reason = "the test reads its manifest alone; the mod never starts"
    )]
    pub(super) struct Lockers;

    impl enshwick::Mod for Lockers {
        fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Lockers,
        r#"
        id = "you.lockers"
        side = "server"
        "#
    );
}

#[cfg(test)]
mod tests {
    use enshwick::services::Call;

    use super::{Deal, Rent, Stall, generated};

    #[test]
    fn the_schema_is_the_one_cargo_enshwick_is_tested_with() {
        // SAFETY: `enshwick_mod_query` returns the manifest of
        // `declare_mod!`, valid until its shutdown, which no test runs.
        let manifest = unsafe { &*super::lockers::enshwick_mod_query() };
        // SAFETY: the manifest's schema text lives as long as the manifest.
        let text = unsafe { manifest.points.as_str() }
            .unwrap_or_default()
            .replacen(
                &format!("version = \"{}\"", enshwick::VERSION),
                "version = \"1.2.0\"",
                1,
            );
        assert_eq!(
            text,
            include_str!("../../cargo-enshwick/tests/fixtures/lockers.calls.toml")
        );
    }

    #[test]
    fn the_written_declaration_matches_the_server_s_call() {
        let server = <Rent as Call>::LAYOUT.iter().map(|f| f.hash);
        let written = <generated::Rent as Call>::LAYOUT.iter().map(|f| f.hash);
        assert!(server.eq(written));
        assert_eq!(<generated::Rent as Call>::NAME, <Rent as Call>::NAME);
        assert_eq!(<generated::Rent as Call>::MAJOR, 2);
        assert_eq!(
            <generated::Rent as Call>::OUTPUT[0].hash,
            <Rent as Call>::OUTPUT[0].hash
        );
        assert_eq!(generated::Deal::SELL.0, Deal::SELL.0);
        assert_eq!(generated::Stall::Tools as u8, Stall::Tools as u8);
    }
}
