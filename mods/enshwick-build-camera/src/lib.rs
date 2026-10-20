//! Airborne Architect's building fly mode for every player.
//!
//! The game lets a player enter, and stay in, fly mode when the character is
//! near a building zone, alive and not gliding, may edit the base, and has
//! unlocked the Airborne Architect skill node. The server decides; the client
//! predicts the same gate. Both ask one generic test, `node_unlocked(node,
//! unlocked bits, skill tree)`, which about fifty other places ask too, some
//! with node ids read at run time. The hook answers "unlocked" only for the
//! fly mode node and only at the gate's own call site, told by its return
//! address, so skill points, prerequisites and the skill tree stay vanilla.
//!
//! On a client the one gate serves its own world and, while it hosts, the
//! built-in server's; both get the same answer. `enabled` is a join value: a
//! joined player predicts with the server's.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use enshwick_api::abi::HookFlags;
use enshwick_api::{Load, Mod, Next, Off};

/// The Airborne Architect node in the skill tree, as the gate passes it.
pub const NODE: u32 = 0x4475_e8dd;

/// `node_unlocked(node, unlocked bits, skill tree) -> bool in al`.
type NodeUnlocked = unsafe extern "system" fn(node: u32, bits: *const u64, tree: *const u8) -> u8;

/// Whether fly mode is open to everyone now; the settings handler flips it.
static ENABLED: AtomicBool = AtomicBool::new(false);
/// Where the gate's call to `node_unlocked` returns to; 0 until init.
static GATE_RETURN: AtomicU64 = AtomicU64::new(0);

/// Build camera settings.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick_api::Config)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// Every player may use the building fly mode near a base without the
    /// Airborne Architect skill. The vanilla limits stay: near a building
    /// zone, allowed to edit the base, alive and not gliding.
    /// A joined player runs with the server's value.
    #[config(sync)]
    enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// Whether the hook answers "unlocked" for a call: the fly mode node, asked
/// from the gate, while the mod is enabled.
#[must_use]
pub fn lifts(node: u32, return_address: u64, gate: u64, enabled: bool) -> bool {
    enabled && node == NODE && gate != 0 && return_address == gate
}

/// The mod. Its running state is in statics the hook reads.
#[derive(Debug)]
pub struct BuildCamera;

enshwick_api::detour! {
    static NODE_UNLOCKED: unsafe extern "system" fn(node: u32, bits: *const u64, tree: *const u8) -> u8 =
        node_unlocked, flags = HookFlags::CALLER_FRAME;
}

fn node_unlocked(next: Next<NodeUnlocked>, node: u32, bits: *const u64, tree: *const u8) -> u8 {
    // Fast path first: another node, or the mod is off.
    let enabled = node == NODE && ENABLED.load(Ordering::Relaxed);
    if enabled && let Some(host) = enshwick_api::host() {
        // SAFETY: the frame was taken at entry of this chain.
        let from = unsafe { host.caller_frame().return_address() };
        if lifts(node, from, GATE_RETURN.load(Ordering::Relaxed), enabled) {
            return 1;
        }
    }
    // SAFETY: the rest of the chain takes the arguments the game passed.
    unsafe { next.get()(node, bits, tree) }
}

/// Applies `settings` to the running hook.
fn apply(settings: &Settings) {
    let was = ENABLED.swap(settings.enabled, Ordering::Relaxed);
    if was != settings.enabled || !settings.enabled {
        enshwick_api::info!(
            "fly mode without the skill: {}",
            if settings.enabled { "on" } else { "off" }
        );
    }
}

impl Mod for BuildCamera {
    fn manifest() -> &'static enshwick_api::abi::Manifest {
        enshwick_api::manifest!(
            id: "build-camera",
            side: BOTH,
            join = true,
            updates: github("themaiby/enshwick", "enshwick-build-camera"),
            signed_by: ["RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF"],
            builds: [
                {
                    side: SERVER,
                    guid: "1c129660cd6efc4f9f0b21652d5ff2bf",
                    age: 387,
                    // `player_control_action_sequence`'s fly mode gate.
                    symbols: ["build_camera.gate_return" => 0xc_4875],
                },
                {
                    // The same gate, predicting for the client's world and
                    // deciding for the built-in server's.
                    side: CLIENT,
                    guid: "69f0e9573b32c24a8838d3fff550824c",
                    age: 410,
                    symbols: ["build_camera.gate_return" => 0x2d_8295],
                },
            ],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        GATE_RETURN.store(load.symbol("build_camera.gate_return")?, Ordering::Relaxed);
        load.hook(&NODE_UNLOCKED, load.symbol("skill.node_unlocked")?)?;
        load.on_config_change(|settings: Settings| apply(&settings))?;
        apply(&settings);
        Ok(Self)
    }
}

enshwick_api::declare_mod!(BuildCamera);

#[cfg(test)]
mod tests {
    use enshwick_api::abi::{CallerFrame, Status};
    use enshwick_api::config::Config as _;
    use enshwick_api::events;

    use super::*;

    const GATE: u64 = 0x1_400c_4875;

    #[test]
    fn only_the_fly_node_at_the_gate_is_lifted() {
        assert!(lifts(NODE, GATE, GATE, true));
        assert!(!lifts(NODE, GATE, GATE, false));
        assert!(!lifts(NODE, GATE + 5, GATE, true));
        assert!(!lifts(NODE ^ 1, GATE, GATE, true));
        assert!(!lifts(NODE, 0, 0, true));
    }

    #[test]
    fn enabled_is_the_join_value_and_on_by_default() {
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert!(settings.enabled);
        let synced = settings.synced().unwrap();
        assert_eq!(synced, "{ enabled = true }");
        let mut local: Settings = toml::from_str("enabled = true").unwrap();
        local.merge_synced("{ enabled = false }").unwrap();
        assert!(!local.enabled);
    }

    /// The return address the fake caller frame points at.
    static RETURN: AtomicU64 = AtomicU64::new(0);

    extern "system" fn locked(_node: u32, _bits: *const u64, _tree: *const u8) -> u8 {
        0
    }

    #[test]
    fn the_gate_follows_the_settings_and_the_servers_join_value() {
        let loader = enshwick_test::FakeHost::new()
            .symbol(
                "skill.node_unlocked",
                enshwick_test::address(
                    locked as extern "system" fn(u32, *const u64, *const u8) -> u8,
                ),
            )
            .symbol("build_camera.gate_return", GATE)
            .caller_frame(CallerFrame {
                rsp: RETURN.as_ptr() as u64,
                ..CallerFrame::default()
            })
            .settings("enabled = true\n")
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let detour = loader.detour(&NODE_UNLOCKED).unwrap();
        let ask = |node, from| {
            RETURN.store(from, Ordering::Relaxed);
            // SAFETY: the vanilla stand-in reads nothing.
            unsafe { detour(node, core::ptr::null(), core::ptr::null()) }
        };

        assert_eq!(ask(NODE, GATE), 1);
        assert_eq!(ask(NODE, 0x1_4000_3e78), 0, "another caller stays vanilla");
        assert_eq!(ask(0xa667_75b7, GATE), 0, "another node stays vanilla");

        loader.set_server_synced(Some("{ enabled = false }"));
        assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
        assert_eq!(ask(NODE, GATE), 0, "the server turned it off");

        loader.set_server_synced(None);
        assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
        assert_eq!(ask(NODE, GATE), 1, "the file's value again");
        assert!(loader.faults().is_empty());
    }
}
