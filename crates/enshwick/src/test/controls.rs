//! The fake loader's controls: the mod's declared controls, keys the test
//! holds down, game actions it fires and the action runs a client sent.

use core::ffi::c_void;
use core::time::Duration;

use crate::abi::{
    ControlEvent, ControlFn, ControlSpec, Header, ModHandle, PlayerInfo, Status, Str, read_headed,
};
use crate::controls::{GameAction, Key, Tracker, chord_down, is_valid_name, phases, valid_phases};

use super::table::{Registration, text, with};
use super::{FakeHost, Installed};

/// A control the mod declared.
struct FakeControl {
    name: String,
    label: String,
    keys: [Option<Key>; 2],
    link: Option<GameAction>,
    fires: u32,
    hold_ms: u32,
    callback: ControlFn,
    user: usize,
    tracker: Tracker,
    /// The game action's tracker for each player entity.
    players: Vec<(u32, Tracker)>,
}

/// The mod's controls, the fake clock of its keys and the runs sent.
#[derive(Default)]
pub(crate) struct FakeControls {
    controls: Vec<FakeControl>,
    /// Milliseconds since the first look at the keys.
    now: u64,
    /// The action runs the mod sent the server: action and arguments.
    runs: Vec<(String, String)>,
}

pub(crate) extern "system" fn control(
    _me: ModHandle,
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
    if !is_valid_name(&name)
        || keys.iter().any(|k| matches!(k, Some(None)))
        || !valid_phases(spec.fires)
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
            callback,
            user: user.expose_provenance(),
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
    callback: ControlFn,
    user: usize,
}

impl FakeControl {
    fn call(&self, kind: u32) -> Call {
        Call {
            name: self.name.clone(),
            kind,
            callback: self.callback,
            user: self.user,
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
        unsafe {
            (call.callback)(
                &raw const event,
                core::ptr::with_exposed_provenance_mut(call.user),
            );
        };
    }
    calls.len()
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// Looks at the keys as the client's key reader does, `after` the last
    /// look: `down` are the keys held now, each with the modifiers held
    /// with it. Each control whose key matches fires what the look decides.
    /// As in the loader, only a client reads keys: on a server nothing
    /// fires. Returns how many calls the mod got.
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
            if s.host.side != crate::abi::Side::CLIENT {
                return Vec::new();
            }
            let now = advance(&mut s.controls, after);
            let mut calls = Vec::new();
            for control in &mut s.controls.controls {
                let held = control.keys.iter().flatten().any(|bound| {
                    down.iter().any(|k| {
                        k.code() == bound.code() && chord_down(*bound, true, k.modifiers())
                    })
                });
                let kind = control.tracker.look(held, now, control.hold_ms) & control.fires;
                calls.extend(phases(kind).map(|kind| control.call(kind)));
            }
            calls
        });
        deliver(&calls, None)
    }

    /// Looks at `player`'s game actions on the server as the loader does
    /// once a step, `after` the last look: `held` are the actions the
    /// player's input sets now. Each control linked to one of them fires
    /// what the look decides: [`Phase::Press`](crate::controls::Phase::Press)
    /// when it comes on, then hold and release. Returns how many calls the
    /// mod got.
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
            let now = advance(&mut s.controls, after);
            let mut calls = Vec::new();
            for control in &mut s.controls.controls {
                let Some(action) = control.link else {
                    continue;
                };
                let at = match control.players.iter().position(|(e, _)| *e == entity) {
                    Some(at) => at,
                    None => {
                        control.players.push((entity, Tracker::default()));
                        control.players.len() - 1
                    }
                };
                let tracker = &mut control.players[at].1;
                let kind =
                    tracker.look(held.contains(&action), now, control.hold_ms) & control.fires;
                calls.extend(phases(kind).map(|kind| control.call(kind)));
            }
            calls
        });
        deliver(&calls, Some(player))
    }

    /// The mod's controls as the controls page lists them: name, label and
    /// the keys in force.
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
            s.controls
                .controls
                .iter()
                .map(|c| (c.name.clone(), c.label.clone(), c.keys))
                .collect()
        })
    }

    /// The player actions the mod asked the server to run, with
    /// [`Actions::run`](crate::actions::Actions::run): action and arguments
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
}
