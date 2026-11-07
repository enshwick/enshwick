//! The fake loader's controls: the mod's declared controls, keys the test
//! holds down, game actions it fires and the action runs a client sent.

use core::ffi::c_void;
use core::time::Duration;

use crate::abi::{
    ControlEvent, ControlFn, ControlSpec, Header, ModHandle, PlayerInfo, Status, Str, read_headed,
};
use crate::controls::{GameAction, Key, Tracker, chords, is_valid_name, phases, valid_phases};

use super::table::{Registered, Registration, text, with};
use super::{FakeHost, Installed};

/// A control the mod declared.
struct FakeControl {
    name: String,
    label: String,
    keys: [Option<Key>; 2],
    link: Option<GameAction>,
    fires: u32,
    hold_ms: u32,
    /// The mod's action a firing runs on the server; empty for none.
    action: String,
    handler: Registered<ControlFn>,
    tracker: Tracker,
    /// The game action's tracker for each player entity.
    players: Vec<(u32, Tracker)>,
}

/// The mod's controls, the fake clock of its keys and the runs sent.
#[derive(Default)]
pub(crate) struct FakeControls {
    controls: Vec<FakeControl>,
    /// The slots of each control down at the last look at the keys.
    slots: Vec<u8>,
    /// Milliseconds since the first look at the keys.
    now: u64,
    /// The action runs the mod sent the server: action and arguments.
    runs: Vec<(String, String)>,
    /// The object each run named, in the order of `runs`.
    targets: Vec<Option<crate::actions::ActionTarget>>,
}

pub(crate) extern "system" fn control(
    me: ModHandle,
    spec: *const ControlSpec,
    callback: ControlFn,
    user: *mut c_void,
) -> Status {
    // SAFETY: the safe layer passes a live `ControlSpec` for the call.
    let Some(spec) = (unsafe { read_headed(spec, size_of::<ControlSpec>()) }) else {
        return Status::BAD_ARGS;
    };
    let name = text(spec.name);
    let link = text(spec.link);
    let key = |bits: u32| (bits != 0).then(|| Key::from_bits(bits));
    let keys = [key(spec.key), key(spec.second)];
    let link = match GameAction::ALL.iter().find(|a| a.name() == link) {
        Some(action) => Some(*action),
        None if link.is_empty() => None,
        None => return Status::BAD_ARGS,
    };
    let action = text(spec.action);
    if !is_valid_name(&name)
        || keys.iter().any(|k| matches!(k, Some(None)))
        || !valid_phases(spec.fires)
        || !(action.is_empty() || is_valid_name(&action))
    {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        if s.controls.controls.iter().any(|c| c.name == name) {
            return Status::REFUSED;
        }
        let mut keys = keys.map(Option::flatten);
        for (_, slot, key) in s.host.bindings.iter().filter(|(n, _, _)| *n == name) {
            if let Some(kept) = keys.get_mut(*slot) {
                *kept = *key;
            }
        }
        s.controls.controls.push(FakeControl {
            keys,
            label: text(spec.label),
            name,
            link,
            fires: spec.fires,
            hold_ms: spec.hold_ms,
            action,
            handler: Registered::new(callback, user, me),
            tracker: Tracker::default(),
            players: Vec::new(),
        });
        Status::OK
    })
}

pub(crate) extern "system" fn run_action(_me: ModHandle, action: Str, args: Str) -> Status {
    let (action, args) = (text(action), text(args));
    with(Status::MISSING, |s| {
        if s.host.side != crate::abi::Side::CLIENT {
            return Status::MISSING;
        }
        s.controls.runs.push((action, args));
        s.controls.targets.push(None);
        Status::OK
    })
}

pub(crate) extern "system" fn run_action_at(
    _me: ModHandle,
    action: Str,
    args: Str,
    target: *const crate::abi::ActionTarget,
) -> Status {
    // SAFETY: the safe layer passes null or a live `ActionTarget` for the call.
    let named = match unsafe { crate::abi::ActionTarget::read(target) } {
        Ok(named) => named,
        Err(status) => return status,
    };
    let named = named
        .as_ref()
        .and_then(crate::actions::ActionTarget::from_raw);
    let (action, args) = (text(action), text(args));
    with(Status::MISSING, |s| {
        if s.host.side != crate::abi::Side::CLIENT {
            return Status::MISSING;
        }
        s.controls.runs.push((action, args));
        s.controls.targets.push(named);
        Status::OK
    })
}

impl FakeHost {
    /// The player's key for slot `slot` (0 the key, 1 the second) of control
    /// `name`, as `mod-controls.toml` holds it: in place of the mod's
    /// default for that slot only, `None` to unbind it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::controls::Key;
    ///
    /// let loader = enshwick::test::FakeHost::new()
    ///     .bind("horn", 0, Some(Key::parse("J")?))
    ///     .install();
    /// # Ok::<(), enshwick::controls::ParseKeyError>(())
    /// ```
    #[must_use]
    pub fn bind(mut self, name: &str, slot: usize, key: Option<Key>) -> Self {
        self.bindings.push((name.to_owned(), slot, key));
        self
    }
}

/// One call of a control's callback.
struct Call {
    name: String,
    kind: u32,
    handler: Registered<ControlFn>,
}

impl FakeControl {
    fn call(&self, kind: u32) -> Call {
        Call {
            name: self.name.clone(),
            kind,
            handler: self.handler,
        }
    }
}

/// Moves the fake clock `after` on and returns it, in milliseconds.
fn advance(fake: &mut FakeControls, after: Duration) -> u64 {
    fake.now = fake
        .now
        .saturating_add(u64::try_from(after.as_millis()).unwrap_or(u64::MAX));
    fake.now
}

fn deliver(calls: &[Call], server: Option<&PlayerInfo>) -> usize {
    let zero = PlayerInfo {
        header: Header::of::<PlayerInfo>(),
        slot: 0,
        entity: crate::Entity(0),
        key: crate::PlayerKey::new(0),
        permissions: 0,
        name_len: 0,
        name: [0; 64],
    };
    for call in calls {
        let event = ControlEvent {
            header: Header::of::<ControlEvent>(),
            name: Str::new(&call.name),
            kind: call.kind,
            server: u8::from(server.is_some()),
            reserved: [0; 3],
            player: *server.unwrap_or(&zero),
        };
        // SAFETY: `event` lives for the call and `user` is the pointer the
        // mod declared `callback` with.
        call.handler
            .run(|callback, user| unsafe { callback(&raw const event, user) });
    }
    calls.len()
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// Looks at the keys as the client's key reader does, `after` the last
    /// look: `down` are the keys held now, and the modifiers of all of them
    /// are held together, as on a keyboard. Each control the rule of the
    /// [controls](crate::controls) brings down fires what the look decides.
    /// As in the loader, only a client reads keys: on a server nothing
    /// fires, nor for a mod of side server on a client that does not
    /// host. Returns how many calls the mod got.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    /// use enshwick::controls::Key;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.keys(&[Key::parse("H")?], Duration::from_millis(16)), 0);
    /// # Ok::<(), enshwick::controls::ParseKeyError>(())
    /// ```
    #[must_use = "the count says whether a control fired"]
    pub fn keys(&self, down: &[Key], after: Duration) -> usize {
        let calls = with(Vec::new(), |s| {
            // In another's world a mod of side server fires on its server,
            // which carries the control to the client.
            let server_mod = s.manifest.as_ref().is_some_and(|m| !m.joins);
            if s.host.side != crate::abi::Side::CLIENT || (server_mod && !s.host.hosting) {
                return Vec::new();
            }
            let now = advance(&mut s.controls, after);
            let FakeControls {
                controls,
                slots,
                runs,
                targets,
                ..
            } = &mut s.controls;
            let bound: Vec<[u32; 2]> = controls
                .iter()
                .map(|c| c.keys.map(|key| key.map_or(0, Key::bits)))
                .collect();
            let mods = down.iter().fold(0, |mods, k| mods | k.modifiers());
            chords(
                &bound,
                |_, _, key| down.iter().any(|k| k.code() == key.code()).then_some(mods),
                slots,
            );
            let mut calls = Vec::new();
            for (control, slots) in controls.iter_mut().zip(slots.iter()) {
                let kind = control.tracker.look(*slots != 0, now, control.hold_ms) & control.fires;
                for kind in phases(kind) {
                    calls.push(control.call(kind));
                    if !control.action.is_empty() {
                        runs.push((control.action.clone(), String::new()));
                        targets.push(None);
                    }
                }
            }
            calls
        });
        deliver(&calls, None)
    }

    /// Looks at `player`'s game actions on the server as the loader does
    /// once a step, `after` the last look: `held` are the actions the
    /// player's input sets now. Each control linked to one of them fires
    /// what the look decides: [`Phase::Press`](crate::controls::Phase::Press)
    /// when it comes on, then hold and release. As in the loader, they fire
    /// in a server's world only: a fake on the client side, a client in
    /// another's world, fires nothing. Returns how many calls the mod got.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    /// use enshwick::abi::{Header, PlayerInfo};
    /// use enshwick::controls::GameAction;
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
    /// let held = [GameAction::ContextualAction];
    /// assert_eq!(loader.game_actions(&player, &held, Duration::from_millis(50)), 0);
    /// ```
    #[must_use = "the count says whether a control fired"]
    pub fn game_actions(&self, player: &PlayerInfo, held: &[GameAction], after: Duration) -> usize {
        let entity = player.entity.0;
        let calls = with(Vec::new(), |s| {
            if s.host.side == crate::abi::Side::CLIENT {
                return Vec::new();
            }
            let now = advance(&mut s.controls, after);
            let mut calls = Vec::new();
            for control in &mut s.controls.controls {
                let Some(action) = control.link else {
                    continue;
                };
                if !control.players.iter().any(|(e, _)| *e == entity) {
                    control.players.push((entity, Tracker::default()));
                }
                let Some((_, tracker)) = control.players.iter_mut().find(|(e, _)| *e == entity)
                else {
                    continue;
                };
                let kind =
                    tracker.look(held.contains(&action), now, control.hold_ms) & control.fires;
                calls.extend(phases(kind).map(|kind| control.call(kind)));
            }
            calls
        });
        deliver(&calls, Some(player))
    }

    /// The mod's controls as the controls page lists them: name, label and
    /// the keys in force. As in the loader, the label is a key of the texts
    /// the mod handed over, in the fake's language, else as written.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.controls().is_empty());
    /// ```
    #[must_use]
    pub fn controls(&self) -> Vec<(String, String, [Option<Key>; 2])> {
        with(Vec::new(), |s| {
            let texts = s
                .published_texts
                .as_deref()
                .and_then(|toml| crate::i18n::Texts::parse(toml).ok());
            let language = crate::i18n::Language(s.host.language);
            s.controls
                .controls
                .iter()
                .map(|c| {
                    let label = texts
                        .as_ref()
                        .and_then(|t| t.get(&c.label, language))
                        .unwrap_or(&c.label);
                    (c.name.clone(), label.to_owned(), c.keys)
                })
                .collect()
        })
    }

    /// The player actions the mod asked the server to run, with
    /// [`Actions::run`](crate::actions::Actions::run) or a control's
    /// [`Control::runs`](crate::controls::Control::runs): action and arguments
    /// in the sheet's encoding, in order. As in the loader, only a client
    /// sends them: a fake on the server side refuses the run.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.action_runs().is_empty());
    /// ```
    #[must_use]
    pub fn action_runs(&self) -> Vec<(String, String)> {
        with(Vec::new(), |s| s.controls.runs.clone())
    }

    /// The object each of [`action_runs`](Self::action_runs) named with
    /// [`Actions::run_at`](crate::actions::Actions::run_at), in the same
    /// order; `None` for a run without one.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.action_targets().is_empty());
    /// ```
    #[must_use]
    pub fn action_targets(&self) -> Vec<Option<crate::actions::ActionTarget>> {
        with(Vec::new(), |s| s.controls.targets.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "system" fn nothing(event: *const ControlEvent, _: *mut c_void) {
        // SAFETY: the fake never calls this callback; a caller would pass
        // null or a live event readable for the size its header states.
        let _read = unsafe { read_headed(event, size_of::<ControlEvent>()) };
    }

    fn declared(key: u32, fires: u32) -> Status {
        let spec = ControlSpec {
            header: Header::of::<ControlSpec>(),
            name: Str::new("horn"),
            label: Str::new("Horn"),
            key,
            second: 0,
            link: Str::EMPTY,
            fires,
            hold_ms: 500,
            action: Str::EMPTY,
        };
        control(
            ModHandle(1),
            &raw const spec,
            nothing,
            core::ptr::null_mut(),
        )
    }

    #[test]
    fn the_fake_refuses_a_control_that_never_fires_or_an_unbindable_key() {
        let h = Key::parse("H").unwrap().bits();
        assert_eq!(declared(h, 0), Status::BAD_ARGS, "no phase");
        assert_eq!(declared(h, 8), Status::BAD_ARGS, "an unknown phase");
        assert_eq!(
            declared(0x01, ControlSpec::PRESS),
            Status::BAD_ARGS,
            "Escape"
        );
        assert_ne!(declared(h, ControlSpec::PRESS), Status::BAD_ARGS);
    }
}
