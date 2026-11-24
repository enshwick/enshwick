//! Which carried controls each player gets: only those whose action the
//! player may run, sent again when the server changes their permissions or
//! a mod says who may run its actions changed.

use core::sync::atomic::{AtomicU64, Ordering};

use enshwick_core::PlayerKey;
use enshwick_core::abi::{PlayerInfo, Side};
use rustc_hash::FxHashMap;

use super::{DECLARED, Declared, STATE, label_of};
use crate::base::remote::{Carried, MAX_CARRIED, MAX_CARRIED_TEXT, Message, cut};
use crate::engine::loader::Loader;
use crate::engine::mod_index::ModIndex;
use enshwick_base::Relock;

/// Sends the client of `key`, which just said hello, the controls of every
/// running mod of side server that run an action its player may run, in its
/// player's language; false when its link had no room.
pub(crate) fn hello(key: u64) -> bool {
    let (Some(loader), Some(declared)) = (crate::engine::loader::loader(), DECLARED.sealed())
    else {
        return true;
    };
    if declared.iter().all(|d| d.action.is_empty()) {
        return true;
    }
    with_rights(|rights| rights.hello(key));
    let player = crate::engine::world::player_with_key(loader, key);
    send_carried(loader, declared, key, player.as_ref(), &Resend::All)
}

/// Sends again the carried controls each client is due, at most once per
/// [`EVERY_MS`], on the server's game thread after its step: every mod's to
/// a player whose permissions changed since their client got them, else
/// those of the mods whose rules changed.
pub(crate) fn resend_on_rights(loader: &Loader) {
    if !due(&STATE.next_look, super::now_ms()) {
        return;
    }
    let Some(declared) = DECLARED.sealed() else {
        return;
    };
    if declared.iter().all(|d| d.action.is_empty()) {
        return;
    }
    if with_rights(|rights| rights.sent.is_empty()) {
        return;
    }
    // A failed read is no list of who left: it would forget every client.
    let Ok(players) = crate::engine::world::players(loader) else {
        return;
    };
    for (player, resend) in with_rights(|rights| rights.due(&players)) {
        let key = player.key.get();
        if !send_carried(loader, declared, key, Some(&player), &resend) {
            with_rights(|rights| rights.unsent(key, &resend));
        }
    }
}

/// Mod `owner` said who may run its actions changed: the next look sends
/// every client that mod's carried controls again, until a send to it goes
/// through.
pub(crate) fn rules_changed(owner: ModIndex) {
    with_rights(|rights| rights.stale(owner));
}

/// Forgets what client `key` got, when its player leaves the server.
pub(crate) fn left(key: u64) {
    if let Some(rights) = STATE.rights.relock().as_mut() {
        rights.left(key);
    }
}

fn with_rights<R>(f: impl FnOnce(&mut Rights) -> R) -> R {
    f(STATE.rights.relock().get_or_insert_with(Rights::default))
}

/// Sends client `key` the carried controls `player` may run of the mods
/// `resend` covers, none while its player is not in the world, and after a
/// full send notes the permissions they went for; false when its link had
/// no room.
fn send_carried(
    loader: &Loader,
    declared: &[Declared],
    key: u64,
    player: Option<&PlayerInfo>,
    resend: &Resend,
) -> bool {
    // A client that just said hello holds nothing to take back.
    let sent = player.is_none()
        || crate::base::net::for_player(key, || {
            carried_messages(loader, declared, resend, |owner| {
                player
                    .and_then(|p| crate::actions::player_actions::allowed(loader, owner, p))
                    .unwrap_or_default()
            })
            .iter()
            .all(|message| crate::base::net::to_player(key, message))
        });
    if sent && *resend == Resend::All {
        with_rights(|rights| rights.sent(key, player.map(|p| p.permissions)));
    }
    sent
}

/// One [`Message::Controls`] per mod of side server with controls that run
/// an action and that `resend` covers, its texts in the reader's language:
/// those of the actions `allowed` names for the mod, none for a mod that is
/// off or to take back what a client got before.
fn carried_messages(
    loader: &Loader,
    declared: &[Declared],
    resend: &Resend,
    mut allowed: impl FnMut(ModIndex) -> Vec<String>,
) -> Vec<Message> {
    let mut messages = Vec::new();
    for (index, record) in loader.mods.iter().enumerate() {
        if record.manifest.side != Side::SERVER
            || !resend.covers(ModIndex::new(index))
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
            name: cut(&crate::base::manifest::label(record), MAX_CARRIED_TEXT).to_owned(),
            controls,
        });
    }
    messages
}

/// How often, in milliseconds, the server compares its players' permissions
/// with those their carried controls were sent for.
const EVERY_MS: u64 = 2_000;

/// Whether a look is due at `now` (milliseconds), once per [`EVERY_MS`]
/// after the last; one atomic read while none is.
fn due(next: &AtomicU64, now: u64) -> bool {
    if now < next.load(Ordering::Relaxed) {
        return false;
    }
    next.store(now.saturating_add(EVERY_MS), Ordering::Relaxed);
    true
}

/// Which mods' carried controls go to a client.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Resend {
    /// Every mod's: the client is new or its player's permissions changed.
    All,
    /// Only those of these mods, whose rules changed.
    Mods(Vec<ModIndex>),
}

impl Resend {
    /// Whether the controls of mod `owner` go.
    fn covers(&self, owner: ModIndex) -> bool {
        match self {
            Self::All => true,
            Self::Mods(mods) => mods.contains(&owner),
        }
    }
}

/// What one client got: the permissions its carried controls were sent for,
/// `None` while its player was not in the world yet, and the mods whose
/// rules changed since.
#[derive(Debug, Default)]
struct Sent {
    permissions: Option<u32>,
    stale: Vec<ModIndex>,
}

impl Sent {
    /// The controls of mod `owner` go again at the next look.
    fn mark(&mut self, owner: ModIndex) {
        if !self.stale.contains(&owner) {
            self.stale.push(owner);
        }
    }
}

/// What each client's carried controls were sent for, by account key.
#[derive(Debug, Default)]
pub(super) struct Rights {
    sent: FxHashMap<u64, Sent>,
}

impl Rights {
    /// Client `key` said hello: noted before its first send, so a rules
    /// change during that send marks it too.
    pub(super) fn hello(&mut self, key: u64) {
        self.sent.entry(key).or_default();
    }

    /// Client `key` got every mod's controls its player's `permissions`
    /// allow; a rule that changed during the send stays due.
    pub(super) fn sent(&mut self, key: u64, permissions: Option<u32>) {
        self.sent.entry(key).or_default().permissions = permissions;
    }

    /// Client `key` left, whether or not its player reached the world.
    pub(super) fn left(&mut self, key: u64) {
        self.sent.remove(&key);
    }

    /// The rules of mod `owner` changed: its controls go again to every
    /// client at the next look.
    pub(super) fn stale(&mut self, owner: ModIndex) {
        for sent in self.sent.values_mut() {
            sent.mark(owner);
        }
    }

    /// The send of `resend` to client `key` found no room: what it carried
    /// goes again at the next look. A full send needs nothing, since the
    /// permissions it was for were never noted.
    pub(super) fn unsent(&mut self, key: u64, resend: &Resend) {
        let (Resend::Mods(mods), Some(sent)) = (resend, self.sent.get_mut(&key)) else {
            return;
        };
        for &owner in mods {
            sent.mark(owner);
        }
    }

    /// The players among `players` whose client got controls and is due
    /// some again, with what goes: all of them when the player's
    /// permissions differ from those they were sent for, else those of the
    /// mods whose rules changed, taken now so a change during the send is
    /// not lost. A client whose player left is forgotten.
    pub(super) fn due(&mut self, players: &[PlayerInfo]) -> Vec<(PlayerInfo, Resend)> {
        let here = |key: u64| players.iter().any(|p| p.key == PlayerKey::new(key));
        self.sent
            .retain(|&key, sent| sent.permissions.is_none() || here(key));
        players
            .iter()
            .filter_map(|p| {
                let sent = self.sent.get_mut(&p.key.get())?;
                let stale = core::mem::take(&mut sent.stale);
                if sent.permissions != Some(p.permissions) {
                    return Some((*p, Resend::All));
                }
                (!stale.is_empty()).then_some((*p, Resend::Mods(stale)))
            })
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
    use enshwick_core::abi::{ControlEvent, Header};
    use enshwick_core::{Entity, PlayerKey};

    use super::*;
    use crate::engine::mod_index::ModIndex;

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

    fn player(key: u64, permissions: u32) -> PlayerInfo {
        PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: Entity(1),
            key: PlayerKey::new(key),
            permissions,
            name_len: 0,
            name: [0; 128],
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
        // What the mod allows each: its allow keeps `wipe` for admins.
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
        let keys = |due: Vec<(PlayerInfo, Resend)>| -> Vec<u64> {
            due.iter().map(|(p, _)| p.key.get()).collect()
        };
        // 8 said hello before its player was in the world; 9 never did.
        let here = [player(7, 0), player(8, 0), player(9, 1)];
        assert_eq!(keys(rights.due(&here)), [8]);
        rights.sent(8, Some(0));
        assert_eq!(keys(rights.due(&here)), Vec::<u64>::new());
        assert_eq!(keys(rights.due(&[player(7, 1), player(8, 0)])), [7]);
        // 7 left: a later player with its key starts without controls.
        assert_eq!(keys(rights.due(&[player(8, 0)])), Vec::<u64>::new());
        assert_eq!(keys(rights.due(&[player(7, 1)])), Vec::<u64>::new());
        // 10 said hello and left before its player was in the world.
        rights.sent(10, None);
        rights.left(10);
        assert_eq!(keys(rights.due(&[player(10, 0)])), Vec::<u64>::new());
    }

    #[test]
    fn only_the_mod_whose_rules_changed_goes_again_to_every_client() {
        let mut rights = Rights::default();
        rights.sent(7, Some(0));
        rights.sent(8, Some(1));
        let here = [player(7, 0), player(8, 1), player(9, 0)];
        let due = |rights: &mut Rights| -> Vec<(u64, Resend)> {
            rights
                .due(&here)
                .into_iter()
                .map(|(p, resend)| (p.key.get(), resend))
                .collect()
        };
        assert_eq!(due(&mut rights), []);
        rights.stale(ModIndex::new(2));
        rights.stale(ModIndex::new(2));
        let mod_2 = || Resend::Mods(vec![ModIndex::new(2)]);
        assert_eq!(
            due(&mut rights),
            [(7, mod_2()), (8, mod_2())],
            "9 never said hello"
        );
        assert_eq!(due(&mut rights), [], "a look takes what it sends");
        // 8's send found no room: it goes again at the next look.
        rights.unsent(8, &mod_2());
        assert_eq!(due(&mut rights), [(8, mod_2())]);
    }

    #[test]
    fn a_rules_change_during_a_send_goes_at_the_next_look() {
        let mut rights = Rights::default();
        rights.sent(7, Some(0));
        rights.stale(ModIndex::new(1));
        let here = [player(7, 1)];
        let resends = |rights: &mut Rights| -> Vec<Resend> {
            rights.due(&here).into_iter().map(|(_, r)| r).collect()
        };
        assert_eq!(
            resends(&mut rights),
            [Resend::All],
            "its permissions changed"
        );
        // Mod 3 changed its rules while the full send was on its way.
        rights.stale(ModIndex::new(3));
        rights.sent(7, Some(1));
        assert_eq!(resends(&mut rights), [Resend::Mods(vec![ModIndex::new(3)])]);
    }

    #[test]
    fn a_rules_change_during_a_new_client_s_first_send_goes_at_the_next_look() {
        let mut rights = Rights::default();
        rights.hello(7);
        // Mod 2 changed its rules while the hello send was on its way.
        rights.stale(ModIndex::new(2));
        rights.sent(7, Some(0));
        let resends: Vec<Resend> = rights
            .due(&[player(7, 0)])
            .into_iter()
            .map(|(_, r)| r)
            .collect();
        assert_eq!(resends, [Resend::Mods(vec![ModIndex::new(2)])]);
    }

    #[test]
    fn a_resend_covers_all_mods_or_those_it_names() {
        assert!(Resend::All.covers(ModIndex::new(4)));
        let some = Resend::Mods(vec![ModIndex::new(1), ModIndex::new(3)]);
        assert!(some.covers(ModIndex::new(3)));
        assert!(!some.covers(ModIndex::new(2)));
    }

    #[test]
    fn a_look_is_due_once_per_interval() {
        let next = AtomicU64::new(0);
        assert!(due(&next, 5));
        assert!(!due(&next, 5 + EVERY_MS / 2));
        assert!(due(&next, 5 + EVERY_MS));
        assert!(!due(&next, 5 + EVERY_MS + 1));
    }
}
