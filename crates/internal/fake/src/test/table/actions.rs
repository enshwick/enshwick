//! The fake's action entries: chat filters, controls, commands, and the
//! filters on interactions, crafting and building.

use crate::test::table::base::args;
use crate::test::table::entries;
use core::ffi::c_void;

use crate::test::table::{Registered, put, text, with, with_open};
use enshwick_core::abi::{
    ActionTarget, BuildFilterFn, ChatFilterFn, CraftFilterFn, InteractionFilterFn,
    InteractionPrompt, ModHandle, Status, Str,
};
#[cfg(not(feature = "unstable"))]
use enshwick_core::abi::{ControlFn, ControlSpec};

pub(crate) mod commands;

/// What the installed fake's actions answer and have recorded.
#[derive(Default)]
pub(crate) struct ActionsState {
    /// The mod's player actions callback, once registered.
    pub(crate) callback: Option<Registered<enshwick_core::abi::ActionFn>>,
    /// The lines the mod ran on the console, in order.
    pub(crate) console: Vec<String>,
    pub(crate) craft_filters: Vec<Registered<CraftFilterFn>>,
    pub(crate) build_filters: Vec<Registered<BuildFilterFn>>,
    /// The mod's controls and the action runs it sent.
    #[cfg(feature = "unstable")]
    pub(crate) controls: crate::test::actions::controls::FakeControls,
    pub(crate) interaction_filters: Vec<Registered<InteractionFilterFn>>,
    /// The prompts mods gave the objects placed from their items.
    pub(crate) interaction_prompts: Vec<InteractionPrompt>,
    pub(crate) chat_filters: Vec<Registered<ChatFilterFn>>,
    /// Each account's chat rate: the start of its window in fake server
    /// time and the lines heard in it.
    pub(crate) chat_rate: Vec<(enshwick_core::PlayerKey, u64, u32)>,
    /// The command name and alias the mod took, and how often it said
    /// its rules changed.
    pub(crate) commands: commands::FakeCommands,
    /// The answer `run_console` last handed out, alive until the next call.
    run_out: Option<Box<str>>,
    /// The client's keyboard and mouse bindings of the game's controls.
    pub(crate) bindings: GameBindings,
}

#[cfg(feature = "unstable")]
pub(crate) use crate::test::actions::controls::{control, run_action};

entries! {
    pub(crate) fn actions(
        me as Mod,
        callback: Option<enshwick_core::abi::ActionFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        with_open(|s| {
            if s.actions.callback.is_some() {
                return Status::REFUSED;
            }
            s.actions.callback = Some(Registered::new(callback, user, me));
            Status::OK
        })
    }

    pub(crate) fn run_console(
        me as Caller,
        line: Str as args::Text,
        out: *mut Str as args::Out,
    ) -> Status {
        let line = text(line);
        let (word, rest) = line.split_once(' ').unwrap_or((&line, ""));
        let (word, rest) = (word.to_owned(), rest.trim().to_owned());
        with((), |s| s.actions.console.push(line.clone()));
        let answer = match with(None, |s| s.host.actions.console_answers.get(&word).cloned()) {
            Some(answer) => answer,
            // The mod's own actions, asked with the table unlocked: they may call
            // back.
            None => match with(None, |s| s.actions.callback) {
                Some(actions) => {
                    let reply = crate::test::ask(
                        actions,
                        enshwick_core::abi::ActionCall::CONSOLE,
                        &crate::test::console(),
                        0,
                        &rest,
                        &word.to_lowercase(),
                        ActionTarget::NONE,
                    );
                    reply.get(1..).unwrap_or_default().to_owned()
                }
                None => return Status::MISSING,
            },
        };
        with(Status::MISSING, |s| {
            let kept = s.actions.run_out.insert(answer.into_boxed_str());
            put(out, Str::new(kept))
        })
    }

    pub(crate) fn chat_filter(
        me as Mod,
        callback: Option<ChatFilterFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        with_open(|s| {
            s.actions
                .chat_filters
                .push(Registered::new(callback, user, me));
            Status::OK
        })
    }

    pub(crate) fn interaction_filter(
        me as Mod,
        callback: Option<InteractionFilterFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        with_open(|s| {
            s.actions
                .interaction_filters
                .push(Registered::new(callback, user, me));
            Status::OK
        })
    }

    /// As the loader: an unreadable spec is a bad argument, then a call after
    /// init is too late, then a malformed spec is a bad argument, and a second
    /// prompt on an item is refused whichever mod asks.
    pub(crate) fn interaction_prompt(
        me as Mod,
        spec: *const InteractionPrompt as args::Headed,
    ) -> Status {
        // SAFETY: the safe layer passes null or a live `InteractionPrompt` for
        // the call, readable for the size its header states.
        let Some(spec) =
            (unsafe { enshwick_core::abi::read_headed(spec, size_of::<InteractionPrompt>()) })
        else {
            return Status::BAD_ARGS;
        };
        with_open(|s| {
            if !spec.is_valid() {
                return Status::BAD_ARGS;
            }
            if s.actions
                .interaction_prompts
                .iter()
                .any(|p| p.item == spec.item)
            {
                return Status::REFUSED;
            }
            s.actions.interaction_prompts.push(spec);
            Status::OK
        })
    }

    pub(crate) fn craft_filter(
        me as Mod,
        callback: Option<CraftFilterFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        with_open(|s| {
            s.actions
                .craft_filters
                .push(Registered::new(callback, user, me));
            Status::OK
        })
    }

    pub(crate) fn build_filter(
        me as Mod,
        callback: Option<BuildFilterFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        with_open(|s| {
            s.actions
                .build_filters
                .push(Registered::new(callback, user, me));
            Status::OK
        })
    }

    /// Without the `unstable` feature the fake runs no actions.
    #[cfg(not(feature = "unstable"))]
    pub(crate) fn run_action(
        me as Mod,
        _action: Str as args::Text,
        _args: Str as args::Text,
        target: *const ActionTarget,
    ) -> Status {
        // SAFETY: the safe layer passes null or a live `ActionTarget` for the call.
        let _read = unsafe { enshwick_core::abi::read_headed(target, size_of::<ActionTarget>()) };
        Status::UNAVAILABLE
    }

    /// Without the `unstable` feature the fake has no controls.
    #[cfg(not(feature = "unstable"))]
    pub(crate) fn control(
        me as Mod,
        spec: *const ControlSpec as args::Headed,
        callback: Option<ControlFn> as args::Callback,
        _user: *mut c_void,
    ) -> Status {
        if callback.is_none() {
            return Status::BAD_ARGS;
        }
        // SAFETY: the safe layer passes a live `ControlSpec` for the call.
        match unsafe { enshwick_core::abi::read_headed(spec, size_of::<ControlSpec>()) } {
            Some(_) => Status::UNAVAILABLE,
            None => Status::BAD_ARGS,
        }
    }
}

/// The modifiers the fake build's binding modifier list holds: Shift, Ctrl
/// and Alt, in the ABI's key form.
const LISTED: u32 = 0b111 << 16;

/// The client's keyboard and mouse bindings of the game's own controls on
/// the fake build, each key in the ABI's form.
#[derive(Debug)]
pub(crate) struct GameBindings {
    /// Each game action the build names, by its reflected name, with the
    /// two keys the player bound to it.
    pub(crate) player: Vec<(String, [u32; 2])>,
    /// The mods' rebinds, oldest first, each its mod's, an action and its
    /// first key; of an action the newest is written.
    rebinds: Vec<(ModHandle, String, u32)>,
    /// The modifier bits the game's binding modifier list holds.
    listed: u32,
    /// The rebinds the interface wrote at its last step, each action's
    /// first key.
    written: Vec<(String, u32)>,
    /// What the interface read at its last step; `None` before a step ran,
    /// as before the loader's first frame.
    read: Option<Vec<(String, [u32; 2])>>,
}

impl Default for GameBindings {
    fn default() -> Self {
        Self {
            player: Vec::new(),
            rebinds: Vec::new(),
            listed: LISTED,
            written: Vec::new(),
            read: None,
        }
    }
}

impl GameBindings {
    /// The keys the game's bindings give `action` now, the player's with
    /// the rebinds the last step wrote; what its input and glyphs use.
    /// `None` for an action the build does not name.
    pub(crate) fn table(&self, action: &str) -> Option<[u32; 2]> {
        let &(_, [first, second]) = self.player.iter().find(|(name, _)| name == action)?;
        let first = self
            .written
            .iter()
            .find(|(name, _)| name == action)
            .map_or(first, |&(_, key)| key);
        Some([first, second])
    }

    /// The first key the newest rebind of `action` holds.
    #[cfg(feature = "unstable")]
    pub(crate) fn rebound(&self, action: &str) -> Option<u32> {
        self.rebinds
            .iter()
            .rev()
            .find(|(_, name, _)| name == action)
            .map(|&(_, _, key)| key)
    }

    /// An interface step: the loader writes the newest rebind of each
    /// action into the game's bindings and reads them back.
    pub(crate) fn step(&mut self) {
        let mut written: Vec<(String, u32)> = Vec::new();
        for (_, action, key) in self.rebinds.iter().rev() {
            if !written.iter().any(|(name, _)| name == action) {
                written.push((action.clone(), *key));
            }
        }
        self.written = written;
        let read = self
            .player
            .iter()
            .filter_map(|(name, _)| Some((name.clone(), self.table(name)?)))
            .collect();
        self.read = Some(read);
    }

    /// The keys of `action` as the last step read them; as the loader:
    /// unavailable before the first step, missing for an action the build
    /// does not name.
    fn binding(&self, action: &str) -> Result<[u32; 2], Status> {
        let read = self.read.as_ref().ok_or(Status::UNAVAILABLE)?;
        if !self.player.iter().any(|(name, _)| name == action) {
            return Err(Status::MISSING);
        }
        Ok(read
            .iter()
            .find(|(name, _)| name == action)
            .map_or([0, 0], |&(_, keys)| keys))
    }

    /// Sets `me`'s rebind of `action` to `key`, or with 0 ends it, as the
    /// loader: unavailable before the first step, missing for an action
    /// the build does not name, a bad argument for a key a mod control may
    /// not take or a modifier the list lacks. Asking again for the rebind
    /// a mod holds keeps its place.
    fn rebind(&mut self, me: ModHandle, action: String, key: u32) -> Status {
        if let Err(status) = self.binding(&action) {
            return status;
        }
        if key != 0 && !bindable(key, self.listed) {
            return Status::BAD_ARGS;
        }
        let at = self
            .rebinds
            .iter()
            .position(|(owner, name, _)| *owner == me && *name == action);
        if at.and_then(|at| self.rebinds.get(at)).map_or(0, |r| r.2) == key {
            return Status::OK;
        }
        if let Some(at) = at {
            self.rebinds.remove(at);
        }
        if key != 0 {
            self.rebinds.push((me, action, key));
        }
        Status::OK
    }
}

entries! {
    /// The keys of the game action `action` as the interface last read
    /// them; as the loader: unavailable on a server and before the
    /// interface's first step, missing for an action the build does not
    /// name.
    pub(crate) fn game_binding(
        me as Caller,
        action: Str as args::Text,
        out: *mut enshwick_core::abi::GameBinding as args::HeadedOut,
    ) -> Status {
        use enshwick_core::abi::GameBinding;
        let action = text(action);
        with(Status::MISSING, |s| {
            if s.host.base.side == enshwick_core::abi::Side::SERVER {
                return Status::UNAVAILABLE;
            }
            match s.actions.bindings.binding(&action) {
                Ok([first, second]) => super::put_headed(
                    out,
                    enshwick_core::headed!(GameBinding {
                        first,
                        second,
                        ..GameBinding::EMPTY
                    }),
                ),
                Err(status) => status,
            }
        })
    }

    /// Rebinds the first key of the game action `action` for the mod `me`,
    /// or with 0 ends its rebind, as the loader: unavailable on a server and
    /// before the interface's first step, missing for an action the build
    /// does not name, a bad argument for a key a mod control may not take
    /// or a modifier the game's list lacks. The interface's next step
    /// writes it.
    pub(crate) fn game_rebind(me as Mod, action: Str as args::Text, key: u32) -> Status {
        let action = text(action);
        with(Status::MISSING, |s| {
            if s.host.base.side == enshwick_core::abi::Side::SERVER {
                return Status::UNAVAILABLE;
            }
            s.actions.bindings.rebind(me, action, key)
        })
    }
}

/// Whether `key` is one a mod control may take with modifiers all in
/// `listed`, as the loader checks it.
#[cfg(feature = "unstable")]
fn bindable(key: u32, listed: u32) -> bool {
    enshwick_core::controls::Key::from_bits(key).is_some_and(|key| key.modifiers() & !listed == 0)
}

/// Without `unstable` no mod reaches a rebind, so every key passes.
#[cfg(not(feature = "unstable"))]
const fn bindable(_key: u32, _listed: u32) -> bool {
    true
}

#[cfg(test)]
#[cfg(feature = "unstable")]
mod tests {
    use enshwick_core::controls::Key;

    use super::*;

    const A: ModHandle = ModHandle(1);
    const B: ModHandle = ModHandle(2);

    fn key(name: &str) -> u32 {
        Key::parse(name).unwrap().bits()
    }

    fn jump() -> GameBindings {
        let mut bindings = GameBindings::default();
        bindings.player.push(("Jump".to_owned(), [key("Space"), 0]));
        bindings
    }

    #[test]
    fn a_rebind_waits_for_the_first_step_and_refuses_what_the_loader_refuses() {
        let mut bindings = jump();
        assert_eq!(
            bindings.rebind(A, "Jump".into(), key("J")),
            Status::UNAVAILABLE
        );
        bindings.step();
        assert_eq!(
            bindings.rebind(A, "Sneak".into(), key("J")),
            Status::MISSING
        );
        bindings.listed = 1 << 16;
        assert_eq!(
            bindings.rebind(A, "Jump".into(), key("Alt+J")),
            Status::BAD_ARGS
        );
        assert_eq!(
            bindings.rebind(A, "Jump".into(), key("Shift+J")),
            Status::OK
        );
    }

    #[test]
    fn the_glyph_follows_a_rebind_only_after_the_step_that_writes_it() {
        let mut bindings = jump();
        bindings.step();
        assert_eq!(bindings.rebind(A, "Jump".into(), key("J")), Status::OK);
        assert_eq!(bindings.table("Jump"), Some([key("Space"), 0]));
        bindings.step();
        assert_eq!(bindings.table("Jump"), Some([key("J"), 0]));
        assert_eq!(bindings.binding("Jump"), Ok([key("J"), 0]));
    }

    #[test]
    fn a_mod_s_reset_ends_only_its_own_rebind() {
        let mut bindings = jump();
        bindings.step();
        assert_eq!(bindings.rebind(A, "Jump".into(), key("J")), Status::OK);
        assert_eq!(bindings.rebind(B, "Jump".into(), key("K")), Status::OK);
        assert_eq!(bindings.rebind(A, "Jump".into(), key("J")), Status::OK);
        assert_eq!(
            bindings.rebound("Jump"),
            Some(key("K")),
            "a's repeat keeps its place"
        );
        assert_eq!(bindings.rebind(A, "Jump".into(), 0), Status::OK);
        assert_eq!(bindings.rebound("Jump"), Some(key("K")));
        assert_eq!(bindings.rebind(B, "Jump".into(), 0), Status::OK);
        assert_eq!(bindings.rebound("Jump"), None);
    }
}
