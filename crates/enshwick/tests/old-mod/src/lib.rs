//! A mod that only starts, built against the last published API.

struct OldMod;

impl enshwick::Mod for OldMod {
    fn manifest() -> &'static enshwick::abi::Manifest {
        enshwick::manifest!(id: "you.old-mod", side: SERVER)
    }

    fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
        Ok(Self)
    }
}

enshwick::declare_mod!(OldMod);
