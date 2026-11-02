//! A point declared only by its attribute, with a local name: marked
//! `export`, its schema reaches its owner's manifest and no other mod's,
//! its name the owner's exports, and another mod links to it; a point
//! without the flag ships nothing and another mod's link to it is refused,
//! and an exported point opens no event the owner keeps by the same local
//! name, since each export names its kind.
//! This crate's `Cargo.toml` gives the owner's id, `you.hello`.

use std::sync::atomic::{AtomicBool, Ordering};

/// What the owner pays for an item.
#[enshwick::point(name = "price", export, mode = Override, output = u32)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Price {
    /// The item.
    pub(crate) item: u32,
}

/// A point of the owner for itself, without `export`.
#[enshwick::point(name = "hidden", mode = Override, output = u32)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Hidden {
    /// The item.
    pub(crate) item: u32,
}

/// A point of the owner marked `export`, whose local name an event the
/// owner keeps shares: the export, `point:fee`, opens the point alone.
#[enshwick::point(name = "fee", export, mode = Override, output = u32)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct FeePoint {
    /// The item.
    pub(crate) item: u32,
}

/// The owner's event of the same local name, without `export`.
#[enshwick::event(name = "fee")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct FeeEvent {
    /// The item.
    pub(crate) item: u32,
}

/// Whether the extender's link to [`Hidden`] was refused.
static HIDDEN_REFUSED: AtomicBool = AtomicBool::new(false);

/// Whether the extender's subscription to [`FeeEvent`] was refused.
static FEE_EVENT_REFUSED: AtomicBool = AtomicBool::new(false);

/// The mod that owns the points.
mod owner {
    #[expect(
        dead_code,
        reason = "the tests read its manifest alone; the mod never starts"
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
        "#
    );
}

/// The owner again, for the test that frees its manifest while other tests
/// read the first one's.
mod restarted {
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
        "#
    );
}

/// A mod that links to both points.
mod extender {
    use std::sync::atomic::Ordering;

    use enshwick::points::{Order, Verdict};

    pub(super) struct Extender;

    impl enshwick::Mod for Extender {
        fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            load.extend_point(&Order::new(), |price: &super::Price, so_far: &u32| {
                Verdict::Set(so_far + price.item)
            })?;
            let hidden = load.extend_point(&Order::new(), |_: &super::Hidden, so_far: &u32| {
                Verdict::Set(*so_far)
            });
            super::HIDDEN_REFUSED.store(hidden.is_err(), Ordering::Relaxed);
            load.extend_point(&Order::new(), |fee: &super::FeePoint, so_far: &u32| {
                Verdict::Set(so_far + fee.item)
            })?;
            let kept = load.on_mod_event(|_: &super::FeeEvent| {});
            super::FEE_EVENT_REFUSED.store(kept.is_err(), Ordering::Relaxed);
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
    use super::*;

    use enshwick::abi::{Manifest, Status};
    use enshwick::points::Decision;

    /// The manifest `query` returned.
    fn manifest(query: *const Manifest) -> &'static Manifest {
        // SAFETY: `enshwick_mod_query` returns the manifest of
        // `declare_mod!`, valid until its shutdown; only one test shuts a
        // mod down, `restarted`, and reads it only before and after.
        unsafe { &*query }
    }

    /// The schema text of a manifest.
    fn points(query: *const Manifest) -> String {
        // SAFETY: the manifest's schema text lives as long as the manifest.
        unsafe { manifest(query).points.as_str() }
            .unwrap_or_default()
            .to_owned()
    }

    /// The local names a manifest exports.
    fn exports(query: *const Manifest) -> Vec<String> {
        let manifest = manifest(query);
        // SAFETY: `exports` points at `export_count` names that live as long
        // as the manifest.
        let names = unsafe { core::slice::from_raw_parts(manifest.exports, manifest.export_count) };
        names
            .iter()
            // SAFETY: each name lives as long as the manifest.
            .map(|name| unsafe { name.as_str() }.unwrap_or_default().to_owned())
            .collect()
    }

    #[test]
    fn the_owner_s_manifest_carries_the_exported_point_alone() {
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
        assert_eq!(names, ["you.hello:fee", "you.hello:price"]);
        assert_eq!(
            exports(owner::enshwick_mod_query()),
            ["point:fee", "point:price"]
        );
    }

    #[test]
    fn a_shutdown_frees_the_manifest_and_the_next_query_builds_it_again() {
        let before = points(restarted::enshwick_mod_query());
        restarted::enshwick_mod_shutdown();
        assert_eq!(points(restarted::enshwick_mod_query()), before);
    }

    #[test]
    fn another_mod_s_manifest_carries_none_of_the_owner_s_schemas() {
        assert_eq!(points(extender::enshwick_mod_query()), "");
        assert!(exports(extender::enshwick_mod_query()).is_empty());
    }

    #[test]
    fn another_mod_links_to_the_exported_point_and_is_refused_the_other() {
        let offered = exports(owner::enshwick_mod_query());
        let offered: Vec<&str> = offered.iter().map(String::as_str).collect();
        let loader = enshwick::test::FakeHost::new()
            .exports("you.hello", &offered)
            .install();
        // SAFETY: both functions are the exports of `declare_mod!(Extender)`.
        let status =
            unsafe { loader.start(extender::enshwick_mod_init, extender::enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(
            loader.attached_points(),
            ["you.hello:price", "you.hello:fee"]
        );
        assert!(HIDDEN_REFUSED.load(Ordering::Relaxed));
        assert!(
            FEE_EVENT_REFUSED.load(Ordering::Relaxed),
            "`point:fee` opens no event `fee`"
        );
        assert_eq!(
            loader.ask_point(&Price { item: 2 }, 3),
            Decision::Changed(5)
        );
    }
}
