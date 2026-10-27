//! A call's schema: what `cargo enshwick call` reads from its server.

use enshwick::ecs::Entity;

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

/// The rent of a stall, in coins.
#[enshwick::call(name = "you.stalls:rent", output = u64, version = 2)]
#[derive(Clone, Copy, Debug)]
pub struct Rent {
    /// The player's character.
    pub character: Entity,
    /// What the player does.
    pub deal: Deal,
    /// Whether the stall is the player's own.
    pub own: bool,
}

/// The declaration `cargo enshwick call` writes from that schema.
#[path = "../../cargo-enshwick/tests/fixtures/stalls_rent.rs"]
pub mod generated;

#[cfg(test)]
mod tests {
    use enshwick::services::Call;

    use super::{Deal, Rent, generated};

    #[test]
    fn the_schema_is_the_one_cargo_enshwick_is_tested_with() {
        let manifest =
            enshwick::manifest!(id: "you.stalls", side: SERVER, version: "1.2.0", calls: [Rent]);
        // SAFETY: the manifest's static text.
        let text = unsafe { manifest.points.as_str() }.unwrap_or_default();
        assert_eq!(
            text,
            include_str!("../../cargo-enshwick/tests/fixtures/stalls.calls.toml")
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
    }
}
