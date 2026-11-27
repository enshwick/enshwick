//! The world's players: who is in it, the world's key, the accounts that
//! join, and the chat lines, notices and renames the mod sends them.

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
        self.base.world_key = Some(key.to_owned());
        self
    }

    /// A connected player with a character in the world.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Header, PlayerInfo};
    ///
    /// let player = enshwick::abi::headed!(PlayerInfo {
    ///     slot: 0,
    ///     entity: enshwick::Entity::from_raw(2).ok_or("no entity")?,
    ///     key: enshwick::PlayerKey::new(7),
    ///     permissions: PlayerInfo::CAN_KICK_BAN,
    ///     name_len: 0,
    ///     name: [0; 128],
    /// });
    /// let loader = enshwick::test::FakeHost::new().player(player).install();
    /// # let _ = loader;
    /// # Ok::<(), &str>(())
    /// ```
    #[must_use]
    pub fn player(mut self, player: PlayerInfo) -> Self {
        self.base.players.push(player);
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
        table::with(Vec::new(), |s| s.world.chat.clone())
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
        table::with(Vec::new(), |s| s.world.notices.clone())
    }

    /// Asks the mod's rename filters about `player` renaming `entity` to
    /// `text`; whether every filter lets it through.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.rename(enshwick::Entity::from_raw(4096).ok_or("no entity")?, None, "Tools"));
    /// # Ok::<(), &str>(())
    /// ```
    #[must_use]
    pub fn rename(
        &self,
        entity: enshwick_core::Entity,
        player: Option<&PlayerInfo>,
        text: &str,
    ) -> bool {
        let filters = table::with(Vec::new(), |s| s.world.rename_filters.clone());
        let call = enshwick_core::headed!(RenameCall {
            entity: enshwick_core::Entity::from_engine(entity.get()),
            player: player.map_or(core::ptr::null(), core::ptr::from_ref),
            text: Str::new(text),
        });
        filters.iter().all(|filter| {
            // SAFETY: a valid call; `user` is the pointer the mod registered with.
            filter.run(|callback, user| unsafe { callback(&raw const call, user) })
                != enshwick_core::abi::FILTER_REFUSE
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
        let rules = table::with(Vec::new(), |s| s.world.join_rules.clone());
        let call = enshwick_core::headed!(JoinCall {
            account: player.unwrap_or(enshwick_core::PlayerKey::new(0)),
        });
        rules.iter().all(|rule| {
            // SAFETY: a valid call; `user` is the pointer the mod registered with.
            rule.run(|callback, user| unsafe { callback(&raw const call, user) })
                != enshwick_core::abi::FILTER_REFUSE
        })
    }
}
