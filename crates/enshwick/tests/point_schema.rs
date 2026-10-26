//! A point's schema: what `cargo enshwick point` reads from its owner.

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

/// A "fair" price, in coins.
#[enshwick::point(name = "you.stalls:price", mode = Override, output = u32, refusable)]
#[derive(Clone, Copy, Debug)]
pub struct Price {
    /// The player's character.
    pub character: Entity,
    /// What the player does.
    pub deal: Deal,
    /// Whether the stall is the player's own.
    pub own: bool,
}

/// The definition `cargo enshwick point` writes from that schema.
#[path = "../../cargo-enshwick/tests/fixtures/stalls_price.rs"]
pub mod generated;

#[cfg(test)]
mod tests {
    use enshwick::points::Point;

    use super::{Deal, Price, generated};

    #[test]
    fn the_manifest_carries_the_schema_of_every_point_it_defines() {
        let manifest =
            enshwick::manifest!(id: "you.stalls", side: SERVER, version: "1.2.0", points: [Price]);
        // SAFETY: the manifest's static text.
        let text = unsafe { manifest.points.as_str() }.unwrap_or_default();
        let schema: toml::Table = toml::from_str(text).unwrap();
        assert_eq!(schema["mod"].as_str(), Some("you.stalls"));
        assert_eq!(schema["version"].as_str(), Some("1.2.0"));
        let point = &schema["point"][0];
        assert_eq!(point["name"].as_str(), Some("you.stalls:price"));
        assert_eq!(point["mode"].as_str(), Some("override"));
        assert_eq!(point["refusable"].as_bool(), Some(true));
        assert_eq!(point["output"].as_str(), Some("u32"));
        let layout: Vec<String> = <Price as Point>::LAYOUT
            .iter()
            .map(|f| f.hash.to_string())
            .collect();
        let written: Vec<&str> = point["layout"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(toml::Value::as_str)
            .collect();
        assert_eq!(written, layout);
        let types = schema["type"].as_array().unwrap();
        let price = &types[0];
        assert_eq!(price["doc"].as_str(), Some("A \"fair\" price, in coins."));
        let fields: Vec<(&str, &str)> = price["field"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| (f["name"].as_str().unwrap(), f["type"].as_str().unwrap()))
            .collect();
        assert_eq!(
            fields,
            [
                ("character", "enshwick.entity"),
                ("deal", "Deal"),
                ("own", "bool")
            ]
        );
        let deal = &types[1];
        assert_eq!(deal["kind"].as_str(), Some("values"));
        assert_eq!(deal["repr"].as_str(), Some("u32"));
        assert_eq!(deal["value"][1]["name"].as_str(), Some("SELL"));
        assert_eq!(deal["value"][1]["value"].as_integer(), Some(1));
    }

    #[test]
    fn a_mod_without_points_carries_no_schema() {
        let manifest = enshwick::manifest!(id: "you.quiet", side: SERVER);
        // SAFETY: the manifest's static text.
        assert_eq!(unsafe { manifest.points.as_str() }, Some(""));
    }

    #[test]
    fn the_schema_is_the_one_cargo_enshwick_is_tested_with() {
        let manifest =
            enshwick::manifest!(id: "you.stalls", side: SERVER, version: "1.2.0", points: [Price]);
        // SAFETY: the manifest's static text.
        let text = unsafe { manifest.points.as_str() }.unwrap_or_default();
        assert_eq!(
            text,
            include_str!("../../cargo-enshwick/tests/fixtures/stalls.points.toml")
        );
    }

    #[test]
    fn the_written_definition_matches_the_owner_s_point() {
        let owner = <Price as Point>::LAYOUT.iter().map(|f| f.hash);
        let written = <generated::Price as Point>::LAYOUT.iter().map(|f| f.hash);
        assert!(owner.eq(written));
        assert_eq!(<generated::Price as Point>::NAME, <Price as Point>::NAME);
        assert_eq!(
            <generated::Price as Point>::REFUSE,
            <Price as Point>::REFUSE
        );
        assert_eq!(
            <generated::Price as Point>::OUTPUT[0].hash,
            <Price as Point>::OUTPUT[0].hash
        );
        assert_eq!(generated::Deal::SELL.0, Deal::SELL.0);
    }
}

/// A limit, with an example that only compiles in its owner.
///
/// # Examples
///
/// ```
/// let limit = 4;
/// ```
#[enshwick::point(name = "you.stalls:limit", mode = Observe, output = u32)]
#[derive(Clone, Copy, Debug)]
pub struct Limit {
    /// The level.
    ///
    /// # Panics
    ///
    /// Never.
    pub level: u32,
}

#[cfg(test)]
mod docs {
    use enshwick::points::Point;

    #[test]
    fn a_schema_s_docs_stop_before_the_first_heading() {
        let schema = <super::Limit as Point>::SCHEMA;
        assert!(
            schema.contains("doc = \"A limit, with an example that only compiles in its owner.\"")
        );
        assert!(schema.contains("doc = \"The level.\""));
        assert!(
            !schema.contains("Examples") && !schema.contains("Panics"),
            "{schema}"
        );
    }
}
