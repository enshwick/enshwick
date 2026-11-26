//! The vanilla workshops the API names against the installed client's
//! archive, which never enters the repository: these run only where
//! `ENSH_CLIENT_EXE` names the client, and pass trivially elsewhere.

#[cfg(test)]
mod tests {
    use std::path::Path;

    use enshwick::content::workshops::{FLAME_ALTAR, WORKSHOP_REGISTRY};
    use enshwick_gamedata::archive::Archive;

    #[test]
    fn the_workshop_registry_holds_the_flame_altar_s_workshop() {
        let Some(exe) = std::env::var_os("ENSH_CLIENT_EXE") else {
            return;
        };
        let dir = Path::new(&exe).parent().unwrap();
        let archive = Archive::open(dir, "enshrouded").unwrap();
        let key = archive
            .keys()
            .into_iter()
            .find(|key| key.guid == WORKSHOP_REGISTRY);
        let registry = archive.resource(&key.unwrap()).unwrap();
        // A workshop's GUID and its game id stand side by side (archive 1287369).
        let entry = [&FLAME_ALTAR.guid().0[..], &FLAME_ALTAR.id().to_le_bytes()].concat();
        assert!(registry.windows(20).any(|w| w == entry));
    }
}
