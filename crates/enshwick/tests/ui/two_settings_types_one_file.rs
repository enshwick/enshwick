/// Altar settings.
#[enshwick::config(file = "altars")]
#[derive(Default)]
struct AltarCount {
    /// Altars per player.
    altars: u8,
}

/// Altar zones.
#[enshwick::config(file = "altars")]
#[derive(Default)]
struct AltarZones {
    /// Zone size.
    size: u8,
}

fn main() {}
