//! Development only, never shipped: logs what a running client does where
//! the platform still needs engine facts, to `logs/enshwick/mods/research.log`.
//!
//! Off until `enabled = true` in `mod-configs/research.toml`; read at start.
//! On a client it looks at the item tooltip (which argument names the
//! hovered item) and at join attempts (what a refused join leaves behind).
//! `/research mark` starts a new section of the log while the client hosts
//! a game.

mod join;
mod memory;
mod tooltip;

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
    /// Look at join attempts and the menus after them.
    #[serde(default = "yes")]
    #[config(client)]
    join: bool,
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
            join: yes(),
            dump_interval_ms: interval_ms(),
            max_dumps: max_dumps(),
        }
    }
}

/// The research mod.
#[derive(Debug)]
pub struct Research;

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
                ],
            }],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        if !settings.enabled {
            return Ok(Self);
        }
        if load.host().build().side == Side::CLIENT {
            if settings.tooltip {
                match tooltip::link(load, settings.dump_interval_ms, settings.max_dumps) {
                    Ok(hooked) => {
                        enshwick_loader_api::info!("R1 {hooked} tooltip functions looked at");
                    }
                    Err(off) => enshwick_loader_api::warn!("R1 off: {off}"),
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
                ["status"] => match tooltip::dumps() {
                    Some((dumps, most)) => {
                        format!("{dumps} of {most} tooltip dumps since the last mark").into()
                    }
                    None => "the tooltip is not looked at in this process".into(),
                },
                _ => "/research mark [text and values] | status".into(),
            },
        )?;
        enshwick_loader_api::info!("research on");
        Ok(Self)
    }
}

enshwick_loader_api::declare_mod!(Research);
