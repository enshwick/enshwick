//! Which carried controls each player gets: only those whose action the
//! player may run, sent again when the server changes their permissions.

use std::time::{Duration, Instant};

use enshwick::PlayerKey;
use enshwick::abi::{PlayerInfo, Side};
use rustc_hash::FxHashMap;

use super::{DECLARED, Declared, STATE, label_of};
use crate::host::Loader;
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::remote::{Carried, MAX_CARRIED, MAX_CARRIED_TEXT, Message, cut};

/// Sends the client of `key`, which just said hello, the controls of every
/// running mod of side server that run an action its player may run, in its
/// player's language; false when its link had no room.
pub(crate) fn hello(key: u64) -> bool {
    let (Some(loader), Some(declared)) = (crate::host::loader(), DECLARED.sealed()) else {
        return true;
    };
    if declared.iter().all(|d| d.action.is_empty()) {
        return true;
    }
    let player = crate::actions::player(loader, key);
    send_carried(loader, declared, key, player.as_ref())
}

/// Sends again the carried controls of each player whose permissions
/// changed since their client got them, at most once per [`EVERY`], on the
/// server's game thread after its step.
pub(crate) fn resend_on_rights(loader: &Loader) {
    let Some(declared) = DECLARED.sealed() else {
        return;
    };
    if declared.iter().all(|d| d.action.is_empty())
        || !with_rights(|rights| rights.due(Instant::now()))
    {
        return;
    }
    let players = crate::world::players(loader).unwrap_or_default();
    for player in with_rights(|rights| rights.changed(&players)) {
        let _sent = send_carried(loader, declared, player.key.get(), Some(&player));
    }
}

fn with_rights<R>(f: impl FnOnce(&mut Rights) -> R) -> R {
    f(STATE.rights.relock().get_or_insert_with(Rights::default))
}

/// Sends client `key` the carried controls `player` may run, none while its
/// player is not in the world, and notes the permissions they went for;
/// false when its link had no room.
fn send_carried(
    loader: &Loader,
    declared: &[Declared],
    key: u64,
    player: Option<&PlayerInfo>,
) -> bool {
    let sent = crate::texts::for_player(key, || {
        carried_messages(loader, declared, |owner| {
            player
                .and_then(|p| crate::actions::allowed(loader, owner, p))
                .unwrap_or_default()
        })
        .iter()
        .all(|message| crate::net::to_player(key, message))
    });
    if sent {
        with_rights(|rights| rights.sent(key, player.map(|p| p.permissions)));
    }
    sent
}

/// One [`Message::Controls`] per running mod of side server with controls
/// that run an action, its texts in the reader's language: those of the
/// actions `allowed` names for the mod, none to take back what a client got
/// before.
fn carried_messages(
    loader: &Loader,
    declared: &[Declared],
    mut allowed: impl FnMut(ModIndex) -> Vec<String>,
) -> Vec<Message> {
    let mut messages = Vec::new();
    for (index, record) in loader.mods.iter().enumerate() {
        if record.manifest.side != Side::SERVER
            || record.off.load(core::sync::atomic::Ordering::Acquire)
            || !declared
                .iter()
                .any(|d| d.owner.get() == index && !d.action.is_empty())
        {
            continue;
        }
        let allowed = allowed(ModIndex::new(index));
        let controls: Vec<Carried> = carried_for(declared, index, &allowed)
            .take(MAX_CARRIED)
            .map(|d| Carried {
                name: d.name.clone(),
                label: cut(&label_of(record, &d.label), MAX_CARRIED_TEXT).to_owned(),
                action: d.action.clone(),
                keys: d.defaults,
                fires: d.fires,
                hold_ms: d.hold_ms,
            })
            .collect();
        messages.push(Message::Controls {
            id: record.id.clone(),
            name: cut(&record.label(), MAX_CARRIED_TEXT).to_owned(),
            controls,
        });
    }
    messages
}

/// How often the server compares its players' permissions with those their
/// carried controls were sent for.
const EVERY: Duration = Duration::from_secs(2);

/// The permissions each client's carried controls were sent for, by account
/// key; `None` while its player was not in the world yet.
#[derive(Debug, Default)]
pub(super) struct Rights {
    sent: FxHashMap<u64, Option<u8>>,
    looked: Option<Instant>,
}

impl Rights {
    /// Client `key` got the controls its player's `permissions` allow.
    pub(super) fn sent(&mut self, key: u64, permissions: Option<u8>) {
        self.sent.insert(key, permissions);
    }

    /// Whether a look is due at `now`, once per [`EVERY`].
    pub(super) fn due(&mut self, now: Instant) -> bool {
        let due = self.looked.is_none_or(|at| now.duration_since(at) >= EVERY);
        if due {
            self.looked = Some(now);
        }
        due
    }

    /// The players among `players` whose client got controls and whose
    /// permissions differ from those they were sent for; a client whose
    /// player left is forgotten.
    pub(super) fn changed(&mut self, players: &[PlayerInfo]) -> Vec<PlayerInfo> {
        let here = |key: u64| players.iter().any(|p| p.key == PlayerKey::new(key));
        self.sent
            .retain(|&key, permissions| permissions.is_none() || here(key));
        players
            .iter()
            .filter(|p| {
                self.sent
                    .get(&p.key.get())
                    .is_some_and(|sent| *sent != Some(p.permissions))
            })
            .copied()
            .collect()
    }
}

/// The controls of mod `owner` in `declared` that run an action `allowed`
/// names.
pub(super) fn carried_for<'a>(
    declared: &'a [Declared],
    owner: usize,
    allowed: &'a [String],
) -> impl Iterator<Item = &'a Declared> {
    declared.iter().filter(move |d| {
        d.owner.get() == owner && !d.action.is_empty() && allowed.contains(&d.action)
    })
}

#[cfg(test)]
mod tests {
    use enshwick::abi::{ControlEvent, Header};
    use enshwick::{Entity, PlayerKey};

    use super::*;
    use crate::mod_index::ModIndex;

    unsafe extern "system" fn ignored(_event: *const ControlEvent, _user: *mut core::ffi::c_void) {}

    fn control(owner: usize, name: &str, action: &str) -> Declared {
        Declared {
            owner: ModIndex::new(owner),
            id: format!("you.locker:{name}"),
            name: name.to_owned(),
            label: name.to_owned(),
            defaults: [0, 0],
            link: String::new(),
            action: action.to_owned(),
            fires: 1,
            hold_ms: 0,
            callback: ignored,
            user: 0,
        }
    }

    fn player(key: u64, permissions: u8) -> PlayerInfo {
        PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: Entity(1),
            key: PlayerKey::new(key),
            permissions,
            name_len: 0,
            name: [0; 64],
        }
    }

    fn names<'a>(controls: impl Iterator<Item = &'a Declared>) -> Vec<&'a str> {
        controls.map(|d| d.name.as_str()).collect()
    }

    #[test]
    fn an_admin_gets_an_admin_only_control_and_a_player_does_not() {
        let declared = [
            control(0, "greet", "greet"),
            control(0, "wipe", "wipe"),
            control(0, "look", ""),
            control(1, "other", "greet"),
        ];
        // What each one's sheet lists: the mod's allow keeps `wipe` for admins.
        let admin = ["greet".to_owned(), "wipe".to_owned()];
        let player = ["greet".to_owned()];
        assert_eq!(names(carried_for(&declared, 0, &admin)), ["greet", "wipe"]);
        assert_eq!(names(carried_for(&declared, 0, &player)), ["greet"]);
        assert_eq!(names(carried_for(&declared, 0, &[])), Vec::<&str>::new());
    }

    #[test]
    fn controls_go_again_when_a_player_s_permissions_change() {
        let mut rights = Rights::default();
        rights.sent(7, Some(0));
        rights.sent(8, None);
        let keys = |changed: Vec<PlayerInfo>| -> Vec<u64> {
            changed.iter().map(|p| p.key.get()).collect()
        };
        // 8 said hello before its player was in the world; 9 never did.
        let here = [player(7, 0), player(8, 0), player(9, 1)];
        assert_eq!(keys(rights.changed(&here)), [8]);
        rights.sent(8, Some(0));
        assert_eq!(keys(rights.changed(&here)), Vec::<u64>::new());
        assert_eq!(keys(rights.changed(&[player(7, 1), player(8, 0)])), [7]);
        // 7 left: a later player with its key starts without controls.
        assert_eq!(keys(rights.changed(&[player(8, 0)])), Vec::<u64>::new());
        assert_eq!(keys(rights.changed(&[player(7, 1)])), Vec::<u64>::new());
    }

    #[test]
    fn a_look_is_due_once_per_interval() {
        let mut rights = Rights::default();
        let start = Instant::now();
        assert!(rights.due(start));
        assert!(!rights.due(start + EVERY / 2));
        assert!(rights.due(start + EVERY));
    }
}
