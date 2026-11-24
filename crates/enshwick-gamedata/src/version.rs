//! The game's own build numbers: Steam's build id and the version the game
//! reports.

/// Steam's id for one published build of the game, the `buildid` that
/// steamcmd and `steamapps/appmanifest_*.acf` show; it grows with each build.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::SteamBuild;
///
/// assert!(SteamBuild::new(26_083_076) > SteamBuild::new(26_082_781));
/// assert_eq!(SteamBuild::new(26_083_076).to_string(), "26083076");
/// ```
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Deserialize, serde::Serialize,
)]
#[serde(transparent)]
pub struct SteamBuild(pub(crate) u64);

impl SteamBuild {
    /// Wraps a build id read from Steam.
    ///
    /// # Examples
    ///
    /// See [`SteamBuild`].
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// The id as Steam spells it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_gamedata::SteamBuild;
    ///
    /// assert_eq!(SteamBuild::new(7).get(), 7);
    /// ```
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// The build Steam installed in the game folder `game_dir` for `side`,
    /// from the appmanifest Steam keeps beside its library (`steamapps/`,
    /// two folders up) or `SteamCMD` inside the install (`steamapps/` in it);
    /// `None` where neither reads.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_gamedata::{Side, SteamBuild};
    ///
    /// assert_eq!(SteamBuild::installed("no-such-game".as_ref(), Side::Server), None);
    /// ```
    #[must_use]
    pub fn installed(game_dir: &std::path::Path, side: crate::Side) -> Option<Self> {
        let file = format!("appmanifest_{}.acf", side.steam_app());
        let text = std::fs::read_to_string(game_dir.join("steamapps").join(&file))
            .ok()
            .or_else(|| {
                let library = game_dir.parent()?.parent()?.join(&file);
                std::fs::read_to_string(library).ok()
            })?;
        Self::from_manifest(&text)
    }

    /// The `buildid` of an appmanifest's text.
    fn from_manifest(text: &str) -> Option<Self> {
        text.lines().find_map(|line| {
            let mut quoted = line.split('"').skip(1).step_by(2);
            (quoted.next()? == "buildid")
                .then(|| quoted.next()?.parse().ok().map(Self))
                .flatten()
        })
    }
}

/// The game's own version number, which a dedicated server puts first after
/// the protocol in its Steam keywords; several Steam builds can share one.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::GameVersion;
///
/// assert_eq!(GameVersion::new(1_269_579).to_string(), "1269579");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Deserialize)]
#[serde(transparent)]
pub struct GameVersion(pub(crate) u32);

impl GameVersion {
    /// Wraps a version number read from the game.
    ///
    /// # Examples
    ///
    /// See [`GameVersion`].
    #[must_use]
    pub const fn new(number: u32) -> Self {
        Self(number)
    }

    /// The number as the game spells it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_gamedata::GameVersion;
    ///
    /// assert_eq!(GameVersion::new(7).get(), 7);
    /// ```
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Side;

    /// The Steam build is the appmanifest's `buildid`, beside a Steam library
    /// (two folders up) or inside a `SteamCMD` install, also one at a drive's
    /// root.
    #[test]
    fn the_steam_build_comes_from_the_appmanifest_where_steam_keeps_it() {
        let manifest =
            "\"AppState\"\n{\n\t\"appid\"\t\t\"2278520\"\n\t\"buildid\"\t\t\"26430253\"\n}\n";
        let build = Some(SteamBuild::new(26_430_253));
        assert_eq!(SteamBuild::from_manifest(manifest), build);
        assert_eq!(SteamBuild::from_manifest("\"buildid\"\t\"next\"\n"), None);
        let root =
            std::env::temp_dir().join(format!("gamedata-appmanifest-{}", std::process::id()));
        let library = root.join("steamapps");
        let client = library.join("common").join("Enshrouded");
        let server = root.join("server");
        std::fs::create_dir_all(&client).unwrap();
        std::fs::create_dir_all(server.join("steamapps")).unwrap();
        std::fs::write(library.join("appmanifest_1203620.acf"), manifest).unwrap();
        std::fs::write(
            server.join("steamapps").join("appmanifest_2278520.acf"),
            manifest,
        )
        .unwrap();
        // A one-component path has no library two folders up.
        let shallow = std::path::PathBuf::from(format!("gamedata-shallow-{}", std::process::id()));
        std::fs::create_dir_all(shallow.join("steamapps")).unwrap();
        std::fs::write(
            shallow.join("steamapps").join("appmanifest_2278520.acf"),
            manifest,
        )
        .unwrap();
        let found = (
            SteamBuild::installed(&client, Side::Client),
            SteamBuild::installed(&server, Side::Server),
            SteamBuild::installed(&server, Side::Client),
            SteamBuild::installed(&shallow, Side::Server),
        );
        std::fs::remove_dir_all(&root).unwrap();
        std::fs::remove_dir_all(&shallow).unwrap();
        assert_eq!(found, (build, build, None, build));
    }
}
