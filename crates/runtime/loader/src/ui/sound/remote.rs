//! Sounds a server mod plays on the clients that run Enshwick and the mod:
//! the server picks the clients, sends each a sound message on the own
//! channel (see [`wire::sound`](crate::base::wire::sound)), and each client
//! starts it through the mod's own queue, as the mod's client part would.
//! A hosting client's own player gets it in process, without a message.
//!
//! The server counts each mod's sounds and keeps to whom its last
//! [`MAX_STEERED`] went, so a change reaches the same clients; each client
//! keeps which of its sounds stands for each of those ids.

use std::collections::VecDeque;

use enshwick_core::abi::SoundAudience;

/// The sounds of one mod a change still reaches, newest last, on the
/// server and on each client.
pub(crate) const MAX_STEERED: usize = 256;

/// The newest [`MAX_STEERED`] ids of one mod with what each keeps.
#[derive(Debug)]
struct Recent<T>(VecDeque<(u64, T)>);

impl<T> Default for Recent<T> {
    fn default() -> Self {
        Self(VecDeque::new())
    }
}

impl<T> Recent<T> {
    /// Keeps `value` for `id`, forgetting the oldest past [`MAX_STEERED`].
    fn keep(&mut self, id: u64, value: T) {
        if self.0.len() >= MAX_STEERED {
            self.0.pop_front();
        }
        self.0.push_back((id, value));
    }

    /// Forgets what `id` keeps.
    #[cfg_attr(
        not(windows),
        expect(dead_code, reason = "only the Windows relay forgets a heard sound")
    )]
    fn forget(&mut self, id: u64) {
        self.0.retain(|(kept, _)| *kept != id);
    }

    /// What `id` keeps, while it is among the newest.
    fn get(&self, id: u64) -> Option<&T> {
        self.0
            .iter()
            .rev()
            .find(|(kept, _)| *kept == id)
            .map(|(_, value)| value)
    }
}

/// One mod's sounds sent from the server.
#[derive(Debug, Default)]
struct Sent {
    /// The last id given; ids count from 1.
    last: u64,
    /// The step of the last sound and how many went in it.
    step: (u64, u32),
    /// The clients each recent sound went to.
    #[cfg_attr(
        not(windows),
        expect(dead_code, reason = "only the Windows relay steers a sent sound")
    )]
    to: Recent<Box<[u64]>>,
}

impl Sent {
    /// The next sound's id in server step `step`, or `None` past
    /// [`SoundAudience::MAX_PLAYS`] in that step.
    fn admit(&mut self, step: u64) -> Option<u64> {
        let count = if self.step.0 == step { self.step.1 } else { 0 };
        if count >= SoundAudience::MAX_PLAYS {
            return None;
        }
        self.step = (step, count + 1);
        self.last += 1;
        Some(self.last)
    }
}

#[cfg(windows)]
pub(crate) use windows::*;

#[cfg(windows)]
mod windows {
    use std::sync::Mutex;

    use enshwick_core::abi::{SoundAudience, SoundPlay, SoundSteer};
    use enshwick_core::{Entity, PlayerKey};
    use rustc_hash::FxHashMap;

    use super::{Recent, Sent};
    use crate::base::wire::{SoundMessage, SoundOp};
    use crate::engine::loader::Loader;
    use crate::engine::mod_index::ModIndex;
    use crate::engine::world::WorldError;
    use enshwick_base::Relock;

    /// What the relay keeps while the game runs: each mod's sounds by mod
    /// index, on the server and on the client.
    struct State {
        sent: Mutex<Option<FxHashMap<usize, Sent>>>,
        /// For each mod and server id, the client's own sound id.
        heard: Mutex<Option<FxHashMap<usize, Recent<u64>>>>,
    }

    static STATE: State = State {
        sent: Mutex::new(None),
        heard: Mutex::new(None),
    };

    /// Why a mod's sound for clients is refused.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
    pub(crate) enum RelayError {
        /// No server runs in this process.
        #[error("no server runs here")]
        NotServing,
        /// The mod does not run on both sides, so no client plays it.
        #[error("the mod does not run on both sides")]
        OneSided,
        /// The audience, play or change is out of bounds.
        #[error("the audience, play or change is out of bounds")]
        BadArgs,
        /// The entity to play on is not in the world.
        #[error("the entity is not in the world")]
        NoEntity,
        /// A read of the world off the server's game thread.
        #[error("called off the game thread")]
        WrongThread,
        /// The mod's sounds for this server step are spent.
        #[error("the mod's sounds for this step are spent")]
        Full,
    }

    impl From<WorldError> for RelayError {
        fn from(error: WorldError) -> Self {
            match error {
                WorldError::WrongThread | WorldError::InStep => Self::WrongThread,
                WorldError::Absent => Self::NoEntity,
                _ => Self::NotServing,
            }
        }
    }

    /// Refuses a mod's call where no server runs or for a mod that does not
    /// run on both sides; the hash that names the mod in its messages.
    fn serving(loader: &Loader, owner: ModIndex) -> Result<u32, RelayError> {
        if !crate::engine::world::hosting() {
            return Err(RelayError::NotServing);
        }
        match loader.mods.get(owner.get()) {
            Some(record) if record.joins_when_on() => Ok(owner_hash(&record.id)),
            _ => Err(RelayError::OneSided),
        }
    }

    /// Where `entity` stands in metres, on the game thread.
    fn metres(loader: &Loader, entity: Entity) -> Result<[f64; 3], RelayError> {
        crate::engine::world::reader()?;
        crate::engine::world::fixed_position(loader, entity)
            .map(enshwick_core::ecs::metres)
            .ok_or(RelayError::NoEntity)
    }

    /// The account keys of the clients `audience` names that run Enshwick.
    fn audience_keys(loader: &Loader, audience: &SoundAudience) -> Result<Vec<u64>, RelayError> {
        let mut positions = FxHashMap::default();
        if audience.kind == SoundAudience::NEAR {
            for player in crate::engine::world::players(loader)? {
                if let Some(at) = crate::engine::world::fixed_position(loader, player.entity) {
                    positions.insert(player.key.get(), enshwick_core::ecs::metres(at));
                }
            }
        }
        let mut keys: Vec<u64> = crate::base::net::clients()
            .into_iter()
            .chain(crate::base::carrier::own_account().map(PlayerKey::get))
            .filter(|&key| audience.hears(PlayerKey::new(key), positions.get(&key).copied()))
            .collect();
        // The host's own player may also have said hello in process.
        keys.sort_unstable();
        keys.dedup();
        Ok(keys)
    }

    /// Sends `message` to `keys`: the hosting client's own player hears it
    /// in process, the rest by the own channel.
    fn send(loader: &Loader, keys: &[u64], message: &SoundMessage) {
        let (own, others): (Vec<u64>, Vec<u64>) = keys
            .iter()
            .partition(|&&key| crate::base::carrier::is_host(key));
        if !own.is_empty() {
            take(loader, *message);
        }
        if !others.is_empty() {
            crate::base::net::sound(&others, &crate::base::wire::sound(message));
        }
    }

    /// The FNV-1a hash that names mod `id` in a sound message.
    fn owner_hash(id: &str) -> u32 {
        enshwick_gamedata::reflect::fnv1a32(id.as_bytes())
    }

    /// Sends mod `owner`'s `play` to the clients of `audience`; the sound's
    /// id.
    ///
    /// # Errors
    ///
    /// [`RelayError`] as its variants say.
    pub(crate) fn play_for(
        loader: &Loader,
        owner: ModIndex,
        audience: &SoundAudience,
        play: &SoundPlay,
    ) -> Result<u64, RelayError> {
        let hash = serving(loader, owner)?;
        if !audience.in_bounds() || !play.in_bounds() {
            return Err(RelayError::BadArgs);
        }
        let mut play = *play;
        if play.place == SoundPlay::ON {
            play.at = metres(loader, play.entity)?;
            play.place = SoundPlay::AT;
            play.entity = Entity::NONE;
        }
        let keys = audience_keys(loader, audience)?;
        let id = STATE
            .sent
            .relock()
            .get_or_insert_with(FxHashMap::default)
            .entry(owner.get())
            .or_default()
            .admit(crate::engine::ticks::steps())
            .ok_or(RelayError::Full)?;
        let message = SoundMessage {
            owner: hash,
            id,
            op: SoundOp::Play(play),
        };
        send(loader, &keys, &message);
        if let Some(sent) = STATE
            .sent
            .relock()
            .as_mut()
            .and_then(|s| s.get_mut(&owner.get()))
        {
            sent.to.keep(id, keys.into_boxed_slice());
        }
        Ok(id)
    }

    /// Sends mod `owner`'s `steer` of its sound `id` to the clients that got
    /// it; nothing for an id of no recent sound.
    ///
    /// # Errors
    ///
    /// [`RelayError::NotServing`], [`RelayError::OneSided`], and
    /// [`RelayError::BadArgs`] out of bounds or for
    /// [`SoundSteer::STOP_ALL`].
    pub(crate) fn steer_for(
        loader: &Loader,
        owner: ModIndex,
        id: u64,
        steer: &SoundSteer,
    ) -> Result<(), RelayError> {
        let hash = serving(loader, owner)?;
        if !steer.in_bounds() || steer.kind == SoundSteer::STOP_ALL {
            return Err(RelayError::BadArgs);
        }
        let keys: Option<Vec<u64>> = STATE
            .sent
            .relock()
            .as_ref()
            .and_then(|sent| Some(sent.get(&owner.get())?.to.get(id)?.to_vec()));
        if let Some(keys) = keys {
            let message = SoundMessage {
                owner: hash,
                id,
                op: SoundOp::Steer(*steer),
            };
            send(loader, &keys, &message);
        }
        Ok(())
    }

    /// Takes a sound message the server sent, after its control kind, on
    /// the client: a mod it does not run, a message it cannot read and a
    /// start the client's sounds refuse are dropped, a container it lacks
    /// with one log line.
    pub(crate) fn received(loader: &Loader, bytes: &[u8]) {
        if let Some(message) = crate::base::wire::read_sound(bytes) {
            take(loader, message);
        } else {
            crate::log_once!(
                WARN,
                "server sounds: a message that does not read is dropped"
            );
        }
    }

    /// The client's own id for mod `owner`'s server sound `id`, for a test.
    #[cfg(test)]
    pub(super) fn heard(owner: usize, id: u64) -> Option<u64> {
        STATE
            .heard
            .relock()
            .as_ref()
            .and_then(|heard| heard.get(&owner)?.get(id).copied())
    }

    /// Keeps the client's own sound `local` for mod `owner`'s server sound
    /// `id`; `None`, for a start the client dropped, forgets an older sound
    /// of the same id, as from an earlier server.
    pub(super) fn hear(owner: ModIndex, id: u64, local: Option<u64>) {
        let mut heard = STATE.heard.relock();
        let recent = heard
            .get_or_insert_with(FxHashMap::default)
            .entry(owner.get())
            .or_default();
        match local {
            Some(local) => recent.keep(id, local),
            None => recent.forget(id),
        }
    }

    /// Plays or steers `message` through the mod's own sounds.
    fn take(loader: &Loader, message: SoundMessage) {
        let Some(owner) = loader
            .mods
            .iter()
            .position(|record| owner_hash(&record.id) == message.owner)
            .map(ModIndex::new)
            .filter(|&owner| !crate::engine::loader::is_off(owner))
        else {
            return;
        };
        match message.op {
            SoundOp::Play(play) => {
                let started = crate::ui::sound::play(loader, owner, &play);
                if started == Err(crate::ui::sound::SoundError::Missing) {
                    crate::log_once!(
                        WARN,
                        "server sounds: a sound container this client lacks is dropped ({})",
                        play.container
                    );
                }
                hear(owner, message.id, started.ok());
            }
            SoundOp::Steer(steer) => {
                let local = STATE
                    .heard
                    .relock()
                    .as_ref()
                    .and_then(|heard| heard.get(&owner.get())?.get(message.id).copied());
                if let Some(local) = local {
                    let _steered = crate::ui::sound::steer(loader, owner, local, &steer);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_count_from_one_and_stop_at_the_step_bound() {
        let mut sent = Sent::default();
        let ids: Vec<u64> = (0..SoundAudience::MAX_PLAYS)
            .filter_map(|_| sent.admit(4))
            .collect();
        assert_eq!(ids.first(), Some(&1));
        assert_eq!(ids.len(), SoundAudience::MAX_PLAYS as usize);
        assert_eq!(sent.admit(4), None);
        assert_eq!(sent.admit(5), Some(u64::from(SoundAudience::MAX_PLAYS) + 1));
    }

    /// A client running mods `ids`, the loader this test's thread answers.
    #[cfg(windows)]
    fn client(ids: &[&'static str]) -> &'static crate::engine::loader::Loader {
        crate::engine::loader::install_for_test(crate::engine::loader::Loader {
            side: enshwick_core::abi::Side::CLIENT,
            ..crate::start::tests::loader_running(ids)
        })
    }

    #[cfg(windows)]
    fn start_of(owner: &str, container: [u8; 16]) -> Vec<u8> {
        use enshwick_core::abi::SoundPlay;
        crate::base::wire::sound(&crate::base::wire::SoundMessage {
            owner: enshwick_gamedata::reflect::fnv1a32(owner.as_bytes()),
            id: 1,
            op: crate::base::wire::SoundOp::Play(enshwick_abi::headed!(SoundPlay {
                container: enshwick_core::content::Guid(container),
                ..SoundPlay::EMPTY
            })),
        })
    }

    #[cfg(windows)]
    #[test]
    fn a_client_drops_a_sound_whose_container_it_lacks() {
        let loader = client(&["you.bells"]);
        received(loader, &start_of("you.bells", [7; 16])[2..]);
        assert_eq!(heard(0, 1), None);
    }

    #[cfg(windows)]
    #[test]
    fn a_dropped_start_forgets_an_earlier_server_s_sound_of_the_same_id() {
        // The second mod, so no other test's sounds share its ids.
        let loader = client(&["you.anvils", "you.bells"]);
        let bells = crate::engine::mod_index::ModIndex::new(1);
        hear(bells, 1, Some(0x5_0000_0003));
        received(loader, &start_of("you.bells", [7; 16])[2..]);
        assert_eq!(heard(1, 1), None);
    }

    #[cfg(windows)]
    #[test]
    fn a_client_drops_a_sound_of_a_mod_it_does_not_run_and_one_that_does_not_read() {
        let loader = client(&["you.bells"]);
        received(loader, &start_of("you.horns", [7; 16])[2..]);
        received(loader, &[1, 2, 3]);
        assert_eq!(heard(0, 1), None);
    }

    #[cfg(windows)]
    #[test]
    fn a_mod_sends_clients_no_sound_where_no_server_runs() {
        use enshwick_core::abi::{SoundPlay, SoundSteer};
        let loader = client(&["you.bells"]);
        let owner = crate::engine::mod_index::ModIndex::new(0);
        let play = play_for(loader, owner, &SoundAudience::EMPTY, &SoundPlay::EMPTY);
        assert_eq!(play, Err(RelayError::NotServing));
        let steer = steer_for(loader, owner, 1, &SoundSteer::EMPTY);
        assert_eq!(steer, Err(RelayError::NotServing));
    }

    #[test]
    fn only_the_newest_ids_are_kept() {
        let mut recent = Recent::default();
        for id in 1..=MAX_STEERED as u64 + 1 {
            recent.keep(id, id * 10);
        }
        assert_eq!(recent.get(1), None);
        assert_eq!(recent.get(2), Some(&20));
        assert_eq!(
            recent.get(MAX_STEERED as u64 + 1),
            Some(&(MAX_STEERED as u64 * 10 + 10))
        );
    }
}
