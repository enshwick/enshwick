//! The table of functions the loader hands every mod. One `repr(C)` struct
//! whose fields keep their ABI order, so it stays in one file.

use core::ffi::c_void;

use crate::abi::{
    AccountActionType, ActionFn, ActiveBuff, AffixDefData, AffixRoll, AltarInfo,
    AltarLimitFilterFn, AltarZoneFn, BaseLimitInfo, BedCheckFn, BuffRequest, BuffTypeInfo,
    BuildCheckFn, BuildInfo, BuildRights, CallFn, CallSpec, CallerFrame, ChangeId, ChangeOp,
    ComfortFn, ConfigCheckFn, CraftCheckFn, DamageFilterFn, DataChange, DaytimeInfo,
    EntityDataFlags, EntityPlace, Event, EventFn, EventSpec, EventTypeInfo, ExperiencePassFn,
    FogInfo, GameEventFn, Header, HookSpec, InteractionFn, InventoryRequest, InventoryResultFn,
    ItemData, ItemLine, KnowledgeInfo, KnowledgeInputFn, Level, LoopWaitFn, LoopWaitSites,
    MapMarkerInfo, MidHookSpec, MigrationSet, ModEventFn, ModHandle, ModInfo, NetMessageFn,
    PlayerInfo, PointFn, PointHandle, PointLink, PointSpec, PointVerdict, RateId, RecipeKind,
    RecipeRuleKind, RenameCallFn, ResourceKey, ResourcePatchFn, ServerWaitFn, ServiceFound,
    ServiceSpec, SkillCheckFn, SpawnRequest, StatChangeFn, StatDefData, StatModifierData,
    StatModify, StatRef, StatSource, Status, StatusFn, Str, SummonFn, SurvivorLookupFn,
    SurvivorStatusFn, TeleportRequest, TerrainEdit, TerrainMaterialInfo, TickHandle,
    TransactionInfo, TransactionOp, UiAnswerFn, UiDialog, UiFrameFn, UiMarker, UiWidget, Voxel,
    WeatherFront, ZoneCheckFn,
};
use crate::abi::{Entity, ItemId, KnowledgeId, PlayerKey, RecipeId};

/// The table of services the loader offers a mod.
///
/// Fields are only ever appended; a mod checks [`Header::covers`] before it
/// calls a field added after the minor it was built against.
///
/// # Examples
///
/// The safe layer wraps this table; see [`crate::Load`].
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct HostApi {
    /// Size and version of this struct.
    pub header: Header,
    /// The loader's own version.
    pub host_version: Str,
    /// Writes the running game's identity, as far as the [`Header`] `out`
    /// starts with says the caller's struct reaches.
    pub build: extern "system" fn(out: *mut BuildInfo),
    /// Resolves a symbolic id for the running build: the mod's own rows first,
    /// then the loader's table.
    pub symbol: extern "system" fn(me: ModHandle, name: Str, out: *mut u64) -> Status,
    /// Resolves an ECS system's update function by its reflected name.
    pub system: extern "system" fn(name: Str, out: *mut u64) -> Status,
    /// Resolves a field's offset in a reflected type.
    pub field_offset: extern "system" fn(type_name: Str, field: Str, out: *mut u64) -> Status,
    /// Resolves a reflected type's size.
    pub type_size: extern "system" fn(type_name: Str, out: *mut u32) -> Status,
    /// Links a detour; allowed during `enshwick_mod_init` only.
    pub hook: extern "system" fn(me: ModHandle, spec: *const HookSpec) -> Status,
    /// Writes the game caller's context at entry of the current chain, for
    /// links with [`HookFlags::CALLER_FRAME`](super::HookFlags::CALLER_FRAME); valid until the link calls
    /// anything hooked.
    pub caller_frame: extern "system" fn(out: *mut CallerFrame),
    /// Reports that the mod failed at run time; the loader unlinks every link
    /// of the mod and turns it off.
    pub fault: extern "system" fn(me: ModHandle, reason: Str),
    /// Writes a line to the log under the mod's id.
    pub log: extern "system" fn(me: ModHandle, level: Level, message: Str),
    /// The text of the mod's settings file, written from `defaults` first when
    /// it is missing; the text stays valid for the life of the process, so
    /// a thread may read it while another asks again.
    pub settings: extern "system" fn(me: ModHandle, defaults: Str, out: *mut Str) -> Status,
    /// Publishes the service `spec` describes, table and layout, for other
    /// mods; [`Status::REFUSED`] when another mod publishes its name and
    /// major. Allowed during `enshwick_mod_init` only.
    pub publish: extern "system" fn(me: ModHandle, spec: *const ServiceSpec) -> Status,
    /// Finds the service `wanted` describes, its table null, and checks
    /// that the published layout starts with the wanted one; writes the
    /// table and its publisher to `out`, and on [`Status::MISSING`] or
    /// [`Status::REFUSED`] the reason.
    pub service: extern "system" fn(
        me: ModHandle,
        wanted: *const ServiceSpec,
        out: *mut ServiceFound,
    ) -> Status,
    /// Resolves an engine constant of the running build, such as a field
    /// offset reflection does not describe, by its symbolic id.
    pub constant: extern "system" fn(me: ModHandle, name: Str, out: *mut u64) -> Status,
    /// The mod's own data folder, `mod-data/<id>`, created on first use; the
    /// path stays valid for the process.
    pub data_dir: extern "system" fn(me: ModHandle, out: *mut Str) -> Status,
    /// The most detailed level kept for the mod's lines; the mod skips
    /// formatting anything more detailed.
    pub log_level: extern "system" fn(me: ModHandle) -> Level,
    /// Calls `callback` with `user` on every `event` for the mod, on the
    /// thread that raises it, until the mod is off; allowed during
    /// `enshwick_mod_init` only.
    pub subscribe: extern "system" fn(
        me: ModHandle,
        event: Event,
        callback: EventFn,
        user: *mut c_void,
    ) -> Status,
    /// The ECS type hash of a reflected component type, by name.
    pub type_hash: extern "system" fn(type_name: Str, out: *mut u32) -> Status,
    /// A read-only pointer to `entity`'s component of type `type_hash`, valid
    /// until the game thread moves on; on the server's game thread only.
    /// For the static half of an ECS type, such as `FuelSetup`, it is the
    /// set-up data the entity's template holds.
    pub component: extern "system" fn(
        me: ModHandle,
        entity: Entity,
        type_hash: u32,
        out: *mut *const c_void,
    ) -> Status,
    /// The template GUID of `entity`, in file order; on the server's game
    /// thread only.
    pub entity_template:
        extern "system" fn(me: ModHandle, entity: Entity, out: *mut [u8; 16]) -> Status,
    /// The connected player whose character is `entity`; on the server's game
    /// thread only.
    pub player: extern "system" fn(me: ModHandle, entity: Entity, out: *mut PlayerInfo) -> Status,
    /// Up to `capacity` connected players with characters in the world, and
    /// how many there are; on the server's game thread only.
    /// Every entry is as large as the header of the first one says.
    pub players: extern "system" fn(
        me: ModHandle,
        out: *mut PlayerInfo,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Serves the resource `key` as `patch` makes it from the resource
    /// `base`, built the first time the game reads it: a new resource when
    /// the keys differ, a changed vanilla one when they are equal. Allowed
    /// during `enshwick_mod_init` only; one mod per key.
    pub serve_resource: extern "system" fn(
        me: ModHandle,
        key: *const ResourceKey,
        base: *const ResourceKey,
        patch: ResourcePatchFn,
        user: *mut c_void,
    ) -> Status,
    /// Adds the text of `LocaTag` `id` in `language` (`LanguageId`, 0 for any
    /// language without its own); the client shows it wherever the game asks
    /// for that tag. Allowed during `enshwick_mod_init` only.
    pub add_text: extern "system" fn(me: ModHandle, id: u32, language: u32, text: Str) -> Status,
    /// Up to `capacity` loaded mods, the loader first, and how many there
    /// are. Entries are `entry_size` bytes apart, the caller's
    /// `size_of::<ModInfo>()`; the loader fills what both sides know of each
    /// and its own [`Header`].
    pub mods: extern "system" fn(
        me: ModHandle,
        out: *mut ModInfo,
        entry_size: u32,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Reports the mod's join values as its settings file holds them,
    /// `synced` (an inline TOML table), and writes the values to run with
    /// over them: the server's while this client is joined to a server that
    /// advertises values for the mod, else an empty string. A server
    /// advertises the `synced` of each join mod. The text written stays valid
    /// for the life of the process.
    pub sync: extern "system" fn(me: ModHandle, synced: Str, out: *mut Str) -> Status,
    /// Writes where the game's code section (`.text`) is loaded and how many
    /// bytes it spans.
    pub code_range: extern "system" fn(start: *mut u64, len: *mut u64) -> Status,
    /// Replaces `len` bytes of game code at `address`, which must read
    /// `expected` at arming, with `replacement`; both point at `len` bytes
    /// valid for the call. The loader writes the patch with the hooks, refuses
    /// one that overlaps another patch, and skips one that overlaps hooked
    /// bytes or no longer reads `expected`, turning the mod off. Allowed
    /// during `enshwick_mod_init` only.
    pub patch_code: extern "system" fn(
        me: ModHandle,
        address: u64,
        expected: *const u8,
        replacement: *const u8,
        len: usize,
    ) -> Status,
    /// The most recipes one workshop takes on the running game: 512, the
    /// game's own list size, or [`u32::MAX`] once the loader has lifted that
    /// limit at arming.
    pub workshop_recipes: extern "system" fn() -> u32,
    /// 1 while this process runs a game server: always on the dedicated
    /// server, on the client while it hosts a game, solo play included.
    pub hosting: extern "system" fn() -> u8,
    /// The entity world of the game server this process runs, the value a
    /// system context of that world holds at `ecs.context_entity_world`; 0
    /// while none runs or before its first step.
    pub server_world: extern "system" fn() -> u64,
    /// Sends `text` to the chat of the player whose account key is `to`, or of
    /// everyone for 0, from any thread; the server posts it after the next
    /// step. Lines past 1024 bytes are cut.
    pub chat: extern "system" fn(me: ModHandle, to: PlayerKey, text: Str) -> Status,
    /// Gives `entity` the name `text`, as a player's rename does: shown on
    /// hover, sent to every client and saved with the world; cut at 255
    /// bytes, removed when empty. On the server's game thread only.
    pub set_label: extern "system" fn(me: ModHandle, entity: Entity, text: Str) -> Status,
    /// Calls `callback` with `user` before the server applies a player's
    /// rename of an entity; any filter returning 0 refuses it. Allowed during
    /// `enshwick_mod_init` only.
    pub rename_filter:
        extern "system" fn(me: ModHandle, callback: RenameCallFn, user: *mut c_void) -> Status,
    /// Up to `capacity` of the server's join mods that the last refused join
    /// found missing on this client or in another version, and how many
    /// there are; laid out as [`HostApi::mods`] lays out its entries. Each
    /// entry has the server's version, [`ManifestFlags::JOIN`](super::ManifestFlags::JOIN), `off` 1, the
    /// client's DLL path when it has the mod (empty otherwise), and the name
    /// and homepage the server advertises. The strings stay valid for the
    /// process; once 64 KiB of advertised text is kept, an entry with new
    /// text is left out. Empty before a refused join; see
    /// [`Event::JOIN_REFUSED`].
    pub join_missing: extern "system" fn(
        me: ModHandle,
        out: *mut ModInfo,
        entry_size: u32,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Runs a handler before the instruction at an address inside a game
    /// function, with the registers there; allowed during
    /// `enshwick_mod_init` only.
    pub hook_at: extern "system" fn(me: ModHandle, spec: *const MidHookSpec) -> Status,
    /// Asks for inventory transactions and item data; allowed during
    /// `enshwick_mod_init` only. [`Status::MISSING`] when this game build
    /// lacks a piece of them. Requests run in the game server: the dedicated
    /// server, or the client while it hosts.
    pub inventory_open: extern "system" fn(me: ModHandle) -> Status,
    /// Writes the static data of the item `item`, as far as the [`Header`]
    /// `out` starts with says the caller's struct reaches;
    /// [`Status::MISSING`] for an unknown item, or while no world runs.
    /// Any thread.
    pub item_info: extern "system" fn(me: ModHandle, item: ItemId, out: *mut ItemData) -> Status,
    /// Queues `request`, from any thread, for the game's inventory system,
    /// which runs it as one transaction: all or nothing. `done` gets the
    /// result with `user` on the game thread after a later step, unless the
    /// mod is off by then. On an error status nothing is queued and `done` is
    /// never called.
    pub inventory_submit: extern "system" fn(
        me: ModHandle,
        request: *const InventoryRequest,
        done: InventoryResultFn,
        user: *mut c_void,
    ) -> Status,
    /// Adds `callback` with `user` to the mod's status sources: the loader
    /// calls it, on a thread of its own or the game thread, whenever it
    /// writes its status summary (`logs/enshwick/status.log`, `/enshwick
    /// status`) and shows its lines under the mod. Allowed during
    /// `enshwick_mod_init` only.
    pub status: extern "system" fn(me: ModHandle, callback: StatusFn, user: *mut c_void) -> Status,
    /// Runs the console line `line` as the server's console, an
    /// administrator with no character, and writes its reply to `out`, valid
    /// until the next call on this thread: `enshwick ...` for the loader's
    /// own tools, or a mod's id and the text form of one of its
    /// [actions](HostApi::actions). On the server's game thread only;
    /// [`Status::MISSING`] when nothing answers the line's first word.
    pub run_console: extern "system" fn(me: ModHandle, line: Str, out: *mut Str) -> Status,
    /// Asks the loader to follow the Flame Altars of the server's world, for
    /// [`HostApi::altars`] and [`Event::ALTARS_CHANGED`]. Allowed during
    /// `enshwick_mod_init` only.
    pub altars_open: extern "system" fn(me: ModHandle) -> Status,
    /// Up to `capacity` altars of the server's world and how many there are.
    /// Entries are `entry_size` bytes apart, the caller's
    /// `size_of::<AltarInfo>()`; the loader fills what both sides know of
    /// each and its own [`Header`]. Any thread; empty while no world runs.
    /// [`Status::REFUSED`] for a mod that did not open altars.
    pub altars: extern "system" fn(
        me: ModHandle,
        out: *mut AltarInfo,
        entry_size: u32,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Calls `callback` with `user` when the game refuses a placement whose
    /// box does not lie in one building zone; any filter returning 1 accepts
    /// it after all. Runs on both sides, since a client predicts placement.
    /// Allowed during `enshwick_mod_init` only.
    pub zone_check_filter:
        extern "system" fn(me: ModHandle, callback: ZoneCheckFn, user: *mut c_void) -> Status,
    /// Makes `callback` with `user` the answer to the game's survivor
    /// lookups that [`SurvivorLookup`](super::SurvivorLookup) names; one mod at a time
    /// ([`Status::REFUSED`] for another). Runs on the server's worker
    /// threads, in the dedicated server or a hosting client, and for
    /// [`SurvivorLookup::CURSOR`](super::SurvivorLookup::CURSOR) on a client alone too. Allowed during
    /// `enshwick_mod_init` only; [`Status::MISSING`] on a build that lacks a
    /// piece.
    pub survivor_lookup:
        extern "system" fn(me: ModHandle, callback: SurvivorLookupFn, user: *mut c_void) -> Status,
    /// Calls `callback` with `user` after a survivor's own bed check kept its
    /// bed; any filter returning 0 clears it. Server's worker threads.
    /// Allowed during `enshwick_mod_init` only.
    pub survivor_bed_check:
        extern "system" fn(me: ModHandle, callback: BedCheckFn, user: *mut c_void) -> Status,
    /// Calls `callback` with `user` before a summoning staff spawns or moves
    /// a survivor; any filter returning 1 makes the use take the item.
    /// Server's game thread. Allowed during `enshwick_mod_init` only.
    pub summon_filter:
        extern "system" fn(me: ModHandle, callback: SummonFn, user: *mut c_void) -> Status,
    /// Files item `item` as a summoning staff of the survivor `survivor`, so
    /// the game treats it as that survivor's staff, on both sides. Allowed
    /// during `enshwick_mod_init` only.
    pub summon_item: extern "system" fn(me: ModHandle, item: ItemId, survivor: u32) -> Status,
    /// Makes `callback` with `user` merge the statuses the client's survivor
    /// menu shows for a survivor with several entities. Client only; one mod
    /// at a time. Allowed during `enshwick_mod_init` only.
    pub survivor_status:
        extern "system" fn(me: ModHandle, callback: SurvivorStatusFn, user: *mut c_void) -> Status,
    /// Calls `callback` with `user` before a player's interaction that
    /// [`Interaction`](super::Interaction) names; any filter returning 0 refuses it. Runs in the
    /// server's world only, on the thread of the system that checks it.
    /// Allowed during `enshwick_mod_init` only; [`Status::MISSING`] on a
    /// build that lacks a piece.
    pub interaction_filter:
        extern "system" fn(me: ModHandle, callback: InteractionFn, user: *mut c_void) -> Status,
    /// Calls `callback` with `user` when the game asks whether a character
    /// unlocked skill node `node`; the first filter that answers decides.
    /// Runs on both sides, on the asking thread. Allowed during
    /// `enshwick_mod_init` only.
    pub skill_override: extern "system" fn(
        me: ModHandle,
        node: crate::abi::SkillNode,
        callback: SkillCheckFn,
        user: *mut c_void,
    ) -> Status,
    /// Calls `callback` with `user` when a comfort provider registers its
    /// comfort; filters run in load order, each on the last one's value.
    /// Server's worker threads. Allowed during `enshwick_mod_init` only.
    pub comfort_filter:
        extern "system" fn(me: ModHandle, callback: ComfortFn, user: *mut c_void) -> Status,
    /// Calls `callback` with `user` before the server applies the
    /// characters' gained experience, once per batch; filters run in load
    /// order. Server's worker threads. Allowed during `enshwick_mod_init`
    /// only.
    pub experience_filter:
        extern "system" fn(me: ModHandle, callback: ExperiencePassFn, user: *mut c_void) -> Status,
    /// Lets the game group 32 altar zones into its 16 bases instead of 16
    /// zones, refusing an altar that would start a 17th base; the patches
    /// count as the mod's own. [`Status::MISSING`] when this build's code is
    /// not the one the loader knows: the game keeps 16. Allowed during
    /// `enshwick_mod_init` only.
    pub raise_base_limit: extern "system" fn(me: ModHandle) -> Status,
    /// Writes how many altar zones and bases the game's last grouping found;
    /// [`Status::MISSING`] before the first one or without the raised limit.
    /// Any thread.
    pub base_zones: extern "system" fn(me: ModHandle, zones: *mut u32, bases: *mut u32) -> Status,
    /// Calls `callback` with `user` on every game event of the reflected
    /// type whose hash is `type_hash`, or on every game event for 0, on
    /// the server's game thread before the systems of the next step run.
    /// Allowed during `enshwick_mod_init` only.
    pub game_event: extern "system" fn(
        me: ModHandle,
        type_hash: u32,
        callback: GameEventFn,
        user: *mut c_void,
    ) -> Status,
    /// Up to `capacity` entities of the server's world that have a component
    /// of the reflected type `type_hash` and a place in the world, and how
    /// many there are. On the server's game thread only.
    pub entities: extern "system" fn(
        me: ModHandle,
        type_hash: u32,
        out: *mut EntityPlace,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Asks the server to destroy `entity` after the systems of the next
    /// step; [`Status::REFUSED`] for a player's character,
    /// [`Status::MISSING`] for an entity not in the world. On the server's
    /// game thread only.
    pub destroy: extern "system" fn(me: ModHandle, entity: Entity) -> Status,
    /// Asks the server to save the world now, as its timed saves do;
    /// [`Event::WORLD_SAVED`] follows once the save is on disk. A save under
    /// way counts as asked; [`Status::REFUSED`] while the world may not be
    /// saved, as before it has loaded. On the server's game thread only.
    pub save_world: extern "system" fn(me: ModHandle) -> Status,
    /// Writes the key of the world the server runs: its save file's name and
    /// a hash of its folder, `[a-z0-9_-]`, valid for the process.
    /// [`Status::MISSING`] before the server has read or written the world.
    /// Any thread.
    pub world_key: extern "system" fn(me: ModHandle, out: *mut Str) -> Status,
    /// A writable pointer to `entity`'s component of the reflected type
    /// `type_hash`, valid until the game thread returns to the game; the
    /// loader marks the component changed, as the game's own writes do, so
    /// the game sends it to clients and saves it. [`Status::REFUSED`] for a
    /// copy the game keeps for clients (`Network*`, `Client*`), which it
    /// overwrites; [`Status::STATIC_HALF`] for the static half of an ECS
    /// type, the template's set-up data;
    /// [`Status::MISSING`] when the entity lacks the component. On the
    /// server's game thread only.
    pub component_write: extern "system" fn(
        me: ModHandle,
        entity: Entity,
        type_hash: u32,
        out: *mut *mut c_void,
    ) -> Status,
    /// Raises the game event at `data`, `size` bytes of the reflected type
    /// `type_hash`, its whole layout; the loader stamps it with the game
    /// time and the systems of the next step see it. Only types the loader
    /// allows, which the server's own systems raise: [`Status::REFUSED`]
    /// for any other or when the step's stream is full,
    /// [`Status::BAD_ARGS`] for a size that is not the type's. On the
    /// server's game thread only.
    pub emit:
        extern "system" fn(me: ModHandle, type_hash: u32, data: *const u8, size: u32) -> Status,
    /// Asks the server to move an entity as its admin teleport does, after
    /// the next step; [`Status::MISSING`] for an entity not in the world,
    /// [`Status::REFUSED`] for one the game's teleport does not move, as a
    /// prop: it moves actors and characters.
    /// On the server's game thread only.
    pub teleport: extern "system" fn(me: ModHandle, request: *const TeleportRequest) -> Status,
    /// Asks the server to spawn an entity from a template, as its admin
    /// spawn does, and writes the new entity's id to `out`; the entity is in
    /// the world after the next step. [`Status::BAD_ARGS`] for a zero
    /// template; a template the game does not know spawns nothing. On the
    /// server's game thread only.
    pub spawn:
        extern "system" fn(me: ModHandle, request: *const SpawnRequest, out: *mut Entity) -> Status,
    /// Calls `callback` with `user` on each player's craft request in the
    /// server's world, on the game thread before the crafting system reads
    /// it; any filter returning 0 refuses it, and nothing is made or taken.
    /// Allowed during `enshwick_mod_init` only.
    pub craft_filter:
        extern "system" fn(me: ModHandle, callback: CraftCheckFn, user: *mut c_void) -> Status,
    /// Calls `callback` with `user` after the game's cursor check allows a
    /// placement; any filter returning 0 refuses it as the game refuses a
    /// placement without permission. Runs on both sides, on the cursor
    /// systems' worker threads: the server's check decides, a client's
    /// shows. Allowed during `enshwick_mod_init` only.
    pub build_filter:
        extern "system" fn(me: ModHandle, callback: BuildCheckFn, user: *mut c_void) -> Status,
    /// Opens the mod's channel `name`, 1 to 64 bytes, between its server
    /// and client parts, and writes its id to `out`; `callback` gets each
    /// message on it with `user`: on the server's game thread, on a client
    /// on the thread that takes the server's chat. Join mods only
    /// ([`Status::REFUSED`] for another), since both ends must run the mod.
    /// Allowed during `enshwick_mod_init` only.
    pub net_channel: extern "system" fn(
        me: ModHandle,
        name: Str,
        callback: NetMessageFn,
        user: *mut c_void,
        out: *mut u32,
    ) -> Status,
    /// Sends `size` bytes at `data`, at most 4096, on the mod's `channel`:
    /// to the server for [`NetMessage::SERVER`](super::NetMessage::SERVER), else from the server to the
    /// client of the player with account key `peer`, or to every client
    /// that runs Enshwick for 0. Any thread. [`Status::MISSING`] while the
    /// peer cannot take messages, [`Status::REFUSED`] over the peer's rate,
    /// [`Status::BAD_ARGS`] for too many bytes or another mod's channel.
    pub net_send: extern "system" fn(
        me: ModHandle,
        channel: u32,
        peer: PlayerKey,
        data: *const u8,
        size: u32,
    ) -> Status,
    /// [`Status::OK`] when `peer` takes messages: a player's client that
    /// runs Enshwick, on the server; the server, for [`NetMessage::SERVER`](super::NetMessage::SERVER)
    /// on a client. [`Status::MISSING`] otherwise. Any thread.
    pub net_ready: extern "system" fn(me: ModHandle, peer: PlayerKey) -> Status,
    /// Shows `text`, cut at 255 bytes, on the screen of the player with
    /// account key `key`, as the game shows its own "can't do that"
    /// messages; [`Status::MISSING`] when the player's client lacks
    /// Enshwick and cannot show it, and nothing is sent. On the server's
    /// game thread only.
    pub notify: extern "system" fn(me: ModHandle, key: PlayerKey, text: Str) -> Status,
    /// Writes the clock of the server's world, as far as the [`Header`]
    /// `out` starts with says the caller's struct reaches. On the server's
    /// game thread only.
    pub time_of_day: extern "system" fn(me: ModHandle, out: *mut DaytimeInfo) -> Status,
    /// Sets the clock of the server's world to `time`, game nanoseconds of
    /// the day, taken modulo 24 hours; clients follow after the next step.
    /// On the server's game thread only.
    pub set_time_of_day: extern "system" fn(me: ModHandle, time: i64) -> Status,
    /// Lets the mod change how fast the clock runs, with
    /// [`HostApi::set_time_speed`]. Allowed during `enshwick_mod_init` only.
    pub time_speed_control: extern "system" fn(me: ModHandle) -> Status,
    /// Sets the mod's factor on the clock's speed, from 0 (stopped) to 1000;
    /// the clock runs at the game's own speed times every mod's factor.
    /// [`Status::REFUSED`] for a mod without
    /// [`HostApi::time_speed_control`], [`Status::BAD_ARGS`] for a factor
    /// out of range. On the server's game thread only.
    pub set_time_speed: extern "system" fn(me: ModHandle, factor: f32) -> Status,
    /// Copies the server's live game settings, `keen::ecs::GameSettings`,
    /// into `out`, `size` bytes, which must be the type's reflected size
    /// ([`Status::BAD_ARGS`] otherwise), and writes the preset the server
    /// runs (`GameSettingsPreset`) to `preset`. On the server's game thread
    /// only.
    pub game_settings:
        extern "system" fn(me: ModHandle, out: *mut u8, size: u32, preset: *mut u32) -> Status,
    /// Replaces the server's live game settings with `data`, the type's
    /// whole reflected layout, as the game's admin command does: systems
    /// see them from the next step and every client gets them; the preset
    /// becomes custom. Until the server stops: nothing writes them to disk.
    /// [`Status::BAD_ARGS`] for a wrong size or a day or night length of
    /// zero or less. On the server's game thread only.
    pub set_game_settings: extern "system" fn(me: ModHandle, data: *const u8, size: u32) -> Status,
    /// Calls `callback` with `user` on damage about to reach an entity's
    /// health in the server's world, before the game applies it: hits,
    /// falls and the shroud; what the filters leave in the
    /// [`DamageCheck`](super::DamageCheck) is what the game applies. Runs on the game's worker
    /// threads, in load order. Allowed during `enshwick_mod_init` only.
    pub damage_filter:
        extern "system" fn(me: ModHandle, callback: DamageFilterFn, user: *mut c_void) -> Status,
    /// Up to `capacity` buff and debuff types of the running game, and how
    /// many there are. On the server's game thread only.
    /// Every entry is as large as the header of the first one says.
    pub buff_types: extern "system" fn(
        me: ModHandle,
        out: *mut BuffTypeInfo,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Up to `capacity` buffs `entity` has, and how many there are;
    /// [`Status::MISSING`] for an entity that holds no buffs. On the
    /// server's game thread only.
    /// Every entry is as large as the header of the first one says.
    pub buffs: extern "system" fn(
        me: ModHandle,
        entity: Entity,
        out: *mut ActiveBuff,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Gives an entity a buff, as the game restores a character's saved
    /// buffs: at once, for the duration asked. One the entity has already
    /// is renewed to that duration. [`Status::MISSING`] for an entity that
    /// holds no buffs, [`Status::BAD_ARGS`] for an unknown type,
    /// [`Status::REFUSED`] for a type removed in this step, which the game
    /// clears only in the next. On the server's game thread only.
    pub apply_buff: extern "system" fn(me: ModHandle, request: *const BuffRequest) -> Status,
    /// Removes every buff of type `id` from `entity`, as the game cleanses
    /// one: it goes in the next step. Writes 1 to `removed` when the entity
    /// had it, else 0; [`Status::MISSING`] for an entity that holds no
    /// buffs. On the server's game thread only.
    pub remove_buff:
        extern "system" fn(me: ModHandle, entity: Entity, id: u32, removed: *mut u8) -> Status,
    /// Writes knowledge `id` of the player whose character is `entity`, or
    /// of the world for 0 or for an id of the world's;
    /// [`Status::MISSING`] for an id the game does not know,
    /// [`Status::REFUSED`] for a player not in the game. On the server's
    /// game thread only.
    pub knowledge: extern "system" fn(
        me: ModHandle,
        entity: Entity,
        id: KnowledgeId,
        out: *mut KnowledgeInfo,
    ) -> Status,
    /// Grants knowledge `id`, of `kind`, to the player whose character is
    /// `entity`, or to the world for 0, as the game grants it: the next
    /// step applies it with what follows (experience, unlocks, clients,
    /// the save). `value` is ignored for [`KnowledgeInfo::BOOL`], added for
    /// [`KnowledgeInfo::COUNTER`], the bit to set for
    /// [`KnowledgeInfo::FLAGS`]. [`Status::BAD_ARGS`] for an unknown id,
    /// a kind that is not the id's or a bit the game does not expect,
    /// [`Status::REFUSED`] for a player not in the
    /// game or a full queue. On the server's game thread only.
    pub grant_knowledge: extern "system" fn(
        me: ModHandle,
        entity: Entity,
        id: KnowledgeId,
        kind: u32,
        value: u32,
    ) -> Status,
    /// Up to `capacity` quest ids of the running game, and how many there
    /// are. On the server's game thread only.
    pub quests: extern "system" fn(
        me: ModHandle,
        out: *mut KnowledgeId,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Writes how clear, rainy, snowy and stormy it is at `position`, in
    /// 32.32 fixed-point metres, as weights that add up to 1, as the game's
    /// systems weigh it. On the server's game thread only.
    pub weather_at:
        extern "system" fn(me: ModHandle, position: *const [i64; 3], out: *mut [f32; 4]) -> Status,
    /// Up to `capacity` current weather fronts of the server's world, one per
    /// layer that has one, and how many there are. On the server's game
    /// thread only.
    /// Every entry is as large as the header of the first one says.
    pub weather_fronts: extern "system" fn(
        me: ModHandle,
        out: *mut WeatherFront,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Starts new weather in every layer of the server's world: fronts of
    /// `effect` ([`WeatherFront::NONE`] for clear skies) where a layer has
    /// one, clear skies elsewhere, for `duration` game nanoseconds or the
    /// game's own for 0; `u32::MAX` lets the game choose anew. Clients
    /// follow. On the server's game thread only.
    pub set_weather: extern "system" fn(me: ModHandle, effect: u32, duration: i64) -> Status,
    /// Up to `capacity` terrain materials of the running game, and how many
    /// there are. On the server's game thread only.
    /// Every entry is as large as the header of the first one says.
    pub terrain_materials: extern "system" fn(
        me: ModHandle,
        out: *mut TerrainMaterialInfo,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Runs a terrain edit in the server's world as the game's terraforming
    /// does, and writes the volume it changed in cubic metres (for a
    /// flatten, 1 when anything changed) to `amount`; clients see it and
    /// the save keeps it. Building voxels are left alone.
    /// [`Status::BAD_ARGS`] for an edit out of range;
    /// [`Status::OUTSIDE_WORLD`] for one that reaches past the world's edge
    /// or above the height the loader takes, [`Status::NOT_LOADED`] unless
    /// every column of the box it can write is loaded, writing nothing. On
    /// the server's game thread only.
    pub terrain_edit:
        extern "system" fn(me: ModHandle, edit: *const TerrainEdit, amount: *mut f32) -> Status,
    /// Hands the loader the mod's settings schema, `schema`, the TOML text
    /// of a [`config::Schema`](crate::config::Schema); a later call replaces
    /// it. The loader answers `enshwick config` on the console from it and checks values against it
    /// before it writes them. [`Status::BAD_ARGS`] for text that is not a
    /// schema. Any time.
    pub config_schema: extern "system" fn(me: ModHandle, schema: Str) -> Status,
    /// Calls `callback` with `user` after a summoning staff's use spawned or
    /// moved a survivor, with [`Summon::consumed`](super::Summon::consumed) set; its return value is
    /// ignored. On the thread of the use, as [`HostApi::summon_filter`].
    /// Allowed during `enshwick_mod_init` only.
    pub summoned:
        extern "system" fn(me: ModHandle, callback: SummonFn, user: *mut c_void) -> Status,
    /// Opens the item data kind `id`, `name` for the mod's own
    /// (`<mod>:<name>`) or `mod:name` for any mod's, each side 1 to 64 of
    /// `[a-z0-9_-]`, and writes its index to `out`; every mod gets the same
    /// index for one id. Allowed during `enshwick_mod_init` only.
    pub item_data_open: extern "system" fn(me: ModHandle, id: Str, out: *mut u32) -> Status,
    /// Copies the value of kind `kind` on the item entity `pide` holding
    /// item `item` into `out`, at most `capacity` bytes, and writes its
    /// length to `len`; [`Status::MISSING`] when it has none or its entity
    /// holds another item, [`Status::BAD_ARGS`] when `capacity` is short,
    /// with `len` written. On the server the server's values, on a client
    /// what the server sent. Any thread.
    pub item_data_get: extern "system" fn(
        me: ModHandle,
        kind: u32,
        pide: Entity,
        item: ItemId,
        out: *mut u8,
        capacity: u32,
        len: *mut u32,
    ) -> Status,
    /// Sets the value of kind `kind` on the item entity `pide` holding item
    /// `item` to `len` bytes at `data`, at most 512, or removes it for 0;
    /// the clients that run Enshwick get the change. [`Status::REFUSED`]
    /// on a client alone, [`Status::MISSING`] before the world has loaded.
    /// Any thread.
    pub item_data_set: extern "system" fn(
        me: ModHandle,
        kind: u32,
        pide: Entity,
        item: ItemId,
        data: *const u8,
        len: u32,
    ) -> Status,
    /// Calls `callback` with `user` once per frame of the client's
    /// interface, on the thread that draws it, while the mod is on; the
    /// callback draws with [`HostApi::ui_widget`]. Client only
    /// ([`Status::MISSING`] on a server); when the interface could not be
    /// linked, the callback never runs. Allowed during `enshwick_mod_init`
    /// only.
    pub ui_frame:
        extern "system" fn(me: ModHandle, callback: UiFrameFn, user: *mut c_void) -> Status,
    /// Draws `widget` in the frame `frame` names and writes back its
    /// `index`, `value` and `result`. Only inside the mod's frame callback
    /// ([`Status::WRONG_THREAD`] elsewhere); [`Status::REFUSED`] for a
    /// widget outside a window or an [`UiWidget::END`] without one;
    /// [`Status::MISSING`] for a widget this game build lacks.
    pub ui_widget: extern "system" fn(me: ModHandle, frame: u64, widget: *mut UiWidget) -> Status,
    /// Queues `dialog` and calls `callback` with the answer and `user` once
    /// the player gives it; dialogs show one at a time, in order. Any
    /// thread. Client only ([`Status::MISSING`] on a server); when the
    /// interface could not be linked, the dialog shows as a system message
    /// box. [`Status::REFUSED`] when too many wait; [`Status::WRONG_THREAD`]
    /// for [`UiDialog::WAITING`] on the thread that draws the interface. On
    /// an error status `callback` is never called.
    pub ui_dialog: extern "system" fn(
        me: ModHandle,
        dialog: *const UiDialog,
        callback: UiAnswerFn,
        user: *mut c_void,
    ) -> Status,
    /// Calls `callback` with `user` once per frame while the game draws its
    /// HUD in game, hidden with it; the callback draws labels and bars with
    /// [`HostApi::ui_widget`] at their `anchor`. Client only
    /// ([`Status::MISSING`] on a server). Allowed during
    /// `enshwick_mod_init` only.
    pub ui_hud: extern "system" fn(me: ModHandle, callback: UiFrameFn, user: *mut c_void) -> Status,
    /// Replaces the mod's markers on the client's map with the `count` at
    /// `markers`, copied; none clears them. Any thread. Client only
    /// ([`Status::MISSING`] on a server); [`Status::BAD_ARGS`] past 256
    /// markers.
    /// Every entry is as large as the header of the first one says.
    pub ui_markers:
        extern "system" fn(me: ModHandle, markers: *const UiMarker, count: u32) -> Status,
    /// Makes `callback` with `user` the mod's check of a new settings text,
    /// which the loader runs before it writes a change from the console or the
    /// Mods screen; a refusal keeps the file. Allowed during
    /// `enshwick_mod_init` only.
    pub config_check:
        extern "system" fn(me: ModHandle, callback: ConfigCheckFn, user: *mut c_void) -> Status,
    /// Hands the loader the mod's texts, a TOML table per language code as
    /// [`crate::i18n::Texts`] reads them, for the loader's own screens: the
    /// Mods screen labels the mod's settings with them. A later call
    /// replaces them.
    pub texts: extern "system" fn(me: ModHandle, toml: Str) -> Status,
    /// Serves `len` bytes at `data` as game content, the pixels of a
    /// texture the mod serves, and writes the content hash a resource names
    /// them by to `out`; the same bytes give the same hash. The loader
    /// keeps the bytes for the life of the process. Any thread.
    pub content_serve:
        extern "system" fn(me: ModHandle, data: *const u8, len: u32, out: *mut [u32; 4]) -> Status,
    /// Copies the game's own content `hash` from its archive into `out`, at
    /// most `capacity` bytes, and writes its length to `len`;
    /// [`Status::MISSING`] for a hash the archive lacks or keeps
    /// compressed, [`Status::BAD_ARGS`] when `capacity` is short, with
    /// `len` written. Reads the archive's files, never the game's memory.
    /// Any thread.
    pub content_read: extern "system" fn(
        me: ModHandle,
        hash: *const [u32; 4],
        out: *mut u8,
        capacity: u32,
        len: *mut u32,
    ) -> Status,
    /// [`Status::OK`] when this client shows the UI images mods serve: the
    /// content and UI registry links run and the registry knows the served
    /// images; [`Status::MISSING`] otherwise, and always on a server. Any
    /// thread.
    pub icons_ready: extern "system" fn(me: ModHandle) -> Status,
    /// Defines the stat kind `id` with `def`, or opens it with a null
    /// `def`, and writes its index and key to `out`. A mod defines kinds in
    /// its own namespace (`name` or `<mod>:name`); any mod opens any kind,
    /// the loader's built-in ones (`enshwick:<name>`) among them. Every mod
    /// gets the same index for one id. [`Status::BAD_ARGS`] for a malformed
    /// id or definition, [`Status::REFUSED`] for a kind defined twice or in
    /// another mod's namespace. Allowed during `enshwick_mod_init` only.
    pub stat_open: extern "system" fn(
        me: ModHandle,
        id: Str,
        def: *const StatDefData,
        out: *mut StatRef,
    ) -> Status,
    /// Writes the definition of stat `stat` to `def` and its full id to
    /// `id`, a string the loader keeps for the process; [`Status::MISSING`]
    /// for a kind opened but never defined. Any thread.
    pub stat_info:
        extern "system" fn(me: ModHandle, stat: u32, def: *mut StatDefData, id: *mut Str) -> Status,
    /// Writes the value of stat `stat` on `entity` to `out`: its default
    /// when nothing modifies it. Values are worked out on the server after
    /// each step a source changed; a client answers defaults. Any thread,
    /// without locks.
    pub stat_value:
        extern "system" fn(me: ModHandle, entity: Entity, stat: u32, out: *mut f32) -> Status,
    /// Sets or removes a modifier the mod keeps on an entity; the value
    /// follows after the next step. [`Status::REFUSED`] on a client alone,
    /// or for a saved modifier on an entity that is no player. Any thread.
    pub stat_modify: extern "system" fn(me: ModHandle, request: *const StatModify) -> Status,
    /// Gives the buff type `buff` the `count` modifiers at `modifiers`:
    /// they count toward every watched entity that has the buff. Allowed
    /// during `enshwick_mod_init` only.
    pub stat_buff: extern "system" fn(
        me: ModHandle,
        buff: u32,
        modifiers: *const StatModifierData,
        count: u32,
    ) -> Status,
    /// Calls `callback` with `user` for every stat value that changed on a
    /// watched entity, on the game thread after the step that changed it.
    /// Allowed during `enshwick_mod_init` only.
    pub stats_changed:
        extern "system" fn(me: ModHandle, callback: StatChangeFn, user: *mut c_void) -> Status,
    /// Writes up to `capacity` modifiers that count toward stat `stat` on
    /// `entity`, in the order they apply, and how many there are to
    /// `count`; every stat for `u32::MAX`. Any thread.
    pub stat_sources: extern "system" fn(
        me: ModHandle,
        entity: Entity,
        stat: u32,
        out: *mut StatSource,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Watches `entity` for the items it wears and the buffs it has, or
    /// stops for `watch` 0: players are always watched, and so is every
    /// entity a mod keeps a modifier on. Any thread.
    pub stat_watch: extern "system" fn(me: ModHandle, entity: Entity, watch: u8) -> Status,
    /// Defines the mod event `spec` describes, named `<id>:<name>` after
    /// the defining mod, and writes the id [`HostApi::send_event`] takes;
    /// [`Status::REFUSED`] for a name outside the mod's id or defined
    /// twice. Allowed during `enshwick_mod_init` only.
    pub define_event:
        extern "system" fn(me: ModHandle, spec: *const EventSpec, out: *mut u32) -> Status,
    /// Calls `callback` with `user` and the event's data on every mod event
    /// `spec` names, once its definition, checked against `spec`'s layout
    /// when every mod is initialised, matches. Allowed during
    /// `enshwick_mod_init` only.
    pub subscribe_event: extern "system" fn(
        me: ModHandle,
        spec: *const EventSpec,
        callback: ModEventFn,
        user: *mut c_void,
    ) -> Status,
    /// Hands `data` to every subscriber of the mod's own event `event`, on
    /// this thread, before it returns; reaches no one before every mod is
    /// initialised. [`Status::REFUSED`] for an event another mod defined.
    /// Any thread.
    pub send_event: extern "system" fn(me: ModHandle, event: u32, data: *const c_void) -> Status,
    /// Defines the affix `id` in the mod's namespace with `def` and writes
    /// its index and key to `out`. Allowed during `enshwick_mod_init` only.
    pub affix_open: extern "system" fn(
        me: ModHandle,
        id: Str,
        def: *const AffixDefData,
        out: *mut StatRef,
    ) -> Status,
    /// Rolls affixes onto an item from every mod's affixes that fit its
    /// category, replacing those it had, and writes how many to `count`;
    /// [`Status::REFUSED`] for a [`AffixRoll::ONCE`] roll of an item that
    /// rolled before, or off the server. On the server's game thread only.
    pub affix_roll:
        extern "system" fn(me: ModHandle, roll: *const AffixRoll, count: *mut u32) -> Status,
    /// Writes up to `capacity` lines of what the item gives, its own
    /// modifiers first and then each affix's, and how many to `count`. On a
    /// client, from what the server sent. Any thread.
    pub item_lines: extern "system" fn(
        me: ModHandle,
        pide: Entity,
        item: ItemId,
        out: *mut ItemLine,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Writes the text key and rarity of affix `affix` (an index) to `text`
    /// and `rarity`. Any thread.
    pub affix_info:
        extern "system" fn(me: ModHandle, affix: u32, text: *mut Str, rarity: *mut u8) -> Status,
    /// Registers the mod's migrations, `set`, copied: the loader runs the
    /// settings steps on the settings file now, and every step on each
    /// world's files when the world loads, once per world, and records what
    /// ran in `mod-data/enshwick/migrations.toml`. [`Status::REFUSED`] for
    /// a second call or a migration that failed on the settings file, with
    /// the reason logged. Allowed during `enshwick_mod_init` only.
    pub migrate: extern "system" fn(me: ModHandle, set: *const MigrationSet) -> Status,
    /// Declares the content id `id`, `<mod>:<name>` in the mod's own
    /// namespace, and writes its GUID to `out`; [`Status::REFUSED`] for a
    /// malformed id, one outside the namespace, or one whose game id
    /// another content id has, which turns the mod off with both ids and
    /// both mods named. Allowed during `enshwick_mod_init` only.
    pub declare_content: extern "system" fn(me: ModHandle, id: Str, out: *mut [u8; 16]) -> Status,
    /// Opens the entity data kind `id`, as [`HostApi::item_data_open`] opens
    /// an item kind, and writes its index to `out`; with
    /// [`EntityDataFlags::SENT`] the clients that run Enshwick get its
    /// values. [`HostApi::item_data_get`] and [`HostApi::item_data_set`]
    /// read and write it with `item` 0; a set runs on the server's game
    /// thread. [`Status::REFUSED`] for an id opened as an item kind. Allowed
    /// during `enshwick_mod_init` only.
    pub entity_data_open:
        extern "system" fn(me: ModHandle, id: Str, flags: EntityDataFlags, out: *mut u32) -> Status,
    /// Lists the mods in `mods/` that a present mod replaces (see
    /// [`Relation::REPLACES`](super::Relation::REPLACES)), which the loader never started, as
    /// [`HostApi::mods`] lists the loaded ones, with
    /// [`ModInfo::replaced_by`] set. Any thread.
    pub replaced: extern "system" fn(
        me: ModHandle,
        out: *mut ModInfo,
        entry_size: u32,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Registers a filter on the knowledge clients ask the server for in
    /// their input, which a vanilla game grants unchecked. Filters run in
    /// load order after the loader's guard, each with the verdict so far,
    /// and the last answer decides. It runs on the server's worker threads,
    /// in the dedicated server or a hosting client. [`Status::MISSING`] on
    /// a build without the input site. Allowed during `enshwick_mod_init`
    /// only.
    pub knowledge_input_filter:
        extern "system" fn(me: ModHandle, callback: KnowledgeInputFn, user: *mut c_void) -> Status,
    /// The game language of the reader, as the game numbers them (see
    /// [`crate::i18n::Language`]). On a server, while the loader calls the
    /// mod about one player (an action, a settings check, the player's mod
    /// message) on that thread, the language that player's game named when
    /// it joined, always one the game knows; else `0`, the fallback. On a
    /// client, the language it last asked a text in, `0` before its first
    /// text. Any thread; the answer holds for the calling thread.
    pub language: extern "system" fn(me: ModHandle) -> u32,
    /// Replaces the live game settings with `data` as
    /// [`HostApi::set_game_settings`] does, and keeps the fields with a
    /// byte set in `mask` (as long as `data`) with the running world: the
    /// loader applies them again each time the world loads, by the end of
    /// its first step. [`Status::MISSING`] while no world runs.
    /// On the server's game thread only.
    pub keep_game_settings:
        extern "system" fn(me: ModHandle, data: *const u8, mask: *const u8, size: u32) -> Status,
    /// Forgets the game settings kept for the running world and puts back
    /// the values it loaded with. On the server's game thread only.
    pub forget_game_settings: extern "system" fn(me: ModHandle) -> Status,
    /// Takes knowledge `id`, of `kind`, back from the player whose character
    /// is `entity`, or from the world for 0 or an id of the world's, and has
    /// the clients sent it again: a [`KnowledgeInfo::BOOL`] cleared, `value`
    /// taken from a [`KnowledgeInfo::COUNTER`] (down to 0), bit `value` of
    /// [`KnowledgeInfo::FLAGS`] cleared. Saved with the next save; what
    /// followed the grant, such as experience, stays.
    /// [`Status::BAD_ARGS`] for an unknown id, a kind that is not the id's or
    /// a bit past 31, [`Status::REFUSED`] for a player not in the game or
    /// whose save has not loaded yet. On the server's game thread only.
    pub revoke_knowledge: extern "system" fn(
        me: ModHandle,
        entity: Entity,
        id: KnowledgeId,
        kind: u32,
        value: u32,
    ) -> Status,
    /// Grants knowledge `id` of a player's, as [`HostApi::grant_knowledge`]
    /// does, or takes it back as [`HostApi::revoke_knowledge`] does when
    /// `revoke` is 1, for the account whose [`PlayerInfo::key`] is `key`:
    /// now when its character is in the game, else once its next character
    /// has loaded in this world; the loader keeps waiting changes with the
    /// world. [`Status::BAD_ARGS`] for an unknown id, an id of the world's
    /// or a kind that is not the id's, [`Status::MISSING`] while no world
    /// runs. On the server's game thread only.
    pub account_knowledge: extern "system" fn(
        me: ModHandle,
        key: PlayerKey,
        id: KnowledgeId,
        kind: u32,
        value: u32,
        revoke: u32,
    ) -> Status,
    /// Copies the game's own resource `key` from its archive into `out`, at
    /// most `capacity` bytes, as the game reads it before it resolves its
    /// references, and writes its length to `len`; [`Status::MISSING`] for
    /// a key the archive lacks, [`Status::BAD_ARGS`] when `capacity` is
    /// short, with `len` written. Reads the archive's files, never the
    /// game's memory. Any thread.
    pub resource_read: extern "system" fn(
        me: ModHandle,
        key: *const ResourceKey,
        out: *mut u8,
        capacity: u32,
        len: *mut u32,
    ) -> Status,
    /// Writes the game's own text of tag `id` in `language`, as the game
    /// numbers them (0 for English), to `out`, valid for the life of the
    /// process; a text missing in `language` falls back to English as the
    /// game's does. Read from the game's archive, the first call for a
    /// language reads its table. [`Status::MISSING`] for a tag the game
    /// lacks, and always on a server, whose archive has no texts. Any
    /// thread.
    pub game_text:
        extern "system" fn(me: ModHandle, id: u32, language: u32, out: *mut Str) -> Status,
    /// During `enshwick_mod_init`, by a mod that serves `key`: keeps the
    /// base of `key` when it is first built, so it can be built again.
    /// Afterwards: builds `key` again from that base with the patches of
    /// every mod still on, as values they read changed, and writes the
    /// bytes that differ over the resource the game holds; [`Status::OK`]
    /// with nothing done while `key` was not built yet, as its first build
    /// takes the values in force. Any thread. [`Status::MISSING`] when its
    /// base was not kept, [`Status::REFUSED`] when the new build differs in length, which the
    /// game's copy cannot take, [`Status::BAD_ARGS`] for a key the mod does
    /// not serve. Each rebuild keeps the patches' new bytes for the process.
    pub rebuild_resource: extern "system" fn(me: ModHandle, key: *const ResourceKey) -> Status,
    /// Adds the calling mod's rule that disables recipes (`disable` 1), or
    /// takes its own rule of that kind and name back (`disable` 0). `name`
    /// names a recipe, an item every recipe of which is disabled, or a
    /// workshop, as `kind` says, in the forms the loader's recipe file
    /// takes; the loader resolves it against the recipe registry and logs a
    /// name it cannot resolve. The server's recipe file wins over every
    /// mod's rules. Any thread; applied within a second.
    pub recipe_rule:
        extern "system" fn(me: ModHandle, kind: RecipeRuleKind, name: Str, disable: u32) -> Status,
    /// Writes 1 to `out` when the recipe with game id `recipe` is disabled
    /// in this process now, else 0: on a client joined to a server, by the
    /// server's rules. Any thread.
    pub recipe_disabled:
        extern "system" fn(me: ModHandle, recipe: RecipeId, out: *mut u32) -> Status,
    /// Defines the extension point `spec` describes, named
    /// `<id>:<point>` after the defining mod, and writes its id and the
    /// count of its live links to `out`; [`Status::REFUSED`] for a name
    /// outside the mod's id or defined twice. Allowed during
    /// `enshwick_mod_init` only.
    pub define_point:
        extern "system" fn(me: ModHandle, spec: *const PointSpec, out: *mut PointHandle) -> Status,
    /// Attaches `callback` with `user` to the extension point `link`
    /// names, in the order it asks for. Matched against the point's
    /// definition once every mod is initialised; a point no running mod
    /// defines, or one whose layout or mode differs, leaves the link inert.
    /// Allowed during `enshwick_mod_init` only.
    pub attach_point: extern "system" fn(
        me: ModHandle,
        link: *const PointLink,
        callback: PointFn,
        user: *mut c_void,
    ) -> Status,
    /// Runs the live links of the mod's own extension point `point` over
    /// `value`, which holds the mod's own answer, on this thread, and writes
    /// to `verdict` whether the answer was kept, changed or refused.
    /// [`Status::REFUSED`] for a point another mod defined. Any thread.
    pub run_point: extern "system" fn(
        me: ModHandle,
        point: u32,
        input: *const c_void,
        value: *mut c_void,
        verdict: *mut PointVerdict,
    ) -> Status,
    /// Registers the mod's player actions: the loader asks `callback` with
    /// `user`, on the server's game thread, for the actions a player may
    /// use, to run one a player picked in the interface or the console
    /// named, and for the hints a player has at hand. One registration per
    /// mod. Allowed during
    /// `enshwick_mod_init` only.
    pub actions: extern "system" fn(me: ModHandle, callback: ActionFn, user: *mut c_void) -> Status,
    /// Writes the first `capacity` of the running build's game event types
    /// to `out`, null or room for `capacity` entries each as large as the
    /// header of the first one says, and how many there are to `count`:
    /// every reflected type that derives from the engine's event base, in no
    /// set order. The names are the loader's, valid for the rest of the
    /// process. A null `count` gives [`Status::BAD_ARGS`] after the entries
    /// are written; a handle the loader never gave out gives
    /// [`Status::BAD_ARGS`] and a mod that is off [`Status::FAULTED`], with
    /// nothing written. Any thread.
    pub event_types: extern "system" fn(
        me: ModHandle,
        out: *mut EventTypeInfo,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Writes to `out` the kind of the recipes that make the item `item`,
    /// from the item's static data; [`Status::MISSING`] while the game has
    /// not loaded the item, which on a dedicated server comes after its
    /// recipe registry is first built, and while the loader serves no
    /// resource. Any thread.
    pub recipe_kind:
        extern "system" fn(me: ModHandle, item: ItemId, out: *mut RecipeKind) -> Status,
    /// Reports whether the mod's feature `name` runs on this game build:
    /// off for the reason `off`, or on when `off` is empty. The loader
    /// logs it and shows it in the status summary and on the Mods screen; a
    /// later report of the same name replaces the earlier one.
    /// [`Status::BAD_ARGS`] for an empty name. Both strings are borrowed for
    /// the call. Any thread.
    pub feature: extern "system" fn(me: ModHandle, name: Str, off: Str) -> Status,
    /// Declares that the calling mod multiplies `rate`, so the loader arms
    /// what the rate needs; its factor is 1 until
    /// [`HostApi::set_rate`]. [`Status::MISSING`] on a build that lacks
    /// what the rate needs, [`Status::BAD_ARGS`] for a rate this loader
    /// does not know, [`Status::REFUSED`] for a rate clients count too in a
    /// mod without [`ManifestFlags::JOIN`](super::ManifestFlags::JOIN). A second declaration of the same rate is
    /// [`Status::OK`]. [`Status::TOO_LATE`] after `enshwick_mod_init`,
    /// [`Status::FAULTED`] for a mod that is off. Allowed during
    /// `enshwick_mod_init` only.
    pub rate: extern "system" fn(me: ModHandle, rate: RateId) -> Status,
    /// Puts `factor` in force as the calling mod's factor for `rate`: the
    /// game uses the product of every declaring mod's factor, held to 0 to
    /// 100, from the next time it reads the rate. [`Status::BAD_ARGS`] for
    /// a factor that is not finite or lies outside 0 to 100, or a rate the
    /// mod did not declare, [`Status::FAULTED`] for a mod that is off. Any
    /// thread; neither allocates nor locks once every mod is initialised,
    /// except for a recipe output rate, whose change builds the recipe
    /// registry again.
    pub set_rate: extern "system" fn(me: ModHandle, rate: RateId, factor: f32) -> Status,
    /// Declares a change of a vanilla value. The loader merges the changes
    /// of every mod once their `init` ran: a set in load order, the last
    /// winning and the overridden one reported in both mods' logs and the
    /// conflict report; then every factor, multiplied. It builds the changed
    /// resource the first time the game reads it, on each side from its own
    /// mods. [`Status::BAD_ARGS`] for a null `change`, an unknown field or
    /// operation and a value outside the field's range; [`Status::MISSING`]
    /// when the build does not reflect the field. Init only.
    pub change_data: extern "system" fn(me: ModHandle, change: *const DataChange) -> Status,
    /// Calls `callback` with `user` before every wait of the engine's event
    /// loop on its handles, on the thread that waits, and writes to `found`
    /// which waits it hooked ([`LoopWaitSites`]); a wait this build lacks,
    /// whose code is not what its table expects, or that the mod hooked
    /// already is left out. [`Status::BAD_ARGS`] for a null `found`, before
    /// anything is hooked. Handlers run in load order. Allowed during
    /// `enshwick_mod_init` only.
    pub loop_wait: extern "system" fn(
        me: ModHandle,
        callback: LoopWaitFn,
        user: *mut c_void,
        found: *mut LoopWaitSites,
    ) -> Status,
    /// Calls `callback` with `user` once a pass where the server's game loop
    /// has picked its wait before the next pass, in nanoseconds, which the
    /// handler may change; writes 1 to `found` when the loader hooked the
    /// place, else 0. [`Status::BAD_ARGS`] for a null `found`, before
    /// anything is hooked. Handlers run in load order, each on the last one's wait. On
    /// the game thread of the server loop. Allowed during `enshwick_mod_init`
    /// only.
    pub server_wait: extern "system" fn(
        me: ModHandle,
        callback: ServerWaitFn,
        user: *mut c_void,
        found: *mut u32,
    ) -> Status,
    /// Caps the worker threads of the task system's default and background
    /// groups (`foreground` and `background`) as it starts, 0 leaving a group as the game sizes it; with
    /// several mods the lowest cap wins. Writes 1 to `found` when a cap is
    /// hooked and the task system starts after the hooks arm, which only the
    /// dedicated server does, else 0. [`Status::BAD_ARGS`] for a null
    /// `found`, before anything is hooked. Allowed during
    /// `enshwick_mod_init` only.
    pub task_workers: extern "system" fn(
        me: ModHandle,
        foreground: u32,
        background: u32,
        found: *mut u32,
    ) -> Status,
    /// Calls `callback` with `user` for every altar of the server's world
    /// when the loader sizes the altar zones: after altars change and after
    /// [`HostApi::altar_zones_changed`]. Each filter, in load order, sees the
    /// size so far, the vanilla one first, and may change it: a `grow` below
    /// 0 counts as 0, a `height` below one cell keeps the height before it,
    /// and the altar and level stay. It opens altars too, and may call the
    /// host, such as [`HostApi::altars`]. On the server's game thread.
    /// Allowed during `enshwick_mod_init` only.
    pub altar_zone_filter:
        extern "system" fn(me: ModHandle, callback: AltarZoneFn, user: *mut c_void) -> Status,
    /// Asks the loader to size the altar zones again on the server's next
    /// step, as when a mod's settings change what its filter answers. Any
    /// thread, also during init. [`Status::REFUSED`] for a mod without an
    /// altar zone filter.
    pub altar_zones_changed: extern "system" fn(me: ModHandle) -> Status,
    /// Calls `callback` with `user` when the game asks how many altars a
    /// Flame level allows, on whichever thread asks: the placement cursor
    /// while a player aims an altar and the server's placement. Each filter,
    /// in load order, sees the game's own count and the count so far and may
    /// change `allowed`; the level and the game's count stay. The loader keeps
    /// the answer within the zones the game can group
    /// ([`HostApi::base_limit`]). A filter runs often: keep it cheap; it may
    /// call the host. [`Status::TOO_LATE`] after `enshwick_mod_init`,
    /// [`Status::MISSING`] on a build without the game's lookup.
    pub altar_limit_filter: extern "system" fn(
        me: ModHandle,
        callback: AltarLimitFilterFn,
        user: *mut c_void,
    ) -> Status,
    /// Writes to `out` the most altar zones and bases the game groups now:
    /// its own, or the loader's maximum once [`HostApi::raise_base_limit`] is
    /// in force, from `enshwick_mod_ready` on, with the zones its own
    /// grouping takes. [`Status::BAD_ARGS`] for a null `out`. Any thread.
    pub base_limit: extern "system" fn(me: ModHandle, out: *mut BaseLimitInfo) -> Status,
    /// Reads the voxels of the server's world in the box of `size` voxels
    /// from `min`, in voxels of half a metre from the world's corner, into
    /// `out`: x fastest, then y, then z, `size[0] * size[1] * size[2]`
    /// entries. `min` and `size` are borrowed for the call; `out` points at
    /// `capacity` writable entries, untouched unless the call returns
    /// [`Status::OK`]. [`Status::BAD_ARGS`] for a null pointer, an empty
    /// box, one past [`Voxel::END`], more than [`Voxel::READ_MAX`] voxels or
    /// a `capacity` below them; [`Status::OUTSIDE_WORLD`] for a box past
    /// the world's edge or above the height the loader takes;
    /// [`Status::NOT_LOADED`] while part of the box is not loaded
    /// yet, which starts loading it; [`Status::MISSING`] before a world
    /// runs or on a build without the read. On the server's game thread at
    /// a quiet moment only, as the world's writes: [`Status::WRONG_THREAD`]
    /// inside a step.
    pub voxel_read: extern "system" fn(
        me: ModHandle,
        min: *const [u32; 3],
        size: *const [u32; 3],
        out: *mut Voxel,
        capacity: u64,
    ) -> Status,
    /// Declares a change of a vanilla value, as
    /// [`HostApi::change_data`] does, whose value the mod sets later with
    /// [`HostApi::set_change`]; until then the mod changes nothing. The
    /// loader builds every resource the change reaches from the first read,
    /// so a later set takes effect in place. Writes the change's number to
    /// `out`. `change.op` and `change.value` are ignored.
    /// [`Status::BAD_ARGS`] for a null `change` or `out`, an unknown field
    /// or selection, and an ingredient, whose change alters a recipe's
    /// length; [`Status::MISSING`] when the build does not reflect the
    /// field; [`Status::REFUSED`] for a field clients read in a mod without
    /// [`ManifestFlags::JOIN`](super::ManifestFlags::JOIN); [`Status::TOO_LATE`] after
    /// `enshwick_mod_init`.
    pub declare_change:
        extern "system" fn(me: ModHandle, change: *const DataChange, out: *mut ChangeId) -> Status,
    /// Puts `value` in force for the calling mod's change `id` by `op`:
    /// [`ChangeOp::SET`], [`ChangeOp::TIMES`], or [`ChangeOp::RESET`] to
    /// take the mod's value out. The loader merges every mod's changes
    /// again and builds each resource it patches anew in place; the game
    /// reads the new value from then on. During `enshwick_mod_init` the
    /// value counts from the first read. [`Status::BAD_ARGS`] for an id
    /// the mod did not declare, an unknown operation, or a value the field
    /// does not take; [`Status::FAULTED`] for a mod that is off;
    /// [`Status::TOO_LATE`] when the value takes a restart, and the log says
    /// why: it reaches something the loader did not patch at start (the
    /// value is dropped and the old one stays in force), or a resource does
    /// not build again in place (the value is in force for what did build
    /// and for every first read, the rest until a restart). Any
    /// thread; it allocates and builds resources, so it is no per-frame
    /// call.
    pub set_change:
        extern "system" fn(me: ModHandle, id: ChangeId, op: ChangeOp, value: f64) -> Status,
    /// Fills `check.refusal` with whether the server lets the player whose
    /// character is `check.player` edit the box from `check.min` to
    /// `check.max`. The permissions follow the game's terrain tool:
    /// `CanEditWorld` for every edit, `CanEditBase` where the box overlaps a
    /// building zone. The loader adds two tests the building cursor runs for
    /// items: both corners inside the world's building area, and no strict
    /// no-build zone the world places. A player not in game has no
    /// permissions. `check` is borrowed for the call. [`Status::BAD_ARGS`]
    /// for a null `check`, an old header, a corner that is not finite or a
    /// `max` below `min`; [`Status::MISSING`] while no world is loaded or on
    /// a build without the checks. On the server's game thread at a quiet
    /// moment only, outside a step or in a game event:
    /// [`Status::WRONG_THREAD`] inside a step.
    pub build_rights: extern "system" fn(me: ModHandle, check: *mut BuildRights) -> Status,
    /// Queues the `count` changes at `ops` as one transaction of the
    /// server's world and writes its id to `id`: the loader writes them in
    /// order on the server's game thread at quiet moments, a budget of
    /// bricks a step, keeping the earlier state of every brick a change can
    /// touch so [`HostApi::transaction_undo`] can put it back. `ops` and the
    /// records they point at are borrowed for the call. [`Status::BAD_ARGS`]
    /// for a null pointer, no changes, a kind or record the loader refuses
    /// (as [`HostApi::terrain_edit`] does, and a dry run), or more than its
    /// cap of changes or bricks; [`Status::OUTSIDE_WORLD`] for a change that
    /// leaves the world, once the loader knows its extent (a transaction
    /// committed earlier fails with it before writing); [`Status::REFUSED`]
    /// while the queue is full; [`Status::MISSING`] while no world runs or
    /// on a build without the edits, the voxel read or the undo write. Any
    /// thread.
    pub transaction_commit: extern "system" fn(
        me: ModHandle,
        ops: *const TransactionOp,
        count: u32,
        id: *mut u64,
    ) -> Status,
    /// Queues the undo of transaction `id` of this mod: a queued one is
    /// dropped, one written in whole or in part gets each brick it touched
    /// back as it was, in reverse order, on the server's game thread at
    /// quiet moments. Before writing, the undo checks that every brick still
    /// holds what the transaction left there and that all of them are
    /// loaded, and it checks each brick again right before writing it back;
    /// a brick changed meanwhile, or a column that stays unloaded for the
    /// loader's wait limit, stops it, and the state turns
    /// [`TransactionInfo::UNDO_REFUSED`] with what was put back kept. A
    /// refused undo may be asked again. [`Status::OK`] when queued or
    /// already undoing; [`Status::REFUSED`] for an id this mod did not get,
    /// one the loader forgot, one undone, one whose undo data is gone, or
    /// while the transactions in work already claim the loader's cap of
    /// undo data. Any thread.
    pub transaction_undo: extern "system" fn(me: ModHandle, id: u64) -> Status,
    /// Writes where transaction `id` of this mod stands to `out`, a
    /// [`TransactionInfo`] with its header. [`Status::BAD_ARGS`] for a null
    /// `out`; [`Status::MISSING`] for an id this mod did not get or one the
    /// loader forgot (it keeps the newest few thousand). Any thread.
    pub transaction_status:
        extern "system" fn(me: ModHandle, id: u64, out: *mut TransactionInfo) -> Status,
    /// As [`HostApi::transaction_commit`], for the player whose character
    /// is `actor`: before it writes anything the loader checks that the
    /// server lets the player edit the box of every change, as
    /// [`HostApi::build_rights`] does, and marks building events with the
    /// player. A refusal fails it with [`Status::REFUSED`] and the reason in
    /// [`TransactionInfo::refusal`]; nothing is written. The check runs at
    /// its first step on the server's game thread, never at the commit.
    /// [`Status::BAD_ARGS`] for an `actor` of 0. Any thread.
    pub transaction_commit_for: extern "system" fn(
        me: ModHandle,
        actor: Entity,
        ops: *const TransactionOp,
        count: u32,
        id: *mut u64,
    ) -> Status,
    /// [`Status::OK`] on the server's game thread at a quiet moment while a
    /// world runs, where a mod may read live components that no system
    /// writes meanwhile; [`Status::WRONG_THREAD`] on another thread or
    /// inside a step, [`Status::MISSING`] while no world runs.
    pub world_quiet: extern "system" fn(me: ModHandle) -> Status,
    /// Asks the loader to keep a copy of the client's fog of war for this
    /// mod. Client only ([`Status::MISSING`] on a dedicated server).
    /// Allowed during `enshwick_mod_init` only.
    pub fog_open: extern "system" fn(me: ModHandle) -> Status,
    /// Writes the fog's [`FogInfo`] to `info`, then copies the box of
    /// `width` × `height` pixels at (`x`, `y`) into `out`, row by row, at
    /// most `capacity` bytes; a box of no pixels copies nothing. Any thread.
    /// [`Status::MISSING`] while the client is in no world, when this build
    /// lacks the fog, or after the loader stopped copying it; [`Status::REFUSED`] for a mod that did not
    /// open it; [`Status::OUTSIDE_WORLD`] for a box past the image;
    /// [`Status::BAD_ARGS`] for a null `info`, or a null or short `out`
    /// for a box of pixels. The loader may refresh the copy meanwhile:
    /// compare `version`.
    pub fog_read: extern "system" fn(
        me: ModHandle,
        info: *mut FogInfo,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        out: *mut u8,
        capacity: u32,
    ) -> Status,
    /// Serves the call `spec` describes with `callback` and `user`, named
    /// `<id>:<call>` after the serving mod or one of its former ids;
    /// [`Status::REFUSED`] for another name, or a name and major served
    /// twice, [`Status::BAD_ARGS`] for a spec that does not read. Allowed
    /// during `enshwick_mod_init` only: [`Status::TOO_LATE`] once every mod
    /// is initialised.
    pub serve_call: extern "system" fn(
        me: ModHandle,
        spec: *const CallSpec,
        callback: CallFn,
        user: *mut c_void,
    ) -> Status,
    /// Asks for the call `spec` describes and writes its id to `out`.
    /// Matched against the served call by name and major once every mod is
    /// initialised: the input and the answer must agree field by field and
    /// in size, or the call stays unmatched with the reason in the log.
    /// [`Status::REFUSED`] when the mod asked for the name and major already
    /// with another spec, [`Status::BAD_ARGS`] for a spec that does not read
    /// or a null `out`. Allowed during `enshwick_mod_init` only:
    /// [`Status::TOO_LATE`] once every mod is initialised.
    pub find_call:
        extern "system" fn(me: ModHandle, spec: *const CallSpec, out: *mut u32) -> Status,
    /// Runs the call `id` from [`HostApi::find_call`] on this thread: the
    /// server's handler reads `input` and writes the answer to `output`,
    /// both as large as the spec says. [`Status::MISSING`] while the call
    /// is unmatched or before every mod is initialised, [`Status::REFUSED`]
    /// when the serving mod or its handler is off, [`Status::BAD_ARGS`] for
    /// a null pointer or an id the mod was not given, or what the handler
    /// answers. Any thread.
    pub call: extern "system" fn(
        me: ModHandle,
        id: u32,
        input: *const c_void,
        output: *mut c_void,
    ) -> Status,
    /// Writes the entities of the props transaction `id` of this mod placed
    /// and still holds, oldest first, up to `capacity` of them at `out`,
    /// and their number to `count`; a prop is in the world from the step
    /// after the one that placed it. `out` may be null with `capacity` 0 to
    /// learn the number. [`Status::BAD_ARGS`] for a null `count`;
    /// [`Status::MISSING`] for an id this mod did not get or one the loader
    /// forgot. Any thread.
    pub transaction_entities: extern "system" fn(
        me: ModHandle,
        id: u64,
        out: *mut Entity,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Writes to `kept` 1 when a save keeps changes in the box from `min`
    /// to `max`, in metres, and 0 when not: the game saves a world only
    /// inside its bases' zones, so the box must lie wholly inside one, half
    /// a metre in from its edges. [`Status::BAD_ARGS`] for a null pointer,
    /// a corner that is not finite or a `max` below `min`;
    /// [`Status::MISSING`] while no world is loaded or on a build without
    /// the zone list. On the server's game thread at a quiet moment only:
    /// [`Status::WRONG_THREAD`] elsewhere or inside a step.
    pub world_persists: extern "system" fn(
        me: ModHandle,
        min: *const [f64; 3],
        max: *const [f64; 3],
        kept: *mut u8,
    ) -> Status,
    /// Up to `capacity` map markers of the scene the server runs and how
    /// many there are: what the scene's global chunk places, read from the
    /// game's archive once per scene on a thread of the loader's;
    /// [`Status::NOT_LOADED`] until that read is done. Entries are `entry_size` bytes apart,
    /// the caller's `size_of::<MapMarkerInfo>()`, at least a [`Header`]
    /// ([`Status::BAD_ARGS`] below it); a null `out` asks only for the
    /// count, and a null `count` is not written. On the server's game
    /// thread at a quiet moment ([`Status::WRONG_THREAD`] elsewhere or
    /// inside a step); [`Status::MISSING`] while no world runs or when the
    /// scene does not read on this build.
    pub map_markers: extern "system" fn(
        me: ModHandle,
        out: *mut MapMarkerInfo,
        entry_size: u32,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Asks the game for the account action `action` on `account`, as its
    /// administrators' account actions do: [`AccountActionType::KICK`]
    /// removes a player online, [`AccountActionType::BAN`] puts the account
    /// on the server's ban list and removes its player when online,
    /// [`AccountActionType::UN_BAN`] takes it off the list. A ban of an
    /// account not online goes straight onto the list, which the dedicated
    /// server writes to `bannedAccounts` of `enshrouded_server.json`.
    /// [`Status::REFUSED`] for a kick or a ban of an administrator online,
    /// whom the game never removes, for a kick or a ban of a hosted game's
    /// host, for a kick of an account not online, and for a ban of an account
    /// not online that the game's list does not take; an account on the
    /// list already is no error. [`Status::BAD_ARGS`] for account 0 or an
    /// action this loader does not know; [`Status::MISSING`] while no server runs or on a build that
    /// lacks the action. On the server's game thread between steps only:
    /// [`Status::WRONG_THREAD`] elsewhere.
    pub account_action:
        extern "system" fn(me: ModHandle, action: AccountActionType, account: PlayerKey) -> Status,
    /// Puts `account` on the loader's allowlist when `listed` is 1, or takes
    /// it off when 0: the accounts a dedicated server with the loader's
    /// `allowlist` setting on lets join, beside its administrators. The
    /// list is the file `mod-data/enshwick/allowlist.txt`, written at once
    /// whether the setting is on or not; it applies from the next join.
    /// `Status::OK` also when the list already says so;
    /// [`Status::BAD_ARGS`] for account 0 or `listed` above 1;
    /// [`Status::REFUSED`] when the file is not written;
    /// [`Status::MISSING`] outside a dedicated server. Any thread, also
    /// while another call or the console changes the list: changes take
    /// turns and the file is replaced whole. A panic in the loader returns
    /// [`Status::REFUSED`]. Since minor 1.
    pub allowlist_set: extern "system" fn(me: ModHandle, account: PlayerKey, listed: u8) -> Status,
    /// Writes 1 to `listed` when the loader's allowlist lists `account`, 0
    /// when not or when the file never read, whether the `allowlist`
    /// setting is on or not. `listed` is the caller's, writable for one
    /// byte for the call, and written only on [`Status::OK`].
    /// [`Status::BAD_ARGS`] for account 0 or a null `listed`;
    /// [`Status::MISSING`] outside a dedicated server. Any thread, also
    /// while the list changes. A panic in the loader returns
    /// [`Status::REFUSED`]. Since minor 1.
    pub allowlist_has:
        extern "system" fn(me: ModHandle, account: PlayerKey, listed: *mut u8) -> Status,
    /// Calls `callback` with `user`, [`Event::TICKED`] and a [`TickData`](super::TickData)
    /// on the server's game thread after a step, once `every_nanos` of wall
    /// time have passed since its last call, and on the first step after it
    /// subscribed; 0 calls it every step. Steps the handler skips cost it
    /// nothing. The data lives for the call; the loader keeps `user` and
    /// never frees it. Writes the handler's handle to `out`, the caller's,
    /// writable for four bytes, on [`Status::OK`] only. [`Status::BAD_ARGS`]
    /// for a null `out`; [`Status::TOO_LATE`] outside `enshwick_mod_init`
    /// and a reloading mod's init again. The handler runs until the mod is
    /// off; a panic in it turns the mod off. It may call
    /// [`HostApi::set_tick_every`] on itself. [`Status::REFUSED`] once the
    /// loader holds `u32::MAX` handlers, and when the loader panics.
    /// Since minor 3.
    pub subscribe_tick: extern "system" fn(
        me: ModHandle,
        every_nanos: u64,
        callback: EventFn,
        user: *mut c_void,
        out: *mut TickHandle,
    ) -> Status,
    /// Sets the period of the mod's periodic tick handler `handle` to
    /// `every_nanos`, 0 for every step, from the next step on: the next
    /// call comes `every_nanos` after its last one, or on the next step when
    /// that time has passed. [`Status::REFUSED`] for another mod's handle;
    /// [`Status::MISSING`] for a handle the loader never gave out or one of
    /// the mod's copy before a reload. Any thread, the handler itself
    /// included. A panic in the loader returns [`Status::REFUSED`]. Since
    /// minor 3.
    pub set_tick_every:
        extern "system" fn(me: ModHandle, handle: TickHandle, every_nanos: u64) -> Status,
}
