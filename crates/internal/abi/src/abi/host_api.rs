// Long file: one `repr(C)` table whose fields keep their ABI order.
//! The table of functions the loader hands every mod.

use core::ffi::c_void;

use crate::abi::{
    AccountActionType, ActionFn, ActionReply, ActionTarget, ActiveBuff, AffixRoll, AffixSpec,
    AltarInfo, AltarLimitFilterFn, AltarZoneFilterFn, BaseLimitInfo, BedCheckFn, BuffRequest,
    BuffTypeInfo, BuildFilterFn, BuildInfo, BuildRights, BuildingZoneFilterFn, CallFn, CallSpec,
    CallerFrame, ChangeId, ChangeOp, ChatFilterFn, ClientCommand, ClientCommandStatus,
    ClientSnapshot, ComfortFilterFn, CommandInfo, ConfigCheckFn, ContentRename, ControlFn,
    ControlSpec, CraftFilterFn, DamageFilterFn, DataChange, DaytimeInfo, DropRequest,
    EntityDataFlags, EntityPlace, Event, EventFn, EventSpec, EventTypeInfo, ExperienceFilterFn,
    FogInfo, GameEventFn, Header, HookSpec, InteractionFilterFn, InteractionPrompt,
    InventoryRequest, InventoryResultFn, ItemFilterFn, ItemLine, ItemTypeInfo, JoinRuleFn,
    KnowledgeChange, KnowledgeInfo, KnowledgeInputFilterFn, Level, LoaderOff, LocalInputInfo,
    LoopWaitFn, LoopWaitSites, MapFrameInfo, MapMarkerInfo, MapMarkerTypeInfo, MidHookSpec,
    ModEventFn, ModHandle, ModInfo, NetMessageFn, PlayerInfo, PointFn, PointHandle, PointLink,
    PointSpec, PointVerdict, RateId, RecipeKind, RecipeRuleKind, RenameFilterFn, ResourceKey,
    ResourcePatchFn, SaveInfo, ServerWaitFn, ServiceFound, ServiceSpec, SkillCheckFn, SpawnRequest,
    StatChangeFn, StatModifier, StatModify, StatRef, StatSource, StatSpec, Status, StatusFn, Str,
    SummonFilterFn, SurvivorLookupFn, SurvivorStatusFn, TeleportRequest, TerrainEdit,
    TerrainMaterialAdd, TerrainMaterialInfo, TickHandle, TransactionInfo, TransactionOp,
    TransactionSettledFn, TypeEntry, UiAnswerFn, UiDialog, UiExtension, UiFrameFn, UiMarker,
    UiTooltipFn, UiWidget, Voxel, WeatherFront, WeatherSample,
};
use crate::abi::{BuffTypeId, Entity, ItemId, KnowledgeId, Metres, NpcId, PlayerKey, RecipeId};
use crate::content::Guid;

/// The table of services the loader offers a mod.
///
/// A function appended at its end is a new
/// [`ABI_MINOR`](crate::abi::ABI_MINOR), any other change a new
/// [`ABI_MAJOR`](crate::abi::ABI_MAJOR). A mod starts only on a table of its
/// major and its minor or a newer one, whose header's `size` covers it whole.
///
/// Every list answers the same way. The caller passes `out`, room for
/// `capacity` entries `entry_size` bytes apart, the size of the entry as it
/// knows it, at least a [`Header`] for a headed entry; the loader fills what
/// both sides know of each entry, its own header in each, and writes how
/// many there are to `count`. A null `out` with `capacity` 0 asks only for
/// the count. A null `count`, an `entry_size` shorter than the entry's first
/// version or a null `out` with a nonzero `capacity` is
/// [`Status::BAD_ARGS`], with nothing written.
///
/// A list of plain values (ids, hashes, keys, an entity and its place, a
/// stat modifier and its source) takes no `entry_size`: its entries are
/// packed, of a type frozen at its size, since a value never gains a field;
/// a value that needs more is a new entry. Otherwise it answers as every list does.
///
/// No entry unwinds into the mod. A panic in the loader's own code answers
/// [`Status::LOADER_FAULT`] and turns that entry off, which then answers
/// [`Status::LOADER_FAULT`] to every mod; a panic in a mod's callback that
/// an entry runs turns that mod off. An entry allowed during
/// `enshwick_mod_init` only answers [`Status::TOO_LATE`] after it.
///
/// Every callback a mod hands the loader, and the `user` pointer beside it,
/// stays registered for the rest of the process: nothing unregisters it, and
/// the loader never frees `user`. A callback must not unwind into the
/// loader: a C++ exception or a `longjmp` out of it is undefined behaviour.
/// Each callback's comment names the threads it runs on; one that names
/// several threads, or worker threads, may run on them at once, so its
/// state needs locks or atomics. A callback may call the entries allowed on
/// its thread; one that sends an event, asks a point or makes a call may
/// see its own callback, or another mod's, run inside that entry.
///
/// Every [`Str`] the loader writes, as an `out` or as a field of an entry
/// it fills, names how long it stays valid: for the call, until the next
/// call on the same thread, or for the process. Text kept for the process
/// is never freed, so a mod asks for it once rather than every frame.
///
/// # Examples
///
/// The safe layer wraps this table; see [`enshwick::Load`](../struct.Load.html).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct HostApi {
    /// Size and version of this struct.
    pub header: Header,
    /// The loader's own SemVer version, valid for the process: a release's,
    /// or a development build's, such as `1.0.0-alpha.15+dev.5f647f46`,
    /// which sorts with the release it follows. Compatibility is the ABI
    /// version's alone ([`Header::abi_major`], [`Header::abi_minor`]), never
    /// this text's.
    pub host_version: Str,
    /// Writes the running game's identity, its side, image base, build
    /// fingerprint and whether the loader found every entry it knows, as far as the
    /// [`Header`] `out` starts with says the caller's struct reaches.
    /// [`Status::BAD_ARGS`] for a null `out`. Any thread.
    pub build: extern "system" fn(me: ModHandle, out: *mut BuildInfo) -> Status,
    /// Resolves a symbolic id for the running build: the mod's own rows first,
    /// then what the loader found. Any thread.
    pub symbol: extern "system" fn(me: ModHandle, name: Str, out: *mut u64) -> Status,
    /// Resolves an ECS system's update function by its reflected name.
    /// [`Status::MISSING`] for a system the running build lacks. Any thread.
    pub system: extern "system" fn(me: ModHandle, name: Str, out: *mut u64) -> Status,
    /// Resolves a field's offset in a reflected type. [`Status::MISSING`]
    /// for a type or field the running build lacks. Any thread.
    pub field_offset:
        extern "system" fn(me: ModHandle, type_name: Str, field: Str, out: *mut u64) -> Status,
    /// Resolves a reflected type's size. [`Status::MISSING`] for a type the
    /// running build lacks. Any thread.
    pub type_size: extern "system" fn(me: ModHandle, type_name: Str, out: *mut u32) -> Status,
    /// Links a detour; allowed during `enshwick_mod_init` only.
    pub hook: extern "system" fn(me: ModHandle, spec: *const HookSpec) -> Status,
    /// Writes the game caller's context at entry of the current chain, for
    /// links with [`HookFlags::CALLER_FRAME`](super::HookFlags::CALLER_FRAME); valid until the link calls
    /// anything hooked; all zero outside a chain. On the thread the chain
    /// runs on.
    pub caller_frame: extern "system" fn(me: ModHandle, out: *mut CallerFrame) -> Status,
    /// Reports that the mod failed at run time; the loader unlinks every link
    /// of the mod and turns it off. Any thread; a mod already off gets
    /// [`Status::OK`] too.
    pub fault: extern "system" fn(me: ModHandle, reason: Str) -> Status,
    /// Writes a line to the log under the mod's id, as long as the mod is
    /// loaded, on or off. [`Status::BAD_ARGS`] for text that is not UTF-8.
    /// Any thread.
    pub log: extern "system" fn(me: ModHandle, level: Level, message: Str) -> Status,
    /// The text of the mod's settings file `file` (its name without
    /// `.toml`) in force: the running world's, which the loader made or
    /// brought in line with the schema [`HostApi::config_schema`] handed it
    /// as the world loaded; on a client in another's world, the values that
    /// world's server sent; empty while no world runs. The text stays valid
    /// for the life of the process, so a thread may read it while another
    /// asks again. [`Status::MISSING`] for a file no schema names.
    pub settings: extern "system" fn(me: ModHandle, file: Str, out: *mut Str) -> Status,
    /// Publishes the service `spec` describes, table and layout, for other
    /// mods; [`Status::REFUSED`] when another mod publishes its name and
    /// major. Allowed during `enshwick_mod_init` only.
    pub publish: extern "system" fn(me: ModHandle, spec: *const ServiceSpec) -> Status,
    /// Finds the service `wanted` describes, its table null, and checks
    /// that the published layout starts with the wanted one; writes the
    /// table and its publisher to `out`, and on [`Status::MISSING`] or
    /// [`Status::REFUSED`] the reason; refused for a service of another loaded
    /// mod whose [`Manifest::exports`](super::Manifest::exports) lack its kind and name. Any thread.
    pub service: extern "system" fn(
        me: ModHandle,
        wanted: *const ServiceSpec,
        out: *mut ServiceFound,
    ) -> Status,
    /// Resolves an engine constant of the running build, such as a field
    /// offset reflection does not describe, by its symbolic id. Any thread.
    pub constant: extern "system" fn(me: ModHandle, name: Str, out: *mut u64) -> Status,
    /// The mod's data folder in the world the server runs,
    /// `<stem>-mod-data/<id>` beside the world's save, created on first use;
    /// the path stays valid for the process. [`Status::UNAVAILABLE`] while
    /// no world runs, as during init. Any thread.
    pub data_dir: extern "system" fn(me: ModHandle, out: *mut Str) -> Status,
    /// The most detailed level kept for the mod's lines; the mod skips
    /// formatting anything more detailed. Any thread, without locks.
    pub log_level: extern "system" fn(me: ModHandle, out: *mut Level) -> Status,
    /// Calls `callback` with `user` on every `event` for the mod, on the
    /// thread that raises it, until the mod is off; allowed during
    /// `enshwick_mod_init` only.
    pub subscribe: extern "system" fn(
        me: ModHandle,
        event: Event,
        callback: Option<EventFn>,
        user: *mut c_void,
    ) -> Status,
    /// The ECS type hash of a reflected component type, by name.
    /// [`Status::MISSING`] for a type the running build lacks. Any thread.
    pub type_hash: extern "system" fn(me: ModHandle, type_name: Str, out: *mut u32) -> Status,
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
    /// The template GUID of `entity`; on the server's game
    /// thread only.
    pub entity_template:
        extern "system" fn(me: ModHandle, entity: Entity, out: *mut Guid) -> Status,
    /// Up to `capacity` loaded mods, the loader first, and how many there
    /// are. Entries are `entry_size` bytes apart, the caller's
    /// `size_of::<ModInfo>()`; the loader fills what both sides know of each
    /// and its own [`Header`]. The strings stay valid for the process, since
    /// no mod unloads before it ends. Any thread.
    pub mods: extern "system" fn(
        me: ModHandle,
        out: *mut ModInfo,
        entry_size: u32,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Writes where the game's code section (`.text`) is loaded and how many
    /// bytes it spans. [`Status::UNAVAILABLE`] when the loader found no code
    /// section. Any thread.
    pub code_range: extern "system" fn(me: ModHandle, start: *mut u64, len: *mut u32) -> Status,
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
        len: u32,
    ) -> Status,
    /// The most recipes one workshop takes on the running game: 512, the
    /// game's own list size, or [`u32::MAX`] once the loader has lifted that
    /// limit at arming. Any thread.
    pub workshop_recipes: extern "system" fn(me: ModHandle, out: *mut u32) -> Status,
    /// Writes 1 while this process runs a game server: always on the
    /// dedicated server, on the client while it hosts a game, solo play
    /// included; else 0. Any thread.
    pub hosting: extern "system" fn(me: ModHandle, out: *mut u8) -> Status,
    /// Writes the entity world of the game server this process runs, the
    /// value a system context of that world holds, a pointer whose meaning
    /// belongs to the running game build; 0 while none runs or before its
    /// first step. Any thread.
    pub server_world: extern "system" fn(me: ModHandle, out: *mut u64) -> Status,
    /// Up to `capacity` of the server's [`Side::BOTH`](super::Side::BOTH)
    /// mods that the last refused join found missing on this client or in
    /// another version, and how many there are; laid out as
    /// [`HostApi::mods`] lays out its entries. Each entry has the id and
    /// version the server advertises, [`Side::BOTH`](super::Side::BOTH),
    /// `off` 1 and the client's DLL path when it has the mod (empty
    /// otherwise); name, homepage and the rest stay empty, since the server
    /// chose none of them. The strings stay valid for the process; once
    /// 64 KiB of advertised text is kept, an entry with new text is left
    /// out. Empty before a refused join; see [`Event::JOIN_REFUSED`]. Any thread.
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
    /// Adds `callback` with `user` to the mod's status sources: the loader
    /// calls it, on a thread of its own or the game thread, whenever it
    /// writes its status summary (`enshwick status` on the console, into
    /// the loader's log) and shows its lines under the mod. Allowed during
    /// `enshwick_mod_init` only.
    pub status:
        extern "system" fn(me: ModHandle, callback: Option<StatusFn>, user: *mut c_void) -> Status,
    /// Calls `callback` with `user` on every game event of the reflected
    /// type whose hash is `type_hash`, or on every game event for 0, on
    /// the server's game thread before the systems of the next step run.
    /// Allowed during `enshwick_mod_init` only, [`Status::TOO_LATE`] after.
    /// `callback` is non-null and stays callable while the mod is loaded;
    /// the loader keeps `user` and never reads or frees it. The event and
    /// its bytes are borrowed for the call. `callback` may call the loader
    /// again, and must not unwind: the Rust layer catches its handler's
    /// panic and turns the mod off.
    pub subscribe_game_event: extern "system" fn(
        me: ModHandle,
        type_hash: u32,
        callback: Option<GameEventFn>,
        user: *mut c_void,
    ) -> Status,
    /// Opens the mod's channel `name`, 1 to 64 bytes, between its server
    /// and client parts, and writes its id to `out`; `callback` gets each
    /// message on it with `user`: on the server's game thread, on a client
    /// on the thread that takes the server's chat. Mods of
    /// [`Side::BOTH`](super::Side::BOTH) only ([`Status::REFUSED`] for a
    /// server mod), since both ends must run the mod.
    /// Allowed during `enshwick_mod_init` only.
    pub net_channel: extern "system" fn(
        me: ModHandle,
        name: Str,
        callback: Option<NetMessageFn>,
        user: *mut c_void,
        out: *mut u32,
    ) -> Status,
    /// Sends `size` bytes at `data`, at most 8192, on the mod's `channel`:
    /// to the server for [`NetMessage::SERVER`](super::NetMessage::SERVER), else from the server to the
    /// client of the player with account key `peer`, or to every client
    /// that runs Enshwick for 0. Any thread. [`Status::MISSING`] while the
    /// peer cannot take messages, [`Status::BUSY`] over the peer's rate,
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
    /// Hands the loader the schema of the mod's settings file `file`, its
    /// name without `.toml`: `schema`, the TOML text of a
    /// settings schema (the format the book's settings chapter gives), and
    /// `defaults`, the file
    /// written from the defaults with their comments. As each world loads,
    /// the loader writes the file from `defaults` beside the save, or adds
    /// the keys it lacks and removes the ones the schema lacks. The console
    /// and the Mods screen read and check values against `schema`.
    /// [`Status::BAD_ARGS`] for text that is not a schema or a file name
    /// that is not lowercase letters, digits, `-` and `_`;
    /// [`Status::REFUSED`] for a file another schema names. Any thread, any time; a
    /// schema handed while a world runs is written as the next world loads.
    pub config_schema:
        extern "system" fn(me: ModHandle, file: Str, schema: Str, defaults: Str) -> Status,
    /// Makes `callback` with `user` the mod's check of a new text of its
    /// settings file `file`, which the loader runs before it writes a change
    /// from the console or the Mods screen; a refusal keeps the file.
    /// [`Status::REFUSED`] for a file that has a check. Allowed during
    /// `enshwick_mod_init` only.
    pub config_check: extern "system" fn(
        me: ModHandle,
        file: Str,
        callback: Option<ConfigCheckFn>,
        user: *mut c_void,
    ) -> Status,
    /// Hands the loader the mod's texts, a TOML table per language code as
    /// the book's texts chapter gives them, for the loader's own screens: the
    /// Mods screen labels the mod's settings with them. A later call
    /// replaces them. Any thread.
    pub texts: extern "system" fn(me: ModHandle, toml: Str) -> Status,
    /// Defines the mod event `spec` describes, named `<id>:<name>` after
    /// the defining mod, and writes the id [`HostApi::send_event`] takes;
    /// [`Status::REFUSED`] for a name outside the mod's id or defined
    /// twice. Allowed during `enshwick_mod_init` only.
    pub define_event:
        extern "system" fn(me: ModHandle, spec: *const EventSpec, out: *mut u32) -> Status,
    /// Calls `callback` with `user` and the event's data on every mod event
    /// `spec` names, once its definition, checked against `spec`'s layout
    /// when every mod is initialised, matches. [`Status::REFUSED`] for an event
    /// of another loaded mod whose [`Manifest::exports`](super::Manifest::exports)
    /// lack its name. Allowed during `enshwick_mod_init` only.
    pub subscribe_mod_event: extern "system" fn(
        me: ModHandle,
        spec: *const EventSpec,
        callback: Option<ModEventFn>,
        user: *mut c_void,
    ) -> Status,
    /// Hands `data` to every subscriber of the mod's own event `event`, on
    /// this thread, before it returns; reaches no one before every mod is
    /// initialised. [`Status::REFUSED`] for an event another mod defined.
    /// Any thread.
    pub send_event: extern "system" fn(me: ModHandle, event: u32, data: *const c_void) -> Status,
    /// Declares the mod's content renames, the `count` entries at `renames`,
    /// copied: content saved under a rename's `from` reads as the content
    /// now under its `to`. [`Status::REFUSED`] for a second call or a
    /// rename to itself; [`Status::BAD_ARGS`] for a null `renames` with a
    /// nonzero `count`. Allowed during `enshwick_mod_init` only.
    pub rename_content:
        extern "system" fn(me: ModHandle, renames: *const ContentRename, count: u32) -> Status,
    /// The mod's folder in `mods/`, which holds its installation; the path
    /// stays valid for the process. Any thread.
    pub mod_dir: extern "system" fn(me: ModHandle, out: *mut Str) -> Status,
    /// The game language of the reader, as the game numbers them. On a
    /// server, while the loader calls the
    /// mod about one player (an action, a settings check, the player's mod
    /// message, a building event of that player's character) on that
    /// thread, the language that player's game named when
    /// it joined, always one the game knows; else `0`, the fallback. On a
    /// client, the language it last asked a text in, `0` before its first
    /// text. Any thread; the answer holds for the calling thread.
    pub language: extern "system" fn(me: ModHandle, out: *mut u32) -> Status,
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
    /// [`Status::REFUSED`] for a point of another loaded mod whose exports
    /// ([`Manifest::exports`](super::Manifest::exports)) lack its kind and name, or of
    /// a required mod that does not define it as `link` declares it. Allowed
    /// during `enshwick_mod_init` only.
    pub attach_point: extern "system" fn(
        me: ModHandle,
        link: *const PointLink,
        callback: Option<PointFn>,
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
    /// Up to `capacity` of the running build's game event types, and how
    /// many there are, as every list answers: every reflected type that
    /// derives from the engine's event base, in no set order. The names are
    /// the loader's, valid for the rest of the process. A handle the loader
    /// never gave out gives [`Status::BAD_ARGS`] and a mod that is off
    /// [`Status::FAULTED`], with nothing written. Any thread.
    pub event_types: extern "system" fn(
        me: ModHandle,
        out: *mut EventTypeInfo,
        entry_size: u32,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Reports whether the mod's feature `name` runs on this game build:
    /// off for the reason `off`, or on when `off` is empty. The loader
    /// logs it and shows it in the status summary and on the Mods screen; a
    /// later report of the same name replaces the earlier one.
    /// [`Status::BAD_ARGS`] for an empty name. Both strings are borrowed for
    /// the call. Any thread.
    pub feature: extern "system" fn(me: ModHandle, name: Str, off: Str) -> Status,
    /// Calls `callback` with `user` before every wait of the engine's event
    /// loop on its handles, on the thread that waits, and writes to `found`
    /// which waits it hooked ([`LoopWaitSites`]); a wait this build lacks,
    /// whose code is not what its table expects, or that the mod hooked
    /// already is left out. [`Status::BAD_ARGS`] for a null `found`, before
    /// anything is hooked. Handlers run in load order. Allowed during
    /// `enshwick_mod_init` only.
    pub loop_wait: extern "system" fn(
        me: ModHandle,
        callback: Option<LoopWaitFn>,
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
        callback: Option<ServerWaitFn>,
        user: *mut c_void,
        found: *mut u8,
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
        found: *mut u8,
    ) -> Status,
    /// Serves the call `spec` describes with `callback` and `user`, named
    /// `<id>:<call>` after the serving mod; [`Status::REFUSED`] for another
    /// name, or a name and major served twice, [`Status::BAD_ARGS`] for a
    /// spec that does not read. Allowed during `enshwick_mod_init` only:
    /// [`Status::TOO_LATE`] once every mod is initialised.
    pub serve_call: extern "system" fn(
        me: ModHandle,
        spec: *const CallSpec,
        callback: Option<CallFn>,
        user: *mut c_void,
    ) -> Status,
    /// Asks for the call `spec` describes and writes its id to `out`.
    /// Matched against the served call by name and major once every mod is
    /// initialised: the input and the answer must agree field by field and
    /// in size, or the call stays unmatched with the reason in the log.
    /// [`Status::REFUSED`] when the mod asked for the name and major already
    /// with another spec, for a call of another loaded mod whose exports
    /// ([`Manifest::exports`](super::Manifest::exports)) lack its kind and name, or of
    /// a required mod that does not serve it as `spec` declares it;
    /// [`Status::BAD_ARGS`] for a spec that does not read or a null `out`.
    /// Allowed during `enshwick_mod_init` only: [`Status::TOO_LATE`] once
    /// every mod is initialised.
    pub find_call:
        extern "system" fn(me: ModHandle, spec: *const CallSpec, out: *mut u32) -> Status,
    /// Runs the call `id` from [`HostApi::find_call`] on this thread: the
    /// server's handler reads `input` and writes the answer to `output`,
    /// both as large as the spec says. [`Status::MISSING`] while the call
    /// is unmatched, [`Status::UNAVAILABLE`] before every mod is initialised, [`Status::REFUSED`]
    /// when the serving mod or its handler is off, [`Status::BAD_ARGS`] for
    /// a null pointer or an id the mod was not given, or what the handler
    /// answers. Any thread.
    pub call: extern "system" fn(
        me: ModHandle,
        id: u32,
        input: *const c_void,
        output: *mut c_void,
    ) -> Status,
    /// Calls `callback` with `user`, [`Event::TICKED`] and a [`TickData`](super::TickData)
    /// on the server's game thread after a step, once `every_ns` of wall
    /// time have passed since its last call, and on the first step after it
    /// subscribed; 0 calls it every step. It is also called once at each
    /// world's start, before [`Event::WORLD_LOADED`], with that game in
    /// [`TickData::game`](super::TickData::game), unless its period is
    /// `u64::MAX`. Steps the handler skips cost it nothing. The data lives for the call; the loader keeps `user` and
    /// never frees it. Writes the handler's handle to `out`, the caller's,
    /// writable for four bytes, on [`Status::OK`] only. [`Status::BAD_ARGS`]
    /// for a null `out`; [`Status::TOO_LATE`] outside `enshwick_mod_init`.
    /// The handler runs until the mod is off; a panic in it turns the mod
    /// off. It may call
    /// [`HostApi::set_tick_every`] on itself. [`Status::REFUSED`] once the
    /// loader holds `u32::MAX` handlers.
    pub subscribe_tick: extern "system" fn(
        me: ModHandle,
        every_ns: u64,
        callback: Option<EventFn>,
        user: *mut c_void,
        out: *mut TickHandle,
    ) -> Status,
    /// Sets the period of the mod's periodic tick handler `handle` to
    /// `every_ns`, 0 for every step, from the next step on: the next
    /// call comes `every_ns` after its last one, or on the next step when
    /// that time has passed. [`Status::REFUSED`] for another mod's handle;
    /// [`Status::MISSING`] for a handle the loader never gave out. Any
    /// thread, the handler itself included.
    pub set_tick_every:
        extern "system" fn(me: ModHandle, handle: TickHandle, every_ns: u64) -> Status,
    /// Writes `value`, one TOML value, as the setting `key` of the mod's own
    /// settings file `file` in the world this process runs, as the console
    /// does: checked against the file's schema and by the mod's
    /// [`HostApi::config_check`], written atomically with the file's
    /// comments kept, then raised as a settings change. A secret setting
    /// too, whose value the log never shows. Any thread; writes of settings
    /// wait for each other, and a write from the mod's settings handler
    /// raises the change again. The strings are borrowed for the call.
    /// [`Status::UNAVAILABLE`] while no world runs here, as on a client in
    /// another's world; [`Status::MISSING`] for a file or key the mod's
    /// settings lack; [`Status::BAD_ARGS`] for a value the setting does not
    /// accept or a string that is not UTF-8; [`Status::REFUSED`] when the
    /// mod's check refuses the file with the value, the file does not read
    /// or write, or the write comes from inside the mod's own check or
    /// nested more than four deep through its settings handler. On an error the file stays as it was and the loader
    /// logs why.
    pub set_config: extern "system" fn(me: ModHandle, file: Str, key: Str, value: Str) -> Status,
    /// As [`HostApi::subscribe_game_event`] for the events of the client's own world,
    /// on the thread that steps it, right after the engine moved them; no
    /// filter, veto or item action runs on them. One move hands the
    /// client's callbacks at most 256 events and 64 KiB of them, the rest
    /// dropped with a warning logged once. On a dedicated server
    /// [`Status::OK`] without keeping `callback`, which never runs there.
    pub subscribe_client_game_event: extern "system" fn(
        me: ModHandle,
        type_hash: u32,
        callback: Option<GameEventFn>,
        user: *mut c_void,
    ) -> Status,
    /// Serves the resource `key` as `patch` makes it from the resource
    /// `base`, built the first time the game reads it: a new resource when
    /// the keys differ, a changed vanilla one when they are equal. Allowed
    /// during `enshwick_mod_init` only. Several mods may serve one key:
    /// their patches run in load order, each on the one before's result.
    pub serve_resource: extern "system" fn(
        me: ModHandle,
        key: *const ResourceKey,
        base: *const ResourceKey,
        patch: Option<ResourcePatchFn>,
        user: *mut c_void,
    ) -> Status,
    /// Adds the text of `LocaTag` `id` in `language` (`LanguageId`, 0 for any
    /// language without its own); the client shows it wherever the game asks
    /// for that tag. Allowed during `enshwick_mod_init` only.
    pub add_text: extern "system" fn(me: ModHandle, id: u32, language: u32, text: Str) -> Status,
    /// Opens the item data kind `id`, `name` for the mod's own
    /// (`<mod>:<name>`) or `mod:name` for any mod's, each side 1 to 64 of
    /// `[a-z0-9_-]`, and writes its index to `out`; every mod gets the same
    /// index for one id. Allowed during `enshwick_mod_init` only.
    pub define_item_data: extern "system" fn(me: ModHandle, id: Str, out: *mut u32) -> Status,
    /// Copies the value of kind `kind` on the item entity `pide` holding
    /// item `item` into `out`, at most `capacity` bytes, and writes its
    /// length to `len`; [`Status::MISSING`] when it has none or its entity
    /// holds another item, [`Status::TOO_SMALL`] when `capacity` is short,
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
    /// the clients that run Enshwick get the change. [`Status::UNAVAILABLE`]
    /// on a client alone or before the world has loaded.
    /// Any thread.
    pub set_item_data: extern "system" fn(
        me: ModHandle,
        kind: u32,
        pide: Entity,
        item: ItemId,
        data: *const u8,
        len: u32,
    ) -> Status,
    /// Serves `len` bytes at `data` as game content, the pixels of a
    /// texture the mod serves, and writes the content hash a resource names
    /// them by to `out`; the same bytes give the same hash. The loader
    /// keeps the bytes for the life of the process: bytes served before
    /// answer their hash without a second copy, and every other serve adds
    /// to memory until the process ends. Any thread.
    pub content_serve:
        extern "system" fn(me: ModHandle, data: *const u8, len: u32, out: *mut [u32; 4]) -> Status,
    /// Copies the game's own content `hash` from its archive into `out`, at
    /// most `capacity` bytes, and writes its length to `len`;
    /// [`Status::MISSING`] for a hash the archive lacks or keeps
    /// compressed, [`Status::TOO_SMALL`] when `capacity` is short or `out`
    /// null, with `len` written, so a null `out` asks for the length. Reads the archive's files, never the game's memory.
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
    /// images; [`Status::UNAVAILABLE`] otherwise, and always on a server. Any
    /// thread.
    pub icons_ready: extern "system" fn(me: ModHandle) -> Status,
    /// Declares the content id `id`, `<mod>:<name>` in the mod's own
    /// namespace, and writes its GUID to `out`; [`Status::REFUSED`] for a
    /// malformed id, one outside the namespace, or one whose game id
    /// another content id has, which turns the mod off with both ids and
    /// both mods named. Allowed during `enshwick_mod_init` only.
    pub declare_content: extern "system" fn(me: ModHandle, id: Str, out: *mut Guid) -> Status,
    /// Opens the entity data kind `id`, as [`HostApi::define_item_data`] opens
    /// an item kind, and writes its index to `out`; with
    /// [`EntityDataFlags::SENT`] the clients that run Enshwick get its
    /// values. [`HostApi::item_data_get`] and [`HostApi::set_item_data`]
    /// read and write it with `item` 0; a set runs on the server's game
    /// thread. [`Status::REFUSED`] for an id opened as an item kind. Allowed
    /// during `enshwick_mod_init` only.
    pub define_entity_data:
        extern "system" fn(me: ModHandle, id: Str, flags: EntityDataFlags, out: *mut u32) -> Status,
    /// Copies the game's own resource `key` from its archive into `out`, at
    /// most `capacity` bytes, as the game reads it before it resolves its
    /// references, and writes its length to `len`; [`Status::MISSING`] for
    /// a key the archive lacks, [`Status::TOO_SMALL`] when `capacity` is
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
    /// with `key` itself left while it was not built yet, as its first
    /// build takes the values in force. Then every key whose build read
    /// `key` as served is built again the same way. Any thread.
    /// [`Status::MISSING`] when its base was not kept, [`Status::REFUSED`]
    /// when the new build differs in length, which the game's copy cannot
    /// take, [`Status::BAD_ARGS`] for a key the mod does not serve,
    /// [`Status::BUILDING`] from a patch of `key` itself, whose build would
    /// wait on its own. The
    /// loader frees what each rebuild's patches built in a
    /// [`resource_buffer`](Self::resource_buffer).
    pub rebuild_resource: extern "system" fn(me: ModHandle, key: *const ResourceKey) -> Status,
    /// Adds the calling mod's rule that disables recipes (`disable` 1), or
    /// takes its own rule of that kind and name back (`disable` 0). `name`
    /// names a recipe, an item every recipe of which is disabled, or a
    /// workshop, as `kind` says, in the forms the loader's recipe file
    /// takes; the loader resolves it against the recipe registry and logs a
    /// name it cannot resolve. The server's recipe file wins over every
    /// mod's rules. Any thread; applied within a second.
    pub recipe_rule:
        extern "system" fn(me: ModHandle, kind: RecipeRuleKind, name: Str, disable: u8) -> Status,
    /// Writes 1 to `out` when the recipe with game id `recipe` is disabled
    /// in this process now, else 0: on a client joined to a server, by the
    /// server's rules. Any thread.
    pub recipe_disabled:
        extern "system" fn(me: ModHandle, recipe: RecipeId, out: *mut u8) -> Status,
    /// Writes to `out` the kind of the recipes that make the item `item`,
    /// from the item's static data; [`Status::MISSING`] while the game has
    /// not loaded the item, which on a dedicated server comes after its
    /// recipe registry is first built, and while the loader serves no
    /// resource. Any thread.
    pub recipe_kind:
        extern "system" fn(me: ModHandle, item: ItemId, out: *mut RecipeKind) -> Status,
    /// Declares that the calling mod multiplies `rate`, so the loader arms
    /// what the rate needs; its factor is 1 until
    /// [`HostApi::set_rate`]. [`Status::UNAVAILABLE`] on a build that lacks
    /// what the rate needs, [`Status::BAD_ARGS`] for a rate this loader
    /// does not know, [`Status::REFUSED`] for a rate clients count too in a
    /// [`Side::SERVER`](super::Side::SERVER) mod. A second declaration of the same rate is
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
    /// Declares a change of a vanilla value, of one entry or of each item or
    /// recipe a selection picks. The loader merges the changes
    /// of every mod once their `init` ran: a set in load order, the last
    /// winning and the overridden one reported in both mods' logs and the
    /// conflict report; then every factor, multiplied. It builds the changed
    /// resource the first time the game reads it, on each side from its own
    /// mods. [`Status::BAD_ARGS`] for a null `change`, an unknown field or
    /// operation and a value outside the field's range;
    /// [`Status::UNAVAILABLE`] when the build does not reflect the field.
    ///
    /// With `out`, the change is live: the mod sets its value later with
    /// [`HostApi::set_change`], and until then it changes nothing;
    /// `change.op` and `change.value` are ignored, and the change's number
    /// goes to `out`. The loader builds every resource a live change reaches
    /// from the first read, so a later set takes effect in place.
    /// [`Status::BAD_ARGS`] for an ingredient, whose change alters a
    /// recipe's length, and [`Status::REFUSED`] for a field clients read in
    /// a [`Side::SERVER`](super::Side::SERVER) mod. During
    /// `enshwick_mod_init` only, on its thread.
    pub change_data:
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
    /// Checks that the mod may name the content id `id`, `<mod>:<name>`:
    /// [`Status::OK`] for one of its own, one of a mod that is not loaded,
    /// and one whose mod's manifest exports `<name>` or the name it is a
    /// part of (`locker` for `locker.recipe`); [`Status::REFUSED`] for any
    /// other, with the id and the owner's exports logged;
    /// [`Status::BAD_ARGS`] for a malformed id. Any thread.
    pub use_content: extern "system" fn(me: ModHandle, id: Str) -> Status,
    /// Up to `capacity` keys of the game's own resources of type
    /// `type_hash`, or of every resource for 0, in the order their bytes lie
    /// in the archive, and how many there are; read from the archive's files
    /// at the first call and kept, as every list of plain values answers.
    /// [`Status::UNAVAILABLE`] when the archive does not read. Any thread.
    pub resource_keys: extern "system" fn(
        me: ModHandle,
        type_hash: u32,
        out: *mut ResourceKey,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Up to `capacity` hashes of the game's own contents, in the archive's
    /// order, and how many there are; [`HostApi::content_read`] reads each,
    /// as every list of plain values answers.
    /// [`Status::UNAVAILABLE`] when the archive does not read. Any thread.
    pub content_hashes: extern "system" fn(
        me: ModHandle,
        out: *mut [u32; 4],
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Up to `capacity` reflected types of the running game, each a name
    /// carries alone, and how many there are; built at the first call and
    /// kept, as every list answers. The strings and arrays an
    /// entry points at are the loader's, valid and unchanged for the life of
    /// the process. [`Status::UNAVAILABLE`] when the game holds no reflection
    /// data. Any thread.
    pub types: extern "system" fn(
        me: ModHandle,
        out: *mut TypeEntry,
        entry_size: u32,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Writes the version string of the game's archive to `out`, such as
    /// `1283531|^/game38/branches/release_1.0|2026-10-30T09:15:50.868196Z`:
    /// the build number, the branch and when the build was made. The string
    /// is the loader's, valid and unchanged for the life of the process.
    /// [`Status::UNAVAILABLE`] when the archive does not read. Any thread.
    pub game_version: extern "system" fn(me: ModHandle, out: *mut Str) -> Status,
    /// Gives the crafting prop `prop` its index in the game's crafting prop
    /// mask and writes it to `out`: the first bit the game's own props leave
    /// free for the first prop any mod adds, then one more for each new prop
    /// in call order, and the same index again for a prop given one before.
    /// The loader reads the mask's bits and the game's props from the
    /// running game, so a game patch that adds a prop needs no new mod.
    /// [`Status::REFUSED`] when the mask is full, when the game's mask or
    /// props do not read, or for a mod not of side both, whose bit clients
    /// would lack, each of which turns the mod off naming the prop;
    /// [`Status::BAD_ARGS`] for a null `prop` or `out`. Allowed during
    /// `enshwick_mod_init` only ([`Status::TOO_LATE`] after); `prop` and
    /// `out` are borrowed for the call.
    pub crafting_prop: extern "system" fn(me: ModHandle, prop: *const Guid, out: *mut u8) -> Status,
    /// Writes to `out` how many bytes from `address` on, at most `max`,
    /// lie in committed, unguarded, read-only pages: the run of such
    /// regions that `address` starts, cut at `max`; 0 when `address` does
    /// not lie in one. It reads no byte of that memory, so any `address` is
    /// allowed. No thread writes those pages without changing their
    /// protection, and the count holds only while nothing frees or protects
    /// them, which the caller knows of the memory it asks about. For a
    /// private run of 16 MiB or more the loader may answer from what it
    /// found earlier on the same thread in the same world, without asking
    /// Windows again, until the world goes away; smaller runs and mapped
    /// views are asked afresh. [`Status::BAD_ARGS`] for a null `out`. Any thread,
    /// re-entrant; `out` is borrowed for the call.
    pub read_only:
        extern "system" fn(me: ModHandle, address: u64, max: u32, out: *mut u32) -> Status,
    /// Copies resource `key` as the game's first read would build it now
    /// into `out`, at most `capacity` bytes, and writes its length to
    /// `len`. A key mods serve, or the key a mod renamed it to, is built
    /// from its base in the archive with the patches of every mod on, on
    /// the calling thread; a patch that fails is skipped and its mod stays
    /// on. The build is kept nowhere, so a mod turned off since the game's
    /// own build is left out, and the loader frees each output a patch
    /// built in a [`resource_buffer`](Self::resource_buffer) once it has
    /// copied the bytes. When every patch fails, a changed vanilla key
    /// copies as the archive has it. Any other key, and every key while
    /// served resources are off, copies as the game opens it from the
    /// archive. A game build that reads a served key here is built again
    /// whenever that key is rebuilt. [`Status::MISSING`] for a key neither
    /// served nor in the archive, or a new key whose every patch failed;
    /// [`Status::TOO_SMALL`] when `capacity` is short or `out` null, with
    /// `len` written, and for a short `out` the build kept for this
    /// thread's next call with the same key; [`Status::REFUSED`] during `enshwick_mod_init`, while
    /// not every patch is in; [`Status::BUILDING`] from a patch for a key
    /// whose patches run on this thread, which would build it again without
    /// end. Any thread; re-entrant from a patch for another key. `key` and
    /// `len` are borrowed for the call.
    pub resource_served: extern "system" fn(
        me: ModHandle,
        key: *const ResourceKey,
        out: *mut u8,
        capacity: u32,
        len: *mut u32,
    ) -> Status,
    /// Writes to `out` a pointer to `len` zeroed, writable bytes the loader
    /// owns, for the output of the [`ResourcePatchFn`](super::ResourcePatchFn)
    /// running on the calling thread: the patch builds its output there and
    /// returns where it lies from the same call, the only place an output
    /// may lie. The bytes stay valid until the build that runs the patch
    /// ends; the game's first build keeps the buffer of the last patch's
    /// output for the process, as the game holds it, and the loader frees
    /// every other buffer, a rebuild's and a served read's
    /// ([`resource_served`](Self::resource_served)) included, when the
    /// build ends. A mod that is off gets one too, as a rebuild runs the
    /// patches of the mods that built what the game holds.
    /// [`Status::REFUSED`] outside a patch or when `len` bytes do not
    /// allocate; [`Status::BAD_ARGS`] for a null `out` or a handle the
    /// loader never gave out. The patch's thread only, re-entrant from a
    /// patch for another key; `out` is borrowed for the call.
    pub resource_buffer: extern "system" fn(me: ModHandle, len: u32, out: *mut *mut u8) -> Status,
    /// Writes to `out` the content id, `<mod>:<name>`, that a loaded mod
    /// declared for exactly the GUID at `guid`, not only for its game id: a
    /// mod's item, recipe or other content. The text lives for the process.
    /// Any thread. [`Status::MISSING`] for a GUID no mod declared, such as a
    /// vanilla item's; [`Status::BAD_ARGS`] for a null `guid` or `out`.
    pub content_name: extern "system" fn(me: ModHandle, guid: *const Guid, out: *mut Str) -> Status,
    /// The connected player whose character is `entity`; on the server's game
    /// thread only.
    pub player: extern "system" fn(me: ModHandle, entity: Entity, out: *mut PlayerInfo) -> Status,
    /// Up to `capacity` connected players with characters in the world, and
    /// how many there are, as every list answers (see [`HostApi`]); on the
    /// server's game thread only.
    pub players: extern "system" fn(
        me: ModHandle,
        out: *mut PlayerInfo,
        entry_size: u32,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Sends `text` to the chat of the player whose account key is `to`, from
    /// any thread; the server posts it after the next step. Lines past 1024
    /// bytes are cut. Key 0, which names the console or a remote caller in
    /// a request and no player, answers [`Status::BAD_ARGS`] and sends
    /// nothing: answer such a request in its outcome, and send to everyone
    /// with [`HostApi::chat_all`].
    pub chat: extern "system" fn(me: ModHandle, to: PlayerKey, text: Str) -> Status,
    /// Sends `text` to the chat of every connected player, from any thread;
    /// the server posts it after the next step. Lines past 1024 bytes are
    /// cut.
    pub chat_all: extern "system" fn(me: ModHandle, text: Str) -> Status,
    /// Gives `entity` the name `text`, as a player's rename does: shown on
    /// hover, sent to every client and saved with the world; cut at 255
    /// bytes, removed when empty. On the server's game thread only.
    pub set_label: extern "system" fn(me: ModHandle, entity: Entity, text: Str) -> Status,
    /// Calls `callback` with `user` before the server applies a player's
    /// rename of an entity; any filter returning [`FILTER_REFUSE`](super::FILTER_REFUSE) refuses it. Allowed during
    /// `enshwick_mod_init` only.
    pub rename_filter: extern "system" fn(
        me: ModHandle,
        callback: Option<RenameFilterFn>,
        user: *mut c_void,
    ) -> Status,
    /// Writes the static data of the item `item`, as far as the [`Header`]
    /// `out` starts with says the caller's struct reaches;
    /// [`Status::MISSING`] for an unknown item, [`Status::UNAVAILABLE`] while
    /// no world runs.
    /// Any thread.
    pub item_info:
        extern "system" fn(me: ModHandle, item: ItemId, out: *mut ItemTypeInfo) -> Status,
    /// Asks the loader to follow the Flame Altars of the server's world, for
    /// [`HostApi::altars`] and [`Event::ALTARS_CHANGED`]. Allowed during
    /// `enshwick_mod_init` only.
    pub request_altars: extern "system" fn(me: ModHandle) -> Status,
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
    /// box does not lie in one building zone; any filter returning
    /// [`FILTER_YES`](super::FILTER_YES) accepts it after all. Runs on both sides, since a client predicts placement.
    /// Allowed during `enshwick_mod_init` only.
    pub building_zone_filter: extern "system" fn(
        me: ModHandle,
        callback: Option<BuildingZoneFilterFn>,
        user: *mut c_void,
    ) -> Status,
    /// Writes how many altar zones and bases the game's last grouping found;
    /// [`Status::UNAVAILABLE`] before the first one or without the raised
    /// limit.
    /// Any thread.
    pub base_zones: extern "system" fn(me: ModHandle, zones: *mut u32, bases: *mut u32) -> Status,
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
    /// [`Status::MISSING`] for an entity not in the world,
    /// [`Status::UNAVAILABLE`] where no server runs or on a build without the
    /// command. On the server's game thread only.
    pub destroy: extern "system" fn(me: ModHandle, entity: Entity) -> Status,
    /// Asks the server to save the world now, as its timed saves do;
    /// [`Event::WORLD_SAVED`] follows once the save is on disk. A save under
    /// way counts as asked; [`Status::REFUSED`] while the world may not be
    /// saved, as before it has loaded. On the server's game thread only.
    pub save_world: extern "system" fn(me: ModHandle) -> Status,
    /// Writes the key of the world the server runs: its save file's name
    /// without `.ksd`, such as `world0`, `[A-Za-z0-9_-]`, valid for the
    /// process.
    /// [`Status::UNAVAILABLE`] before the server has read or written the
    /// world.
    /// Any thread.
    pub world_key: extern "system" fn(me: ModHandle, out: *mut Str) -> Status,
    /// A writable pointer to `entity`'s component of the reflected type
    /// `type_hash`, valid until the game thread returns to the game; the
    /// loader marks the component changed, as the game's own writes do, so
    /// the game sends it to clients and saves it. [`Status::REFUSED`] for a
    /// copy the game keeps for clients (`Network*`, `Client*`), which it
    /// overwrites; [`Status::STATIC_HALF`] for the static half of an ECS
    /// type, the template's set-up data;
    /// [`Status::MISSING`] when the entity lacks the component or is not in
    /// the world; [`Status::UNAVAILABLE`] where no server runs or on a build
    /// without writes. On the server's game thread only.
    pub component_write: extern "system" fn(
        me: ModHandle,
        entity: Entity,
        type_hash: u32,
        out: *mut *mut c_void,
    ) -> Status,
    /// Shows `text`, cut at 255 bytes, on the screen of the player with
    /// account key `key`, as the game shows its own "can't do that"
    /// messages; [`Status::UNAVAILABLE`] when the player's client lacks
    /// Enshwick and cannot show it, [`Status::BUSY`] when the server's line
    /// queue is full, [`Status::REFUSED`] when it sends no lines to one
    /// player, and nothing is sent after any of them. On the server's game
    /// thread only.
    pub notify: extern "system" fn(me: ModHandle, key: PlayerKey, text: Str) -> Status,
    /// Writes the clock of the server's world, as far as the [`Header`]
    /// `out` starts with says the caller's struct reaches. On the server's
    /// game thread only.
    pub time_of_day: extern "system" fn(me: ModHandle, out: *mut DaytimeInfo) -> Status,
    /// Sets the clock of the server's world to `time_ns`, game nanoseconds of
    /// the day, taken modulo 24 hours; clients follow after the next step.
    /// On the server's game thread only.
    pub set_time_of_day: extern "system" fn(me: ModHandle, time_ns: i64) -> Status,
    /// Lets the mod change how fast the clock runs, with
    /// [`HostApi::set_time_speed`]; every mod may, each with a factor of
    /// its own. Allowed during `enshwick_mod_init` only.
    pub request_time_speed: extern "system" fn(me: ModHandle) -> Status,
    /// Sets the mod's factor on the clock's speed, from 0 (stopped) to 1000;
    /// the clock runs at the game's own speed times every mod's factor.
    /// [`Status::REFUSED`] for a mod without
    /// [`HostApi::request_time_speed`], [`Status::BAD_ARGS`] for a factor
    /// out of range. A hosted game's end puts every factor back at 1. On
    /// the server's game thread only.
    pub set_time_speed: extern "system" fn(me: ModHandle, factor: f32) -> Status,
    /// Copies the server's live game settings, the game's `GameSettings` type,
    /// into `out`, `size` bytes, which must be the type's reflected size
    /// ([`Status::BAD_ARGS`] otherwise), and writes the preset the server
    /// runs (`GameSettingsPreset`) to `preset`. On the server's game thread
    /// only.
    pub game_settings:
        extern "system" fn(me: ModHandle, out: *mut u8, size: u32, preset: *mut u32) -> Status,
    /// Replaces the server's live game settings with `data`, `size` bytes of
    /// the type's whole reflected layout, as the game's admin command does:
    /// systems see them from the next step and every client gets them; the
    /// preset becomes custom. Until the server stops: nothing writes them to
    /// disk.
    ///
    /// With `mask`, `size` bytes as well, the fields with a byte set in it
    /// stay with the running world: the loader applies them again each time
    /// the world loads, by the end of its first step;
    /// [`Status::UNAVAILABLE`] while no world runs. A null `data`, with a
    /// null `mask` and a `size` of 0, forgets the fields kept for the running
    /// world and puts back the values it loaded with.
    ///
    /// [`Status::BAD_ARGS`] for a wrong size, a day or night length of zero
    /// or less, or a `mask` or `size` with a null `data`. On the server's
    /// game thread only.
    pub set_game_settings:
        extern "system" fn(me: ModHandle, data: *const u8, mask: *const u8, size: u32) -> Status,
    /// Up to `capacity` buff and debuff types of the running game, and how
    /// many there are, as every list answers. On the server's game thread
    /// only.
    pub buff_types: extern "system" fn(
        me: ModHandle,
        out: *mut BuffTypeInfo,
        entry_size: u32,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Up to `capacity` buffs `entity` has, and how many there are, as every
    /// list answers; [`Status::MISSING`] for an entity that holds no buffs.
    /// On the server's game thread only.
    pub buffs: extern "system" fn(
        me: ModHandle,
        entity: Entity,
        out: *mut ActiveBuff,
        entry_size: u32,
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
    pub remove_buff: extern "system" fn(
        me: ModHandle,
        entity: Entity,
        id: BuffTypeId,
        removed: *mut u8,
    ) -> Status,
    /// Writes how the weather weighs at `position`, in
    /// metres, as the game's systems weigh it, as far as the [`Header`]
    /// `out` starts with says the caller's struct reaches. On the server's
    /// game thread only.
    pub weather_at: extern "system" fn(
        me: ModHandle,
        position: *const Metres,
        out: *mut WeatherSample,
    ) -> Status,
    /// Up to `capacity` current weather fronts of the server's world, one per
    /// layer that has one, and how many there are, as every list answers.
    /// On the server's game thread only.
    pub weather_fronts: extern "system" fn(
        me: ModHandle,
        out: *mut WeatherFront,
        entry_size: u32,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Starts new weather in every layer of the server's world: fronts of
    /// `effect` ([`WeatherFront::NONE`] for clear skies) where a layer has
    /// one, clear skies elsewhere, for `duration_ns` game nanoseconds or the
    /// game's own for 0; `u32::MAX` lets the game choose anew. Clients
    /// follow. On the server's game thread only.
    pub set_weather: extern "system" fn(me: ModHandle, effect: u32, duration_ns: i64) -> Status,
    /// Up to `capacity` terrain materials of the running game, and how many
    /// there are, as every list answers. On the server's game thread only.
    pub terrain_materials: extern "system" fn(
        me: ModHandle,
        out: *mut TerrainMaterialInfo,
        entry_size: u32,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Runs a terrain edit in the server's world as the game's terraforming
    /// does, and writes the volume it changed in cubic metres (for a
    /// flatten, 1 when anything changed) to `amount`; clients see it, and
    /// the save keeps it inside a base's zone only. Building voxels are left
    /// alone.
    /// [`Status::BAD_ARGS`] for an edit out of range;
    /// [`Status::OUTSIDE_WORLD`] for one that reaches past the world's edge
    /// or above the height the loader takes, [`Status::NOT_LOADED`] unless
    /// every column of the box it can write is loaded, writing nothing. On
    /// the server's game thread only.
    pub terrain_edit:
        extern "system" fn(me: ModHandle, edit: *const TerrainEdit, amount: *mut f32) -> Status,
    /// Calls `callback` with `user` for every altar of the server's world
    /// when the loader sizes the altar zones: after altars change and after
    /// [`HostApi::altar_zones_changed`]. Each filter, in load order, sees the
    /// size so far, the vanilla one first, and may change it: a `grow` below
    /// 0 counts as 0, a `height` below one cell keeps the height before it,
    /// and the altar and level stay. It opens altars too, and may call the
    /// host, such as [`HostApi::altars`]. On the server's game thread.
    /// Allowed during `enshwick_mod_init` only.
    pub altar_zone_filter: extern "system" fn(
        me: ModHandle,
        callback: Option<AltarZoneFilterFn>,
        user: *mut c_void,
    ) -> Status,
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
    /// [`Status::UNAVAILABLE`] on a build without the game's lookup.
    pub altar_limit_filter: extern "system" fn(
        me: ModHandle,
        callback: Option<AltarLimitFilterFn>,
        user: *mut c_void,
    ) -> Status,
    /// Writes to `out` the most altar zones and bases the game groups now:
    /// its own, or the loader's maximum once [`HostApi::request_base_limits`] is
    /// in force, from `enshwick_mod_ready` on, with the zones its own
    /// grouping takes. [`Status::BAD_ARGS`] for a null `out`. Any thread.
    pub base_limit: extern "system" fn(me: ModHandle, out: *mut BaseLimitInfo) -> Status,
    /// Reads the voxels of the server's world, decoded at full detail, in the
    /// box of `size` voxels
    /// from `min`, in voxels of half a metre from the world's corner, into
    /// `out`: x fastest, then y, then z, `size[0] * size[1] * size[2]`
    /// entries. `min` and `size` are borrowed for the call; `out` points at
    /// `capacity` writable entries, untouched unless the call returns
    /// [`Status::OK`]. [`Status::BAD_ARGS`] for a null pointer, an empty
    /// box, one past [`Voxel::END`], more than [`Voxel::READ_MAX`] voxels or
    /// a `capacity` below them; [`Status::OUTSIDE_WORLD`] for a box past
    /// the world's edge or above the height the loader takes;
    /// [`Status::NOT_LOADED`] while part of the box is not loaded
    /// yet, which starts loading it; [`Status::UNAVAILABLE`] before a world
    /// runs or on a build without the read. On the server's game thread at
    /// a quiet moment only, as the world's writes: [`Status::WRONG_THREAD`]
    /// inside a step.
    pub voxel_read: extern "system" fn(
        me: ModHandle,
        min: *const [u32; 3],
        size: *const [u32; 3],
        out: *mut Voxel,
        capacity: u32,
    ) -> Status,
    /// Fills `check.refusal` with whether the server lets the player whose
    /// character is `check.character` edit the box from `check.min` to
    /// `check.max`. The permissions follow the game's terrain tool:
    /// `CanEditWorld` for every edit, `CanEditBase` where the box overlaps a
    /// building zone. The loader adds two tests the building cursor runs for
    /// items: both corners inside the world's building area, and no strict
    /// no-build zone the world places. A player not in game has no
    /// permissions. `check` is borrowed for the call. [`Status::BAD_ARGS`]
    /// for a null `check`, an old header, a corner that is not finite or a
    /// `max` below `min`; [`Status::UNAVAILABLE`] while no world is loaded or
    /// on a build without the checks. On the server's game thread at a quiet
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
    /// committed earlier fails with it before writing); [`Status::BUSY`]
    /// while the queue is full; [`Status::UNAVAILABLE`] while no world runs
    /// or on a build without the edits, the voxel read or the undo write.
    ///
    /// `actor` names the character of the player the transaction acts for,
    /// 0 for none: before it writes anything the loader checks that the
    /// server lets the player edit the box of every change, as
    /// [`HostApi::build_rights`] does, and marks building events with the
    /// player. A refusal fails it with [`Status::REFUSED`] and the reason in
    /// [`TransactionInfo::refusal`]; nothing is written. The check runs at
    /// its first step on the server's game thread, never at the commit. Any
    /// thread.
    pub transaction_commit: extern "system" fn(
        me: ModHandle,
        actor: Entity,
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
    /// [`Status::OK`] on the server's game thread at a quiet moment while a
    /// world runs, where a mod may read live components that no system
    /// writes meanwhile; [`Status::WRONG_THREAD`] on another thread or
    /// inside a step, [`Status::UNAVAILABLE`] while no world runs.
    pub world_quiet: extern "system" fn(me: ModHandle) -> Status,
    /// Writes the entities of the props transaction `id` of this mod placed
    /// and did not undo, kept when its undo data expires, oldest first, up to `capacity` of them at `out`,
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
    /// [`Status::UNAVAILABLE`] while no world is loaded or on a build without
    /// the zone list. On the server's game thread at a quiet moment only:
    /// [`Status::WRONG_THREAD`] elsewhere or inside a step. The mod passes
    /// `min` and `max` readable for three `f64` each and `kept` writable for
    /// one byte, all three borrowed for the call and untouched by other
    /// threads during it; none needs alignment.
    pub world_persists: extern "system" fn(
        me: ModHandle,
        min: *const Metres,
        max: *const Metres,
        kept: *mut u8,
    ) -> Status,
    /// Up to `capacity` map markers of the scene the server runs and how
    /// many there are: what the scene's global chunk places, read from the
    /// game's archive once per scene on a thread of the loader's;
    /// [`Status::NOT_LOADED`] until that read is done; as every list answers. On the server's game
    /// thread at a quiet moment ([`Status::WRONG_THREAD`] elsewhere or
    /// inside a step); [`Status::UNAVAILABLE`] while no world runs or when the
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
    /// [`AccountActionType::UNBAN`] takes it off the list; the dedicated
    /// server writes the list to `bannedAccounts` of `enshrouded_server.json`.
    /// [`Status::REFUSED`] for a kick or a ban of an administrator online,
    /// whom the game never removes, for a kick or a ban of a hosted game's
    /// host, and for a kick or a ban of an account not online; an account on
    /// the list already is no error. [`Status::BAD_ARGS`] for account 0 or an
    /// action this loader does not know; [`Status::UNAVAILABLE`] while no server runs or on a build that
    /// lacks the action. On the server's game thread between steps only:
    /// [`Status::WRONG_THREAD`] elsewhere.
    pub account_action:
        extern "system" fn(me: ModHandle, action: AccountActionType, account: PlayerKey) -> Status,
    /// Writes to `out` the frame of the map of the scene the server runs:
    /// the scene's biome map, read from the game's archive once per scene
    /// on a thread of the loader's; [`Status::NOT_LOADED`] until that read
    /// is done. `out` is the caller's, at least a [`Header`] and writable
    /// for the size its header states; [`Status::BAD_ARGS`] for a null or
    /// smaller one. On the server's game thread at a quiet moment
    /// ([`Status::WRONG_THREAD`] elsewhere or inside a step);
    /// [`Status::UNAVAILABLE`] while no world runs, for a scene without a biome
    /// map, or when the scene does not read on this build.
    pub map_frame: extern "system" fn(me: ModHandle, out: *mut MapFrameInfo) -> Status,
    /// Asks the game to group up to `zones` altar zones into up to `bases`
    /// bases. Raised, the game groups every listed zone into its base;
    /// `zones` caps the altars a Flame level allows, and an altar that would
    /// start a base past `bases` is refused. Values below the game's own
    /// count as the game's, and the loader takes the largest request of
    /// every mod for each. The loader owns the hooks; when one cannot be
    /// armed after init the game keeps its own limits although the call
    /// returned [`Status::OK`], so read [`HostApi::base_limit`] from
    /// `enshwick_mod_ready` on. [`Status::BAD_ARGS`] for a value past the
    /// most the loader takes ([`BaseLimitInfo::max_zones`],
    /// [`BaseLimitInfo::max_bases`]); [`Status::UNAVAILABLE`] when this build's
    /// code is not the one the loader knows, and the game keeps its own.
    /// Allowed during `enshwick_mod_init` only.
    pub request_base_limits: extern "system" fn(me: ModHandle, zones: u32, bases: u32) -> Status,
    /// Mutes `account` in chat when `muted` is 1, or lifts its mute when 0.
    /// The server drops every line the account's player types before any
    /// other player reads it, and tells that player with one of the game's
    /// notices at most every 30 seconds; lines mods send and mod messages
    /// pass. The mutes belong to the world: the file
    /// `<stem>-mod-data/enshwick/muted.txt` beside its save, written at once
    /// and read again as the world loads. `Status::OK` also when the
    /// list already says so; [`Status::BAD_ARGS`] for account 0 or `muted`
    /// above 1; [`Status::REFUSED`] for a mute of an administrator online
    /// or of a hosted game's host, whose lines pass even when muted before,
    /// and when the file is not written;
    /// [`Status::UNAVAILABLE`] outside a server, the dedicated one or the one a
    /// client hosts, and while no world runs. Any thread, also while another call changes the list:
    /// changes take turns and the file is replaced whole.
    pub set_chat_mute: extern "system" fn(me: ModHandle, account: PlayerKey, muted: u8) -> Status,
    /// Writes 1 to `muted` when `account` is muted in chat, 0 when not.
    /// `muted` is the caller's, writable for one byte for the call, and
    /// written only on [`Status::OK`]. [`Status::BAD_ARGS`] for account 0
    /// or a null `muted`; [`Status::UNAVAILABLE`] outside a server, the
    /// dedicated one or the one a client hosts. Any thread.
    pub chat_muted: extern "system" fn(me: ModHandle, account: PlayerKey, muted: *mut u8) -> Status,
    /// Caps the bases the server holds at `bases`, below the largest
    /// [`HostApi::request_base_limits`]: an altar that would start a base
    /// past it is refused. Each call replaces the mod's own cap, and the
    /// lowest cap of every mod holds, so a mod asks for its most at init
    /// and sets each world's cap from its settings as the world loads; a
    /// cap at or past the request leaves the request; a hosted game's end
    /// drops every cap. [`HostApi::base_limit`]
    /// reads the capped bases. [`Status::BAD_ARGS`] below the game's 16.
    /// Any time, any thread.
    pub set_base_limit: extern "system" fn(me: ModHandle, bases: u32) -> Status,
    /// Writes the save of the world the server runs to `out`, which the
    /// mod starts with a header of its own size and ABI version: [`Status::MISSING`] before its first
    /// save, [`Status::UNAVAILABLE`] while no world runs or on a client that
    /// hosts none, and
    /// [`Status::REFUSED`] when the file does not read. Any thread; `out` is
    /// borrowed for the call.
    pub save_info: extern "system" fn(me: ModHandle, out: *mut SaveInfo) -> Status,
    /// Holds the world's saves while `held` is 1, or lets them go for 0:
    /// the game saves only while no running mod holds them, the timed save
    /// waiting and [`HostApi::save_world`] refused meanwhile, and the log names each
    /// mod that holds or lets go. A mod that turns off lets go. At shutdown
    /// every hold lets go before [`Event::SHUTDOWN_STARTED`]; a hold set
    /// while the mod hears that event refuses the final save, and a hold
    /// asked for after it is not taken. Server only
    /// ([`Status::UNAVAILABLE`] on a build without the save layout); any thread,
    /// taking effect after the next step, or at once while the mod hears
    /// [`Event::SHUTDOWN_STARTED`]; [`Status::BAD_ARGS`] for another
    /// `held`.
    pub hold_saves: extern "system" fn(me: ModHandle, held: u8) -> Status,
    /// Keeps `entity`, which a mod spawned or placed, out of the world's
    /// saves (`saved` 0) or puts it back in them (any other `saved`), from
    /// the next step on; the next save after that keeps it or leaves it
    /// out. [`Status::REFUSED`] for an entity no mod spawned since the world
    /// loaded, such as a vanilla one, or a full command buffer; [`Status::MISSING`] for an
    /// entity not in the world; [`Status::UNAVAILABLE`] where no server
    /// runs, as on a client in another's game, or on a build without the
    /// commands; [`Status::BAD_ARGS`] for entity 0. On the server's game
    /// thread only ([`Status::WRONG_THREAD`] elsewhere).
    pub set_saved: extern "system" fn(me: ModHandle, entity: Entity, saved: u8) -> Status,
    /// Writes the level or experience of the player whose character is
    /// `entity` in the server's next level step, where the game raises
    /// levels itself; `what` says which. [`PROGRESS_SET_LEVEL`](super::PROGRESS_SET_LEVEL): the level
    /// becomes `value`, kept between 1 and the game's level cap, with no
    /// experience inside it and what that level needs to go on, as the
    /// game leaves a level it raised, and a gain not applied yet is
    /// dropped; the game raises no level-up event.
    /// [`PROGRESS_SET_EXPERIENCE`](super::PROGRESS_SET_EXPERIENCE): the experience inside the level becomes
    /// `value`, kept below what the level needs.
    /// [`PROGRESS_ADD_EXPERIENCE`](super::PROGRESS_ADD_EXPERIENCE): `value` is gained, and the game levels
    /// up from it, one level a step, with its own events; experience
    /// filters do not see it. Skill points follow the level, as the game
    /// counts them from it. A mod asks for
    /// the writes during `init` with [`PROGRESS_OPEN`](super::PROGRESS_OPEN) and `Entity(0)`;
    /// [`Status::TOO_LATE`] for that after `init`. [`Status::OK`] queues
    /// the write; the step applies writes in the order given, and
    /// [`Event::PROGRESS_WRITTEN`](super::Event::PROGRESS_WRITTEN) tells the
    /// mod after the step whether it applied the write or why it was
    /// dropped.
    /// [`Status::REFUSED`] for a mod that did not ask, an entity that is
    /// not a player's character in the game; [`Status::BUSY`] while 256
    /// writes wait;
    /// [`Status::BAD_ARGS`] for another `what`; [`Status::WRONG_THREAD`]
    /// off the server's game thread; [`Status::UNAVAILABLE`] for a write during
    /// `init`, on a client, a
    /// build without the level step, or once a panic turned the writes
    /// off.
    pub progress_write:
        extern "system" fn(me: ModHandle, entity: Entity, what: u32, value: u32) -> Status,
    /// Asks the dedicated server to stop the way the game stops on Ctrl+C:
    /// after the current step the server shuts down, `ShutdownStarted` is
    /// raised and the save holds let go, the final save runs, and the
    /// process exits with code 0. Returns at once; the stop runs on a
    /// thread of the loader's. Any thread. [`Status::REFUSED`] when a stop
    /// is under way already, asked by a mod or by the game, or the game has
    /// not started yet;
    /// [`Status::UNAVAILABLE`] off a dedicated server, as in single player
    /// or a hosted game; [`Status::UNAVAILABLE`] when the loader did not find
    /// the game's stop on this build.
    pub server_stop: extern "system" fn(me: ModHandle) -> Status,
    /// Uncovers the fog of war of the player with account key `account` in
    /// the circle of `radius` metres around world `x` and `z`, as exploring
    /// there does: the server's loader sends it to the loader on the
    /// player's client, which hands it to the game's own reveal, and the
    /// character's save keeps it. Returns once it is sent. The server holds
    /// no fog, so a client without Enshwick is not reached. Any thread, on
    /// a dedicated server or a client that hosts. [`Status::UNAVAILABLE`]
    /// for a player whose client takes no messages from the loader, as one
    /// without Enshwick or not online, and on a client in another's game;
    /// [`Status::BAD_ARGS`] for a place that is not finite or a radius that
    /// is not above 0 and at most 100 000; [`Status::REFUSED`] when the
    /// link to the player has no room.
    pub fog_uncover: extern "system" fn(
        me: ModHandle,
        account: PlayerKey,
        x: f32,
        z: f32,
        radius: f32,
    ) -> Status,
    /// The game language the player of account `key` reads, as the game
    /// numbers them: on a
    /// server, the one that player's game named when it joined, always one
    /// the game knows; `0`, the fallback, for a player whose game runs no
    /// Enshwick, for a key no connected player holds and on a client. Any
    /// thread, re-entrant.
    pub language_of: extern "system" fn(me: ModHandle, key: PlayerKey, out: *mut u32) -> Status,
    /// Adds `callback` with `user` to the join rules, which the server asks
    /// in load order, on the thread the game reads a machine's join on, once
    /// for each account the machine brings and before any of its players is
    /// added. The first [`FILTER_REFUSE`](super::FILTER_REFUSE) refuses the machine: it gets the game's own reply
    /// for an account on the ban list, which its player reads as the host
    /// denying access, and loads no world. The loader writes nothing to the
    /// ban list, and the rules of a mod that is off are not asked. A machine
    /// that already joined keeps playing: the game drops its join message.
    /// Server only: on a client it runs while it hosts a game. A build
    /// without the join path answers [`Status::UNAVAILABLE`]. Allowed during
    /// `enshwick_mod_init` only ([`Status::TOO_LATE`] after it);
    /// `callback` and `user` are kept for the life of the process and never
    /// freed by the loader.
    pub join_rule: extern "system" fn(
        me: ModHandle,
        callback: Option<JoinRuleFn>,
        user: *mut c_void,
    ) -> Status,
    /// Asks for inventory transactions and item data; allowed during
    /// `enshwick_mod_init` only. [`Status::UNAVAILABLE`] when this game build
    /// lacks a piece of them. Requests run in the game server: the dedicated
    /// server, or the client while it hosts.
    pub request_inventory: extern "system" fn(me: ModHandle) -> Status,
    /// Queues `request`, from any thread, for the game's inventory system,
    /// which runs it as one transaction: all or nothing. `done` gets the
    /// result with `user` on the game thread after a later step, unless the
    /// mod is off by then. On an error status nothing is queued and `done` is
    /// never called.
    pub inventory_submit: extern "system" fn(
        me: ModHandle,
        request: *const InventoryRequest,
        done: Option<InventoryResultFn>,
        user: *mut c_void,
    ) -> Status,
    /// Calls `callback` with `user` to answer the game's survivor lookups
    /// that [`SurvivorLookup`](super::SurvivorLookup) names: the lookups of
    /// every mod run in load order, the first that answers deciding, and
    /// the game answers when all pass on. A second call replaces the mod's
    /// own lookup. Runs on the server's worker
    /// threads, in the dedicated server or a hosting client, and for
    /// [`SurvivorLookup::CURSOR`](super::SurvivorLookup::CURSOR) on a client alone too. Allowed during
    /// `enshwick_mod_init` only; [`Status::UNAVAILABLE`] on a build that
    /// lacks a piece.
    pub survivor_lookup: extern "system" fn(
        me: ModHandle,
        callback: Option<SurvivorLookupFn>,
        user: *mut c_void,
    ) -> Status,
    /// Calls `callback` with `user` after a survivor's own bed check kept its
    /// bed; any filter returning [`FILTER_REFUSE`](super::FILTER_REFUSE) clears it. Server's worker threads.
    /// Allowed during `enshwick_mod_init` only.
    pub survivor_bed_check: extern "system" fn(
        me: ModHandle,
        callback: Option<BedCheckFn>,
        user: *mut c_void,
    ) -> Status,
    /// Calls `callback` with `user` before a summoning staff spawns or moves
    /// a survivor; any filter returning 1 makes the use take the item.
    /// Server's game thread. Allowed during `enshwick_mod_init` only.
    pub summon_filter: extern "system" fn(
        me: ModHandle,
        callback: Option<SummonFilterFn>,
        user: *mut c_void,
    ) -> Status,
    /// Files item `item` as a summoning staff of the survivor `survivor`, so
    /// the game treats it as that survivor's staff, on both sides. Allowed
    /// during `enshwick_mod_init` only.
    pub summon_item: extern "system" fn(me: ModHandle, item: ItemId, survivor: NpcId) -> Status,
    /// Calls `callback` with `user` to merge the statuses the client's
    /// survivor menu shows for a survivor with several entities: the merges
    /// of every mod run in load order, the first that sets a status
    /// deciding, and the game's stays when all pass on. A second call
    /// replaces the mod's own merge. Client only. Allowed during
    /// `enshwick_mod_init` only.
    pub survivor_status: extern "system" fn(
        me: ModHandle,
        callback: Option<SurvivorStatusFn>,
        user: *mut c_void,
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
    /// prop: it moves actors and characters; [`Status::UNAVAILABLE`] where
    /// no server runs or on a build without teleports.
    /// On the server's game thread only.
    pub teleport: extern "system" fn(me: ModHandle, request: *const TeleportRequest) -> Status,
    /// Asks the server to spawn an entity from a template, as its admin
    /// spawn does, and writes the new entity's id to `out`; the entity is in
    /// the world after the next step. [`Status::BAD_ARGS`] for a zero
    /// template; a template the game does not know spawns nothing. On the
    /// server's game thread only.
    pub spawn:
        extern "system" fn(me: ModHandle, request: *const SpawnRequest, out: *mut Entity) -> Status,
    /// Calls `callback` with `user` on damage about to reach an entity's
    /// health in the server's world, before the game applies it: hits,
    /// falls and the shroud; what the filters leave in the
    /// [`DamageCheck`](super::DamageCheck) is what the game applies. Runs on the game's worker
    /// threads, in load order. Allowed during `enshwick_mod_init` only.
    pub damage_filter: extern "system" fn(
        me: ModHandle,
        callback: Option<DamageFilterFn>,
        user: *mut c_void,
    ) -> Status,
    /// Calls `callback` with `user` after a summoning staff's use spawned or
    /// moved a survivor, with [`Summon::consumed`](super::Summon::consumed) set; its return value is
    /// ignored. On the thread of the use, as [`HostApi::summon_filter`].
    /// Allowed during `enshwick_mod_init` only.
    pub summoned: extern "system" fn(
        me: ModHandle,
        callback: Option<SummonFilterFn>,
        user: *mut c_void,
    ) -> Status,
    /// Defines the stat kind `id` with `def`, or opens it with a null
    /// `def`, and writes its index and key to `out`. `id` is a full content
    /// id, `<mod>:<name>`; the text that names the kind is `<name>.name` in
    /// its owner's texts, which the loader derives. A mod defines kinds in
    /// its own namespace and opens the loader's built-in ones
    /// (`enshwick:<name>`) and those of other mods. Every mod gets the same
    /// index for one id. [`Status::BAD_ARGS`] for a malformed id or
    /// definition; [`Status::REFUSED`] for a kind defined twice or in
    /// another mod's namespace, or a kind of another loaded mod whose
    /// [`Manifest::exports`](super::Manifest::exports) lack its kind and name and the
    /// name it is a part of, with the id and the owner's exports logged.
    /// Allowed during `enshwick_mod_init` only.
    pub define_stat: extern "system" fn(
        me: ModHandle,
        id: Str,
        def: *const StatSpec,
        out: *mut StatRef,
    ) -> Status,
    /// Writes the definition of stat `stat` to `def` and its full id to
    /// `id`, a string the loader keeps for the process; [`Status::MISSING`]
    /// for a kind opened but never defined. Any thread.
    pub stat_info:
        extern "system" fn(me: ModHandle, stat: u32, def: *mut StatSpec, id: *mut Str) -> Status,
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
        modifiers: *const StatModifier,
        count: u32,
    ) -> Status,
    /// Calls `callback` with `user` for every stat value that changed on a
    /// watched entity, on the game thread after the step that changed it.
    /// Allowed during `enshwick_mod_init` only.
    pub stats_changed: extern "system" fn(
        me: ModHandle,
        callback: Option<StatChangeFn>,
        user: *mut c_void,
    ) -> Status,
    /// Writes up to `capacity` modifiers that count toward stat `stat` on
    /// `entity`, in the order they apply, and how many there are to
    /// `count`; every stat for `u32::MAX`. On the server's game thread at a
    /// quiet moment ([`Status::WRONG_THREAD`] inside a step).
    pub stat_sources: extern "system" fn(
        me: ModHandle,
        entity: Entity,
        stat: u32,
        out: *mut StatSource,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Watches `entity` for the items it wears and the buffs it has, or
    /// stops `me`'s watch for `watch` 0: the entity stays watched while any
    /// mod watches it, players are always watched, and so is every entity a
    /// mod keeps a modifier on. Any thread.
    pub stat_watch: extern "system" fn(me: ModHandle, entity: Entity, watch: u8) -> Status,
    /// Defines the affix `id` with `def` and writes its index and key to
    /// `out`. `id` is a full content id in the mod's namespace,
    /// `<mod>:<name>`; the text that names the affix is `<name>.name` in the
    /// mod's texts, which the loader derives. [`Status::BAD_ARGS`] for a
    /// malformed id or definition, [`Status::REFUSED`] for an affix defined
    /// twice or in another mod's namespace. Allowed during
    /// `enshwick_mod_init` only.
    pub define_affix: extern "system" fn(
        me: ModHandle,
        id: Str,
        def: *const AffixSpec,
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
    /// and `rarity`; the text is the loader's, valid for the process. Any
    /// thread.
    pub affix_info:
        extern "system" fn(me: ModHandle, affix: u32, text: *mut Str, rarity: *mut u8) -> Status,
    /// Calls `callback` with `user` for each player's action with an item
    /// in the server's world, on the thread of the system that runs it:
    /// before the game uses, throws away, equips or takes off an item, where
    /// [`FILTER_REFUSE`](super::FILTER_REFUSE) refuses it as an action already run, after a pickup or a
    /// placement, and after the game took the item of a use every filter
    /// allowed ([`ItemAction::CONSUMED`](crate::abi::ItemAction::CONSUMED), so a
    /// use reaches a filter twice), whose answers are ignored. A
    /// filter ignores a kind it does not know. Allowed during
    /// `enshwick_mod_init` only: [`Status::TOO_LATE`] after,
    /// [`Status::UNAVAILABLE`] on a build without the player input it
    /// reads.
    pub item_filter: extern "system" fn(
        me: ModHandle,
        callback: Option<ItemFilterFn>,
        user: *mut c_void,
    ) -> Status,
    /// Drops `request.count` new items of `request.item` on the ground at
    /// `request.position`, as one loose stack any player can pick up, as
    /// the game spills loot; writes the new pickup's entity to `out`. The
    /// game falls it to the ground and does not save it.
    /// [`Status::BAD_ARGS`] for a null `request` or `out`, a count of 0 or over
    /// 65535, or a rarity the game lacks; [`Status::MISSING`] for an item
    /// the game does not know; [`Status::WRONG_THREAD`] off the server's
    /// game thread; [`Status::UNAVAILABLE`] on a client, a build without
    /// the pickup or the item lookup, or while no item registry or world
    /// exists yet, which a later call may find. The loader
    /// reads the pickup off the game thread as the server starts, before its
    /// world loads, in about a tenth of a second; a call before that read
    /// ends waits for the rest of it. `request` and `out` are borrowed for
    /// the call.
    pub drop_items:
        extern "system" fn(me: ModHandle, request: *const DropRequest, out: *mut Entity) -> Status,
    /// Calls `callback` with `user` when the game asks whether a character
    /// unlocked skill node `node`; the first filter that answers decides.
    /// Runs on both sides, on the asking thread. Allowed during
    /// `enshwick_mod_init` only.
    pub skill_override: extern "system" fn(
        me: ModHandle,
        node: crate::abi::SkillNode,
        callback: Option<SkillCheckFn>,
        user: *mut c_void,
    ) -> Status,
    /// Calls `callback` with `user` when a comfort provider registers its
    /// comfort; filters run in load order, each on the last one's value.
    /// Server's worker threads. Allowed during `enshwick_mod_init` only.
    pub comfort_filter: extern "system" fn(
        me: ModHandle,
        callback: Option<ComfortFilterFn>,
        user: *mut c_void,
    ) -> Status,
    /// Calls `callback` with `user` before the server applies the
    /// characters' gained experience, once per batch; filters run in load
    /// order. Server's worker threads. Allowed during `enshwick_mod_init`
    /// only.
    pub experience_filter: extern "system" fn(
        me: ModHandle,
        callback: Option<ExperienceFilterFn>,
        user: *mut c_void,
    ) -> Status,
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
    /// Grants or takes back the knowledge value `change` names.
    ///
    /// A grant to a character or the world works as the game grants: the
    /// next step applies it with what follows (experience, unlocks, clients,
    /// the save); [`Status::BAD_ARGS`] for a bit the game does not expect,
    /// [`Status::BUSY`] for a full queue. A revoke clears a
    /// [`KnowledgeInfo::BOOL`], takes `value` from a
    /// [`KnowledgeInfo::COUNTER`] (down to 0) or clears bit `value` of
    /// [`KnowledgeInfo::FLAGS`], sends it to the clients again and is saved
    /// with the next save; what followed the grant, such as experience,
    /// stays. A change of an account applies now when its character is in
    /// the game, else once its next character has loaded in this world; the
    /// loader keeps waiting changes with the world, and refuses an id of the
    /// world's with [`Status::BAD_ARGS`].
    ///
    /// `change` is borrowed for the call. [`Status::BAD_ARGS`] for a null or
    /// short `change`, both `account` and `entity` set, an unknown id or
    /// `op`, a kind that is not the id's or a bit past 31;
    /// [`Status::REFUSED`] for a character not in the game, or whose save has
    /// not loaded yet, or a player's id named for the world;
    /// [`Status::UNAVAILABLE`] for an account while no world runs. On the
    /// server's game thread only.
    pub change_knowledge:
        extern "system" fn(me: ModHandle, change: *const KnowledgeChange) -> Status,
    /// Up to `capacity` quest ids of the running game, and how many there
    /// are. On the server's game thread only.
    pub quests: extern "system" fn(
        me: ModHandle,
        out: *mut KnowledgeId,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Registers a filter on the knowledge clients ask the server for in
    /// their input, which a vanilla game grants unchecked. Filters run in
    /// load order after the loader's guard, each with the verdict so far,
    /// and the last answer decides. It runs on the server's worker threads,
    /// in the dedicated server or a hosting client. [`Status::UNAVAILABLE`] on
    /// a build without the input site. Allowed during `enshwick_mod_init`
    /// only.
    pub knowledge_input_filter: extern "system" fn(
        me: ModHandle,
        callback: Option<KnowledgeInputFilterFn>,
        user: *mut c_void,
    ) -> Status,
    /// Writes to `holds` 1 when the game query `query` (a query of the
    /// game's query database, which quest steps and recipes name by id)
    /// holds for the player whose character is `entity`, as the game reads
    /// it when it pays a quest (the player's own state), or for the world
    /// when `entity` is `Entity(0)`, and 0 when it does not.
    /// [`Status::UNAVAILABLE`] on a build without the game's query calls or
    /// while no world runs, [`Status::MISSING`] for a query the game does
    /// not know or has not evaluated yet; [`Status::REFUSED`] for a player not in the game;
    /// [`Status::WRONG_THREAD`] off the game thread; [`Status::BAD_ARGS`]
    /// for a null `holds`. `holds` is borrowed for the call.
    pub game_query:
        extern "system" fn(me: ModHandle, entity: Entity, query: u32, holds: *mut u32) -> Status,
    /// Runs the console line `line` as the server's console, an
    /// administrator with no character, and writes its reply to `out`, valid
    /// until the next call on this thread: `enshwick ...` for the loader's
    /// own tools, or a mod's id and the text form of one of its
    /// [actions](HostApi::actions). On the server's game thread only;
    /// [`Status::MISSING`] when nothing answers the line's first word.
    pub run_console: extern "system" fn(me: ModHandle, line: Str, out: *mut Str) -> Status,
    /// Calls `callback` with `user` before a player's interaction that
    /// [`Interaction`](super::Interaction) names; any filter returning [`FILTER_REFUSE`](super::FILTER_REFUSE) refuses it. Runs in the
    /// server's world only, on the thread of the system that checks it.
    /// Allowed during `enshwick_mod_init` only; [`Status::UNAVAILABLE`] on a
    /// build that lacks a piece.
    pub interaction_filter: extern "system" fn(
        me: ModHandle,
        callback: Option<InteractionFilterFn>,
        user: *mut c_void,
    ) -> Status,
    /// Calls `callback` with `user` on each player's craft request in the
    /// server's world, on the game thread before the crafting system reads
    /// it; any filter returning [`FILTER_REFUSE`](super::FILTER_REFUSE) refuses it, and nothing is made or taken.
    /// Allowed during `enshwick_mod_init` only.
    pub craft_filter: extern "system" fn(
        me: ModHandle,
        callback: Option<CraftFilterFn>,
        user: *mut c_void,
    ) -> Status,
    /// Calls `callback` with `user` after the game's cursor check allows a
    /// placement; any filter returning [`FILTER_REFUSE`](super::FILTER_REFUSE) refuses it as the game refuses a
    /// placement without permission. Runs on both sides, on the cursor
    /// systems' worker threads: the server's check decides, a client's
    /// shows. Allowed during `enshwick_mod_init` only.
    pub build_filter: extern "system" fn(
        me: ModHandle,
        callback: Option<BuildFilterFn>,
        user: *mut c_void,
    ) -> Status,
    /// Registers the mod's player actions: the loader asks `callback` with
    /// `user`, on the server's game thread, for the actions a player may
    /// use, to run one a player picked in the interface or the console
    /// named. One registration per
    /// mod. Allowed during
    /// `enshwick_mod_init` only.
    pub actions:
        extern "system" fn(me: ModHandle, callback: Option<ActionFn>, user: *mut c_void) -> Status,
    /// Declares a control of the calling mod from `spec`, with `callback`
    /// and `user` for its events; allowed during `enshwick_mod_init` only.
    /// The client fires it from its keys while the player is in the world
    /// with no menu, chat or inventory open, and never takes the key from
    /// the game; the server fires one with a `link` when that player's game
    /// action fires. [`Status::BAD_ARGS`] for a malformed name, key or
    /// unknown `link`, [`Status::REFUSED`] for a name the mod declared
    /// before, [`Status::TOO_LATE`] after init.
    pub control: extern "system" fn(
        me: ModHandle,
        spec: *const ControlSpec,
        callback: Option<ControlFn>,
        user: *mut c_void,
    ) -> Status,
    /// On a client, asks the server to run the calling mod's player action
    /// `action` with `args` in the sheet's encoding, as the interface does,
    /// on the object `target` names, null for none: the server hands the
    /// target to the action's `at` and handler in
    /// [`ActionCall::target`](crate::abi::ActionCall::target), checked as any
    /// object a client names, runs the handler and shows the reply as a
    /// notice. [`Status::UNAVAILABLE`] on a server or while the server takes
    /// no messages; [`Status::BAD_ARGS`] for unreadable strings or a target
    /// position that is not finite. Any thread; strings and `target` are
    /// borrowed for the call.
    pub run_action: extern "system" fn(
        me: ModHandle,
        action: Str,
        args: Str,
        target: *const ActionTarget,
    ) -> Status,
    /// Adds `callback` with `user` to the chat filters, which run in load
    /// order on the server's game thread, inside the game's chat post, on
    /// every line a player typed that the chat mute let through; the first
    /// [`ChatRewrite::DROP`](crate::abi::ChatRewrite::DROP) drops the line,
    /// and a rewrite hands the new text to the filters after it. Lines the
    /// loader posts for mods pass no filter. Server only: on a client it
    /// runs while it hosts a game ([`Status::UNAVAILABLE`] on a build without
    /// the chat post). Allowed during `enshwick_mod_init` only
    /// ([`Status::TOO_LATE`] after it); `callback` and `user` are kept for
    /// the life of the process and never freed by the loader.
    pub chat_filter: extern "system" fn(
        me: ModHandle,
        callback: Option<ChatFilterFn>,
        user: *mut c_void,
    ) -> Status,
    /// Names the calling mod's [actions](HostApi::actions) as commands: a
    /// line `/<name> <action> <values>`, or the same with `alias`, runs
    /// one of them from a player's chat, the server's console or
    /// [`run_command`](HostApi::run_command), with the caller's rights. The
    /// name is the part of the mod's id after the author. `alias` is 1 to
    /// 32 of `[a-z0-9-]`, shorter
    /// than the name, and neither `enshwick` nor `help`.
    /// [`Status::BAD_ARGS`] for a malformed or unreadable alias, or a mod
    /// whose name is `enshwick` or `help`; [`Status::REFUSED`] for a second
    /// call, or when another mod's commands have the same name or another
    /// mod's whole id is that name, which the
    /// loader logs and reports naming both mods, the mod running on without
    /// commands. An alias that another mod's commands, name or id hold goes
    /// to that mod: the call returns [`Status::OK`], the mod's commands run
    /// by their name alone, and the loader logs and reports it once naming
    /// both mods. Allowed during `enshwick_mod_init` only; `alias` is
    /// borrowed for the call.
    pub commands: extern "system" fn(me: ModHandle, alias: Str) -> Status,
    /// Runs the command line `line`, such as `/adm online`, as a remote
    /// caller named `label` (for the log and the handler, at most 64 bytes)
    /// with the game's permission bits `permissions` (as
    /// [`PlayerInfo::permissions`](super::PlayerInfo::permissions)), never as
    /// a player, and writes the mod's [`ActionReply`](super::ActionReply) to
    /// `out` as far as the header the caller filled reaches, its text valid
    /// until the next call on this thread. The
    /// leading `/` is optional. On the server's game thread only
    /// ([`Status::WRONG_THREAD`]); [`Status::MISSING`] when no running mod
    /// has the line's first word as its command name or alias;
    /// [`Status::BAD_ARGS`] for a line over 1,024 bytes, an empty line or
    /// an unreadable string.
    pub run_command: extern "system" fn(
        me: ModHandle,
        line: Str,
        label: Str,
        permissions: u32,
        out: *mut ActionReply,
    ) -> Status,
    /// Up to `capacity` command sets of the running mods, in load order, and
    /// how many there are. Entries are `entry_size` bytes apart, the
    /// caller's `size_of::<CommandInfo>()`; the loader fills what both sides
    /// know of each and its own [`Header`]. The strings stay valid until the
    /// next call on this thread. [`Status::BAD_ARGS`] for a non-null `out`
    /// whose entries are shorter than a header. Any thread.
    pub command_list: extern "system" fn(
        me: ModHandle,
        out: *mut CommandInfo,
        entry_size: u32,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Tells the loader that who may run the calling mod's actions changed,
    /// such as when a rule reads settings that just changed: the server
    /// asks the mod again and carries each player the controls they may
    /// run now, within two seconds. Any thread. [`Status::MISSING`] for a
    /// mod without actions.
    pub actions_changed: extern "system" fn(me: ModHandle) -> Status,
    /// Shows the calling mod's window `id`, 1 to 32 of `[a-z0-9-]`, to the
    /// `count` players whose account keys `players` holds; closes it on
    /// their clients when `change` is [`WINDOW_CLOSE`](super::WINDOW_CLOSE); or, when it is [`WINDOW_UPDATE`](super::WINDOW_UPDATE), updates it
    /// only on the clients that still show it, opening none. When a client closes
    /// a window or its player leaves, the loader tells the mod with
    /// [`ActionCall::WINDOW_CLOSED`](super::ActionCall::WINDOW_CLOSED).
    /// After the server's step the
    /// loader asks the mod for the window as each player sees it
    /// ([`ActionCall::WINDOW`](super::ActionCall::WINDOW)) and carries the
    /// text to that player's client, which draws it; an empty answer closes
    /// it there. A player not online is skipped. On the server only
    /// ([`Status::UNAVAILABLE`] on a client); [`Status::BAD_ARGS`] for a
    /// malformed id, `count` over 64 or a null `players` with a non-zero
    /// `count`; [`Status::REFUSED`] for a mod without
    /// [actions](HostApi::actions), or while 256 shows or closes wait.
    /// Any thread; `id` and `players` are borrowed for the call.
    pub action_window: extern "system" fn(
        me: ModHandle,
        id: Str,
        players: *const PlayerKey,
        count: u32,
        change: u32,
    ) -> Status,
    /// Calls `callback` with `user` once per frame of the client's
    /// interface, on the thread that draws it, while the mod is on; the
    /// callback draws with [`HostApi::ui_widget`]. Client only
    /// ([`Status::UNAVAILABLE`] on a server); when the interface could not be
    /// linked, the callback never runs. Allowed during `enshwick_mod_init`
    /// only.
    pub ui_frame:
        extern "system" fn(me: ModHandle, callback: Option<UiFrameFn>, user: *mut c_void) -> Status,
    /// Draws `widget` in the frame `frame` names and writes back its
    /// `index`, `value` and `result`. Only inside the mod's frame callback,
    /// or inside its tooltip callback with the [`UiTooltipItem`](crate::abi::UiTooltipItem)'s `token` as
    /// `frame`, where a label adds a line to the item's tooltip
    /// ([`Status::WRONG_THREAD`] elsewhere); [`Status::REFUSED`] for a
    /// widget outside a window, an [`UiWidget::END`] without one or a tooltip
    /// line that is not a label; [`Status::BUSY`] for a tooltip line past
    /// [`UiTooltipItem::MAX_LINES`](crate::abi::UiTooltipItem::MAX_LINES); [`Status::MISSING`] for a widget this game
    /// build lacks or an item slot of an item the client does not know;
    /// [`Status::BAD_ARGS`] for a malformed widget or an item slot of count 0.
    pub ui_widget: extern "system" fn(me: ModHandle, frame: u64, widget: *mut UiWidget) -> Status,
    /// Queues `dialog` and calls `callback` with the answer and `user` once
    /// the player gives it, on the thread that draws the interface; dialogs
    /// show one at a time, in order, in the game's style. Any thread. Client
    /// only ([`Status::UNAVAILABLE`] on a server); when the interface could not
    /// be linked, the dialog shows as a system message box.
    /// [`Status::BUSY`] when too many wait; [`Status::WRONG_THREAD`] for
    /// [`UiDialog::WAITING`] on the thread that draws the interface. On an
    /// error status `callback` is never called.
    pub ui_dialog: extern "system" fn(
        me: ModHandle,
        dialog: *const UiDialog,
        callback: Option<UiAnswerFn>,
        user: *mut c_void,
    ) -> Status,
    /// Calls `callback` with `user` once per frame while the game draws its
    /// HUD in game, hidden with it; the callback draws labels and bars with
    /// [`HostApi::ui_widget`] at their `anchor`. Client only
    /// ([`Status::UNAVAILABLE`] on a server). Allowed during
    /// `enshwick_mod_init` only.
    pub ui_hud:
        extern "system" fn(me: ModHandle, callback: Option<UiFrameFn>, user: *mut c_void) -> Status,
    /// Replaces the mod's markers on the client's map with the `count` at
    /// `markers`, copied; none clears them. Any thread. Client only
    /// ([`Status::UNAVAILABLE`] on a server); [`Status::BAD_ARGS`] past 256
    /// markers. A marker with a non-zero [`UiMarker::marker_type`] has the
    /// loader read the registry as [`HostApi::map_marker_types`] does, on
    /// the calling thread at the first such call: [`Status::BAD_ARGS`] for
    /// a type it does not list, [`Status::UNAVAILABLE`] when it does not
    /// read.
    /// The mod's markers stay as they were after either. A type the
    /// registry the map runs on lacks shows as the loader's own choice.
    /// Every entry is as large as the header of the first one says.
    pub ui_markers:
        extern "system" fn(me: ModHandle, markers: *const UiMarker, count: u32) -> Status,
    /// Asks the loader to keep a copy of the client's fog of war for this
    /// mod. Client only ([`Status::UNAVAILABLE`] on a dedicated server).
    /// Allowed during `enshwick_mod_init` only.
    pub request_fog: extern "system" fn(me: ModHandle) -> Status,
    /// Writes the fog's [`FogInfo`] to `info`, then copies the box of
    /// `width` × `height` pixels at (`x`, `y`) into `out`, row by row, at
    /// most `capacity` bytes; a box of no pixels copies nothing. Any thread.
    /// [`Status::UNAVAILABLE`] while the client is in no world, when this build
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
    /// Up to `capacity` marker types of the client's marker type registry,
    /// in the registry's order, and how many there are, read from the
    /// game's archive at the first call and kept, as every list answers.
    /// [`Status::UNAVAILABLE`] on a dedicated
    /// server, whose archive has no registry, and when the registry does
    /// not read on this build. Any thread.
    pub map_marker_types: extern "system" fn(
        me: ModHandle,
        out: *mut MapMarkerTypeInfo,
        entry_size: u32,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Adds a panel at an edge of the game's screen `spec` names and calls
    /// `callback` with `user` once per frame while that screen is open, on
    /// the thread that draws the interface; the callback draws into the
    /// panel with [`HostApi::ui_widget`], without a window of its own
    /// ([`Status::REFUSED`] for [`UiWidget::WINDOW`] and [`UiWidget::END`]).
    /// Panels at one edge share one window, which opens only once a panel
    /// draws a widget that needs it. The panel never changes the game
    /// screen's own layout. Client only ([`Status::UNAVAILABLE`] on a server,
    /// [`Status::MISSING`] for a screen this game build lacks); [`Status::BAD_ARGS`] for a null
    /// or short `spec`, an unknown edge, an unreadable screen name, or a
    /// screen on which the player is in the world with no window open
    /// (`InGame`, `NoUi`, `TakeScreenshot`, the multitools' building
    /// screens); allowed during `enshwick_mod_init` only, on the thread that
    /// runs it, once per panel; the loader never calls `callback` inside this
    /// call. `spec` and its screen name are borrowed for
    /// the call; `callback` and `user` are kept for the life of the process
    /// and never freed by the loader.
    pub ui_extension: extern "system" fn(
        me: ModHandle,
        spec: *const UiExtension,
        callback: Option<UiFrameFn>,
        user: *mut c_void,
    ) -> Status,
    /// Writes to `out` the game controls the local player holds, the
    /// `Bitmask64` of the game's `PlayerInputType` as the client's
    /// interface read it in its last step: gameplay controls only, not the
    /// client-only ones such as menu keys. The bits are the game's, which a
    /// game build may renumber: find each by its name among the constants
    /// [`HostApi::types`] lists for `PlayerInputType`, never as a fixed
    /// value. The loader reads the mask only once a mod asked for it: the
    /// first call, of any mod, starts the reads, and the interface's next
    /// step reads it; the safe layer calls it when a mod opens its input.
    /// Client only; [`Status::UNAVAILABLE`] on a server, outside a game,
    /// from the first call until that step, while no step read it for a
    /// second, or on a build without the read; [`Status::BAD_ARGS`] for a
    /// null `out`. Any thread; `out` is borrowed for the call.
    pub local_held: extern "system" fn(me: ModHandle, out: *mut u64) -> Status,
    /// Copies the local input sampler's cached diagnostics without requesting reads.
    /// Client only ([`Status::UNAVAILABLE`] on a server); callable on any thread.
    /// No game memory is read. Flags are weakly consistent and age is not session identity.
    /// `out` is a headed, writable `LocalInputInfo`. ABI 12.1.
    pub local_input_status: extern "system" fn(me: ModHandle, out: *mut LocalInputInfo) -> Status,
    /// Writes to `out`, a headed [`ClientSnapshot`], the client's state in
    /// one copy: its session, the open screens and the in-game menu's tab
    /// from the interface's last frame, and the local player's held
    /// controls. The first call, of any mod, starts the interface's reads
    /// and the held controls' reads, so it answers [`ClientSnapshot::NEVER`];
    /// the safe layer calls it when a mod opens its input. Every call after
    /// a frame drew a game answers its copy; until one did, as in the main
    /// menu before the first world, it answers `NEVER`. Client only:
    /// [`Status::UNAVAILABLE`] on a server; [`Status::BAD_ARGS`] for a null
    /// or unheaded `out`. Any thread; it runs no mod callback, and `out` is
    /// borrowed for the call. ABI 12.1.
    pub client_snapshot: extern "system" fn(me: ModHandle, out: *mut ClientSnapshot) -> Status,
    /// Up to `capacity` parts and features of the loader that are off, in
    /// the order they turned off, and how many there are. Entries are
    /// `entry_size` bytes apart, the caller's `size_of::<LoaderOff>()`; the
    /// loader fills what both sides know of each and its own [`Header`].
    /// Everything it does not list runs. The strings stay valid for the
    /// process, since the loader keeps each entry's first reason and drops
    /// none. Any thread; it runs no mod callback. ABI 12.1.
    pub loader_off: extern "system" fn(
        me: ModHandle,
        out: *mut LoaderOff,
        entry_size: u32,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Queues `spec`, a headed [`ClientCommand`], and writes its id to `id`:
    /// the call answers only that it is queued, and
    /// [`HostApi::client_command_status`] tells what came of it. One command
    /// of each kind waits per mod: a second answers [`Status::REFUSED`]
    /// until the first ends. [`Status::BAD_ARGS`] for an unknown kind, a
    /// target the kind does not take, a look past
    /// [`ClientCommand::MAX_LOOK`], a direction longer than 1 or a hold past
    /// [`ClientCommand::MAX_HOLD_MS`]; [`Status::UNAVAILABLE`] on a server
    /// and on a build without what the kind needs. Any thread; the command
    /// runs later on the client's game thread, never inside a mod callback.
    /// ABI 12.1.
    pub client_command:
        extern "system" fn(me: ModHandle, spec: *const ClientCommand, id: *mut u64) -> Status,
    /// Writes to `out`, a headed [`ClientCommandStatus`], where the mod's
    /// command `id` stands. The loader keeps the last 64 commands' outcomes;
    /// [`Status::UNAVAILABLE`] for an id it no longer keeps or another mod's.
    /// Any thread. ABI 12.1.
    pub client_command_status:
        extern "system" fn(me: ModHandle, id: u64, out: *mut ClientCommandStatus) -> Status,
    /// Adds `material`, a headed [`TerrainMaterialAdd`], as a terrain
    /// material of the game, and writes the voxel id it has for this run to
    /// `id`. The loader gives mods' materials the terrain ids no palette of
    /// the game names, in load order, and serves the material's palette
    /// entry, rules and look on both sides; saves name its voxels by its
    /// GUID. [`Status::BAD_ARGS`] for a `like` or `fallback` that is no
    /// terrain material of the game or a change it does not know;
    /// [`Status::REFUSED`] for a GUID added before and once every free id is
    /// taken; [`Status::UNAVAILABLE`] on a build whose archive lacks the
    /// palette or the rules. Allowed during `enshwick_mod_init` only.
    /// ABI 12.2.
    pub terrain_material_add: extern "system" fn(
        me: ModHandle,
        material: *const TerrainMaterialAdd,
        id: *mut u8,
    ) -> Status,
    /// Gives every object placed from `spec`'s item, a headed
    /// [`InteractionPrompt`], its prompt: the game verb and the replaced
    /// action in the server's world, the mod's text on this client. One
    /// prompt per item across all mods. Allowed during `enshwick_mod_init`
    /// only; [`Status::BAD_ARGS`] for a null or short `spec`, item 0, an
    /// unknown flag or a prompt that changes nothing; [`Status::REFUSED`]
    /// for an item another prompt has; [`Status::UNAVAILABLE`] for a verb
    /// or an action on a build without the server's writes, while a build
    /// without the client's text site turns the text off alone. ABI 12.3.
    pub interaction_prompt:
        extern "system" fn(me: ModHandle, spec: *const InteractionPrompt) -> Status,
    /// Registers `callback` to add lines to the tooltip and detail panel of
    /// every item with an entity of its own, after the game's rows. It runs
    /// on the client's main thread, once each time the tooltip shows another
    /// item, with a [`UiTooltipItem`](crate::abi::UiTooltipItem) whose token takes up to
    /// [`UiTooltipItem::MAX_LINES`](crate::abi::UiTooltipItem::MAX_LINES) [`UiWidget::LABEL`] widgets through
    /// [`HostApi::ui_widget`]; the loader keeps the lines and draws them
    /// each frame the tooltip shows. Client only, during init:
    /// [`Status::UNAVAILABLE`] on a server, [`Status::BAD_ARGS`] for a null
    /// callback. ABI 12.4.
    pub ui_item_tooltip: extern "system" fn(
        me: ModHandle,
        callback: Option<UiTooltipFn>,
        user: *mut c_void,
    ) -> Status,
    /// Replaces the mod's change to the permissions of the player in game
    /// with `account`: `deny` takes bits from the player, `grant` gives
    /// bits, both masks of [`PlayerInfo`](super::PlayerInfo)'s permission
    /// bits; both 0 ends the change. The server holds the player to
    /// `(group | every grant) & !every deny`, with `group` the bits of the
    /// player's user group, from its next step on, so the game's own checks
    /// refuse what a taken bit guards, and the loader's own reads of the
    /// player's bits give the bits in force; the player's client still acts
    /// by its group's bits. A change ends when the mod ends it or turns
    /// off, when the player leaves and when the world ends; ending one
    /// needs no player in game. [`Status::BAD_ARGS`] for account 0 or a bit
    /// past the permissions; [`Status::REFUSED`] for a new change to an
    /// account not in game; [`Status::WRONG_THREAD`] off the server's game
    /// thread; [`Status::UNAVAILABLE`] outside a server and on a build
    /// without the permission table, the step link or the player removal
    /// link. ABI 12.5.
    pub change_permissions:
        extern "system" fn(me: ModHandle, account: PlayerKey, deny: u32, grant: u32) -> Status,
    /// Calls `callback` once for transaction `id` of this mod, with where it
    /// stands and `user`, on the server's game thread after the first step
    /// that ends with it no longer [`TransactionInfo::PENDING`]: written
    /// whole ([`TransactionInfo::APPLIED`], which says when a prop may still
    /// be absent), or failed, refused, undoing or undone. A step at least
    /// `within_ns` nanoseconds after the call runs it as it stands, still
    /// pending perhaps. The `info` is null when the loader forgot the
    /// transaction meanwhile. A step runs its due callbacks in the order
    /// they were asked, and a callback may ask again. When a client's hosted
    /// game ends, every callback kept runs with its transaction failed or
    /// expired, never in the next game; a dedicated server that stops runs
    /// none. A mod the loader turned off loses its callbacks unrun. [`Status::BAD_ARGS`] for a null `callback`;
    /// [`Status::MISSING`] for an id this mod did not get or one the loader
    /// forgot; [`Status::UNAVAILABLE`] once the transactions are off, which
    /// runs every callback kept at once. Nothing is kept on an error. Any
    /// thread. ABI 12.7.
    pub transaction_settled: extern "system" fn(
        me: ModHandle,
        id: u64,
        within_ns: u64,
        callback: Option<TransactionSettledFn>,
        user: *mut c_void,
    ) -> Status,
}

/// The largest radius [`HostApi::fog_uncover`] takes, in metres.
pub const MAX_UNCOVER_RADIUS: f32 = 100_000.0;

// Every function keeps its slot: 24 header and version bytes, then 8 per slot.
const _: () = {
    use core::mem::offset_of;
    assert!(offset_of!(HostApi, header) == 0);
    assert!(offset_of!(HostApi, host_version) == 8);
    assert!(offset_of!(HostApi, build) == 24);
    assert!(offset_of!(HostApi, symbol) == 24 + 8);
    assert!(offset_of!(HostApi, system) == 24 + 2 * 8);
    assert!(offset_of!(HostApi, field_offset) == 24 + 3 * 8);
    assert!(offset_of!(HostApi, type_size) == 24 + 4 * 8);
    assert!(offset_of!(HostApi, hook) == 24 + 5 * 8);
    assert!(offset_of!(HostApi, caller_frame) == 24 + 6 * 8);
    assert!(offset_of!(HostApi, fault) == 24 + 7 * 8);
    assert!(offset_of!(HostApi, log) == 24 + 8 * 8);
    assert!(offset_of!(HostApi, settings) == 24 + 9 * 8);
    assert!(offset_of!(HostApi, publish) == 24 + 10 * 8);
    assert!(offset_of!(HostApi, service) == 24 + 11 * 8);
    assert!(offset_of!(HostApi, constant) == 24 + 12 * 8);
    assert!(offset_of!(HostApi, data_dir) == 24 + 13 * 8);
    assert!(offset_of!(HostApi, log_level) == 24 + 14 * 8);
    assert!(offset_of!(HostApi, subscribe) == 24 + 15 * 8);
    assert!(offset_of!(HostApi, type_hash) == 24 + 16 * 8);
    assert!(offset_of!(HostApi, component) == 24 + 17 * 8);
    assert!(offset_of!(HostApi, entity_template) == 24 + 18 * 8);
    assert!(offset_of!(HostApi, mods) == 24 + 19 * 8);
    assert!(offset_of!(HostApi, code_range) == 24 + 20 * 8);
    assert!(offset_of!(HostApi, patch_code) == 24 + 21 * 8);
    assert!(offset_of!(HostApi, workshop_recipes) == 24 + 22 * 8);
    assert!(offset_of!(HostApi, hosting) == 24 + 23 * 8);
    assert!(offset_of!(HostApi, server_world) == 24 + 24 * 8);
    assert!(offset_of!(HostApi, join_missing) == 24 + 25 * 8);
    assert!(offset_of!(HostApi, hook_at) == 24 + 26 * 8);
    assert!(offset_of!(HostApi, status) == 24 + 27 * 8);
    assert!(offset_of!(HostApi, subscribe_game_event) == 24 + 28 * 8);
    assert!(offset_of!(HostApi, net_channel) == 24 + 29 * 8);
    assert!(offset_of!(HostApi, net_send) == 24 + 30 * 8);
    assert!(offset_of!(HostApi, net_ready) == 24 + 31 * 8);
    assert!(offset_of!(HostApi, config_schema) == 24 + 32 * 8);
    assert!(offset_of!(HostApi, config_check) == 24 + 33 * 8);
    assert!(offset_of!(HostApi, texts) == 24 + 34 * 8);
    assert!(offset_of!(HostApi, define_event) == 24 + 35 * 8);
    assert!(offset_of!(HostApi, subscribe_mod_event) == 24 + 36 * 8);
    assert!(offset_of!(HostApi, send_event) == 24 + 37 * 8);
    assert!(offset_of!(HostApi, rename_content) == 24 + 38 * 8);
    assert!(offset_of!(HostApi, mod_dir) == 24 + 39 * 8);
    assert!(offset_of!(HostApi, language) == 24 + 40 * 8);
    assert!(offset_of!(HostApi, define_point) == 24 + 41 * 8);
    assert!(offset_of!(HostApi, attach_point) == 24 + 42 * 8);
    assert!(offset_of!(HostApi, run_point) == 24 + 43 * 8);
    assert!(offset_of!(HostApi, event_types) == 24 + 44 * 8);
    assert!(offset_of!(HostApi, feature) == 24 + 45 * 8);
    assert!(offset_of!(HostApi, loop_wait) == 24 + 46 * 8);
    assert!(offset_of!(HostApi, server_wait) == 24 + 47 * 8);
    assert!(offset_of!(HostApi, task_workers) == 24 + 48 * 8);
    assert!(offset_of!(HostApi, serve_call) == 24 + 49 * 8);
    assert!(offset_of!(HostApi, find_call) == 24 + 50 * 8);
    assert!(offset_of!(HostApi, call) == 24 + 51 * 8);
    assert!(offset_of!(HostApi, subscribe_tick) == 24 + 52 * 8);
    assert!(offset_of!(HostApi, set_tick_every) == 24 + 53 * 8);
    assert!(offset_of!(HostApi, set_config) == 24 + 54 * 8);
    assert!(offset_of!(HostApi, subscribe_client_game_event) == 24 + 55 * 8);
    assert!(offset_of!(HostApi, serve_resource) == 24 + 56 * 8);
    assert!(offset_of!(HostApi, add_text) == 24 + 57 * 8);
    assert!(offset_of!(HostApi, define_item_data) == 24 + 58 * 8);
    assert!(offset_of!(HostApi, item_data_get) == 24 + 59 * 8);
    assert!(offset_of!(HostApi, set_item_data) == 24 + 60 * 8);
    assert!(offset_of!(HostApi, content_serve) == 24 + 61 * 8);
    assert!(offset_of!(HostApi, content_read) == 24 + 62 * 8);
    assert!(offset_of!(HostApi, icons_ready) == 24 + 63 * 8);
    assert!(offset_of!(HostApi, declare_content) == 24 + 64 * 8);
    assert!(offset_of!(HostApi, define_entity_data) == 24 + 65 * 8);
    assert!(offset_of!(HostApi, resource_read) == 24 + 66 * 8);
    assert!(offset_of!(HostApi, game_text) == 24 + 67 * 8);
    assert!(offset_of!(HostApi, rebuild_resource) == 24 + 68 * 8);
    assert!(offset_of!(HostApi, recipe_rule) == 24 + 69 * 8);
    assert!(offset_of!(HostApi, recipe_disabled) == 24 + 70 * 8);
    assert!(offset_of!(HostApi, recipe_kind) == 24 + 71 * 8);
    assert!(offset_of!(HostApi, rate) == 24 + 72 * 8);
    assert!(offset_of!(HostApi, set_rate) == 24 + 73 * 8);
    assert!(offset_of!(HostApi, change_data) == 24 + 74 * 8);
    assert!(offset_of!(HostApi, set_change) == 24 + 75 * 8);
    assert!(offset_of!(HostApi, use_content) == 24 + 76 * 8);
    assert!(offset_of!(HostApi, resource_keys) == 24 + 77 * 8);
    assert!(offset_of!(HostApi, content_hashes) == 24 + 78 * 8);
    assert!(offset_of!(HostApi, types) == 24 + 79 * 8);
    assert!(offset_of!(HostApi, game_version) == 24 + 80 * 8);
    assert!(offset_of!(HostApi, crafting_prop) == 24 + 81 * 8);
    assert!(offset_of!(HostApi, read_only) == 24 + 82 * 8);
    assert!(offset_of!(HostApi, resource_served) == 24 + 83 * 8);
    assert!(offset_of!(HostApi, resource_buffer) == 24 + 84 * 8);
    assert!(offset_of!(HostApi, content_name) == 24 + 85 * 8);
    assert!(offset_of!(HostApi, player) == 24 + 86 * 8);
    assert!(offset_of!(HostApi, players) == 24 + 87 * 8);
    assert!(offset_of!(HostApi, chat) == 24 + 88 * 8);
    assert!(offset_of!(HostApi, chat_all) == 24 + 89 * 8);
    assert!(offset_of!(HostApi, set_label) == 24 + 90 * 8);
    assert!(offset_of!(HostApi, rename_filter) == 24 + 91 * 8);
    assert!(offset_of!(HostApi, item_info) == 24 + 92 * 8);
    assert!(offset_of!(HostApi, request_altars) == 24 + 93 * 8);
    assert!(offset_of!(HostApi, altars) == 24 + 94 * 8);
    assert!(offset_of!(HostApi, building_zone_filter) == 24 + 95 * 8);
    assert!(offset_of!(HostApi, base_zones) == 24 + 96 * 8);
    assert!(offset_of!(HostApi, entities) == 24 + 97 * 8);
    assert!(offset_of!(HostApi, destroy) == 24 + 98 * 8);
    assert!(offset_of!(HostApi, save_world) == 24 + 99 * 8);
    assert!(offset_of!(HostApi, world_key) == 24 + 100 * 8);
    assert!(offset_of!(HostApi, component_write) == 24 + 101 * 8);
    assert!(offset_of!(HostApi, notify) == 24 + 102 * 8);
    assert!(offset_of!(HostApi, time_of_day) == 24 + 103 * 8);
    assert!(offset_of!(HostApi, set_time_of_day) == 24 + 104 * 8);
    assert!(offset_of!(HostApi, request_time_speed) == 24 + 105 * 8);
    assert!(offset_of!(HostApi, set_time_speed) == 24 + 106 * 8);
    assert!(offset_of!(HostApi, game_settings) == 24 + 107 * 8);
    assert!(offset_of!(HostApi, set_game_settings) == 24 + 108 * 8);
    assert!(offset_of!(HostApi, buff_types) == 24 + 109 * 8);
    assert!(offset_of!(HostApi, buffs) == 24 + 110 * 8);
    assert!(offset_of!(HostApi, apply_buff) == 24 + 111 * 8);
    assert!(offset_of!(HostApi, remove_buff) == 24 + 112 * 8);
    assert!(offset_of!(HostApi, weather_at) == 24 + 113 * 8);
    assert!(offset_of!(HostApi, weather_fronts) == 24 + 114 * 8);
    assert!(offset_of!(HostApi, set_weather) == 24 + 115 * 8);
    assert!(offset_of!(HostApi, terrain_materials) == 24 + 116 * 8);
    assert!(offset_of!(HostApi, terrain_edit) == 24 + 117 * 8);
    assert!(offset_of!(HostApi, altar_zone_filter) == 24 + 118 * 8);
    assert!(offset_of!(HostApi, altar_zones_changed) == 24 + 119 * 8);
    assert!(offset_of!(HostApi, altar_limit_filter) == 24 + 120 * 8);
    assert!(offset_of!(HostApi, base_limit) == 24 + 121 * 8);
    assert!(offset_of!(HostApi, voxel_read) == 24 + 122 * 8);
    assert!(offset_of!(HostApi, build_rights) == 24 + 123 * 8);
    assert!(offset_of!(HostApi, transaction_commit) == 24 + 124 * 8);
    assert!(offset_of!(HostApi, transaction_undo) == 24 + 125 * 8);
    assert!(offset_of!(HostApi, transaction_status) == 24 + 126 * 8);
    assert!(offset_of!(HostApi, world_quiet) == 24 + 127 * 8);
    assert!(offset_of!(HostApi, transaction_entities) == 24 + 128 * 8);
    assert!(offset_of!(HostApi, world_persists) == 24 + 129 * 8);
    assert!(offset_of!(HostApi, map_markers) == 24 + 130 * 8);
    assert!(offset_of!(HostApi, account_action) == 24 + 131 * 8);
    assert!(offset_of!(HostApi, map_frame) == 24 + 132 * 8);
    assert!(offset_of!(HostApi, request_base_limits) == 24 + 133 * 8);
    assert!(offset_of!(HostApi, set_chat_mute) == 24 + 134 * 8);
    assert!(offset_of!(HostApi, chat_muted) == 24 + 135 * 8);
    assert!(offset_of!(HostApi, set_base_limit) == 24 + 136 * 8);
    assert!(offset_of!(HostApi, save_info) == 24 + 137 * 8);
    assert!(offset_of!(HostApi, hold_saves) == 24 + 138 * 8);
    assert!(offset_of!(HostApi, set_saved) == 24 + 139 * 8);
    assert!(offset_of!(HostApi, progress_write) == 24 + 140 * 8);
    assert!(offset_of!(HostApi, server_stop) == 24 + 141 * 8);
    assert!(offset_of!(HostApi, fog_uncover) == 24 + 142 * 8);
    assert!(offset_of!(HostApi, language_of) == 24 + 143 * 8);
    assert!(offset_of!(HostApi, join_rule) == 24 + 144 * 8);
    assert!(offset_of!(HostApi, request_inventory) == 24 + 145 * 8);
    assert!(offset_of!(HostApi, inventory_submit) == 24 + 146 * 8);
    assert!(offset_of!(HostApi, survivor_lookup) == 24 + 147 * 8);
    assert!(offset_of!(HostApi, survivor_bed_check) == 24 + 148 * 8);
    assert!(offset_of!(HostApi, summon_filter) == 24 + 149 * 8);
    assert!(offset_of!(HostApi, summon_item) == 24 + 150 * 8);
    assert!(offset_of!(HostApi, survivor_status) == 24 + 151 * 8);
    assert!(offset_of!(HostApi, emit) == 24 + 152 * 8);
    assert!(offset_of!(HostApi, teleport) == 24 + 153 * 8);
    assert!(offset_of!(HostApi, spawn) == 24 + 154 * 8);
    assert!(offset_of!(HostApi, damage_filter) == 24 + 155 * 8);
    assert!(offset_of!(HostApi, summoned) == 24 + 156 * 8);
    assert!(offset_of!(HostApi, define_stat) == 24 + 157 * 8);
    assert!(offset_of!(HostApi, stat_info) == 24 + 158 * 8);
    assert!(offset_of!(HostApi, stat_value) == 24 + 159 * 8);
    assert!(offset_of!(HostApi, stat_modify) == 24 + 160 * 8);
    assert!(offset_of!(HostApi, stat_buff) == 24 + 161 * 8);
    assert!(offset_of!(HostApi, stats_changed) == 24 + 162 * 8);
    assert!(offset_of!(HostApi, stat_sources) == 24 + 163 * 8);
    assert!(offset_of!(HostApi, stat_watch) == 24 + 164 * 8);
    assert!(offset_of!(HostApi, define_affix) == 24 + 165 * 8);
    assert!(offset_of!(HostApi, affix_roll) == 24 + 166 * 8);
    assert!(offset_of!(HostApi, item_lines) == 24 + 167 * 8);
    assert!(offset_of!(HostApi, affix_info) == 24 + 168 * 8);
    assert!(offset_of!(HostApi, item_filter) == 24 + 169 * 8);
    assert!(offset_of!(HostApi, drop_items) == 24 + 170 * 8);
    assert!(offset_of!(HostApi, skill_override) == 24 + 171 * 8);
    assert!(offset_of!(HostApi, comfort_filter) == 24 + 172 * 8);
    assert!(offset_of!(HostApi, experience_filter) == 24 + 173 * 8);
    assert!(offset_of!(HostApi, knowledge) == 24 + 174 * 8);
    assert!(offset_of!(HostApi, change_knowledge) == 24 + 175 * 8);
    assert!(offset_of!(HostApi, quests) == 24 + 176 * 8);
    assert!(offset_of!(HostApi, knowledge_input_filter) == 24 + 177 * 8);
    assert!(offset_of!(HostApi, game_query) == 24 + 178 * 8);
    assert!(offset_of!(HostApi, run_console) == 24 + 179 * 8);
    assert!(offset_of!(HostApi, interaction_filter) == 24 + 180 * 8);
    assert!(offset_of!(HostApi, craft_filter) == 24 + 181 * 8);
    assert!(offset_of!(HostApi, build_filter) == 24 + 182 * 8);
    assert!(offset_of!(HostApi, actions) == 24 + 183 * 8);
    assert!(offset_of!(HostApi, control) == 24 + 184 * 8);
    assert!(offset_of!(HostApi, run_action) == 24 + 185 * 8);
    assert!(offset_of!(HostApi, chat_filter) == 24 + 186 * 8);
    assert!(offset_of!(HostApi, commands) == 24 + 187 * 8);
    assert!(offset_of!(HostApi, run_command) == 24 + 188 * 8);
    assert!(offset_of!(HostApi, command_list) == 24 + 189 * 8);
    assert!(offset_of!(HostApi, actions_changed) == 24 + 190 * 8);
    assert!(offset_of!(HostApi, action_window) == 24 + 191 * 8);
    assert!(offset_of!(HostApi, ui_frame) == 24 + 192 * 8);
    assert!(offset_of!(HostApi, ui_widget) == 24 + 193 * 8);
    assert!(offset_of!(HostApi, ui_dialog) == 24 + 194 * 8);
    assert!(offset_of!(HostApi, ui_hud) == 24 + 195 * 8);
    assert!(offset_of!(HostApi, ui_markers) == 24 + 196 * 8);
    assert!(offset_of!(HostApi, request_fog) == 24 + 197 * 8);
    assert!(offset_of!(HostApi, fog_read) == 24 + 198 * 8);
    assert!(offset_of!(HostApi, map_marker_types) == 24 + 199 * 8);
    assert!(offset_of!(HostApi, ui_extension) == 24 + 200 * 8);
    assert!(offset_of!(HostApi, local_held) == 24 + 201 * 8);
    assert!(offset_of!(HostApi, local_input_status) == 24 + 202 * 8);
    assert!(offset_of!(HostApi, client_snapshot) == 24 + 203 * 8);
    assert!(offset_of!(HostApi, loader_off) == 24 + 204 * 8);
    assert!(offset_of!(HostApi, client_command) == 24 + 205 * 8);
    assert!(offset_of!(HostApi, client_command_status) == 24 + 206 * 8);
    assert!(offset_of!(HostApi, terrain_material_add) == 24 + 207 * 8);
    assert!(offset_of!(HostApi, interaction_prompt) == 24 + 208 * 8);
    assert!(offset_of!(HostApi, ui_item_tooltip) == 24 + 209 * 8);
    assert!(offset_of!(HostApi, change_permissions) == 24 + 210 * 8);
    assert!(offset_of!(HostApi, transaction_settled) == 24 + 211 * 8);
    assert!(size_of::<HostApi>() == 24 + 212 * 8);
};
