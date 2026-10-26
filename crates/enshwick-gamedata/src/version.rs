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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Deserialize)]
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
