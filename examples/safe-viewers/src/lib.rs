//! Example: extends the Safes mod's access rule, `safes:access`. Players
//! listed as viewers may open every Safe, and with `owner_removes_only` no
//! one but a Safe's owner removes it, administrators included.

use std::sync::Arc;

use arc_swap::ArcSwap;
use enshwick_loader_api::points::safes::{Access, SafeAction};
use enshwick_loader_api::points::{Order, Verdict};
use enshwick_loader_api::{Load, Mod, Off};

/// Safe viewers settings.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// Account keys, 16 hex digits as the Safes mod's owners file lists
    /// them, of players who may open every Safe but never remove it.
    #[config(runtime)]
    viewers: Vec<String>,
    /// Only a Safe's owner picks it up or dismantles it, administrators
    /// included.
    #[config(runtime)]
    owner_removes_only: bool,
}

/// The rules the link reads, swapped whole when the settings change.
#[derive(Debug, Default)]
struct Rules {
    viewers: Vec<u64>,
    owner_removes_only: bool,
}

impl Rules {
    fn from_settings(settings: &Settings) -> Self {
        let viewers = settings
            .viewers
            .iter()
            .filter_map(|key| {
                let parsed = u64::from_str_radix(key.trim(), 16).ok();
                if parsed.is_none() {
                    enshwick_loader_api::warn!("viewer {key:?} is no account key; skipped");
                }
                parsed
            })
            .collect();
        Self {
            viewers,
            owner_removes_only: settings.owner_removes_only,
        }
    }

    /// The link's answer: the Safes mod's own rule, `may`, comes first.
    fn judge(&self, access: &Access, may: bool) -> Verdict<bool> {
        if access.action == SafeAction::REMOVE
            && self.owner_removes_only
            && access.player != access.owner
        {
            return Verdict::Refuse;
        }
        if access.action == SafeAction::USE && !may && self.viewers.contains(&access.player) {
            return Verdict::Set(true);
        }
        Verdict::Pass
    }
}

static RULES: std::sync::LazyLock<ArcSwap<Rules>> =
    std::sync::LazyLock::new(|| ArcSwap::from_pointee(Rules::default()));

/// The example's state.
#[derive(Debug)]
pub struct SafeViewers;

impl Mod for SafeViewers {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(
            id: "safe-viewers",
            side: BOTH,
            recommends: ["safes" => ">=0.0.0-dev, <1.0"],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        RULES.store(Arc::new(Rules::from_settings(&settings)));
        load.on_config_change(|settings: Settings| {
            RULES.store(Arc::new(Rules::from_settings(&settings)));
        })?;
        load.extend_point(&Order::new(), |access: &Access, may: &bool| {
            RULES.load().judge(access, *may)
        })?;
        Ok(Self)
    }
}

enshwick_loader_api::declare_mod!(SafeViewers);

#[cfg(test)]
mod tests {
    use enshwick_loader_api::abi::Status;
    use enshwick_loader_api::ecs::Entity;
    use enshwick_loader_api::points::Decision;
    use enshwick_loader_api::points::safes::{Access, SafeAction};

    const OWNER: u64 = 0x1;
    const VIEWER: u64 = 0x00ab_cdef_0123_4567;

    fn access(player: u64, action: SafeAction, admin: bool) -> Access {
        Access {
            player,
            owner: OWNER,
            character: Entity(10),
            safe: Entity(500),
            action,
            admin,
            allowed: false,
        }
    }

    #[test]
    fn viewers_open_and_only_owners_remove() {
        let loader = enshwick_loader_api::test::FakeHost::new()
            .settings("viewers = [\"00abcdef01234567\", \"nope\"]\nowner_removes_only = true\n")
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(super::enshwick_mod_init, super::enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(loader.attached_points(), ["safes:access"]);
        let ask =
            |player, action, admin, may| loader.ask_point(&access(player, action, admin), may);
        assert_eq!(
            ask(VIEWER, SafeAction::USE, false, false),
            Decision::Changed(true)
        );
        assert_eq!(
            ask(VIEWER, SafeAction::REMOVE, false, false),
            Decision::Refused
        );
        assert_eq!(ask(7, SafeAction::USE, false, false), Decision::Kept(false));
        assert_eq!(
            ask(7, SafeAction::REMOVE, true, true),
            Decision::Refused,
            "an admin"
        );
        assert_eq!(
            ask(OWNER, SafeAction::REMOVE, false, true),
            Decision::Kept(true)
        );
        assert!(loader.faults().is_empty());
    }
}
