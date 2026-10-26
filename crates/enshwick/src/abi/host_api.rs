//! The table of functions the loader hands every mod. One `repr(C)` struct
//! whose fields keep their ABI order, so it stays in one file.

use core::ffi::c_void;

use crate::abi::{
    ActionFn, ActiveBuff, AffixDefData, AffixRoll, AltarInfo, AltarLimitFn, AltarZoneSize,
    BedCheckFn, BuffRequest, BuffTypeInfo, BuildCheckFn, BuildInfo, CallerFrame, ComfortFn,
    ConfigCheckFn, CraftCheckFn, DamageFilterFn, DaytimeInfo, EntityDataFlags, EntityPlace, Event,
    EventFn, EventSpec, EventTypeInfo, ExperiencePassFn, GameEventFn, Header, HookSpec,
    InteractionFn, InventoryRequest, InventoryResultFn, ItemData, ItemLine, KnowledgeInfo,
    KnowledgeInputFn, Level, MidHookSpec, MigrationSet, ModEventFn, ModHandle, ModInfo,
    NetMessageFn, PlayerInfo, PointFn, PointHandle, PointLink, PointSpec, PointVerdict, RecipeKind,
    RecipeRuleKind, RenameCallFn, ResourceKey, ResourcePatchFn, ServiceFound, ServiceSpec,
    SkillCheckFn, SpawnRequest, StatChangeFn, StatDefData, StatModifierData, StatModify, StatRef,
    StatSource, Status, StatusFn, Str, SummonFn, SurvivorLookupFn, SurvivorStatusFn,
    TeleportRequest, TerrainEdit, TerrainMaterialInfo, UiAnswerFn, UiDialog, UiFrameFn, UiMarker,
    UiWidget, WeatherFront, ZoneCheckFn,
};

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
    /// major. Allowed during `enshwick_mod_init` only. Since minor 47.
    pub publish: extern "system" fn(me: ModHandle, spec: *const ServiceSpec) -> Status,
    /// Finds the service `wanted` describes, its table null, and checks
    /// that the published layout starts with the wanted one; writes the
    /// table and its publisher to `out`, and on [`Status::MISSING`] or
    /// [`Status::REFUSED`] the reason. Since minor 47.
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
    /// formatting anything more detailed. Since minor 1.
    pub log_level: extern "system" fn(me: ModHandle) -> Level,
    /// Calls `callback` with `user` on every `event` for the mod, on the
    /// thread that raises it, until the mod is off; allowed during
    /// `enshwick_mod_init` only. Since minor 2.
    pub subscribe: extern "system" fn(
        me: ModHandle,
        event: Event,
        callback: EventFn,
        user: *mut c_void,
    ) -> Status,
    /// The ECS type hash of a reflected component type, by name. Since minor 3.
    pub type_hash: extern "system" fn(type_name: Str, out: *mut u32) -> Status,
    /// A read-only pointer to `entity`'s component of type `type_hash`, valid
    /// until the game thread moves on; on the server's game thread only.
    /// Since minor 3.
    pub component: extern "system" fn(
        me: ModHandle,
        entity: u32,
        type_hash: u32,
        out: *mut *const c_void,
    ) -> Status,
    /// The template GUID of `entity`, in file order; on the server's game
    /// thread only. Since minor 3.
    pub entity_template:
        extern "system" fn(me: ModHandle, entity: u32, out: *mut [u8; 16]) -> Status,
    /// The connected player whose character is `entity`; on the server's game
    /// thread only. Since minor 3.
    pub player: extern "system" fn(me: ModHandle, entity: u32, out: *mut PlayerInfo) -> Status,
    /// Up to `capacity` connected players with characters in the world, and
    /// how many there are; on the server's game thread only. Since minor 3.
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
    /// during `enshwick_mod_init` only; one mod per key. Since minor 4.
    pub serve_resource: extern "system" fn(
        me: ModHandle,
        key: *const ResourceKey,
        base: *const ResourceKey,
        patch: ResourcePatchFn,
        user: *mut c_void,
    ) -> Status,
    /// Adds the text of `LocaTag` `id` in `language` (`LanguageId`, 0 for any
    /// language without its own); the client shows it wherever the game asks
    /// for that tag. Allowed during `enshwick_mod_init` only. Since minor 4.
    pub add_text: extern "system" fn(me: ModHandle, id: u32, language: u32, text: Str) -> Status,
    /// Up to `capacity` loaded mods, the loader first, and how many there
    /// are. Entries are `entry_size` bytes apart, the caller's
    /// `size_of::<ModInfo>()`; the loader fills what both sides know of each
    /// and its own [`Header`]. Since minor 5.
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
    /// for the life of the process. Since minor 6.
    pub sync: extern "system" fn(me: ModHandle, synced: Str, out: *mut Str) -> Status,
    /// Writes where the game's code section (`.text`) is loaded and how many
    /// bytes it spans. Since minor 7.
    pub code_range: extern "system" fn(start: *mut u64, len: *mut u64) -> Status,
    /// Replaces `len` bytes of game code at `address`, which must read
    /// `expected` at arming, with `replacement`; both point at `len` bytes
    /// valid for the call. The loader writes the patch with the hooks, refuses
    /// one that overlaps another patch, and skips one that overlaps hooked
    /// bytes or no longer reads `expected`, turning the mod off. Allowed
    /// during `enshwick_mod_init` only. Since minor 7.
    pub patch_code: extern "system" fn(
        me: ModHandle,
        address: u64,
        expected: *const u8,
        replacement: *const u8,
        len: usize,
    ) -> Status,
    /// The most recipes one workshop takes on the running game: 512, the
    /// game's own list size, or [`u32::MAX`] once the loader has lifted that
    /// limit at arming. Since minor 7.
    pub workshop_recipes: extern "system" fn() -> u32,
    /// 1 while this process runs a game server: always on the dedicated
    /// server, on the client while it hosts a game, solo play included.
    /// Since minor 8.
    pub hosting: extern "system" fn() -> u8,
    /// The entity world of the game server this process runs, the value a
    /// system context of that world holds at `ecs.context_entity_world`; 0
    /// while none runs or before its first step. Since minor 8.
    pub server_world: extern "system" fn() -> u64,
    /// Retired in minor 68, when chat commands left the platform: always
    /// [`Status::MISSING`]. The slot keeps the table's layout.
    pub retired_command: extern "system" fn(me: ModHandle) -> Status,
    /// Sends `text` to the chat of the player whose account key is `to`, or of
    /// everyone for 0, from any thread; the server posts it after the next
    /// step. Lines past 1024 bytes are cut. Since minor 9.
    pub chat: extern "system" fn(me: ModHandle, to: u64, text: Str) -> Status,
    /// Gives `entity` the name `text`, as a player's rename does: shown on
    /// hover, sent to every client and saved with the world; cut at 255
    /// bytes, removed when empty. On the server's game thread only. Since
    /// minor 9.
    pub set_label: extern "system" fn(me: ModHandle, entity: u32, text: Str) -> Status,
    /// Calls `callback` with `user` before the server applies a player's
    /// rename of an entity; any filter returning 0 refuses it. Allowed during
    /// `enshwick_mod_init` only. Since minor 9.
    pub rename_filter:
        extern "system" fn(me: ModHandle, callback: RenameCallFn, user: *mut c_void) -> Status,
    /// Up to `capacity` of the server's join mods that the last refused join
    /// found missing on this client or in another version, and how many
    /// there are; laid out as [`HostApi::mods`] lays out its entries. Each
    /// entry has the server's version, [`ManifestFlags::JOIN`](super::ManifestFlags::JOIN), `off` 1, the
    /// client's DLL path when it has the mod (empty otherwise), and the name
    /// and homepage the server advertises. The strings stay valid for the
    /// process. Empty before a refused
    /// join; see [`Event::JOIN_REFUSED`]. Since minor 10.
    pub join_missing: extern "system" fn(
        me: ModHandle,
        out: *mut ModInfo,
        entry_size: u32,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Runs a handler before the instruction at an address inside a game
    /// function, with the registers there; allowed during
    /// `enshwick_mod_init` only. Since minor 11.
    pub hook_at: extern "system" fn(me: ModHandle, spec: *const MidHookSpec) -> Status,
    /// Asks for inventory transactions and item data; allowed during
    /// `enshwick_mod_init` only. [`Status::MISSING`] when this game build
    /// lacks a piece of them. Requests run in the game server: the dedicated
    /// server, or the client while it hosts. Since minor 12.
    pub inventory_open: extern "system" fn(me: ModHandle) -> Status,
    /// Writes the static data of the item `item`, as far as the [`Header`]
    /// `out` starts with says the caller's struct reaches;
    /// [`Status::MISSING`] for an unknown item, or before the game first ran
    /// its inventories. Any thread. Since minor 12.
    pub item_info: extern "system" fn(me: ModHandle, item: u32, out: *mut ItemData) -> Status,
    /// Queues `request`, from any thread, for the game's inventory system,
    /// which runs it as one transaction: all or nothing. `done` gets the
    /// result with `user` on the game thread after a later step, unless the
    /// mod is off by then. On an error status nothing is queued and `done` is
    /// never called. Since minor 12.
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
    /// `enshwick_mod_init` only. Since minor 13.
    pub status: extern "system" fn(me: ModHandle, callback: StatusFn, user: *mut c_void) -> Status,
    /// Runs the console line `line` as the server's console, an
    /// administrator with no character, and writes its reply to `out`, valid
    /// until the next call on this thread: `enshwick ...` for the loader's
    /// own tools, or a mod's id and the text form of one of its
    /// [actions](HostApi::actions). On the server's game thread only;
    /// [`Status::MISSING`] when nothing answers the line's first word. Since
    /// minor 13, with actions since minor 68.
    pub run_console: extern "system" fn(me: ModHandle, line: Str, out: *mut Str) -> Status,
    /// Asks the loader to follow the Flame Altars of the server's world, for
    /// [`HostApi::altars`] and [`Event::ALTARS_CHANGED`]. Allowed during
    /// `enshwick_mod_init` only. Since minor 15.
    pub altars_open: extern "system" fn(me: ModHandle) -> Status,
    /// Up to `capacity` altars of the server's world and how many there are.
    /// Entries are `entry_size` bytes apart, the caller's
    /// `size_of::<AltarInfo>()`; the loader fills what both sides know of
    /// each and its own [`Header`]. Any thread; empty while no world runs.
    /// [`Status::REFUSED`] for a mod that did not open altars. Since minor 15.
    pub altars: extern "system" fn(
        me: ModHandle,
        out: *mut AltarInfo,
        entry_size: u32,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Makes the mod the one that sizes altar zones, through
    /// [`HostApi::set_altar_zones`]; it opens altars too. One mod at a time:
    /// [`Status::REFUSED`] when another holds it. Allowed during
    /// `enshwick_mod_init` only. Since minor 15.
    pub altar_zones_control: extern "system" fn(me: ModHandle) -> Status,
    /// Replaces the sizes the loader gives altar zones with the `count`
    /// sizes at `sizes`; an altar without one keeps its vanilla size. Any
    /// thread; the server's next zone pass writes them. [`Status::REFUSED`]
    /// for a mod that does not control altar zones. Since minor 15.
    pub set_altar_zones:
        extern "system" fn(me: ModHandle, sizes: *const AltarZoneSize, count: usize) -> Status,
    /// Calls `callback` with `user` when the game refuses a placement whose
    /// box does not lie in one building zone; any filter returning 1 accepts
    /// it after all. Runs on both sides, since a client predicts placement.
    /// Allowed during `enshwick_mod_init` only. Since minor 15.
    pub zone_check_filter:
        extern "system" fn(me: ModHandle, callback: ZoneCheckFn, user: *mut c_void) -> Status,
    /// Makes `callback` with `user` the answer to the game's survivor
    /// lookups that [`SurvivorLookup`](super::SurvivorLookup) names; one mod at a time
    /// ([`Status::REFUSED`] for another). Runs on the server's worker
    /// threads, in the dedicated server or a hosting client, and for
    /// [`SurvivorLookup::CURSOR`](super::SurvivorLookup::CURSOR) on a client alone too. Allowed during
    /// `enshwick_mod_init` only; [`Status::MISSING`] on a build that lacks a
    /// piece. Since minor 16.
    pub survivor_lookup:
        extern "system" fn(me: ModHandle, callback: SurvivorLookupFn, user: *mut c_void) -> Status,
    /// Calls `callback` with `user` after a survivor's own bed check kept its
    /// bed; any filter returning 0 clears it. Server's worker threads.
    /// Allowed during `enshwick_mod_init` only. Since minor 16.
    pub survivor_bed_check:
        extern "system" fn(me: ModHandle, callback: BedCheckFn, user: *mut c_void) -> Status,
    /// Calls `callback` with `user` before a summoning staff spawns or moves
    /// a survivor; any filter returning 1 makes the use take the item.
    /// Server's game thread. Allowed during `enshwick_mod_init` only. Since
    /// minor 16.
    pub summon_filter:
        extern "system" fn(me: ModHandle, callback: SummonFn, user: *mut c_void) -> Status,
    /// Files item `item` as a summoning staff of the survivor `survivor`, so
    /// the game treats it as that survivor's staff, on both sides. Allowed
    /// during `enshwick_mod_init` only. Since minor 16.
    pub summon_item: extern "system" fn(me: ModHandle, item: u32, survivor: u32) -> Status,
    /// Makes `callback` with `user` merge the statuses the client's survivor
    /// menu shows for a survivor with several entities. Client only; one mod
    /// at a time. Allowed during `enshwick_mod_init` only. Since minor 16.
    pub survivor_status:
        extern "system" fn(me: ModHandle, callback: SurvivorStatusFn, user: *mut c_void) -> Status,
    /// Calls `callback` with `user` before a player's interaction that
    /// [`Interaction`](super::Interaction) names; any filter returning 0 refuses it. Runs in the
    /// server's world only, on the thread of the system that checks it.
    /// Allowed during `enshwick_mod_init` only; [`Status::MISSING`] on a
    /// build that lacks a piece. Since minor 17.
    pub interaction_filter:
        extern "system" fn(me: ModHandle, callback: InteractionFn, user: *mut c_void) -> Status,
    /// Calls `callback` with `user` when the game asks whether a character
    /// unlocked skill node `node`; the first filter that answers decides.
    /// Runs on both sides, on the asking thread. Allowed during
    /// `enshwick_mod_init` only. Since minor 18.
    pub skill_override: extern "system" fn(
        me: ModHandle,
        node: u32,
        callback: SkillCheckFn,
        user: *mut c_void,
    ) -> Status,
    /// Calls `callback` with `user` when a comfort provider registers its
    /// comfort; filters run in load order, each on the last one's value.
    /// Server's worker threads. Allowed during `enshwick_mod_init` only.
    /// Since minor 18.
    pub comfort_filter:
        extern "system" fn(me: ModHandle, callback: ComfortFn, user: *mut c_void) -> Status,
    /// Calls `callback` with `user` before the server applies the
    /// characters' gained experience, once per batch; filters run in load
    /// order. Server's worker threads. Allowed during `enshwick_mod_init`
    /// only. Since minor 18.
    pub experience_filter:
        extern "system" fn(me: ModHandle, callback: ExperiencePassFn, user: *mut c_void) -> Status,
    /// Makes `callback` with `user` decide how many altars each Flame level
    /// allows, on both sides; one mod at a time. Allowed during
    /// `enshwick_mod_init` only. Since minor 19.
    pub altar_limit:
        extern "system" fn(me: ModHandle, callback: AltarLimitFn, user: *mut c_void) -> Status,
    /// Lets the game group 32 altar zones into its 16 bases instead of 16
    /// zones, refusing an altar that would start a 17th base; the patches
    /// count as the mod's own. [`Status::MISSING`] when this build's code is
    /// not the one the loader knows: the game keeps 16. Allowed during
    /// `enshwick_mod_init` only. Since minor 19.
    pub raise_base_limit: extern "system" fn(me: ModHandle) -> Status,
    /// Writes how many altar zones and bases the game's last grouping found;
    /// [`Status::MISSING`] before the first one or without the raised limit.
    /// Any thread. Since minor 19.
    pub base_zones: extern "system" fn(me: ModHandle, zones: *mut u32, bases: *mut u32) -> Status,
    /// Calls `callback` with `user` on every game event of the reflected
    /// type whose hash is `type_hash`, or on every game event for 0, on
    /// the server's game thread before the systems of the next step run.
    /// Allowed during `enshwick_mod_init` only. Since minor 20.
    pub game_event: extern "system" fn(
        me: ModHandle,
        type_hash: u32,
        callback: GameEventFn,
        user: *mut c_void,
    ) -> Status,
    /// Up to `capacity` entities of the server's world that have a component
    /// of the reflected type `type_hash` and a place in the world, and how
    /// many there are. On the server's game thread only. Since minor 21.
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
    /// game thread only. Since minor 21.
    pub destroy: extern "system" fn(me: ModHandle, entity: u32) -> Status,
    /// Asks the server to save the world now, as its timed saves do;
    /// [`Event::WORLD_SAVED`] follows once the save is on disk. A save under
    /// way counts as asked; [`Status::REFUSED`] while the world may not be
    /// saved, as before it has loaded. On the server's game thread only.
    /// Since minor 21.
    pub save_world: extern "system" fn(me: ModHandle) -> Status,
    /// Writes the key of the world the server runs: its save file's name and
    /// a hash of its folder, `[a-z0-9_-]`, valid for the process.
    /// [`Status::MISSING`] before the server has read or written the world.
    /// Any thread. Since minor 21.
    pub world_key: extern "system" fn(me: ModHandle, out: *mut Str) -> Status,
    /// A writable pointer to `entity`'s component of the reflected type
    /// `type_hash`, valid until the game thread returns to the game; the
    /// loader marks the component changed, as the game's own writes do, so
    /// the game sends it to clients and saves it. [`Status::REFUSED`] for a
    /// copy the game keeps for clients (`Network*`, `Client*`), which it
    /// overwrites; [`Status::MISSING`] when the entity lacks the component.
    /// On the server's game thread only. Since minor 22.
    pub component_write: extern "system" fn(
        me: ModHandle,
        entity: u32,
        type_hash: u32,
        out: *mut *mut c_void,
    ) -> Status,
    /// Raises the game event at `data`, `size` bytes of the reflected type
    /// `type_hash`, its whole layout; the loader stamps it with the game
    /// time and the systems of the next step see it. Only types the loader
    /// allows, which the server's own systems raise: [`Status::REFUSED`]
    /// for any other or when the step's stream is full,
    /// [`Status::BAD_ARGS`] for a size that is not the type's. On the
    /// server's game thread only. Since minor 23.
    pub emit:
        extern "system" fn(me: ModHandle, type_hash: u32, data: *const u8, size: u32) -> Status,
    /// Asks the server to move an entity as its admin teleport does, after
    /// the next step; [`Status::MISSING`] for an entity not in the world,
    /// [`Status::REFUSED`] for one the game's teleport does not move, as a
    /// prop: it moves actors and characters.
    /// On the server's game thread only. Since minor 23.
    pub teleport: extern "system" fn(me: ModHandle, request: *const TeleportRequest) -> Status,
    /// Asks the server to spawn an entity from a template, as its admin
    /// spawn does, and writes the new entity's id to `out`; the entity is in
    /// the world after the next step. [`Status::BAD_ARGS`] for a zero
    /// template; a template the game does not know spawns nothing. On the
    /// server's game thread only. Since minor 24.
    pub spawn:
        extern "system" fn(me: ModHandle, request: *const SpawnRequest, out: *mut u32) -> Status,
    /// Calls `callback` with `user` on each player's craft request in the
    /// server's world, on the game thread before the crafting system reads
    /// it; any filter returning 0 refuses it, and nothing is made or taken.
    /// Allowed during `enshwick_mod_init` only. Since minor 25.
    pub craft_filter:
        extern "system" fn(me: ModHandle, callback: CraftCheckFn, user: *mut c_void) -> Status,
    /// Calls `callback` with `user` after the game's cursor check allows a
    /// placement; any filter returning 0 refuses it as the game refuses a
    /// placement without permission. Runs on both sides, on the cursor
    /// systems' worker threads: the server's check decides, a client's
    /// shows. Allowed during `enshwick_mod_init` only. Since minor 25.
    pub build_filter:
        extern "system" fn(me: ModHandle, callback: BuildCheckFn, user: *mut c_void) -> Status,
    /// Opens the mod's channel `name`, 1 to 64 bytes, between its server
    /// and client parts, and writes its id to `out`; `callback` gets each
    /// message on it with `user`: on the server's game thread, on a client
    /// on the thread that takes the server's chat. Join mods only
    /// ([`Status::REFUSED`] for another), since both ends must run the mod.
    /// Allowed during `enshwick_mod_init` only. Since minor 26.
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
    /// Since minor 26.
    pub net_send: extern "system" fn(
        me: ModHandle,
        channel: u32,
        peer: u64,
        data: *const u8,
        size: u32,
    ) -> Status,
    /// [`Status::OK`] when `peer` takes messages: a player's client that
    /// runs Enshwick, on the server; the server, for [`NetMessage::SERVER`](super::NetMessage::SERVER)
    /// on a client. [`Status::MISSING`] otherwise. Any thread. Since minor
    /// 26.
    pub net_ready: extern "system" fn(me: ModHandle, peer: u64) -> Status,
    /// Shows `text`, cut at 255 bytes, on the screen of the player with
    /// account key `key`, as the game shows its own "can't do that"
    /// messages; [`Status::MISSING`] when the player's client lacks
    /// Enshwick and cannot show it, and nothing is sent. On the server's
    /// game thread only. Since minor 27.
    pub notify: extern "system" fn(me: ModHandle, key: u64, text: Str) -> Status,
    /// Writes the clock of the server's world, as far as the [`Header`]
    /// `out` starts with says the caller's struct reaches. On the server's
    /// game thread only. Since minor 28.
    pub time_of_day: extern "system" fn(me: ModHandle, out: *mut DaytimeInfo) -> Status,
    /// Sets the clock of the server's world to `time`, game nanoseconds of
    /// the day, taken modulo 24 hours; clients follow after the next step.
    /// On the server's game thread only. Since minor 28.
    pub set_time_of_day: extern "system" fn(me: ModHandle, time: i64) -> Status,
    /// Lets the mod change how fast the clock runs, with
    /// [`HostApi::set_time_speed`]. Allowed during `enshwick_mod_init` only.
    /// Since minor 28.
    pub time_speed_control: extern "system" fn(me: ModHandle) -> Status,
    /// Sets the mod's factor on the clock's speed, from 0 (stopped) to 1000;
    /// the clock runs at the game's own speed times every mod's factor.
    /// [`Status::REFUSED`] for a mod without
    /// [`HostApi::time_speed_control`], [`Status::BAD_ARGS`] for a factor
    /// out of range. On the server's game thread only. Since minor 28.
    pub set_time_speed: extern "system" fn(me: ModHandle, factor: f32) -> Status,
    /// Copies the server's live game settings, `keen::ecs::GameSettings`,
    /// into `out`, `size` bytes, which must be the type's reflected size
    /// ([`Status::BAD_ARGS`] otherwise), and writes the preset the server
    /// runs (`GameSettingsPreset`) to `preset`. On the server's game thread
    /// only. Since minor 29.
    pub game_settings:
        extern "system" fn(me: ModHandle, out: *mut u8, size: u32, preset: *mut u32) -> Status,
    /// Replaces the server's live game settings with `data`, the type's
    /// whole reflected layout, as the game's admin command does: systems
    /// see them from the next step and every client gets them; the preset
    /// becomes custom. Until the server stops: nothing writes them to disk.
    /// [`Status::BAD_ARGS`] for a wrong size or a day or night length of
    /// zero or less. On the server's game thread only. Since minor 29.
    pub set_game_settings: extern "system" fn(me: ModHandle, data: *const u8, size: u32) -> Status,
    /// Calls `callback` with `user` on damage about to reach an entity's
    /// health in the server's world, before the game applies it: hits,
    /// falls and the shroud; what the filters leave in the
    /// [`DamageCheck`](super::DamageCheck) is what the game applies. Runs on the game's worker
    /// threads, in load order. Allowed during `enshwick_mod_init` only.
    /// Since minor 30.
    pub damage_filter:
        extern "system" fn(me: ModHandle, callback: DamageFilterFn, user: *mut c_void) -> Status,
    /// Up to `capacity` buff and debuff types of the running game, and how
    /// many there are. On the server's game thread only. Since minor 31.
    /// Every entry is as large as the header of the first one says.
    pub buff_types: extern "system" fn(
        me: ModHandle,
        out: *mut BuffTypeInfo,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Up to `capacity` buffs `entity` has, and how many there are;
    /// [`Status::MISSING`] for an entity that holds no buffs. On the
    /// server's game thread only. Since minor 31.
    /// Every entry is as large as the header of the first one says.
    pub buffs: extern "system" fn(
        me: ModHandle,
        entity: u32,
        out: *mut ActiveBuff,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Gives an entity a buff, as the game restores a character's saved
    /// buffs: at once, for the duration asked. One the entity has already
    /// is renewed to that duration. [`Status::MISSING`] for an entity that
    /// holds no buffs, [`Status::BAD_ARGS`] for an unknown type,
    /// [`Status::REFUSED`] for a type removed in this step, which the game
    /// clears only in the next. On the server's game thread only. Since
    /// minor 31.
    pub apply_buff: extern "system" fn(me: ModHandle, request: *const BuffRequest) -> Status,
    /// Removes every buff of type `id` from `entity`, as the game cleanses
    /// one: it goes in the next step. Writes 1 to `removed` when the entity
    /// had it, else 0; [`Status::MISSING`] for an entity that holds no
    /// buffs. On the server's game thread only. Since minor 31.
    pub remove_buff:
        extern "system" fn(me: ModHandle, entity: u32, id: u32, removed: *mut u8) -> Status,
    /// Writes knowledge `id` of the player whose character is `entity`, or
    /// of the world for 0 or for an id of the world's;
    /// [`Status::MISSING`] for an id the game does not know,
    /// [`Status::REFUSED`] for a player not in the game. On the server's
    /// game thread only. Since minor 32.
    pub knowledge:
        extern "system" fn(me: ModHandle, entity: u32, id: u32, out: *mut KnowledgeInfo) -> Status,
    /// Grants knowledge `id`, of `kind`, to the player whose character is
    /// `entity`, or to the world for 0, as the game grants it: the next
    /// step applies it with what follows (experience, unlocks, clients,
    /// the save). `value` is ignored for [`KnowledgeInfo::BOOL`], added for
    /// [`KnowledgeInfo::COUNTER`], the bit to set for
    /// [`KnowledgeInfo::FLAGS`]. [`Status::BAD_ARGS`] for an unknown id,
    /// a kind that is not the id's or a bit the game does not expect,
    /// [`Status::REFUSED`] for a player not in the
    /// game or a full queue. On the server's game thread only. Since minor
    /// 32.
    pub grant_knowledge:
        extern "system" fn(me: ModHandle, entity: u32, id: u32, kind: u32, value: u32) -> Status,
    /// Up to `capacity` quest ids of the running game, and how many there
    /// are. On the server's game thread only. Since minor 32.
    pub quests:
        extern "system" fn(me: ModHandle, out: *mut u32, capacity: u32, count: *mut u32) -> Status,
    /// Writes how clear, rainy, snowy and stormy it is at `position`, in
    /// 32.32 fixed-point metres, as weights that add up to 1, as the game's
    /// systems weigh it. On the server's game thread only. Since minor 33.
    pub weather_at:
        extern "system" fn(me: ModHandle, position: *const [i64; 3], out: *mut [f32; 4]) -> Status,
    /// Up to `capacity` current weather fronts of the server's world, one per
    /// layer that has one, and how many there are. On the server's game
    /// thread only. Since minor 33.
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
    /// follow. On the server's game thread only. Since minor 33.
    pub set_weather: extern "system" fn(me: ModHandle, effect: u32, duration: i64) -> Status,
    /// Up to `capacity` terrain materials of the running game, and how many
    /// there are. On the server's game thread only. Since minor 34.
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
    /// [`Status::BAD_ARGS`] for an edit out of range. On the server's game
    /// thread only. Since minor 34.
    pub terrain_edit:
        extern "system" fn(me: ModHandle, edit: *const TerrainEdit, amount: *mut f32) -> Status,
    /// Hands the loader the mod's settings schema, `schema`, the TOML text
    /// of a [`config::Schema`](crate::config::Schema); a later call replaces
    /// it. The loader answers `enshwick config` on the console from it and checks values against it
    /// before it writes them. [`Status::BAD_ARGS`] for text that is not a
    /// schema. Any time. Since minor 35.
    pub config_schema: extern "system" fn(me: ModHandle, schema: Str) -> Status,
    /// Calls `callback` with `user` after a summoning staff's use spawned or
    /// moved a survivor, with [`Summon::consumed`](super::Summon::consumed) set; its return value is
    /// ignored. On the thread of the use, as [`HostApi::summon_filter`].
    /// Allowed during `enshwick_mod_init` only. Since minor 37.
    pub summoned:
        extern "system" fn(me: ModHandle, callback: SummonFn, user: *mut c_void) -> Status,
    /// Opens the item data kind `id`, `name` for the mod's own
    /// (`<mod>:<name>`) or `mod:name` for any mod's, each side 1 to 64 of
    /// `[a-z0-9_-]`, and writes its index to `out`; every mod gets the same
    /// index for one id. Allowed during `enshwick_mod_init` only. Since
    /// minor 38.
    pub item_data_open: extern "system" fn(me: ModHandle, id: Str, out: *mut u32) -> Status,
    /// Copies the value of kind `kind` on the item entity `pide` holding
    /// item `item` into `out`, at most `capacity` bytes, and writes its
    /// length to `len`; [`Status::MISSING`] when it has none or its entity
    /// holds another item, [`Status::BAD_ARGS`] when `capacity` is short,
    /// with `len` written. On the server the server's values, on a client
    /// what the server sent. Any thread. Since minor 38.
    pub item_data_get: extern "system" fn(
        me: ModHandle,
        kind: u32,
        pide: u32,
        item: u32,
        out: *mut u8,
        capacity: u32,
        len: *mut u32,
    ) -> Status,
    /// Sets the value of kind `kind` on the item entity `pide` holding item
    /// `item` to `len` bytes at `data`, at most 512, or removes it for 0;
    /// the clients that run Enshwick get the change. [`Status::REFUSED`]
    /// on a client alone, [`Status::MISSING`] before the world has loaded.
    /// Any thread. Since minor 38.
    pub item_data_set: extern "system" fn(
        me: ModHandle,
        kind: u32,
        pide: u32,
        item: u32,
        data: *const u8,
        len: u32,
    ) -> Status,
    /// Calls `callback` with `user` once per frame of the client's
    /// interface, on the thread that draws it, while the mod is on; the
    /// callback draws with [`HostApi::ui_widget`]. Client only
    /// ([`Status::MISSING`] on a server); when the interface could not be
    /// linked, the callback never runs. Allowed during `enshwick_mod_init`
    /// only. Since minor 39.
    pub ui_frame:
        extern "system" fn(me: ModHandle, callback: UiFrameFn, user: *mut c_void) -> Status,
    /// Draws `widget` in the frame `frame` names and writes back its
    /// `index`, `value` and `result`. Only inside the mod's frame callback
    /// ([`Status::WRONG_THREAD`] elsewhere); [`Status::REFUSED`] for a
    /// widget outside a window or an [`UiWidget::END`] without one;
    /// [`Status::MISSING`] for a widget this game build lacks. Since minor
    /// 39.
    pub ui_widget: extern "system" fn(me: ModHandle, frame: u64, widget: *mut UiWidget) -> Status,
    /// Queues `dialog` and calls `callback` with the answer and `user` once
    /// the player gives it; dialogs show one at a time, in order. Any
    /// thread. Client only ([`Status::MISSING`] on a server); when the
    /// interface could not be linked, the dialog shows as a system message
    /// box. [`Status::REFUSED`] when too many wait; [`Status::WRONG_THREAD`]
    /// for [`UiDialog::WAITING`] on the thread that draws the interface. On
    /// an error status `callback` is never called. Since minor 39.
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
    /// `enshwick_mod_init` only. Since minor 40.
    pub ui_hud: extern "system" fn(me: ModHandle, callback: UiFrameFn, user: *mut c_void) -> Status,
    /// Replaces the mod's markers on the client's map with the `count` at
    /// `markers`, copied; none clears them. Any thread. Client only
    /// ([`Status::MISSING`] on a server); [`Status::BAD_ARGS`] past 256
    /// markers. Since minor 40.
    /// Every entry is as large as the header of the first one says.
    pub ui_markers:
        extern "system" fn(me: ModHandle, markers: *const UiMarker, count: u32) -> Status,
    /// Makes `callback` with `user` the mod's check of a new settings text,
    /// which the loader runs before it writes a change from the console or the
    /// Mods screen; a refusal keeps the file. Allowed during
    /// `enshwick_mod_init` only. Since minor 41.
    pub config_check:
        extern "system" fn(me: ModHandle, callback: ConfigCheckFn, user: *mut c_void) -> Status,
    /// Hands the loader the mod's texts, a TOML table per language code as
    /// [`crate::i18n::Texts`] reads them, for the loader's own screens: the
    /// Mods screen labels the mod's settings with them. A later call
    /// replaces them. Since minor 43.
    pub texts: extern "system" fn(me: ModHandle, toml: Str) -> Status,
    /// Serves `len` bytes at `data` as game content, the pixels of a
    /// texture the mod serves, and writes the content hash a resource names
    /// them by to `out`; the same bytes give the same hash. The loader
    /// keeps the bytes for the life of the process. Any thread. Since minor
    /// 44.
    pub content_serve:
        extern "system" fn(me: ModHandle, data: *const u8, len: u32, out: *mut [u32; 4]) -> Status,
    /// Copies the game's own content `hash` from its archive into `out`, at
    /// most `capacity` bytes, and writes its length to `len`;
    /// [`Status::MISSING`] for a hash the archive lacks or keeps
    /// compressed, [`Status::BAD_ARGS`] when `capacity` is short, with
    /// `len` written. Reads the archive's files, never the game's memory.
    /// Any thread. Since minor 44.
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
    /// thread. Since minor 44.
    pub icons_ready: extern "system" fn(me: ModHandle) -> Status,
    /// Defines the stat kind `id` with `def`, or opens it with a null
    /// `def`, and writes its index and key to `out`. A mod defines kinds in
    /// its own namespace (`name` or `<mod>:name`); any mod opens any kind,
    /// the loader's built-in ones (`enshwick:<name>`) among them. Every mod
    /// gets the same index for one id. [`Status::BAD_ARGS`] for a malformed
    /// id or definition, [`Status::REFUSED`] for a kind defined twice or in
    /// another mod's namespace. Allowed during `enshwick_mod_init` only.
    /// Since minor 45.
    pub stat_open: extern "system" fn(
        me: ModHandle,
        id: Str,
        def: *const StatDefData,
        out: *mut StatRef,
    ) -> Status,
    /// Writes the definition of stat `stat` to `def` and its full id to
    /// `id`, a string the loader keeps for the process; [`Status::MISSING`]
    /// for a kind opened but never defined. Any thread. Since minor 45.
    pub stat_info:
        extern "system" fn(me: ModHandle, stat: u32, def: *mut StatDefData, id: *mut Str) -> Status,
    /// Writes the value of stat `stat` on `entity` to `out`: its default
    /// when nothing modifies it. Values are worked out on the server after
    /// each step a source changed; a client answers defaults. Any thread,
    /// without locks. Since minor 45.
    pub stat_value:
        extern "system" fn(me: ModHandle, entity: u32, stat: u32, out: *mut f32) -> Status,
    /// Sets or removes a modifier the mod keeps on an entity; the value
    /// follows after the next step. [`Status::REFUSED`] on a client alone,
    /// or for a saved modifier on an entity that is no player. Any thread.
    /// Since minor 45.
    pub stat_modify: extern "system" fn(me: ModHandle, request: *const StatModify) -> Status,
    /// Gives the buff type `buff` the `count` modifiers at `modifiers`:
    /// they count toward every watched entity that has the buff. Allowed
    /// during `enshwick_mod_init` only. Since minor 45.
    pub stat_buff: extern "system" fn(
        me: ModHandle,
        buff: u32,
        modifiers: *const StatModifierData,
        count: u32,
    ) -> Status,
    /// Calls `callback` with `user` for every stat value that changed on a
    /// watched entity, on the game thread after the step that changed it.
    /// Allowed during `enshwick_mod_init` only. Since minor 45.
    pub stats_changed:
        extern "system" fn(me: ModHandle, callback: StatChangeFn, user: *mut c_void) -> Status,
    /// Writes up to `capacity` modifiers that count toward stat `stat` on
    /// `entity`, in the order they apply, and how many there are to
    /// `count`; every stat for `u32::MAX`. Any thread. Since minor 45.
    pub stat_sources: extern "system" fn(
        me: ModHandle,
        entity: u32,
        stat: u32,
        out: *mut StatSource,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Watches `entity` for the items it wears and the buffs it has, or
    /// stops for `watch` 0: players are always watched, and so is every
    /// entity a mod keeps a modifier on. Any thread. Since minor 45.
    pub stat_watch: extern "system" fn(me: ModHandle, entity: u32, watch: u8) -> Status,
    /// Defines the mod event `spec` describes, named `<id>:<name>` after
    /// the defining mod, and writes the id [`HostApi::send_event`] takes;
    /// [`Status::REFUSED`] for a name outside the mod's id or defined
    /// twice. Allowed during `enshwick_mod_init` only. Since minor 47.
    pub define_event:
        extern "system" fn(me: ModHandle, spec: *const EventSpec, out: *mut u32) -> Status,
    /// Calls `callback` with `user` and the event's data on every mod event
    /// `spec` names, once its definition, checked against `spec`'s layout
    /// when every mod is initialised, matches. Allowed during
    /// `enshwick_mod_init` only. Since minor 47.
    pub subscribe_event: extern "system" fn(
        me: ModHandle,
        spec: *const EventSpec,
        callback: ModEventFn,
        user: *mut c_void,
    ) -> Status,
    /// Hands `data` to every subscriber of the mod's own event `event`, on
    /// this thread, before it returns; reaches no one before every mod is
    /// initialised. [`Status::REFUSED`] for an event another mod defined.
    /// Any thread. Since minor 47.
    pub send_event: extern "system" fn(me: ModHandle, event: u32, data: *const c_void) -> Status,
    /// Defines the affix `id` in the mod's namespace with `def` and writes
    /// its index and key to `out`. Allowed during `enshwick_mod_init` only.
    /// Since minor 48.
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
    /// Since minor 48.
    pub affix_roll:
        extern "system" fn(me: ModHandle, roll: *const AffixRoll, count: *mut u32) -> Status,
    /// Writes up to `capacity` lines of what the item gives, its own
    /// modifiers first and then each affix's, and how many to `count`. On a
    /// client, from what the server sent. Any thread. Since minor 48.
    pub item_lines: extern "system" fn(
        me: ModHandle,
        pide: u32,
        item: u32,
        out: *mut ItemLine,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Writes the text key and rarity of affix `affix` (an index) to `text`
    /// and `rarity`. Any thread. Since minor 48.
    pub affix_info:
        extern "system" fn(me: ModHandle, affix: u32, text: *mut Str, rarity: *mut u8) -> Status,
    /// Registers the mod's migrations, `set`, copied: the loader runs the
    /// settings steps on the settings file now, and every step on each
    /// world's files when the world loads, once per world, and records what
    /// ran in `mod-data/enshwick/migrations.toml`. [`Status::REFUSED`] for
    /// a second call or a migration that failed on the settings file, with
    /// the reason logged. Allowed during `enshwick_mod_init` only. Since
    /// minor 49.
    pub migrate: extern "system" fn(me: ModHandle, set: *const MigrationSet) -> Status,
    /// Declares the content id `id`, `<mod>:<name>` in the mod's own
    /// namespace, and writes its GUID to `out`; [`Status::REFUSED`] for a
    /// malformed id, one outside the namespace, or one whose game id
    /// another content id has, which turns the mod off with both ids and
    /// both mods named. Allowed during `enshwick_mod_init` only. Since
    /// minor 50.
    pub declare_content: extern "system" fn(me: ModHandle, id: Str, out: *mut [u8; 16]) -> Status,
    /// Opens the entity data kind `id`, as [`HostApi::item_data_open`] opens
    /// an item kind, and writes its index to `out`; with
    /// [`EntityDataFlags::SENT`] the clients that run Enshwick get its
    /// values. [`HostApi::item_data_get`] and [`HostApi::item_data_set`]
    /// read and write it with `item` 0; a set runs on the server's game
    /// thread. [`Status::REFUSED`] for an id opened as an item kind. Allowed
    /// during `enshwick_mod_init` only. Since minor 52.
    pub entity_data_open:
        extern "system" fn(me: ModHandle, id: Str, flags: EntityDataFlags, out: *mut u32) -> Status,
    /// Lists the mods in `mods/` that a present mod replaces (see
    /// [`Relation::REPLACES`](super::Relation::REPLACES)), which the loader never started, as
    /// [`HostApi::mods`] lists the loaded ones, with
    /// [`ModInfo::replaced_by`] set. Any thread. Since minor 54.
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
    /// only. Since minor 55.
    pub knowledge_input_filter:
        extern "system" fn(me: ModHandle, callback: KnowledgeInputFn, user: *mut c_void) -> Status,
    /// The game language the client last asked a text in, as the game
    /// numbers them (see [`crate::i18n::Language`]); `0`, the fallback, on a
    /// server and before the client's first text. Any thread. Since minor
    /// 56.
    pub language: extern "system" fn(me: ModHandle) -> u32,
    /// Replaces the live game settings with `data` as
    /// [`HostApi::set_game_settings`] does, and keeps the fields with a
    /// byte set in `mask` (as long as `data`) with the running world: the
    /// loader applies them again each time the world loads, by the end of
    /// its first step. [`Status::MISSING`] while no world runs.
    /// On the server's game thread only. Since minor 57.
    pub keep_game_settings:
        extern "system" fn(me: ModHandle, data: *const u8, mask: *const u8, size: u32) -> Status,
    /// Forgets the game settings kept for the running world and puts back
    /// the values it loaded with. On the server's game thread only. Since
    /// minor 57.
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
    /// Since minor 59.
    pub revoke_knowledge:
        extern "system" fn(me: ModHandle, entity: u32, id: u32, kind: u32, value: u32) -> Status,
    /// Grants knowledge `id` of a player's, as [`HostApi::grant_knowledge`]
    /// does, or takes it back as [`HostApi::revoke_knowledge`] does when
    /// `revoke` is 1, for the account whose [`PlayerInfo::key`] is `key`:
    /// now when its character is in the game, else once its next character
    /// has loaded in this world; the loader keeps waiting changes with the
    /// world. [`Status::BAD_ARGS`] for an unknown id, an id of the world's
    /// or a kind that is not the id's, [`Status::MISSING`] while no world
    /// runs. On the server's game thread only. Since minor 59.
    pub account_knowledge: extern "system" fn(
        me: ModHandle,
        key: u64,
        id: u32,
        kind: u32,
        value: u32,
        revoke: u32,
    ) -> Status,
    /// Copies the game's own resource `key` from its archive into `out`, at
    /// most `capacity` bytes, as the game reads it before it resolves its
    /// references, and writes its length to `len`; [`Status::MISSING`] for
    /// a key the archive lacks, [`Status::BAD_ARGS`] when `capacity` is
    /// short, with `len` written. Reads the archive's files, never the
    /// game's memory. Any thread. Since minor 61.
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
    /// thread. Since minor 61.
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
    /// Since minor 63.
    pub rebuild_resource: extern "system" fn(me: ModHandle, key: *const ResourceKey) -> Status,
    /// Adds the calling mod's rule that disables recipes (`disable` 1), or
    /// takes its own rule of that kind and name back (`disable` 0). `name`
    /// names a recipe, an item every recipe of which is disabled, or a
    /// workshop, as `kind` says, in the forms the loader's recipe file
    /// takes; the loader resolves it against the recipe registry and logs a
    /// name it cannot resolve. The server's recipe file wins over every
    /// mod's rules. Any thread; applied within a second. Since minor 64.
    pub recipe_rule:
        extern "system" fn(me: ModHandle, kind: RecipeRuleKind, name: Str, disable: u32) -> Status,
    /// Writes 1 to `out` when the recipe with game id `recipe` is disabled
    /// in this process now, else 0: on a client joined to a server, by the
    /// server's rules. Any thread. Since minor 64.
    pub recipe_disabled: extern "system" fn(me: ModHandle, recipe: u32, out: *mut u32) -> Status,
    /// Defines the extension point `spec` describes, named
    /// `<id>:<point>` after the defining mod, and writes its id and the
    /// count of its live links to `out`; [`Status::REFUSED`] for a name
    /// outside the mod's id or defined twice. Allowed during
    /// `enshwick_mod_init` only. Since minor 65.
    pub define_point:
        extern "system" fn(me: ModHandle, spec: *const PointSpec, out: *mut PointHandle) -> Status,
    /// Attaches `callback` with `user` to the extension point `link`
    /// names, in the order it asks for. Matched against the point's
    /// definition once every mod is initialised; a point no running mod
    /// defines, or one whose layout or mode differs, leaves the link inert.
    /// Allowed during `enshwick_mod_init` only. Since minor 65.
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
    /// Since minor 65.
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
    /// `enshwick_mod_init` only. Since minor 66.
    pub actions: extern "system" fn(me: ModHandle, callback: ActionFn, user: *mut c_void) -> Status,
    /// Writes the first `capacity` of the running build's game event types
    /// to `out`, null or room for `capacity` entries each as large as the
    /// header of the first one says, and how many there are to `count`:
    /// every reflected type that derives from the engine's event base, in no
    /// set order. The names are the loader's, valid for the rest of the
    /// process. A null `count` gives [`Status::BAD_ARGS`] after the entries
    /// are written; a handle the loader never gave out gives
    /// [`Status::BAD_ARGS`] and a mod that is off [`Status::FAULTED`], with
    /// nothing written. Any thread. Since minor 72.
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
    /// resource. Any thread. Since minor 73.
    pub recipe_kind: extern "system" fn(me: ModHandle, item: u32, out: *mut RecipeKind) -> Status,
}
