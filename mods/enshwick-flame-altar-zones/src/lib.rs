//! Flame altar zones that touch form one cluster: every member grows by the
//! same amount until the cluster has the area of its members' zones without
//! losing any to overlap, no zone ever shrinks, and pieces can be built across
//! the seams between members.
//!
//! The zones are solved on the server, which the game trusts for them; the seam
//! check runs on both sides, since the client predicts placement.

mod altars;
mod seam;
mod zones;

use enshwick_api::abi::Side;
use enshwick_api::{Load, Mod, Off};
use enshwick_geometry::service::{self, AltarZone, ZonesService};

const DEFAULTS: &str = "\
# Altars whose zones touch grow together and share their area.
clusters = true
# Pieces may be placed across the seams between the zones of one cluster.
building_across_seams = true
";

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    clusters: bool,
    building_across_seams: bool,
}

/// The mod's state.
#[derive(Debug)]
pub struct FlameAltarZones {
    clusters: bool,
    seams: bool,
}

extern "system" fn zones(out: *mut AltarZone, capacity: usize) -> usize {
    let zones = std::panic::catch_unwind(altars::zones).unwrap_or_default();
    if !out.is_null() {
        for (i, (altar, zone)) in zones.iter().take(capacity).enumerate() {
            let record = AltarZone {
                altar: *altar,
                min: zone.min,
                max: zone.max,
            };
            // SAFETY: the caller gives room for `capacity` zones at `out`.
            unsafe { out.wrapping_add(i).write(record) };
        }
    }
    zones.len()
}

static ZONES: ZonesService = ZonesService { zones };

impl Mod for FlameAltarZones {
    fn manifest() -> &'static enshwick_api::abi::Manifest {
        enshwick_api::manifest!(id: "flame-altar-zones", side: BOTH, join = true)
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.settings(DEFAULTS)?;
        if settings.building_across_seams {
            seam::link(load)?;
        }
        if settings.clusters && load.host().build().side == Side::SERVER {
            altars::link(load)?;
            load.publish(service::NAME, service::VERSION, &ZONES)?;
        }
        Ok(Self {
            clusters: settings.clusters,
            seams: settings.building_across_seams,
        })
    }

    fn ready(&'static self, _host: enshwick_api::Host) {
        enshwick_api::info!(
            "clusters {}, building across seams {}",
            self.clusters,
            self.seams
        );
    }
}

enshwick_api::declare_mod!(FlameAltarZones);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_settings_parse() {
        let settings: Settings = toml::from_str(DEFAULTS).unwrap();
        assert!(settings.clusters && settings.building_across_seams);
    }
}
