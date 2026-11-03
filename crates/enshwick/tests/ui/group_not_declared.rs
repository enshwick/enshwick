/// Comfort.
#[enshwick::config]
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

fn main() {}
