//! Players and mods: sides, players, loaded mods, actions, chat, messages and services.

use enshwick_core::abi::Header;
use enshwick_core::abi::ModInfo;
use enshwick_core::abi::PlayerInfo;
use enshwick_core::abi::RenameCall;
use enshwick_core::abi::Side;
use enshwick_core::abi::Str;

use super::FakeHost;
use super::Installed;
use super::table;

impl FakeHost {
    /// The side the game runs as.
    ///
    /// # Examples
    ///
    /// See [`FakeHost`].
    #[must_use]
    pub fn side(mut self, side: Side) -> Self {
        self.side = side;
        self
    }

    /// Whether a client hosts a game now; the dedicated server always runs
    /// one.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Side;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).hosting(true).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn hosting(mut self, hosting: bool) -> Self {
        self.hosting = hosting;
        self
    }

    /// The account of the hosted game's host, whom account actions never
    /// kick or ban.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Side;
    ///
    /// let loader = enshwick::test::FakeHost::new()
    ///     .side(Side::CLIENT)
    ///     .hosting(true)
    ///     .host_key(enshwick::PlayerKey::new(7))
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn host_key(mut self, key: enshwick_core::PlayerKey) -> Self {
        self.host_key = Some(key);
        self
    }

    /// The entity world of the running server, which a system context of
    /// that world holds; 0 for none.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().server_world(0x2000).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn server_world(mut self, world: u64) -> Self {
        self.server_world = world;
        self
    }

    /// A peer that takes mod messages: a player's account key on the
    /// server, or [`Peer::Server`](enshwick_core::net::Peer::Server) on a client.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::net::Peer;
    ///
    /// let loader = enshwick::test::FakeHost::new().net_peer(Peer::Player(enshwick::PlayerKey::new(7))).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn net_peer(mut self, peer: enshwick_core::net::Peer) -> Self {
        let raw = match peer {
            enshwick_core::net::Peer::Server => enshwick_core::abi::NetMessage::SERVER,
            enshwick_core::net::Peer::Player(key) => key,
        };
        self.net_peers.insert(raw.get());
        self
    }

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

    /// A loaded mod `enshwick::mods::all()` lists, in order; the loader
    /// itself comes first in the game.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Header, ManifestFlags, ModInfo, Side, Str};
    ///
    /// let loader = enshwick::test::FakeHost::new()
    ///     .loaded(ModInfo {
    ///         header: Header::of::<ModInfo>(),
    ///         id: Str::new("enshwick"),
    ///         version: Str::new("0.2.0"),
    ///         side: Side::BOTH,
    ///         flags: ManifestFlags::NONE,
    ///         off: 0,
    ///         file: Str::new("mods/enshwick/enshwick.dll"),
    ///         ..ModInfo::EMPTY
    ///     })
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn loaded(mut self, info: ModInfo) -> Self {
        self.mods.push(info);
        self
    }

    /// Another mod `id` that exports only `names`, each `<kind>:<local
    /// name>` as a manifest lists it (`call:price-of`, `content:locker`): as
    /// the loader, the fake refuses the mod a call, link, service, event,
    /// content id or stat of `id` outside them, matching kind and name.
    /// Without it, the fake takes every name of another mod as exported,
    /// where the loader refuses every name a mod does not export: a test
    /// that links another mod passes that mod's exports here, so it fails
    /// where the game would.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new()
    ///     .exports("you.trading-post", &["call:price-of", "event:sold"])
    ///     .install();
    /// # let _ = loader;
    /// ```
    ///
    /// # Panics
    ///
    /// When a name lacks its kind, as the loader refuses a manifest whose
    /// export lacks it.
    #[must_use]
    #[expect(
        clippy::panic,
        reason = "a test that hands the fake a bare name fails as the loader refuses it"
    )]
    pub fn exports(mut self, id: &str, names: &[&str]) -> Self {
        if let Some(bare) = enshwick_core::__fake::rules::bad_export(names) {
            panic!("the export {bare:?} lacks its kind or its local name: write <kind>:<name>");
        }
        self.exports.insert(
            id.to_owned(),
            names.iter().map(|&name| name.to_owned()).collect(),
        );
        self
    }

    /// A server mod the last refused join found missing, which
    /// `enshwick::mods::join_missing()` lists, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Header, ModInfo, Side, Str};
    ///
    /// let loader = enshwick::test::FakeHost::new()
    ///     .join_missing(ModInfo {
    ///         header: Header::of::<ModInfo>(),
    ///         id: Str::new("you.trading-post"),
    ///         version: Str::new("0.2.0"),
    ///         side: Side::BOTH,
    ///         off: 1,
    ///         ..ModInfo::EMPTY
    ///     })
    ///     .install();
    /// // SAFETY: the installed fake's table.
    /// let api = unsafe { &*loader.api() };
    /// let mut count = 0;
    /// let size = u32::try_from(size_of::<ModInfo>())?;
    /// let status = (api.join_missing)(loader.handle(), core::ptr::null_mut(), size, 0, &mut count);
    /// assert!(status.is_ok());
    /// assert_eq!(count, 1);
    /// # Ok::<(), core::num::TryFromIntError>(())
    /// ```
    #[must_use]
    pub fn join_missing(mut self, info: ModInfo) -> Self {
        self.join_missing.push(info);
        self
    }

    /// The answer to console lines whose first word is `word`, which
    /// another mod or the loader gives, when the mod runs such a line with
    /// `console::run`. A line whose first word has no
    /// answer here goes to the mod's own actions, as if the word were its id.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new()
    ///     .console_answer("enshwick", "Enshwick 0.0.0-test: no mods")
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn console_answer(mut self, word: &str, answer: &str) -> Self {
        self.console_answers
            .insert(word.to_owned(), answer.to_owned());
        self
    }

    /// Another running mod, `mod_id`, loaded before the mod under test,
    /// that named its actions as commands with `alias`: the mod under test
    /// gets no alias another mod holds,
    /// [`commands::list`](enshwick_core::commands::list) names both, and a command
    /// line to the other mod gets the
    /// [console answer](Self::console_answer) of the word used, as done.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new()
    ///     .other_mod_commands("you.weather", "wx")
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[cfg(feature = "unstable")]
    #[must_use]
    pub fn other_mod_commands(mut self, mod_id: &str, alias: &str) -> Self {
        self.other_commands
            .push((mod_id.to_owned(), alias.to_owned(), false));
        self
    }

    /// As [`other_mod_commands`](Self::other_mod_commands), for a mod
    /// loaded after the mod under test: it gets no alias the mod under
    /// test holds, and runs by its name alone.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new()
    ///     .later_mod_commands("you.weather", "wx")
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[cfg(feature = "unstable")]
    #[must_use]
    pub fn later_mod_commands(mut self, mod_id: &str, alias: &str) -> Self {
        self.other_commands
            .push((mod_id.to_owned(), alias.to_owned(), true));
        self
    }

    /// A service another mod publishes.
    ///
    /// # Examples
    ///
    /// ```
    /// #[enshwick::service(of = trading_post, name = "prices")]
    /// pub struct Prices {
    ///     pub price: extern "system" fn(item: u32) -> u32,
    /// }
    ///
    /// extern "system" fn price(_item: u32) -> u32 {
    ///     3
    /// }
    ///
    /// static PRICES: Prices = Prices { price };
    ///
    /// let loader = enshwick::test::FakeHost::new().service(&PRICES).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn service<T: enshwick_core::services::Service>(mut self, table: &'static T) -> Self {
        self.services.push(FakeService {
            name: T::NAME.to_owned(),
            major: T::MAJOR,
            fields: T::LAYOUT
                .iter()
                // SAFETY: the declaration's names are static strings.
                .filter_map(|f| unsafe { f.name.as_str() }.map(|n| (n.to_owned(), f.hash)))
                .collect(),
            table: core::ptr::from_ref(table).expose_provenance(),
        });
        self
    }
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// Asks the mod's player actions as the loader does: `kind` is one of
    /// [`ActionCall::SHEET`](enshwick_core::abi::ActionCall::SHEET),
    /// [`ActionCall::RUN`](enshwick_core::abi::ActionCall::RUN) or
    /// [`ActionCall::CONSOLE`](enshwick_core::abi::ActionCall::CONSOLE), for
    /// `player` reading `language`; `action` and `args` for a run. `None` when the
    /// mod has no actions.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{ActionCall, Header, PlayerInfo};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let player = PlayerInfo {
    ///     header: Header::of::<PlayerInfo>(),
    ///     slot: 0,
    ///     entity: enshwick::Entity(2),
    ///     key: enshwick::PlayerKey::new(7),
    ///     permissions: 0,
    ///     name_len: 0,
    ///     name: [0; 64],
    /// };
    /// assert_eq!(loader.ask_actions(ActionCall::SHEET, &player, 0, "", ""), None);
    /// ```
    #[must_use]
    pub fn ask_actions(
        &self,
        kind: u32,
        player: &PlayerInfo,
        language: u32,
        action: &str,
        args: &str,
    ) -> Option<String> {
        let actions = table::with(None, |s| s.actions)?;
        let reply = ask(
            actions,
            kind,
            player,
            language,
            action,
            args,
            enshwick_core::abi::ActionTarget::NONE,
        );
        if kind == enshwick_core::abi::ActionCall::RUN {
            let key = player.key.get();
            table::with((), |s| {
                let held = s.commands.carried.iter().filter(|(k, _, _)| *k == key);
                let again: Vec<_> = held
                    .map(|(k, id, _)| (*k, id.clone(), enshwick_core::abi::WINDOW_UPDATE))
                    .collect();
                s.commands.windows.extend(again);
            });
        }
        Some(reply)
    }

    /// `player`, reading `language`, types `line` in chat, as the loader
    /// takes it: the mod's reply, `1` or `0` and the text, when the line is
    /// a `/` and the mod's command name or alias; `None` for a line that
    /// stays in chat.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Header, PlayerInfo};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let player = PlayerInfo {
    ///     header: Header::of::<PlayerInfo>(),
    ///     slot: 0,
    ///     entity: enshwick::Entity(2),
    ///     key: enshwick::PlayerKey::new(7),
    ///     permissions: 0,
    ///     name_len: 0,
    ///     name: [0; 64],
    /// };
    /// assert_eq!(loader.type_in_chat(&player, 0, "/adm online"), None);
    /// ```
    #[cfg(feature = "unstable")]
    #[must_use]
    pub fn type_in_chat(&self, player: &PlayerInfo, language: u32, line: &str) -> Option<String> {
        if !line.starts_with('/') {
            return None;
        }
        let own = line[1..]
            .split_whitespace()
            .next()
            .is_some_and(table::commands::is_own);
        if !own {
            return None;
        }
        table::commands::answer(line, enshwick_core::abi::ActionCall::CHAT, player, language)
    }

    /// How often the mod said who may run its actions changed, with
    /// [`Actions::rules_changed`](enshwick_core::actions::Actions::rules_changed).
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.rules_changed(), 0);
    /// ```
    #[cfg(feature = "unstable")]
    #[must_use]
    pub fn rules_changed(&self) -> u32 {
        table::with(0, |s| s.commands.rules_changed)
    }

    /// The windows `player`'s client holds after the shows and closes the
    /// mod sent it, as the loader carries them after a step: each window's
    /// id and its text, which `actions::window::decode` reads. The mod
    /// renders each show for `player` reading `language`, and again each
    /// window they hold after their run through
    /// [`ask_actions`](Self::ask_actions); a render that is empty or past
    /// `actions::window::MAX_BYTES` closes the window. A player the host
    /// does not have online holds none.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Header, PlayerInfo};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let player = PlayerInfo {
    ///     header: Header::of::<PlayerInfo>(),
    ///     slot: 0,
    ///     entity: enshwick::Entity(2),
    ///     key: enshwick::PlayerKey::new(7),
    ///     permissions: 0,
    ///     name_len: 0,
    ///     name: [0; 64],
    /// };
    /// assert!(loader.carried_windows(&player, 0).is_empty());
    /// ```
    #[cfg(feature = "unstable")]
    #[must_use]
    pub fn carried_windows(&self, player: &PlayerInfo, language: u32) -> Vec<(String, String)> {
        let key = player.key.get();
        let (actions, waiting) = table::with((None, Vec::new()), |s| {
            let (mine, others): (Vec<_>, Vec<_>) = core::mem::take(&mut s.commands.windows)
                .into_iter()
                .partition(|(k, _, _)| *k == key);
            s.commands.windows = others;
            let online = s.host.players.iter().any(|p| p.key == player.key);
            (s.actions, if online { mine } else { Vec::new() })
        });
        for (_, id, change) in waiting {
            let holds = table::with(false, |s| {
                let mut carried = s.commands.carried.iter();
                carried.any(|(k, i, _)| *k == key && *i == id)
            });
            if change == enshwick_core::abi::WINDOW_UPDATE && !holds {
                continue;
            }
            let text = match (change == enshwick_core::abi::WINDOW_CLOSE, actions) {
                (false, Some(actions)) => ask(
                    actions,
                    enshwick_core::abi::ActionCall::WINDOW,
                    player,
                    language,
                    &id,
                    "",
                    enshwick_core::abi::ActionTarget::NONE,
                ),
                _ => String::new(),
            };
            let text = if text.len() > enshwick_core::actions::window::MAX_BYTES {
                String::new()
            } else {
                text
            };
            let closed = table::with(None, |s| {
                let carried = &mut s.commands.carried;
                let at = carried.iter().position(|(k, i, _)| *k == key && *i == id);
                match (at, text.is_empty()) {
                    (Some(at), true) => {
                        let (_, id, _) = carried.remove(at);
                        (change != enshwick_core::abi::WINDOW_CLOSE).then_some(id)
                    }
                    (Some(at), false) => {
                        carried[at].2 = text;
                        None
                    }
                    (None, false) => {
                        carried.push((key, id, text));
                        let mine: Vec<usize> = (0..carried.len())
                            .filter(|&i| carried[i].0 == key)
                            .collect();
                        (mine.len() > enshwick_core::actions::window::MAX_HELD)
                            .then(|| carried.remove(mine[0]).1)
                    }
                    (None, true) => None,
                }
            });
            if let Some(closed) = closed {
                self.tell_window_closed(player.key, &closed);
            }
        }
        table::with(Vec::new(), |s| {
            s.commands
                .carried
                .iter()
                .filter(|(k, _, _)| *k == key)
                .map(|(_, id, text)| (id.clone(), text.clone()))
                .collect()
        })
    }

    /// `player`'s client closes the mod's window `id`, as when the player
    /// presses its Close: the client holds it no more, and the mod hears of
    /// it as the loader tells it after the step. A close of a window the
    /// client does not hold tells the mod nothing, as the loader drops it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Header, PlayerInfo};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let player = PlayerInfo {
    ///     header: Header::of::<PlayerInfo>(),
    ///     slot: 0,
    ///     entity: enshwick::Entity(2),
    ///     key: enshwick::PlayerKey::new(7),
    ///     permissions: 0,
    ///     name_len: 0,
    ///     name: [0; 64],
    /// };
    /// loader.close_window(&player, "panel");
    /// assert!(loader.carried_windows(&player, 0).is_empty());
    /// ```
    #[cfg(feature = "unstable")]
    pub fn close_window(&self, player: &PlayerInfo, id: &str) {
        let key = player.key.get();
        let held = table::with(false, |s| {
            let before = s.commands.carried.len();
            s.commands
                .carried
                .retain(|(k, i, _)| !(*k == key && i == id));
            s.commands.carried.len() != before
        });
        if held {
            self.tell_window_closed(player.key, id);
        }
    }

    /// `player` leaves, as far as the mod's windows go: their client holds
    /// none any more, what waited for it is dropped, and the mod hears that
    /// the player left, as the loader tells every mod with actions.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.player_left(enshwick::PlayerKey::new(7));
    /// ```
    #[cfg(feature = "unstable")]
    pub fn player_left(&self, player: enshwick_core::PlayerKey) {
        let key = player.get();
        table::with((), |s| {
            s.commands.carried.retain(|(k, _, _)| *k != key);
            s.commands.windows.retain(|(k, _, _)| *k != key);
        });
        self.tell_window_closed(player, "");
    }

    /// Tells the mod that `player`'s client closed its window `id`, or with
    /// an empty id that the player left, with the key alone as the loader
    /// does.
    #[cfg(feature = "unstable")]
    #[expect(
        clippy::unused_self,
        reason = "only an installed fake has a mod to tell"
    )]
    fn tell_window_closed(&self, player: enshwick_core::PlayerKey, id: &str) {
        let Some(actions) = table::with(None, |s| s.actions) else {
            return;
        };
        let who = PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: u32::MAX,
            entity: enshwick_core::Entity(0),
            key: player,
            permissions: 0,
            name_len: 0,
            name: [0; 64],
        };
        let closed = enshwick_core::abi::ActionCall::WINDOW_CLOSED;
        let _heard = ask(
            actions,
            closed,
            &who,
            0,
            id,
            "",
            enshwick_core::abi::ActionTarget::NONE,
        );
    }

    /// Runs the mod's action `action` with `args` for `player` as the
    /// loader does when a client named `target` with
    /// [`Actions::run_at`](enshwick_core::actions::Actions::run_at); the reply, or
    /// `None` when the mod has no actions.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Header, PlayerInfo};
    /// use enshwick::actions::ActionTarget;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let player = PlayerInfo {
    ///     header: Header::of::<PlayerInfo>(),
    ///     slot: 0,
    ///     entity: enshwick::Entity(2),
    ///     key: enshwick::PlayerKey::new(7),
    ///     permissions: 0,
    ///     name_len: 0,
    ///     name: [0; 64],
    /// };
    /// let chest = ActionTarget::at([1.0, 2.0, 3.0]);
    /// assert_eq!(loader.ask_action_at(&player, 0, "open", "", chest), None);
    /// ```
    #[cfg(feature = "unstable")]
    #[must_use]
    pub fn ask_action_at(
        &self,
        player: &PlayerInfo,
        language: u32,
        action: &str,
        args: &str,
        target: enshwick_core::actions::ActionTarget,
    ) -> Option<String> {
        let actions = table::with(None, |s| s.actions)?;
        let run = enshwick_core::abi::ActionCall::RUN;
        Some(ask(
            actions,
            run,
            player,
            language,
            action,
            args,
            enshwick_core::__fake::raw_action_target(target),
        ))
    }

    /// The lines the mod ran with `console::run`, in
    /// order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.console().is_empty());
    /// ```
    #[must_use]
    pub fn console(&self) -> Vec<String> {
        table::with(Vec::new(), |s| s.console.clone())
    }

    /// The mod's features as it last reported them with
    /// [`Load::feature`](enshwick_core::Load::feature), in the order it first named
    /// them: the name and, for one that is off, the reason.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.features().is_empty());
    /// ```
    #[must_use]
    pub fn features(&self) -> Vec<(String, Option<String>)> {
        table::with(Vec::new(), |s| s.features.clone())
    }

    /// The lines the mod's status sources give now, each source's text in
    /// registration order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.status().is_empty());
    /// ```
    #[must_use]
    pub fn status(&self) -> Vec<String> {
        let sources = table::with(Vec::new(), |s| s.status_sources.clone());
        sources
            .into_iter()
            .map(|source| {
                let mut out = Str::EMPTY;
                // SAFETY: `user` is the pointer the mod registered with; a
                // writable out pointer.
                source.run(|callback, user| unsafe { callback(user, &raw mut out) });
                // SAFETY: the text stays valid until the source runs again.
                unsafe { out.as_str() }.unwrap_or("").to_owned()
            })
            .collect()
    }

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

    /// The messages the mod sent: the channel's full name (`name@version`),
    /// the raw peer ([`NetMessage::SERVER`](enshwick_core::abi::NetMessage::SERVER),
    /// a player's key, or 0 for every client) and the encoded bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.net_sent().is_empty());
    /// ```
    #[must_use]
    pub fn net_sent(&self) -> Vec<(String, enshwick_core::PlayerKey, Vec<u8>)> {
        table::with(Vec::new(), |s| {
            s.net_sent
                .iter()
                .map(|(id, peer, bytes)| {
                    let name = s
                        .channels
                        .iter()
                        .find(|c| c.1 == *id)
                        .map_or_else(String::new, |c| c.0.clone());
                    (name, *peer, bytes.clone())
                })
                .collect()
        })
    }

    /// Hands `message` from `from` to the mod's channel `name@version`, as
    /// the loader does when one comes in, encoded as the channel encodes
    /// it; false when the mod opened no such channel.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::net::Peer;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(!loader.net_deliver("markers@1", Peer::Server, &5u32));
    /// ```
    #[must_use]
    pub fn net_deliver<M: serde::Serialize>(
        &self,
        name: &str,
        from: enshwick_core::net::Peer,
        message: &M,
    ) -> bool {
        let Ok(bytes) = postcard::to_allocvec(message) else {
            return false;
        };
        let channel = table::with(None, |s| {
            s.channels.iter().find(|c| c.0 == name).map(|c| (c.1, c.2))
        });
        let Some((id, handler)) = channel else {
            return false;
        };
        let peer = match from {
            enshwick_core::net::Peer::Server => enshwick_core::abi::NetMessage::SERVER,
            enshwick_core::net::Peer::Player(key) => key,
        };
        let raw = enshwick_core::abi::NetMessage {
            header: Header::of::<enshwick_core::abi::NetMessage>(),
            channel: id,
            reserved: 0,
            peer,
            data: bytes.as_ptr(),
            size: u32::try_from(bytes.len()).unwrap_or(0),
            reserved_tail: 0,
        };
        // SAFETY: a handler the mod registered with its own pointer.
        handler.run(|callback, user| unsafe { callback(&raw const raw, user) });
        true
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
}

/// The server's console as a player: an administrator with no character.
pub(crate) fn console() -> PlayerInfo {
    let mut name = [0u8; 64];
    name[..7].copy_from_slice(b"console");
    PlayerInfo {
        header: Header::of::<PlayerInfo>(),
        slot: u32::MAX,
        entity: enshwick_core::Entity(0),
        key: enshwick_core::PlayerKey::new(0),
        permissions: PlayerInfo::CAN_KICK_BAN
            | PlayerInfo::CAN_ACCESS_INVENTORIES
            | PlayerInfo::CAN_EDIT_BASE
            | PlayerInfo::CAN_EDIT_WORLD
            | PlayerInfo::CAN_EXTEND_BASE
            | PlayerInfo::CAN_RECEIVE_EXP,
        name_len: 7,
        name,
    }
}

/// Asks a mod's actions callback as the loader does; its reply. `extra` is
/// the arguments of a run, or the word that named the mod for a typed line
/// (the console, chat or a remote caller).
pub(crate) fn ask(
    actions: table::Registered<enshwick_core::abi::ActionFn>,
    kind: u32,
    player: &PlayerInfo,
    language: u32,
    action: &str,
    extra: &str,
    target: enshwick_core::abi::ActionTarget,
) -> String {
    let mut call = enshwick_core::abi::ActionCall {
        header: Header::of::<enshwick_core::abi::ActionCall>(),
        kind,
        language,
        player: *player,
        action: Str::new(action),
        args: Str::EMPTY,
        target,
        reply: core::ptr::null_mut(),
        word: Str::EMPTY,
    };
    let typed = [
        enshwick_core::abi::ActionCall::CONSOLE,
        enshwick_core::abi::ActionCall::CHAT,
        enshwick_core::abi::ActionCall::REMOTE,
    ];
    if typed.contains(&kind) {
        call.word = Str::new(extra);
    } else {
        call.args = Str::new(extra);
    }
    let mut written: enshwick_core::abi::ActionReply = bytemuck::Zeroable::zeroed();
    if kind != enshwick_core::abi::ActionCall::SHEET {
        call.reply = &raw mut written;
    }
    let mut reply = Str::EMPTY;
    // SAFETY: a valid call whose reply slot outlives it; `user` is the
    // pointer the mod registered with.
    actions.run(|callback, user| unsafe { callback(&raw const call, user, &raw mut reply) });
    if written.header.size != 0 {
        // SAFETY: the mod wrote the reply, whose text stays valid until the
        // callback runs again. The fake's helpers hand a run back as `1` or
        // `0` and the text.
        let text = unsafe { written.text.as_str() }.unwrap_or("");
        return format!("{}{text}", written.done);
    }
    // SAFETY: the reply stays valid until the callback runs again.
    unsafe { reply.as_str() }.unwrap_or("").to_owned()
}

/// A published service, as the fake keeps it.
#[derive(Clone, Debug)]
pub(crate) struct FakeService {
    pub(crate) name: String,
    pub(crate) major: u32,
    pub(crate) fields: Vec<(String, u64)>,
    pub(crate) table: usize,
}
