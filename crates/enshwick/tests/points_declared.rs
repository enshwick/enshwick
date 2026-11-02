//! A point declared only by its attribute, with a local name: its schema
//! reaches its owner's manifest and no other mod's, and another mod links
//! to it. This crate's `Cargo.toml` gives the owner's id, `you.hello`.

/// What the owner pays for an item.
#[enshwick::point(name = "price", mode = Override, output = u32)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Price {
    /// The item.
    pub(crate) item: u32,
}

/// The mod that owns the point.
mod owner {
    #[expect(
        dead_code,
        reason = "the test reads its manifest alone; the mod never starts"
    )]
    pub(super) struct Hello;

    impl enshwick::Mod for Hello {
        fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Hello,
        r#"
        id = "you.hello"
        side = "server"
        exports = ["price"]
        "#
    );
}

/// A mod that links to the point.
mod extender {
    use enshwick::points::{Order, Verdict};

    pub(super) struct Extender;

    impl enshwick::Mod for Extender {
        fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            load.extend_point(&Order::new(), |price: &super::Price, so_far: &u32| {
                Verdict::Set(so_far + price.item)
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Extender,
        r#"
        id = "you.extender"
        side = "server"

        [recommends]
        hello = "you.hello"
        "#
    );
}

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::points::Decision;

    use super::{Price, extender, owner};

    /// The schema text of a manifest.
    fn points(manifest: *const enshwick::abi::Manifest) -> String {
        // SAFETY: `enshwick_mod_query` returns the `'static` manifest of
        // `declare_mod!`, which no thread writes.
        let manifest = unsafe { &*manifest };
        // SAFETY: the manifest's schema text lives for the process.
        unsafe { manifest.points.as_str() }
            .unwrap_or_default()
            .to_owned()
    }

    #[test]
    fn the_owner_s_manifest_carries_a_point_declared_only_by_its_attribute() {
        let text = points(owner::enshwick_mod_query());
        let schema: toml::Table = toml::from_str(&text).unwrap();
        assert_eq!(schema["mod"].as_str(), Some("you.hello"));
        assert_eq!(schema["version"].as_str(), Some(enshwick::VERSION));
        let names: Vec<&str> = schema["point"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|p| p["name"].as_str())
            .collect();
        assert_eq!(names, ["you.hello:price"]);
    }

    #[test]
    fn another_mod_s_manifest_carries_none_of_the_owner_s_schemas() {
        assert_eq!(points(extender::enshwick_mod_query()), "");
    }

    #[test]
    fn another_mod_links_to_the_point() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of `declare_mod!(Extender)`.
        let status =
            unsafe { loader.start(extender::enshwick_mod_init, extender::enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.attached_points(), ["you.hello:price"]);
        assert_eq!(
            loader.ask_point(&Price { item: 2 }, 3),
            Decision::Changed(5)
        );
    }
}
