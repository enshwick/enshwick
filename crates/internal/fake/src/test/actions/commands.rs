//! The mod's actions as players and the console run them: console answers,
//! other mods' commands, chat commands and the windows actions show.

use enshwick_core::abi::Header;
use enshwick_core::abi::PlayerInfo;
use enshwick_core::abi::Str;

use crate::test::{FakeHost, Installed, table};

impl FakeHost {
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
    /// mod has no actions. A run out of the peer bounds is refused as the
    /// loader refuses it; runs are not rated, so a test hears a rate refusal
    /// through `refuse_run`.
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
        // Mods declare actions only with `unstable`, so without it no run gets here.
        #[cfg(feature = "unstable")]
        if kind == enshwick_core::abi::ActionCall::RUN && !enshwick_core::peer::args_in_bounds(args)
        {
            return Some(out_of_bounds(actions, player, action));
        }
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
            .is_some_and(table::actions::commands::is_own);
        if !own {
            return None;
        }
        table::actions::commands::answer(
            line,
            enshwick_core::abi::ActionCall::CHAT,
            player,
            language,
        )
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
        if !enshwick_core::peer::args_in_bounds(args) {
            return Some(out_of_bounds(actions, player, action));
        }
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
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
#[cfg(feature = "unstable")]
impl Installed {
    /// Tells the mod, as the loader does, that it refused `player`'s run
    /// before asking the mod: `via` is the question the run would have been
    /// ([`ActionCall::RUN`](enshwick_core::abi::ActionCall::RUN) with the
    /// action's id and its arguments, or a typed kind with the line after
    /// the command), `reason` an `ActionCall::REFUSED_` value. The mod's
    /// [`Actions::on_refused`](enshwick_core::actions::Actions::on_refused)
    /// hears it; `false` when the mod declared no actions.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{ActionCall, Header, PlayerInfo};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let player = PlayerInfo::without_character(enshwick::PlayerKey::new(7), 0, "Ann");
    /// let told = loader.refuse_run(&player, ActionCall::RUN, ActionCall::REFUSED_RATE, "kick", "");
    /// assert!(!told, "no actions declared");
    /// ```
    #[must_use]
    pub fn refuse_run(
        &self,
        player: &PlayerInfo,
        via: u32,
        reason: u32,
        action: &str,
        args: &str,
    ) -> bool {
        let Some(actions) = table::with(None, |s| s.actions) else {
            return false;
        };
        tell_refused(actions, player, (via, reason), action, args);
        true
    }
}

/// Refuses `player`'s interface run of `action` whose arguments are out of
/// the peer bounds as the loader does: never asks the mod to run it, tells
/// it of the refusal without the arguments, and answers a refusal.
#[cfg(feature = "unstable")]
fn out_of_bounds(
    actions: table::Registered<enshwick_core::abi::ActionFn>,
    player: &PlayerInfo,
    action: &str,
) -> String {
    let action = if enshwick_core::actions::is_valid_id(action) {
        action
    } else {
        ""
    };
    let run = enshwick_core::abi::ActionCall::RUN;
    let bounds = enshwick_core::abi::ActionCall::REFUSED_BOUNDS;
    tell_refused(actions, player, (run, bounds), action, "");
    "0".to_owned()
}

/// Tells the mod's actions callback, as the loader does, of `player`'s run
/// refused before asking the mod: the run's question and the reason, then
/// the action or line and arguments it would have held.
#[cfg(feature = "unstable")]
fn tell_refused(
    actions: table::Registered<enshwick_core::abi::ActionFn>,
    player: &PlayerInfo,
    (via, reason): (u32, u32),
    action: &str,
    args: &str,
) {
    let _heard = ask_call(
        actions,
        enshwick_core::abi::ActionCall {
            header: Header::of::<enshwick_core::abi::ActionCall>(),
            kind: enshwick_core::abi::ActionCall::REFUSED,
            language: 0,
            player: *player,
            action: Str::new(action),
            args: Str::new(args),
            target: enshwick_core::abi::ActionTarget::NONE,
            reply: core::ptr::null_mut(),
            word: Str::EMPTY,
            via,
            reason,
        },
    );
}

/// The server's console as a player: an administrator with no character.
pub(crate) fn console() -> PlayerInfo {
    PlayerInfo::without_character(
        enshwick_core::PlayerKey::new(0),
        PlayerInfo::CAN_KICK_BAN
            | PlayerInfo::CAN_ACCESS_INVENTORIES
            | PlayerInfo::CAN_EDIT_BASE
            | PlayerInfo::CAN_EDIT_WORLD
            | PlayerInfo::CAN_EXTEND_BASE
            | PlayerInfo::CAN_RECEIVE_EXP,
        "console",
    )
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
        via: 0,
        reason: 0,
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
    ask_call(actions, call)
}

/// Asks a mod's actions callback the question `call` as the loader does;
/// its reply.
fn ask_call(
    actions: table::Registered<enshwick_core::abi::ActionFn>,
    mut call: enshwick_core::abi::ActionCall,
) -> String {
    let mut written: enshwick_core::abi::ActionReply = bytemuck::Zeroable::zeroed();
    if call.kind != enshwick_core::abi::ActionCall::SHEET {
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
