//! Development only, never shipped: logs what a running client does where
//! the platform still needs engine facts, to `logs/enshwick/mods/research.log`.
//!
//! Off until `enabled = true` in `mod-configs/research.toml`; read at start.
//! On a client it looks at the item tooltip (which argument names the
//! hovered item), at the interface (scroll areas, tab bars, text input) and
//! at join attempts (what a refused join leaves behind). On both sides it
//! logs the entities near each player, so a client's and a server's log can
//! be joined (which ids a client gives the server's entities), the health
//! the game's impact programs set (heals, revives, scripted kills), and
//! whether any thread runs on a fiber.
//! `/research mark` starts a new section of the log while the client hosts
//! a game.

mod entities;
mod fibers;
mod health;
mod join;
mod memory;
mod tooltip;
mod ui;

use enshwick_loader_api::abi::Side;
use enshwick_loader_api::{Load, Mod, Off};

const fn yes() -> bool {
    true
}

const fn interval_ms() -> u64 {
    1000
}

const fn max_dumps() -> u32 {
    300
}

/// Research settings, read at start.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent switches of a development mod's settings file"
)]
struct Settings {
    /// Log what the client does where the platform needs engine facts;
    /// development only.
    #[serde(default)]
    #[config(client)]
    enabled: bool,
    /// Look at the item tooltip: what each of its functions is handed.
    #[serde(default = "yes")]
    #[config(client)]
    tooltip: bool,
    /// Look at the interface: scroll areas, tab bars and text input.
    #[serde(default = "yes")]
    #[config(client)]
    ui: bool,
    /// Look at join attempts and the menus after them.
    #[serde(default = "yes")]
    #[config(client)]
    join: bool,
    /// Log the entities near each player every five seconds, on the client
    /// and on the server, with their ids, templates and positions.
    #[serde(default = "yes")]
    #[config(client)]
    entities: bool,
    /// Log the health the game's impact programs set, outside hits: heals,
    /// revives, scripted kills; on the client and on the server.
    #[serde(default = "yes")]
    #[config(client)]
    impact_health: bool,
    /// Look for threads on fibers for five minutes after the start.
    #[serde(default = "yes")]
    #[config(client)]
    fibers: bool,
    /// The least time between two dumps of one tooltip function, in
    /// milliseconds, unless a mark comes between.
    #[serde(default = "interval_ms")]
    #[config(client, range = 0..=60_000)]
    dump_interval_ms: u64,
    /// The most tooltip dumps between two marks.
    #[serde(default = "max_dumps")]
    #[config(client, range = 1..=100_000)]
    max_dumps: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            tooltip: yes(),
            ui: yes(),
            join: yes(),
            entities: yes(),
            impact_health: yes(),
            fibers: yes(),
            dump_interval_ms: interval_ms(),
            max_dumps: max_dumps(),
        }
    }
}

/// The research mod: whether it looks for fibers once ready.
#[derive(Debug)]
pub struct Research {
    fibers: bool,
}

impl Mod for Research {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(
            id: "research",
            side: BOTH,
            name: "Research",
            builds: [{
                side: CLIENT,
                guid: "69f0e9573b32c24a8838d3fff550824c",
                age: 410,
                symbols: [
                    "research.tooltip" => 0xbf_ef20,
                    "research.tooltip_body" => 0xbf_f750,
                    "research.stat_rows" => 0xc9_dd10,
                    "research.viewer_compare" => 0xb6_7e00,
                    "research.viewer_b562c0" => 0xb5_62c0,
                    "research.viewer_b56b80" => 0xb5_6b80,
                    "research.viewer_beb220" => 0xbe_b220,
                    "research.viewer_bebb60" => 0xbe_bb60,
                    "research.viewer_c3da50" => 0xc3_da50,
                    "research.viewer_b55140" => 0xb5_5140,
                    "research.scroll_begin" => 0xa2_fb10,
                    "research.scroll_end" => 0xa3_1c50,
                    "research.tabs" => 0xcc_a5a0,
                    "research.text_dialog" => 0xcc_8e30,
                    "research.text_field" => 0xb1_2120,
                    "research.client_cursor" => 0x28_fa70,
                    "research.entities_with" => 0x9a_71d0,
                    "research.entity_slot" => 0x9a_8070,
                    "research.type_index" => 0x9a_5080,
                    "research.component_read_raw" => 0x9a_5ec0,
                    "research.impact_attribute" => 0x3e_2cc0,
                ],
            }, {
                side: SERVER,
                guid: "1c129660cd6efc4f9f0b21652d5ff2bf",
                age: 387,
                symbols: [
                    "research.impact_attribute" => 0x18_4b90,
                ],
            }],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        if !settings.enabled {
            return Ok(Self { fibers: false });
        }
        let client = load.host().build().side == Side::CLIENT;
        if settings.entities {
            entities_link(load, client);
        }
        if settings.impact_health {
            match health::link(load) {
                Ok(()) => enshwick_loader_api::info!("R17 impact health writes looked at"),
                Err(off) => enshwick_loader_api::warn!("R17 off: {off}"),
            }
        }
        if client {
            if settings.tooltip {
                match tooltip::link(load, settings.dump_interval_ms, settings.max_dumps) {
                    Ok(hooked) => {
                        enshwick_loader_api::info!("R1 {hooked} tooltip functions looked at");
                    }
                    Err(off) => enshwick_loader_api::warn!("R1 off: {off}"),
                }
            }
            if settings.ui {
                match ui::link(load) {
                    Ok(hooked) => {
                        enshwick_loader_api::info!(
                            "R2 R3 R5 {hooked} interface functions looked at"
                        );
                    }
                    Err(off) => enshwick_loader_api::warn!("R2 R3 R5 off: {off}"),
                }
            }
            if settings.join {
                match join::link(load) {
                    Ok(()) => enshwick_loader_api::info!("R8 join attempts looked at"),
                    Err(off) => enshwick_loader_api::warn!("R8 off: {off}"),
                }
            }
        }
        load.add_command(
            "research",
            "development research; /research mark [text and values] | status",
            |ctx, args| match args {
                _ if !ctx.is_admin() => "/research is for administrators".into(),
                ["mark", words @ ..] => {
                    let values: Vec<u32> = words.iter().filter_map(|w| w.parse().ok()).collect();
                    let mark = tooltip::mark(&values);
                    enshwick_loader_api::info!(
                        "===== mark {mark} by {}: {} (values looked for: {values:?}) =====",
                        ctx.player.name,
                        words.join(" ")
                    );
                    format!("mark {mark}; hover the item now").into()
                }
                ["status"] => {
                    let (health, other) = health::counts();
                    let impact = format!("; impact writes: {health} health, {other} other");
                    match tooltip::dumps() {
                        Some((dumps, most)) => {
                            format!("{dumps} of {most} tooltip dumps since the last mark{impact}")
                                .into()
                        }
                        None => {
                            format!("the tooltip is not looked at in this process{impact}").into()
                        }
                    }
                }
                _ => "/research mark [text and values] | status".into(),
            },
        )?;
        enshwick_loader_api::info!("research on");
        Ok(Self {
            fibers: settings.fibers,
        })
    }

    fn ready(&'static self, _host: enshwick_loader_api::Host) {
        if self.fibers {
            fibers::start();
        }
    }
}

/// Links R11 on the server side and, on a client, on the client side.
fn entities_link(load: &mut Load<'_>, client: bool) {
    let world = match load.world() {
        Ok(world) => world,
        Err(off) => {
            enshwick_loader_api::warn!("R11 off: {off}");
            return;
        }
    };
    match entities::link_server(load, world) {
        Ok(()) => enshwick_loader_api::info!("R11 server sweeps every 5 s while players are in"),
        Err(off) => enshwick_loader_api::warn!("R11 server side off: {off}"),
    }
    if client {
        match entities::link_client(load, &world) {
            Ok(()) => enshwick_loader_api::info!("R11 client sweeps every 5 s in a game"),
            Err(off) => enshwick_loader_api::warn!("R11 client side off: {off}"),
        }
    }
}

enshwick_loader_api::declare_mod!(Research);
