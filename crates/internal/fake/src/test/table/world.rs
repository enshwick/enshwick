//! The fake world's entries: players, entities, chat, saves, transactions,
//! altars, buffs, its surroundings, the map and build rights.

use crate::test::table::base::args;
use crate::test::table::entries;
use core::ffi::c_void;

use crate::test::table::{Registered, Registration, put, text, with, with_open};
use enshwick_core::abi::{
    AccountActionType, EntityPlace, JoinRuleFn, ModHandle, PlayerInfo, RenameFilterFn, Side,
    Status, Str, TerrainEdit,
};

pub(crate) mod altars;
pub(crate) mod buffs;
pub(crate) mod environment;
pub(crate) mod permissions;
pub(crate) mod saves;
pub(crate) mod transactions;
pub(crate) mod waypoints;

/// What the installed fake's world answers and has recorded.
#[derive(Default)]
pub(crate) struct WorldState {
    pub(crate) chat: Vec<(enshwick_core::PlayerKey, String)>,
    pub(crate) labels: Vec<(u32, String)>,
    pub(crate) rename_filters: Vec<Registered<RenameFilterFn>>,
    pub(crate) join_rules: Vec<Registered<JoinRuleFn>>,
    /// The entities the mod asked to destroy, in order.
    pub(crate) destroyed: Vec<u32>,
    /// How many saves the mod asked for.
    pub(crate) saves: usize,
    /// The accounts on the fake server's ban list, in the order banned.
    pub(crate) banned: Vec<enshwick_core::PlayerKey>,
    /// The accounts the mod removed from the game, by kick or ban, in order.
    pub(crate) removed: Vec<u64>,
    /// The accounts muted in the fake server's chat, in the order muted.
    pub(crate) muted: Vec<enshwick_core::PlayerKey>,
    /// The components the mod asked to write, `(entity, type hash)`.
    pub(crate) writes: Vec<(u32, u32)>,
    /// The game events the mod raised: type hash and bytes.
    pub(crate) emits: Vec<(u32, Vec<u8>)>,
    /// The game events each mod raised with notify in the step it last
    /// raised one.
    pub(crate) notified: Notified,
    /// The teleports the mod asked for.
    pub(crate) teleports: Vec<enshwick_core::abi::TeleportRequest>,
    /// The spawns the mod asked for, with the ids handed out.
    pub(crate) spawns: Vec<(u32, enshwick_core::abi::SpawnRequest)>,
    /// The step each of `spawns` was asked in: the entity is in the world
    /// from the next one, as the loader's spawns are.
    pub(crate) spawn_steps: Vec<u64>,
    /// The level and experience writes the mod asked for: character, what
    /// and value; `None` until it asks for them in `init`.
    pub(crate) progress_writes: Option<Vec<(enshwick_core::Entity, u32, u32)>>,
    /// The mods that asked for level and experience writes in `init`.
    pub(crate) progress_writers: Vec<ModHandle>,
    /// The writes that wait for the level step, and the progress of each
    /// character written.
    pub(crate) progress: crate::test::world::progress::ProgressState,
    /// The drops the mod asked for, with the ids handed out.
    pub(crate) drops: Vec<(enshwick_core::Entity, enshwick_core::abi::DropRequest)>,
    /// The server stops the mod asked for and the fake took: 0 or 1.
    pub(crate) stops: u8,
    /// The fog uncovers the mod sent: player, centre and radius.
    pub(crate) uncovers: Vec<(enshwick_core::PlayerKey, [f32; 2], f32)>,
    /// The waypoints at the watcher's last look.
    pub(crate) waypoints: waypoints::Watched,
    /// Notices the mod showed: account key and text.
    pub(crate) notices: Vec<(enshwick_core::PlayerKey, String)>,
    /// The clock `time_of_day` writes, settable while installed.
    pub(crate) daytime: Option<enshwick_core::abi::DaytimeInfo>,
    /// Each mod's factor on the clock's speed, once it asked to change it.
    pub(crate) clock_speeds: Vec<(ModHandle, f32)>,
    /// The game settings' bytes and preset, settable while installed.
    pub(crate) game_settings: Option<(Vec<u8>, u32)>,
    /// The game settings' bytes the mod kept with the world, by offset.
    pub(crate) kept_settings: Vec<Option<u8>>,
    /// The buff types the fake knows.
    pub(crate) buff_types: Vec<enshwick_core::abi::BuffTypeInfo>,
    /// The buffs of each entity that holds buffs.
    pub(crate) buffs: rustc_hash::FxHashMap<u32, Vec<enshwick_core::abi::ActiveBuff>>,
    /// The weather weights everywhere and the fronts, settable.
    pub(crate) weather: Option<([f32; 4], Vec<enshwick_core::abi::WeatherFront>)>,
    /// The weather the mod started: effect and duration.
    pub(crate) forecasts: Vec<(u32, i64)>,
    /// The terrain materials, once a world runs.
    pub(crate) terrain: Option<Vec<enshwick_core::abi::TerrainMaterialInfo>>,
    /// The terrain materials the mod added, in order.
    pub(crate) terrain_added: Vec<enshwick_core::abi::TerrainMaterialAdd>,
    /// The buff types mods added, in order, each with the handle that
    /// added it.
    #[cfg(feature = "unstable")]
    pub(crate) buff_types_added: Vec<(ModHandle, enshwick_core::__fake::rules::buff_types::Edit)>,
    /// The terrain edits and transactions the mod asked for.
    pub(crate) edits: WorldEdits,
    /// The world key last handed out, alive until the next one.
    world_key: Option<Box<str>>,
    /// The altars `altars` lists, settable while installed.
    pub(crate) altars: Vec<enshwick_core::abi::AltarInfo>,
    /// The mods that opened altars.
    pub(crate) altar_openers: Vec<ModHandle>,
    /// The mods that size the altar zones.
    pub(crate) zone_controllers: Vec<ModHandle>,
    /// The mod's altar zone filters, and how often it asked for a new sizing.
    pub(crate) zone_sizing: altars::ZoneSizing,
    pub(crate) zone_filters: Vec<Registered<enshwick_core::abi::BuildingZoneFilterFn>>,
    /// The world's save on disk and the mod's hold on saves.
    pub(crate) save: saves::Saves,
    pub(crate) altar_limits: Vec<Registered<enshwick_core::abi::AltarLimitFilterFn>>,
    /// Whether the mod raised the base limit, and whether the hooks are
    /// armed, from which on the raised limit holds.
    pub(crate) base_limit: altars::BaseLimitState,
    pub(crate) grouping: Option<(u32, u32)>,
    /// The mods' changes to players' permissions.
    pub(crate) permission_changes: permissions::Changes,
    /// The waits on transactions' ends, in the order asked.
    pub(crate) settle_waits: Vec<transactions::SettleWait>,
}

/// The notifies of each mod in a step, at the loader's limit per step.
pub(crate) struct Notified(pub(crate) enshwick_core::peer::PerStep);

impl Default for Notified {
    fn default() -> Self {
        Self(enshwick_core::peer::PerStep::new(
            enshwick_core::peer::MAX_NOTIFIES_PER_STEP,
        ))
    }
}

/// The changes of the world the mod asked for, and its voxel reads, in
/// order.
#[derive(Default)]
pub(crate) struct WorldEdits {
    /// The voxel boxes the mod read on the game thread, `(min, size)`.
    pub(crate) voxel_reads: Vec<([u32; 3], [u32; 3])>,
    /// Direct terrain edits.
    pub(crate) terrain: Vec<TerrainEdit>,
    /// Committed transactions, oldest first.
    pub(crate) transactions: Vec<crate::test::FakeTransaction>,
    /// The newest id the fake forgot, so a new transaction never takes it.
    pub(crate) forgotten: u64,
    /// Whether the world has stopped, as between two worlds.
    pub(crate) stopped: bool,
    /// Whether the mod's ready runs: the loader calls it at start, before
    /// any world loads.
    pub(crate) readying: bool,
    /// Whether the scene's map markers are held as still being read.
    pub(crate) markers_pending: bool,
    /// The reads of the scene the mod asked for in this world; the first
    /// ask of each starts it.
    pub(crate) scene_asked: Vec<SceneRead>,
}

/// A read of the scene's data the loader runs once per world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SceneRead {
    /// The map markers.
    Markers,
    /// The map's frame.
    Frame,
}

impl WorldEdits {
    /// Notes that the mod asked for `read`; whether this was its first ask.
    pub(crate) fn first_ask(&mut self, read: SceneRead) -> bool {
        let first = !self.scene_asked.contains(&read);
        if first {
            self.scene_asked.push(read);
        }
        first
    }
}

/// Whether the entity calls cannot run here, where the loader answers
/// [`Status::UNAVAILABLE`]: no server runs, or the build lacks the commands.
pub(crate) fn entity_calls_off(s: &crate::test::table::State) -> bool {
    s.host.world.no_entity_commands || !(s.host.base.serving()) || no_world(s)
}

/// Whether no world runs, where the loader answers the world's calls
/// [`Status::UNAVAILABLE`]: while the mod's init or ready runs, before the
/// loader has a server world, or after [`Installed::stop_world`](crate::test::Installed::stop_world).
pub(crate) fn no_world(s: &crate::test::table::State) -> bool {
    s.base.registration == Registration::Init || s.world.edits.readying || s.world.edits.stopped
}

entries! {
    pub(crate) fn player(
        me as Caller,
        entity: enshwick_core::Entity,
        out: *mut PlayerInfo as args::HeadedOut,
    ) -> Status {
        if !crate::test::table::on_game_thread() {
            return Status::WRONG_THREAD;
        }
        let player = with(Err(Status::MISSING), |s| {
            if no_world(s) {
                return Err(Status::UNAVAILABLE);
            }
            Ok(s.host
                .base
                .players
                .iter()
                .find(|p| p.entity == entity)
                .copied())
        });
        match player {
            Ok(player) => super::found_headed(player, out),
            Err(status) => status,
        }
    }

    pub(crate) fn players(
        me as Caller,
        [out: *mut PlayerInfo, entry_size: u32, capacity: u32, count: *mut u32] as args::SizedListOut,
    ) -> Status {
        if !crate::test::table::on_game_thread() {
            return Status::WRONG_THREAD;
        }
        let players = with(Err(Status::MISSING), |s| {
            if no_world(s) {
                return Err(Status::UNAVAILABLE);
            }
            Ok(s.host.base.players.clone())
        });
        match players {
            Ok(players) => super::base::io::list(&players, out, entry_size, capacity, count),
            Err(status) => status,
        }
    }

    pub(crate) fn chat(
        me as Caller,
        to: enshwick_core::PlayerKey,
        value: Str as args::Text,
    ) -> Status {
        if to.get() == 0 {
            return Status::BAD_ARGS;
        }
        let value = text(value);
        with(Status::MISSING, |s| {
            s.world.chat.push((to, value));
            Status::OK
        })
    }

    pub(crate) fn chat_all(me as Caller, value: Str as args::Text) -> Status {
        let value = text(value);
        with(Status::MISSING, |s| {
            s.world.chat.push((enshwick_core::PlayerKey::new(0), value));
            Status::OK
        })
    }

    pub(crate) fn set_label(
        me as Caller,
        entity: enshwick_core::Entity,
        value: Str as args::Text,
    ) -> Status {
        let value = text(value);
        with(Status::MISSING, |s| {
            s.world.labels.push((entity.get(), value));
            Status::OK
        })
    }

    pub(crate) fn rename_filter(
        me as Mod,
        callback: Option<RenameFilterFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        with_open(|s| {
            s.world
                .rename_filters
                .push(Registered::new(callback, user, me));
            Status::OK
        })
    }

    pub(crate) fn join_rule(
        me as Mod,
        callback: Option<JoinRuleFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        with_open(|s| {
            s.world.join_rules.push(Registered::new(callback, user, me));
            Status::OK
        })
    }

    pub(crate) fn entities(
        me as Caller,
        type_hash: u32,
        [out: *mut EntityPlace, capacity: u32, count: *mut u32] as args::ListOut,
    ) -> Status {
        if !crate::test::table::on_game_thread() {
            return Status::WRONG_THREAD;
        }
        if with(true, |s| no_world(s)) {
            return Status::UNAVAILABLE;
        }
        let found: Vec<EntityPlace> = with(Vec::new(), |s| {
            s.host
                .world
                .placed
                .iter()
                .filter(|(ty, _)| *ty == type_hash)
                .map(|(_, place)| *place)
                .collect()
        });
        for (i, place) in found.iter().take(capacity as usize).enumerate() {
            // SAFETY: the safe layer passes room for `capacity` entries.
            unsafe { out.wrapping_add(i).write(*place) };
        }
        put(count, u32::try_from(found.len()).unwrap_or(u32::MAX))
    }

    pub(crate) fn world_quiet(me as Caller) -> Status {
        if !crate::test::table::on_game_thread() {
            return Status::WRONG_THREAD;
        }
        with(Status::MISSING, |s| {
            if no_world(s) {
                Status::UNAVAILABLE
            } else {
                Status::OK
            }
        })
    }

    pub(crate) fn destroy(me as Caller, entity: enshwick_core::Entity) -> Status {
        with(Status::MISSING, |s| {
            if entity_calls_off(s) {
                return Status::UNAVAILABLE;
            }
            if s.host.base.players.iter().any(|p| p.entity == entity) {
                return Status::REFUSED;
            }
            if !s.host.world.placed.iter().any(|(_, p)| p.entity == entity)
                && !s.host.content.templates.contains_key(&entity.get())
            {
                return Status::MISSING;
            }
            s.world.destroyed.push(entity.get());
            Status::OK
        })
    }

    pub(crate) fn save_world(me as Caller) -> Status {
        with(Status::MISSING, |s| {
            if s.world.save.held || s.world.save.refused {
                return Status::REFUSED;
            }
            s.world.saves += 1;
            Status::OK
        })
    }

    pub(crate) fn world_key(me as Caller, out: *mut Str as args::Out) -> Status {
        with(Status::MISSING, |s| {
            let Some(key) = s.host.base.world_key.clone() else {
                return Status::UNAVAILABLE;
            };
            let kept = s.world.world_key.insert(key.into_boxed_str());
            put(out, Str::new(kept))
        })
    }

    pub(crate) fn component_write(
        me as Mod,
        entity: enshwick_core::Entity,
        type_hash: u32,
        out: *mut *mut c_void as args::Out,
    ) -> Status {
        let address = with(Err(Status::MISSING), |s| {
            if entity_calls_off(s) {
                return Err(Status::UNAVAILABLE);
            }
            if s.host.world.refused_writes.contains(&type_hash) {
                return Err(Status::REFUSED);
            }
            if s.host.world.static_halves.contains(&type_hash) {
                return Err(Status::STATIC_HALF);
            }
            let address = s
                .host
                .base
                .components
                .get(&(entity.get(), type_hash))
                .map(|c| c.0)
                .ok_or(Status::MISSING)?;
            s.world.writes.push((entity.get(), type_hash));
            Ok(address)
        });
        match address {
            Ok(address) => put(out, core::ptr::with_exposed_provenance_mut(address)),
            Err(status) => status,
        }
    }

    pub(crate) fn item_info(
        me as Caller,
        item: enshwick_core::ItemId,
        out: *mut enshwick_core::abi::ItemTypeInfo as args::HeadedOut,
    ) -> Status {
        crate::test::table::found_headed(
            with(None, |s| s.host.content.items.get(&item.get()).copied()),
            out,
        )
    }

    pub(crate) fn notify(
        me as Caller,
        key: enshwick_core::PlayerKey,
        value: Str as args::Text,
    ) -> Status {
        let value = text(value);
        with(Status::MISSING, |s| {
            if !s.host.base.net_peers.contains(&key.get()) {
                return Status::UNAVAILABLE;
            }
            s.world.notices.push((key, value));
            Status::OK
        })
    }

    pub(crate) fn language_of(
        me as Mod,
        key: enshwick_core::PlayerKey,
        out: *mut u32 as args::Out,
    ) -> Status {
        let language = with(0, |s| s.host.base.language_of(key));
        put(out, language)
    }
}

/// The first entity id the fake hands out for a spawn.
pub(crate) const FIRST_SPAWNED: u32 = 0x10_0000;

entries! {
    pub(crate) fn progress_write(
        me as Mod,
        entity: enshwick_core::Entity,
        what: u32,
        value: u32,
    ) -> Status {
        use enshwick_core::abi::{
            PROGRESS_ADD_EXPERIENCE, PROGRESS_OPEN, PROGRESS_SET_EXPERIENCE, PROGRESS_SET_LEVEL,
        };
        with(Status::MISSING, |s| {
            if what == PROGRESS_OPEN {
                if entity.get() != 0 {
                    return Status::BAD_ARGS;
                }
                if s.base.closed() {
                    return Status::TOO_LATE;
                }
                s.world.progress_writes.get_or_insert_with(Vec::new);
                if !s.world.progress_writers.contains(&me) {
                    s.world.progress_writers.push(me);
                }
                return Status::OK;
            }
            if ![
                PROGRESS_SET_LEVEL,
                PROGRESS_SET_EXPERIENCE,
                PROGRESS_ADD_EXPERIENCE,
            ]
            .contains(&what)
            {
                return Status::BAD_ARGS;
            }
            // The loader's level step links after `init`.
            if !s.base.closed() {
                return Status::UNAVAILABLE;
            }
            if !s.world.progress_writers.contains(&me) {
                return Status::REFUSED;
            }
            if !s.host.base.serving() || no_world(s) {
                return Status::UNAVAILABLE;
            }
            let in_game = s.host.base.players.iter().any(|p| p.entity == entity);
            let Some(writes) = s.world.progress_writes.as_mut() else {
                return Status::REFUSED;
            };
            if entity.get() == 0 || !in_game {
                return Status::REFUSED;
            }
            if !s.world.progress.push(me, (entity, what, value), s.base.ticks.step) {
                return Status::BUSY;
            }
            writes.push((entity, what, value));
            Status::OK
        })
    }

    pub(crate) fn server_stop(me as Mod) -> Status {
        with(Status::UNAVAILABLE, |s| {
            if s.host.base.side != Side::SERVER || s.host.base.hosting {
                return Status::UNAVAILABLE;
            }
            if s.world.stops > 0 {
                return Status::REFUSED;
            }
            s.world.stops = 1;
            Status::OK
        })
    }

    pub(crate) fn fog_uncover(
        me as Caller,
        player: enshwick_core::PlayerKey,
        x: f32,
        z: f32,
        radius: f32,
    ) -> Status {
        with(Status::UNAVAILABLE, |s| {
            if !s.host.base.serving() {
                return Status::UNAVAILABLE;
            }
            if !(x.is_finite()
                && z.is_finite()
                && radius > 0.0
                && radius <= enshwick_core::abi::MAX_UNCOVER_RADIUS)
            {
                return Status::BAD_ARGS;
            }
            // As the loader: a client that hosts uncovers its own player's fog
            // itself; another player only when its client takes the messages.
            let own = s.host.base.own_key() == Some(player);
            if !own && !s.host.base.net_peers.contains(&player.get()) {
                return Status::UNAVAILABLE;
            }
            s.world.uncovers.push((player, [x, z], radius));
            Status::OK
        })
    }

    pub(crate) fn build_rights(
        me as Caller,
        check: *mut enshwick_core::abi::BuildRights as args::HeadedInOut,
    ) -> Status {
        use enshwick_core::abi::BuildRights;
        let Some(mut request) = super::get_headed::<BuildRights>(check.cast_const()) else {
            return Status::BAD_ARGS;
        };
        if !enshwick_core::__fake::rules::is_edit_box(request.min, request.max) {
            return Status::BAD_ARGS;
        }
        if !crate::test::table::on_game_thread() {
            return Status::WRONG_THREAD;
        }
        let refusal = with(None, |s| {
            let permissions = s
                .host
                .base
                .players
                .iter()
                .find(|p| p.entity == request.character)
                .map_or(0, |p| p.permissions);
            Some(
                s.host
                    .world
                    .zones
                    .refusal(permissions, request.min, request.max),
            )
        });
        let Some(refusal) = refusal else {
            return Status::UNAVAILABLE;
        };
        request.refusal = refusal;
        // SAFETY: the safe layer passes `check` non-null (read above) and
        // writable for the size its header states, untouched by other threads.
        if unsafe { enshwick_core::abi::write_headed(check, request) } {
            Status::OK
        } else {
            Status::BAD_ARGS
        }
    }

    pub(crate) fn map_markers(
        me as Caller,
        [out: *mut enshwick_core::abi::MapMarkerInfo, entry_size: u32, capacity: u32, count: *mut u32] as args::SizedListOut,
    ) -> Status {
        if !crate::test::table::on_game_thread() {
            return Status::WRONG_THREAD;
        }
        // With no world the loader answers before the scene's first read starts.
        let Some(markers) = with(None, |s| {
            if no_world(s) {
                return None;
            }
            let first = s.world.edits.first_ask(SceneRead::Markers);
            Some((!first && !s.world.edits.markers_pending).then(|| s.host.world.map_markers.clone()))
        }) else {
            return Status::UNAVAILABLE;
        };
        let Some(markers) = markers else {
            return Status::NOT_LOADED;
        };
        crate::test::table::base::io::list(&markers, out, entry_size, capacity, count)
    }

    pub(crate) fn map_frame(
        me as Caller,
        out: *mut enshwick_core::abi::MapFrameInfo as args::HeadedOut,
    ) -> Status {
        if !crate::test::table::on_game_thread() {
            return Status::WRONG_THREAD;
        }
        let Some(frame) = with(None, |s| {
            if no_world(s) {
                return None;
            }
            let first = s.world.edits.first_ask(SceneRead::Frame);
            Some((!first && !s.world.edits.markers_pending).then_some(s.host.world.map_frame))
        }) else {
            return Status::UNAVAILABLE;
        };
        let Some(frame) = frame else {
            return Status::NOT_LOADED;
        };
        let Some(frame) = frame else {
            return Status::UNAVAILABLE;
        };
        // SAFETY: the safe layer passes a headed struct for the call.
        if unsafe { enshwick_core::abi::write_headed(out, frame) } {
            Status::OK
        } else {
            Status::BAD_ARGS
        }
    }

    pub(crate) fn world_persists(
        me as Caller,
        min: *const [f64; 3] as args::In,
        max: *const [f64; 3] as args::In,
        kept: *mut u8 as args::Out,
    ) -> Status {
        if min.is_null() || max.is_null() || kept.is_null() {
            return Status::BAD_ARGS;
        }
        // SAFETY: the safe layer passes `min` pointing at three live `f64` for
        // the call, non-null by the check before it.
        let low = unsafe { min.read_unaligned() };
        // SAFETY: the safe layer passes `max` pointing at three live `f64` for
        // the call, non-null by the check before it.
        let high = unsafe { max.read_unaligned() };
        if !enshwick_core::__fake::rules::is_edit_box(low, high) {
            return Status::BAD_ARGS;
        }
        if !crate::test::table::on_game_thread() {
            return Status::WRONG_THREAD;
        }
        let saved = with(false, |s| s.host.world.zones.kept(low, high));
        // SAFETY: the safe layer passes `kept` writable for one byte for the
        // call, non-null by the check before it.
        unsafe { kept.write(u8::from(saved)) };
        Status::OK
    }

    pub(crate) fn account_action(
        me as Caller,
        action: AccountActionType,
        account: enshwick_core::PlayerKey,
    ) -> Status {
        if account == enshwick_core::PlayerKey::new(0) || action.0 > AccountActionType::UNBAN.0 {
            return Status::BAD_ARGS;
        }
        if !crate::test::table::on_game_thread() {
            return Status::WRONG_THREAD;
        }
        with(Status::MISSING, |s| {
            if no_world(s) {
                return Status::UNAVAILABLE;
            }
            let online = s.host.base.players.iter().position(|p| p.key == account);
            let admin = s
                .host
                .base
                .players
                .iter()
                .any(|p| p.key == account && p.is_admin());
            match action {
                AccountActionType::UNBAN => s.world.banned.retain(|&banned| banned != account),
                _ if admin || s.host.base.host_key == Some(account) => return Status::REFUSED,
                _ if online.is_none() => return Status::REFUSED,
                _ => {
                    if action == AccountActionType::BAN && !s.world.banned.contains(&account) {
                        s.world.banned.push(account);
                    }
                    if let Some(at) = online {
                        s.host.base.players.remove(at);
                        s.world.removed.push(account.get());
                        s.world.permission_changes.left(account);
                    }
                }
            }
            Status::OK
        })
    }

    pub(crate) fn set_chat_mute(
        me as Mod,
        account: enshwick_core::PlayerKey,
        muted: u8 as args::YesNo,
    ) -> Status {
        if account == enshwick_core::PlayerKey::new(0) || muted > 1 {
            return Status::BAD_ARGS;
        }
        with(Status::MISSING, |s| {
            if !s.host.base.serving() {
                return Status::UNAVAILABLE;
            }
            // In the loader's order: the protected player, then the world.
            let admin = s
                .host
                .base
                .players
                .iter()
                .any(|p| p.key == account && p.is_admin());
            if muted == 1 && (admin || s.host.base.host_key == Some(account)) {
                return Status::REFUSED;
            }
            if s.host.base.world_key.is_none() {
                return Status::UNAVAILABLE;
            }
            let at = s.world.muted.iter().position(|&k| k == account);
            match (muted == 1, at) {
                (true, None) => s.world.muted.push(account),
                (false, Some(at)) => {
                    s.world.muted.remove(at);
                }
                _ => {}
            }
            Status::OK
        })
    }

    pub(crate) fn chat_muted(
        me as Caller,
        account: enshwick_core::PlayerKey,
        muted: *mut u8 as args::Out,
    ) -> Status {
        if account == enshwick_core::PlayerKey::new(0) || muted.is_null() {
            return Status::BAD_ARGS;
        }
        let has = with(None, |s| {
            s.host
                .base
                .serving()
                .then(|| s.world.muted.contains(&account))
        });
        match has {
            Some(has) => put(muted, u8::from(has)),
            None => Status::UNAVAILABLE,
        }
    }

    /// Copies the fake item's template and optional local corners without edits.
    pub(crate) fn prop_placement_info(
        me as Caller,
        item: enshwick_core::ItemId,
        out: *mut enshwick_core::abi::PropPlacementInfo as args::HeadedOut,
    ) -> Status {
        use enshwick_core::abi::PropPlacementInfo;
        with(Status::UNAVAILABLE, |s| {
            if no_world(s) || !(s.host.base.serving()) {
                return Status::UNAVAILABLE;
            }
            let Some(info) = s.host.content.items.get(&item.get()) else {
                return Status::BAD_ARGS;
            };
            if s.host.world.no_prop_items.contains(&item.get()) {
                return Status::NOT_A_PROP;
            }
            if info.placed_entity == enshwick_core::content::Guid::NIL {
                return Status::BAD_ARGS;
            }
            let bounds = s
                .host
                .world
                .placement_boxes
                .iter()
                .find(|(id, ..)| *id == item.get())
                .filter(|&&(_, min, max)| PropPlacementInfo::declares_box(min, max));
            let (min, max, flags) = bounds.map_or(([0.0; 3], [0.0; 3], 0), |&(_, min, max)| {
                (min, max, PropPlacementInfo::HAS_LOCAL_BOX)
            });
            super::put_headed(
                out,
                enshwick_core::headed!(PropPlacementInfo {
                    template: info.placed_entity,
                    min,
                    max,
                    flags,
                }),
            )
        })
    }
}
