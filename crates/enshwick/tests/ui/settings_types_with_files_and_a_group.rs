/// Comfort.
#[enshwick::config(group)]
#[derive(Default)]
struct Comfort {
    /// Comfort per piece.
    per_piece: u8,
}

/// Settings.
#[enshwick::config]
#[derive(Default)]
struct Settings {
    /// The comfort rules.
    #[config(group)]
    comfort: Comfort,
}

/// Altar settings.
#[enshwick::config(file = "altars")]
#[derive(Default)]
struct Altars {
    /// Altars per player.
    altars: u8,
}

/// Altar zones.
#[enshwick::config(file = "altar-zones")]
#[derive(Default)]
struct AltarZones {
    /// Zone size.
    size: u8,
}

/// Altar zone rules, `_` spelt apart from `-`.
#[enshwick::config(file = "altar_zones")]
#[derive(Default)]
struct AltarZoneRules {
    /// Zone growth.
    growth: u8,
}

fn main() {
    let _ = (
        Settings::default(),
        Altars::default(),
        AltarZones::default(),
        AltarZoneRules::default(),
    );
}
