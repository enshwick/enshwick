//! The world's players: who is in it, the world's key, the accounts that
//! join, and the chat lines, notices and renames the mod sends them.

use enshwick_core::abi::Header;
use enshwick_core::abi::JoinCall;
use enshwick_core::abi::PlayerInfo;
use enshwick_core::abi::RenameCall;
use enshwick_core::abi::Str;

use crate::test::{FakeHost, Installed, table};

impl FakeHost {
    /// The key of the world the server runs, its save's stem, for
    /// `World::key`; while it is set, a world runs and
    /// [`data_dir`](Self::data_dir) answers, so world stores open.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().world_key("world0").install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn world_key(mut self, key: &str) -> Self {
        self.world_key = Some(key.to_owned());
        self
    }

    /// A connected player with a character in the world.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Header, PlayerInfo};
    ///
    /// let player = PlayerInfo {
    ///     header: Header::of::<PlayerInfo>(),
    ///     slot: 0,
    ///     entity: enshwick::Entity(2),
    ///     key: enshwick::PlayerKey::new(7),
    ///     permissions: PlayerInfo::CAN_KICK_BAN,
    ///     name_len: 0,
    ///     name: [0; 64],
    /// };
    /// let loader = enshwick::test::FakeHost::new().player(player).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn player(mut self, player: PlayerInfo) -> Self {
        self.players.push(player);
        self
    }
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// The chat lines the mod sent, as `(account key, text)`; key 0 for
    /// everyone.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.chat().is_empty());
    /// ```
    #[must_use]
    pub fn chat(&self) -> Vec<(enshwick_core::PlayerKey, String)> {
        table::with(Vec::new(), |s| s.chat.clone())
    }

    /// The notices the mod showed on screen, `(account key, text)`, in
    /// order; a player not given with [`FakeHost::net_peer`] has a client
    /// without Enshwick, and gets chat lines instead.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.notices().is_empty());
    /// ```
    #[must_use]
    pub fn notices(&self) -> Vec<(enshwick_core::PlayerKey, String)> {
        table::with(Vec::new(), |s| s.notices.clone())
    }

    /// Asks the mod's rename filters about `player` renaming `entity` to
    /// `text`; whether every filter lets it through.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.rename(enshwick::Entity(4096), None, "Tools"));
    /// ```
    #[must_use]
    pub fn rename(
        &self,
        entity: enshwick_core::Entity,
        player: Option<&PlayerInfo>,
        text: &str,
    ) -> bool {
        let filters = table::with(Vec::new(), |s| s.rename_filters.clone());
        let call = RenameCall {
            header: Header::of::<RenameCall>(),
            entity: enshwick_core::Entity(entity.0),
            known: u8::from(player.is_some()),
            player: player.copied().unwrap_or(PlayerInfo {
                header: Header::of::<PlayerInfo>(),
                slot: u32::MAX,
                entity: enshwick_core::Entity(0),
                key: enshwick_core::PlayerKey::new(0),
                permissions: 0,
                name_len: 0,
                name: [0; 64],
            }),
            text: Str::new(text),
        };
        filters.iter().all(|filter| {
            // SAFETY: a valid call; `user` is the pointer the mod registered with.
            filter.run(|callback, user| unsafe { callback(&raw const call, user) }) != 0
        })
    }

    /// Asks the mod's join rules about an account joining, as the server asks
    /// them before the account's player enters the world; whether every rule
    /// lets it in. `None` stands for an account the game gives no key.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.join(Some(enshwick::PlayerKey::new(7))));
    /// ```
    #[must_use]
    pub fn join(&self, player: Option<enshwick_core::PlayerKey>) -> bool {
        let rules = table::with(Vec::new(), |s| s.join_rules.clone());
        let call = JoinCall {
            header: Header::of::<JoinCall>(),
            player: player.unwrap_or(enshwick_core::PlayerKey::new(0)),
        };
        rules.iter().all(|rule| {
            // SAFETY: a valid call; `user` is the pointer the mod registered with.
            rule.run(|callback, user| unsafe { callback(&raw const call, user) }) != 0
        })
    }
}
