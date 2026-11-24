#ifndef ENSHWICK_H
#define ENSHWICK_H

/* Written by cbindgen from the enshwick crate's ABI module: run `cargo xtask header`, never edit by hand. */

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#if !defined(_WIN64) || !(defined(_M_X64) || defined(__x86_64__))
#error "enshwick.h describes the ABI of 64-bit x86 Windows only"
#endif

#ifdef __cplusplus
#define ENSHWICK_STATIC_ASSERT(condition, message) static_assert(condition, message)
#define ENSHWICK_ALIGNOF(type) alignof(type)
#else
#define ENSHWICK_STATIC_ASSERT(condition, message) _Static_assert(condition, message)
#define ENSHWICK_ALIGNOF(type) _Alignof(type)
#endif

// The ABI's layout holds whatever packing the including unit sets
// (#pragma pack, /Zp); the asserts at the end of this file prove it.
#pragma pack(push, 8)

// The major version of this interface. A loader refuses a mod built for
// another major.
//
// Any change to the layout other than an append is a new major, with
// `ABI_MINOR` back at 0; the layout tests of this crate hold the pair to
// the committed layout.
#define ENSHWICK_ABI_MAJOR 12

// The minor version of this interface: one more for each append to the
// layout of this `ABI_MAJOR`, a field at the end of a struct that starts
// with a `EnshwickHeader` or a new type.
//
// A loader loads mods built for its minor or an older one, and reads a
// field appended after the mod's minor only where
// the header's `size` covers it.
#define ENSHWICK_ABI_MINOR 22

// How many level zones an `EnshwickAltarInfo` has room for: a bound of the ABI,
// above the levels the game gives an altar in its reflected
// `Upgrade.levels` list, which `EnshwickAltarInfo.level_count` reports.
#define ENSHWICK_ALTAR_LEVELS 16

// The damage types this ABI has names for, the values of the game's
// `DamageTypeNew` today. A hit may carry more on a later game build:
// `EnshwickDamageCheck.amount_count` says how many.
#define ENSHWICK_DAMAGE_TYPES 13

// A filter's answer that lets the game decide, or the filters after it.
#define ENSHWICK_FILTER_PASS 0

// A filter's answer that refuses what it is asked about.
#define ENSHWICK_FILTER_REFUSE 2

// A filter's answer that lets through what the game or the filters before
// it refused, in a filter that can; elsewhere the same as `FILTER_PASS`.
#define ENSHWICK_FILTER_YES 1

// The largest radius `EnshwickHostApi.fog_uncover` takes, in metres.
#define ENSHWICK_MAX_UNCOVER_RADIUS 100000.0

// The `what` of `EnshwickHostApi.progress_write`
// that adds experience for the game to apply.
#define ENSHWICK_PROGRESS_ADD_EXPERIENCE 3

// The `what` of `EnshwickHostApi.progress_write`
// that asks for the writes, during `init`.
#define ENSHWICK_PROGRESS_OPEN 0

// The `what` of `EnshwickHostApi.progress_write`
// that sets the experience inside the level.
#define ENSHWICK_PROGRESS_SET_EXPERIENCE 2

// The `what` of `EnshwickHostApi.progress_write`
// that sets the level.
#define ENSHWICK_PROGRESS_SET_LEVEL 1

// The bit of a `EnshwickSurvivorStatusFn` answer that says its low byte is the
// status to show.
#define ENSHWICK_SURVIVOR_STATUS_SET 256

// The `close` value of
// `EnshwickHostApi.action_window` that closes a
// window.
#define ENSHWICK_WINDOW_CLOSE 1

// The `close` value of
// `EnshwickHostApi.action_window` that shows a
// window: it opens, or updates where it shows.
#define ENSHWICK_WINDOW_SHOW 0

// The `close` value of
// `EnshwickHostApi.action_window` that updates a
// window only on the clients that still show it.
#define ENSHWICK_WINDOW_UPDATE 2

// An account action of `EnshwickHostApi.account_action`,
// with the values of the game's own `EnshwickAccountActionType`.
typedef uint8_t EnshwickAccountActionType;
// Removes a player online from the game.
#define EnshwickAccountActionType_KICK 0
// Puts the account on the ban list, and removes its player when
// online.
#define EnshwickAccountActionType_BAN 1
// Takes the account off the ban list.
#define EnshwickAccountActionType_UNBAN 2

// The start of every struct that crosses the boundary on its own: its size
// and the ABI version of its writer.
typedef struct EnshwickHeader {
  // The size of the whole struct as its writer knew it.
  uint32_t size;
  // `ABI_MAJOR` of the writer.
  uint16_t abi_major;
  // `ABI_MINOR` of the writer.
  uint16_t abi_minor;
} EnshwickHeader;

// An entity id, as the engine numbers entities: a character, a prop, an item
// with its own entity, anything the world holds.
//
// The id lasts one session: every load of a world numbers its entities
// anew, in the order of the save, so an id kept across a restart names
// another entity. It therefore has no serde form. Data that must outlast a
// restart goes in entity data or item data (the `unstable` modules
// `entity_data` and `item_data`), which find their entities again after a
// load. The engine's id 0 stands for no entity; the safe API hands it over
// as `None` in an `Option<EnshwickEntity>`, never as an `EnshwickEntity`.
typedef uint32_t EnshwickEntity;

// A player's account by the number the game keeps for it, the same in every
// session, to remember a player across reconnects and restarts.
typedef uint64_t EnshwickPlayerKey;

// A connected player, or a caller with no character, from
// `EnshwickHostApi.player`,
// `EnshwickHostApi.players` or a call about one.
//
// It grows by appending, as any struct with a `EnshwickHeader` does: a call
// hands it by pointer and a list states its entry size, so a later minor
// adds a field without new calls. The caller starts each one it passes with
// a header of its own size and ABI version.
typedef struct EnshwickPlayerInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The server's player slot; `UINT32_MAX` for a caller with no character.
  uint32_t slot;
  // The player character's entity; 0 for a caller with no character.
  EnshwickEntity entity;
  // The same number for the same account in every session.
  EnshwickPlayerKey key;
  // The game's user group bits for the player: 1
  // kick and ban, 2
  // open inventories, 4
  // edit the base, 8
  // edit the world, 16
  // extend the base, 32
  // receive experience. Bits the game adds
  // later are passed on as it sets them; a mod ignores bits it does not
  // know.
  uint32_t permissions;
  // Bytes of `name` in use, at most `EnshwickPlayerInfo_NAME_MAX`.
  uint32_t name_len;
  // The name the client sent, UTF-8; one longer than
  // `EnshwickPlayerInfo_NAME_MAX` bytes is cut at the last whole character
  // that fits, never inside one.
  uint8_t name[128];
} EnshwickPlayerInfo;
// May kick and ban; the server's administrators.
#define EnshwickPlayerInfo_CAN_KICK_BAN 1
// May open chests and other inventories.
#define EnshwickPlayerInfo_CAN_ACCESS_INVENTORIES (1 << 1)
// May build in bases.
#define EnshwickPlayerInfo_CAN_EDIT_BASE (1 << 2)
// May change the world outside bases.
#define EnshwickPlayerInfo_CAN_EDIT_WORLD (1 << 3)
// May extend bases.
#define EnshwickPlayerInfo_CAN_EXTEND_BASE (1 << 4)
// Receives experience.
#define EnshwickPlayerInfo_CAN_RECEIVE_EXP (1 << 5)
// Every permission bit the game defines; any other bit is not one.
#define EnshwickPlayerInfo_ALL_PERMISSIONS (((((EnshwickPlayerInfo_CAN_KICK_BAN | EnshwickPlayerInfo_CAN_ACCESS_INVENTORIES) | EnshwickPlayerInfo_CAN_EDIT_BASE) | EnshwickPlayerInfo_CAN_EDIT_WORLD) | EnshwickPlayerInfo_CAN_EXTEND_BASE) | EnshwickPlayerInfo_CAN_RECEIVE_EXP)
// The most bytes of a name: 32 characters of up to four bytes each,
// the longest name Steam allows.
#define EnshwickPlayerInfo_NAME_MAX 128

// A UTF-8 string borrowed for the duration of a call, or for as long as its
// owner documents.
typedef struct EnshwickStr {
#ifdef __cplusplus
  const char *ptr;
#else
  const uint8_t *ptr;
#endif
  size_t len;
} EnshwickStr;

// A place in the world in metres, y up: the one form of every position the
// ABI passes; the loader converts the engine's own forms.
typedef double EnshwickMetres[3];

// The object a run of an action names, sent with
// `EnshwickHostApi.run_action` and handed back
// to the mod in `EnshwickActionCall.target`.
typedef struct EnshwickActionTarget {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // Which fields hold a value: `EnshwickActionTarget_ENTITY`,
  // `EnshwickActionTarget_POSITION`, both or none.
  uint32_t flags;
  // The object's entity in the server's world, with
  // `EnshwickActionTarget_ENTITY`.
  EnshwickEntity entity;
  // Where it stands, in metres, with `EnshwickActionTarget_POSITION`.
  EnshwickMetres position;
} EnshwickActionTarget;
// `entity` holds the object's entity.
#define EnshwickActionTarget_ENTITY 1
// `position` holds where it stands.
#define EnshwickActionTarget_POSITION 2

// A mod's answer to a run of one of its actions: whether it was done and
// the text the caller reads.
//
// A mod writes it through `EnshwickActionCall.reply`, and
// `EnshwickHostApi.run_command` writes one for its
// caller.
typedef struct EnshwickActionReply {
  // Size and version of this struct; zero until the writer fills it.
  struct EnshwickHeader header;
  // 1 when the action was done, 0 when it was refused or failed.
  uint8_t done;
  // The text the caller reads, empty for none; valid until the writer
  // answers again on the same thread.
  struct EnshwickStr text;
} EnshwickActionReply;

// A question the loader asks a mod about its player actions, registered
// with `EnshwickHostApi.actions`.
typedef struct EnshwickActionCall {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // What the loader asks or tells: `EnshwickActionCall_ALLOWED`,
  // `EnshwickActionCall_RUN`, `EnshwickActionCall_CONSOLE`, `EnshwickActionCall_CHAT`,
  // `EnshwickActionCall_REMOTE`, `EnshwickActionCall_WINDOW` or
  // `EnshwickActionCall_WINDOW_CLOSED`; a mod built for an older minor answers a kind
  // it does not know with an empty reply.
  uint32_t kind;
  // The language the player reads the game in, a
  // `Language` value.
  uint32_t language;
  // The player the question is about, never null, valid for the call.
  const struct EnshwickPlayerInfo *player;
  // The action's id, for `EnshwickActionCall_RUN`; the console line after the
  // mod's id, for `EnshwickActionCall_CONSOLE`; the line after the command's
  // name or alias, for `EnshwickActionCall_CHAT` and `EnshwickActionCall_REMOTE`;
  // valid for the call.
  struct EnshwickStr action;
  // The action's arguments in the sheet's encoding, for
  // `EnshwickActionCall_RUN`; valid for the call.
  struct EnshwickStr args;
  // The object the player's client named for `EnshwickActionCall_RUN`, valid
  // for the call; null for none and for the other questions.
  const struct EnshwickActionTarget *target;
  // Where the mod writes its answer to a run, a zeroed `EnshwickActionReply`
  // the loader owns for the call; null for `EnshwickActionCall_ALLOWED` and
  // `EnshwickActionCall_WINDOW`, whose answer is the callback's `reply` text. A mod writes the whole
  // struct, its header first, and leaves the callback's `reply` text
  // untouched; a run whose reply it leaves zero is not done.
  struct EnshwickActionReply *reply;
  // The word that named the mod in the line, in lower case and without
  // its `/`: the mod's command name, alias or id, for
  // `EnshwickActionCall_CONSOLE`, `EnshwickActionCall_CHAT` and
  // `EnshwickActionCall_REMOTE`, so the mod's help names the command as the
  // caller typed it; empty for the other questions. Valid for the call.
  struct EnshwickStr word;
  // For `EnshwickActionCall_REFUSED`, the question the run would have been:
  // `EnshwickActionCall_RUN`, `EnshwickActionCall_CONSOLE`, `EnshwickActionCall_CHAT` or
  // `EnshwickActionCall_REMOTE`; 0 for the other questions.
  uint32_t via;
  // For `EnshwickActionCall_REFUSED`, why the loader refused the run, one of
  // the `EnshwickActionCall_REFUSED_` reasons; 0 for the other questions.
  uint32_t reason;
} EnshwickActionCall;
// The ids of the actions the player may run now, as the mod's rights
// for them say, one per line in the reply text; the mod looks for no
// subject and builds no label or list. The loader asks it to choose
// the carried controls a player gets, at their join and when their
// permissions or the mod's rules change.
#define EnshwickActionCall_ALLOWED 0
// The player runs an action from the interface: the answer, whether it
// was done and the text the player reads, goes in
// `EnshwickActionCall.reply`.
#define EnshwickActionCall_RUN 1
// The server's console runs an action from its text form, such as
// `access Ann on`, in `action`: the action's id and its values as
// words; `help` or nothing lists the actions. `player` is the console,
// an administrator with no character. The reply is as for
// `EnshwickActionCall_RUN`.
#define EnshwickActionCall_CONSOLE 3
// A player runs a command from chat, `/<name> ` and then what
// `EnshwickActionCall_CONSOLE` takes in `action`; `player` is the sender and
// `language` the language they read. The reply is as for
// `EnshwickActionCall_RUN`.
#define EnshwickActionCall_CHAT 4
// A mod runs a command line for a remote caller, such as a remote
// console, as `EnshwickActionCall_CHAT` does; `player` holds the caller's
// label as its name, its permission bits, key 0 and no character.
#define EnshwickActionCall_REMOTE 5
// The window of the calling mod's that `action` names, as `player`
// sees it in `language`, after
// `EnshwickHostApi.action_window` showed it:
// the reply is the window's text, empty when the mod keeps no such
// window for them or the player may not run an action it names.
#define EnshwickActionCall_WINDOW 6
// The client of the player whose account key `player` holds closed
// the calling mod's window `action`, let it go for a newer one, or
// closed it for an empty or oversized answer to
// `EnshwickActionCall_WINDOW`; or, with an empty `action`, that
// player left, and holds none of the mod's windows any more. The mod
// forgets what it kept for them. `player` holds the account key alone:
// no character, name or permission. No reply is read.
#define EnshwickActionCall_WINDOW_CLOSED 7
// The loader refused a run of one of the mod's actions before asking
// the mod, and answered the caller itself: `via` names the question the
// run would have been, `reason` why, and `action`, `args` and `word`
// hold what that question would have held. `player` is the caller, or
// the account key alone for one no longer in the world. No reply is
// read.
#define EnshwickActionCall_REFUSED 8
// `EnshwickActionCall.reason`: the caller is over the loader's rate of runs.
#define EnshwickActionCall_REFUSED_RATE 1
// `EnshwickActionCall.reason`: the run's arguments are out of the loader's
// bounds, which only a changed client sends.
#define EnshwickActionCall_REFUSED_BOUNDS 2
// `EnshwickActionCall.reason`: the runs waiting for the end of the step fill
// the loader's queue.
#define EnshwickActionCall_REFUSED_BUSY 3
// `EnshwickActionCall.reason`: the object the run names stands out of the
// caller's reach.
#define EnshwickActionCall_REFUSED_REACH 4

// A buff type of the game by its id, the
// 32-bit FNV-1a hash of its GUID's 16 bytes, to apply,
// find or remove one buff.
typedef uint32_t EnshwickBuffTypeId;

// A buff an entity has, written by `EnshwickHostApi.buffs`.
typedef struct EnshwickActiveBuff {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // Its type's id.
  EnshwickBuffTypeId id;
  // The buff's own entity, which carries its effect.
  EnshwickEntity entity;
  // Where the entity holds it, as `EnshwickBuffTypeInfo.slot`.
  uint32_t slot;
  // Its level; 0 for none.
  uint32_t level;
  // How long it lasts in all, in game nanoseconds.
  int64_t duration_ns;
  // How long it has left, in game nanoseconds.
  int64_t remaining_ns;
} EnshwickActiveBuff;

// One modifier of an affix with the range its value rolls in; read as
// arrays.
typedef struct EnshwickAffixModifierSpec {
  // The stat's index.
  uint32_t stat;
  // The operation, as `EnshwickStatModifier.op`.
  uint8_t op;
  // Zero.
  uint8_t reserved[3];
  // The least value.
  float min;
  // The greatest value.
  float max;
  // The value rolls in steps of this from `min`; 0 for any value.
  float step;
} EnshwickAffixModifierSpec;

// An item of the game by the number its item data gives it, as a prop's
// `UsedItem` names the item the prop was placed from.
typedef uint32_t EnshwickItemId;

// Rolls affixes onto an item, for `EnshwickHostApi.affix_roll`.
typedef struct EnshwickAffixRoll {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The item's entity.
  EnshwickEntity pide;
  // The item id it holds.
  EnshwickItemId item;
  // The fewest affixes.
  uint8_t min;
  // The most affixes.
  uint8_t max;
  // The rarest an affix may be, 0 to 4.
  uint8_t max_rarity;
  // `EnshwickAffixRoll_ONCE`.
  uint8_t flags;
  // The seed of the roll: the same seed, item and affixes give the same
  // result.
  uint64_t seed;
} EnshwickAffixRoll;
// Rolls only an item that never rolled; others answer
// `EnshwickStatus_REFUSED`.
#define EnshwickAffixRoll_ONCE 1

// An affix's definition, for `EnshwickHostApi.define_affix`.
typedef struct EnshwickAffixSpec {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // Affixes of one group never roll together on an item; empty for none.
  struct EnshwickStr group;
  // Its modifiers' ranges.
  const struct EnshwickAffixModifierSpec *modifiers;
  // How many.
  uint32_t modifier_count;
  // How likely it rolls against the others.
  uint32_t weight;
  // The item categories it rolls on, a bit per `ItemCategory`.
  uint32_t categories;
  // Its rarity, 0 (common) to 4 (legendary).
  uint8_t rarity;
  // Zero.
  uint8_t reserved[3];
} EnshwickAffixSpec;

// A whole cell of the engine's 0.5 m grid, y up, as survivors and altar
// zones count them.
typedef int64_t EnshwickCell[3];

// A box of whole `Cell`s: `min` inclusive, `max` exclusive on each axis;
// axis 1 is up.
typedef struct EnshwickBounds {
  // The inclusive lower corner.
  EnshwickCell min;
  // The exclusive upper corner.
  EnshwickCell max;
} EnshwickBounds;

// One Flame Altar of the server's world, from `EnshwickHostApi.altars`.
//
// Zones are in whole 0.5 m cells, rounded outwards, as the engine builds
// them from the altar's trigger shape.
typedef struct EnshwickAltarInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The altar's entity.
  EnshwickEntity entity;
  // Its Flame level; 0 until the engine has reported it.
  uint8_t level;
  // 1 when the altar is tilted off the vertical: its zone is known, its
  // level zones are not, and it keeps the engine's size.
  uint8_t tilted;
  // How many entries of `levels` hold a zone.
  uint8_t level_count;
  // Zero.
  uint8_t reserved;
  // The zone as the engine has built it now, growth by any mod included.
  struct EnshwickBounds zone;
  // The vanilla zone at each level from 1 to `level_count`; 0 levels when
  // the altar has more than `ALTAR_LEVELS`.
  struct EnshwickBounds levels[ENSHWICK_ALTAR_LEVELS];
  // Where the altar stands, in metres.
  EnshwickMetres position;
} EnshwickAltarInfo;

// How many altars a Flame level allows, as the loader asks every
// `EnshwickHostApi.altar_limit_filter` in
// load order.
typedef struct EnshwickAltarLimit {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The Flame level.
  uint32_t flame_level;
  // The altars the game itself allows at that level.
  uint32_t vanilla;
  // The altars allowed so far, as the filters before have left it; the
  // filter's answer.
  uint32_t allowed;
  // Zero.
  uint32_t reserved;
} EnshwickAltarLimit;

// One altar's zone as the loader sizes it, handed to every altar zone
// filter in load order.
//
// The zone is the altar's vanilla zone at `level` grown by `grow` cells on
// every horizontal side and made `height` cells tall, as the filters before
// have left it; see
// `EnshwickHostApi.altar_zone_filter`.
typedef struct EnshwickAltarZone {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The altar's entity.
  EnshwickEntity altar;
  // The altar's level.
  uint8_t level;
  // Zero.
  uint8_t reserved[3];
  // Cells added on every horizontal side, at least 0; 0 for vanilla.
  int64_t grow;
  // The zone's height in cells; the vanilla height for vanilla.
  int64_t height;
} EnshwickAltarZone;

// The most altar zones and bases the game groups, as
// `EnshwickHostApi.base_limit` writes it.
typedef struct EnshwickBaseLimitInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // Altar zones the game puts into bases now.
  uint32_t zones;
  // Bases the game holds.
  uint32_t bases;
  // Altar zones the game's own grouping takes, without the raised limit.
  uint32_t vanilla_zones;
  // Zero.
  uint32_t reserved;
  // Bases the game's own code holds.
  uint32_t vanilla_bases;
  // The most altar zones a mod may ask for with
  // `EnshwickHostApi.request_base_limits`.
  uint32_t max_zones;
  // The most bases a mod may ask for.
  uint32_t max_bases;
} EnshwickBaseLimitInfo;

// An NPC of the game, such as a survivor, by its id, the
// 32-bit FNV-1a hash of its template's GUID, to
// tell survivors apart in lookups, bed checks and summons.
typedef uint32_t EnshwickNpcId;

// A survivor's own check that its bed is still its, after the game kept it,
// as `EnshwickHostApi.survivor_bed_check` hands it on. Cells are whole 0.5 m
// cells.
//
// The name is enshwick's: the game's bed check has none.
typedef struct EnshwickBedCheck {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The survivor's id.
  EnshwickNpcId npc_id;
  // The survivor's entity.
  EnshwickEntity entity;
  // The bed.
  EnshwickEntity bed_id;
  // Where the survivor belongs: its saved spawn cell, or where it stands.
  EnshwickCell home;
  // Where the bed stands.
  EnshwickCell bed_at;
} EnshwickBedCheck;

// A building block a transaction places, as a player's building tool
// does, for `EnshwickTransactionOp_BLOCK`.
typedef struct EnshwickBlockPlace {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The shape's item: one with a `VoxelBlueprintItem`.
  EnshwickItemId shape;
  // The material's item: one whose `ChangeVoxelData` places a building
  // material.
  EnshwickItemId material;
  // Where the block's middle lies, in metres.
  EnshwickMetres position;
  // The block's rotation, a quaternion x, y, z, w; the game keeps the
  // nearest quarter turns.
  float rotation[4];
} EnshwickBlockPlace;

// A buff to give an entity, for `EnshwickHostApi.apply_buff`.
typedef struct EnshwickBuffRequest {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The entity to give it.
  EnshwickEntity entity;
  // The type's id.
  EnshwickBuffTypeId id;
  // Its level, 1 to 255; 0 for none.
  uint32_t level;
  // Zero.
  uint32_t reserved;
  // How long it lasts, in game nanoseconds; 0 for the type's own.
  int64_t duration_ns;
} EnshwickBuffRequest;

// A GUID in the one byte order every ABI field uses, the game's memory
// order: the `bytes_le` of its textual form, as the game's files store it.
typedef struct EnshwickGuid {
  uint8_t bytes[16];
} EnshwickGuid;

// A buff type of a mod's own, for
// `EnshwickHostApi.buff_type_add`: a copy of the
// vanilla type `like` with the values `changes` names set from this struct.
typedef struct EnshwickBuffTypeAdd {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // Its content id's GUID, which the calling mod declared with
  // `declare_content`; its game id is the FNV-1a 32 of these bytes.
  struct EnshwickGuid guid;
  // The tag of its name, with `EnshwickBuffTypeAdd_NAME`.
  struct EnshwickGuid name;
  // The tag of its description, with `EnshwickBuffTypeAdd_DESCRIPTION`.
  struct EnshwickGuid description;
  // The UI image of its icon, with `EnshwickBuffTypeAdd_ICON`. A client names
  // it only once its UI knows that image, a game one or a served one, and
  // shows the copied type's icon until then.
  struct EnshwickGuid icon;
  // How long it lasts, in game nanoseconds, 0 or more, with
  // `EnshwickBuffTypeAdd_LIFE_TIME`.
  int64_t life_time_ns;
  // The game's buff type it copies.
  EnshwickBuffTypeId like;
  // The values it sets: the change bits of this struct, or none.
  uint16_t changes;
  // Where an entity holds it, as `EnshwickBuffTypeInfo.slot`, with
  // `EnshwickBuffTypeAdd_SLOT`.
  uint8_t slot;
  // `EnshwickBuffTypeAdd_HIDE_IN_UI`, `EnshwickBuffTypeAdd_HIDE_DURATION_IN_UI` and
  // `EnshwickBuffTypeAdd_SAVED` for the values `changes` sets, and
  // `EnshwickBuffTypeAdd_PLAIN`, or none.
  uint8_t flags;
} EnshwickBuffTypeAdd;
// Sets its name.
#define EnshwickBuffTypeAdd_NAME 1
// Sets its description.
#define EnshwickBuffTypeAdd_DESCRIPTION (1 << 1)
// Sets its icon.
#define EnshwickBuffTypeAdd_ICON (1 << 2)
// Sets how long it lasts.
#define EnshwickBuffTypeAdd_LIFE_TIME (1 << 3)
// Sets its slot.
#define EnshwickBuffTypeAdd_SLOT (1 << 4)
// Sets whether the game shows it nowhere, from
// `EnshwickBuffTypeAdd_HIDE_IN_UI`.
#define EnshwickBuffTypeAdd_SET_HIDE_IN_UI (1 << 5)
// Sets whether the HUD hides how long it has left, from
// `EnshwickBuffTypeAdd_HIDE_DURATION_IN_UI`.
#define EnshwickBuffTypeAdd_SET_HIDE_DURATION_IN_UI (1 << 6)
// Sets whether the game saves it, from `EnshwickBuffTypeAdd_SAVED`.
#define EnshwickBuffTypeAdd_SET_SAVED (1 << 7)
// Every change it may set.
#define EnshwickBuffTypeAdd_ALL_CHANGES ((1 << 8) - 1)
// The game shows it nowhere.
#define EnshwickBuffTypeAdd_HIDE_IN_UI 1
// The HUD hides how long it has left.
#define EnshwickBuffTypeAdd_HIDE_DURATION_IN_UI (1 << 1)
// The game saves it with a character.
#define EnshwickBuffTypeAdd_SAVED (1 << 2)
// It drops what the vanilla type runs: its impact program and values,
// tooltip lines, helpers, follow-up and effects.
#define EnshwickBuffTypeAdd_PLAIN (1 << 3)
// Every flag it may hold.
#define EnshwickBuffTypeAdd_ALL_FLAGS ((1 << 4) - 1)

// A buff or debuff type of the running game, written by
// `EnshwickHostApi.buff_types`.
typedef struct EnshwickBuffTypeInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The type's id: FNV-1a 32 of its GUID's bytes.
  EnshwickBuffTypeId id;
  // Where an entity holds it: `EnshwickBuffTypeInfo_FOOD`,
  // `EnshwickBuffTypeInfo_BUFF`, `EnshwickBuffTypeInfo_DEBUFF`,
  // `EnshwickBuffTypeInfo_RESTED` or `EnshwickBuffTypeInfo_TERRAIN`.
  uint16_t slot;
  // `EnshwickBuffTypeInfo_SAVED`, `EnshwickBuffTypeInfo_HIDDEN` or none.
  uint16_t flags;
  // The `LocaTag` of its name, which a client shows.
  uint32_t name;
  // How long it lasts by default, in game nanoseconds.
  int64_t duration_ns;
} EnshwickBuffTypeInfo;
// A food buff.
#define EnshwickBuffTypeInfo_FOOD 0
// A buff.
#define EnshwickBuffTypeInfo_BUFF 1
// A debuff.
#define EnshwickBuffTypeInfo_DEBUFF 2
// Rested.
#define EnshwickBuffTypeInfo_RESTED 3
// A debuff of the ground the entity stands in.
#define EnshwickBuffTypeInfo_TERRAIN 4
// The game saves it with the character.
#define EnshwickBuffTypeInfo_SAVED 1
// The game shows it nowhere.
#define EnshwickBuffTypeInfo_HIDDEN 2

// A placement the game's cursor check allows, handed to the filters
// registered with `EnshwickHostApi.build_filter`.
typedef struct EnshwickBuildCheck {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The building player's character.
  EnshwickEntity character;
  // The item placed: a building block, a prop or another placeable.
  EnshwickItemId item;
  // Where, in metres.
  EnshwickMetres position;
  // Null, or where a filter that refuses writes why: a key of its mod's
  // texts or plain words, which the builder reads. The text it points at
  // stays valid until the filter's next call on the same thread; the
  // loader copies it before then, on that thread.
  struct EnshwickStr *reason;
  // The building player's account, which a new character of the same
  // player keeps; 0 where the loader cannot tell, such as in the own
  // prediction of a client that joined a server. A hosting client's own
  // prediction gets its player's account. A mod reads it only where
  // `header` covers it.
  EnshwickPlayerKey account;
} EnshwickBuildCheck;

// Which binary of the game runs, or where a mod runs.
//
// A mod is `EnshwickSide_SERVER` or `EnshwickSide_BOTH`; `EnshwickSide_CLIENT` names only
// the game client as a binary.
typedef uint8_t EnshwickSide;
// The dedicated server; as a mod's side, a mod that runs only where the
// world runs: the dedicated server, or a client that hosts or plays
// alone. Players need not install it.
#define EnshwickSide_SERVER 1
// The game client, as a binary; never a mod's side.
#define EnshwickSide_CLIENT 2
// As a mod's side, a mod that runs where the world runs and on every
// client playing in it, which must have it in the world's version.
#define EnshwickSide_BOTH 3

// A game build's identity: the CodeView GUID and age.
typedef struct EnshwickFingerprint {
  // The CodeView GUID.
  struct EnshwickGuid guid;
  // The linker's age counter.
  uint32_t age;
} EnshwickFingerprint;

// The running game, as the loader identified it.
typedef struct EnshwickBuildInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // `EnshwickSide_SERVER` or `EnshwickSide_CLIENT`.
  EnshwickSide side;
  // Where the game's image is loaded.
  uint64_t base;
  // The build fingerprint.
  struct EnshwickFingerprint fingerprint;
  // 1 when the loader found every entry it knows on this build, by its
  // engine routes and anchors; 0 when some are missing, and what needs
  // one stays off until a loader release finds it.
  uint8_t complete;
} EnshwickBuildInfo;

// Whether a player may edit a box of the server's world, for
// `EnshwickHostApi.build_rights`: the mod
// fills `character`, `min` and `max`, and the loader writes `refusal`.
typedef struct EnshwickBuildRights {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The player's character entity.
  EnshwickEntity character;
  // `EnshwickBuildRights_ALLOWED` or why the server refuses, written by the
  // loader.
  uint32_t refusal;
  // The box's lowest corner, in metres.
  EnshwickMetres min;
  // The box's highest corner, in metres, at least `min` on every axis.
  EnshwickMetres max;
} EnshwickBuildRights;
// The server lets the player edit the box.
#define EnshwickBuildRights_ALLOWED 0
// The player lacks `CanEditWorld`, which every edit needs.
#define EnshwickBuildRights_NO_CAN_EDIT_WORLD 1
// The player lacks `CanEditBase`, and the box overlaps a building zone.
#define EnshwickBuildRights_NO_CAN_EDIT_BASE 2
// A corner of the box lies outside the world's building area.
#define EnshwickBuildRights_OUTSIDE_BUILDING_ZONE 3
// The box reaches into a strict no-build zone.
#define EnshwickBuildRights_INSIDE_STRICT_BUILD_INHIBITOR_ZONE 4

// One symbol pinned for one game build: a name and its address relative to
// the image base.
typedef struct EnshwickSymbol {
  // The symbolic id, such as `npc.lookup`.
  struct EnshwickStr name;
  // The address relative to the image base.
  uint64_t rva;
} EnshwickSymbol;

// The symbols a mod pins itself for one game build, beside the loader's own
// table.
typedef struct EnshwickBuildRow {
  // The binary the row is for.
  EnshwickSide side;
  // The build the row is for.
  struct EnshwickFingerprint fingerprint;
  // The pinned symbols.
  const struct EnshwickSymbol *symbols;
  // How many symbols `symbols` points at.
  uint32_t symbol_count;
  // Engine constants the mod pins for the build, such as offsets in the
  // caller frames it reads; the `rva` field holds the value.
  const struct EnshwickSymbol *constants;
  // How many constants `constants` points at.
  uint32_t constant_count;
} EnshwickBuildRow;

// The data of the `BuildingPlaced` and
// `BuildingRemoved` events.
typedef struct EnshwickBuildingData {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The placed entity; for a removal, the entity as it was.
  EnshwickEntity entity;
  // The acting player's character entity.
  EnshwickEntity character;
  // The item placed, which the entity keeps as its `UsedItem`; 0 for a
  // removal.
  EnshwickItemId item;
  // What happened: 0 placed, 1
  // picked up, 2 torn down.
  uint8_t kind;
  // 1 when undo or redo replayed the action.
  uint8_t replay;
  // The template GUID of the placed entity; zero for a
  // removal.
  struct EnshwickGuid template_;
  // Where it was placed, in metres; zero for a removal.
  EnshwickMetres position;
} EnshwickBuildingData;
// Placed by a player.
#define EnshwickBuildingData_PLACED 0
// Picked up, its item returned.
#define EnshwickBuildingData_PICKED_UP 1
// Torn down.
#define EnshwickBuildingData_TORN_DOWN 2

// A box in the world, from its lowest corner `min` to its highest `max`, in
// `Metres`.
typedef struct EnshwickExtent {
  // The lowest corner.
  EnshwickMetres min;
  // The highest corner, at least `min` on every axis.
  EnshwickMetres max;
} EnshwickExtent;

// A placement the game refused because the object's box does not lie in a
// single building zone, as `EnshwickHostApi.building_zone_filter` hands it on.
// Coordinates are `Metres`.
//
// The name is enshwick's, after the game's building zone: the game's check
// has no name of its own.
typedef struct EnshwickBuildingZoneCheck {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The building zones, valid for the call.
  const struct EnshwickExtent *zones;
  // How many boxes `zones` holds.
  uint32_t zone_count;
  // The object's box.
  struct EnshwickExtent object;
  // How far the game lets an object reach past a zone on every side, in metres.
  double margin;
} EnshwickBuildingZoneCheck;

// One field of a service table or mod event, as its layout hash sees it.
//
// Frozen, written as an array: a field is its name and its type's hash,
// all a layout compares.
typedef struct EnshwickLayoutField {
  // The field's name.
  struct EnshwickStr name;
  // The hash of the field's name, the shape of its type, its size,
  // alignment and offset.
  uint64_t hash;
} EnshwickLayoutField;

// The layout of a service table or mod event, field by field.
typedef struct EnshwickLayout {
  // The fields, in declaration order.
  const struct EnshwickLayoutField *fields;
  // How many `fields` points at.
  uint32_t count;
  // The size of the whole type.
  uint32_t size;
} EnshwickLayout;

// A described service, a call: one function from an input struct to an
// answer, which one mod serves and others call, or the call a mod asks for.
typedef struct EnshwickCallSpec {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The call's name, `<serving mod id>:<what>`.
  struct EnshwickStr name;
  // The incompatible version: calls of two majors share nothing.
  uint32_t major;
  // The input's fields and size.
  struct EnshwickLayout input;
  // The answer, one field, and its size.
  struct EnshwickLayout output;
} EnshwickCallSpec;

// The game caller's context at entry of a hook chain: its stack pointer and
// callee-saved registers, for links that tell callers apart.
//
// Frozen, with no header: it is the x64 caller's whole callee-saved set,
// which no call convention will add to.
typedef struct EnshwickCallerFrame {
  // The stack pointer at entry; it points at the return address.
  uint64_t rsp;
  // `rbx` at entry.
  uint64_t rbx;
  // `rbp` at entry.
  uint64_t rbp;
  // `rdi` at entry.
  uint64_t rdi;
  // `rsi` at entry.
  uint64_t rsi;
  // `r12` at entry.
  uint64_t r12;
  // `r13` at entry.
  uint64_t r13;
  // `r14` at entry.
  uint64_t r14;
  // `r15` at entry.
  uint64_t r15;
} EnshwickCallerFrame;

// A change a mod declared with
// `EnshwickHostApi.change_data`, whose value it
// sets later; numbered by the loader, unique in the process.
typedef uint32_t EnshwickChangeId;

// How a `EnshwickDataChange` changes its value.
typedef uint32_t EnshwickChangeOp;
// The value becomes `value`.
#define EnshwickChangeOp_SET 0
// The value is multiplied by `value`, a finite factor from 0 to 100.
#define EnshwickChangeOp_TIMES 1
// The mod's value is taken out, as if it never set one; `value` is
// ignored. Only `EnshwickHostApi.set_change`
// takes it.
#define EnshwickChangeOp_RESET 2

// A chat line on the server, borrowed for the call that hands it over.
//
// Raised with `EnshwickEvent_CHAT_SAID` after the step
// for every line that reached the chat, and handed to each
// `EnshwickChatFilterFn` for a player's line before anyone reads it. `text` is
// valid UTF-8 (bytes that are not are replaced), at most the game's own
// limit of 1024 bytes, and lives only for the call. Layout grows only by
// appending; read a later field once the header's `size` covers it.
typedef struct EnshwickChatLine {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The sender's account key; zero when the game names none.
  EnshwickPlayerKey account;
  // The sender's character, `EnshwickEntity(0)` when it has none in the world.
  EnshwickEntity entity;
  // `EnshwickChatLine_PLAYER`, `EnshwickChatLine_EMOTE` or `EnshwickChatLine_NOTICE`.
  uint32_t kind;
  // The two text tags the game posts with the line, zero when unused.
  uint32_t loca[2];
  // The text, UTF-8, borrowed for the call; empty for a notice made of
  // text tags.
  struct EnshwickStr text;
} EnshwickChatLine;
// A line a player typed: the game posted it for what their client
// sent, whatever chat type the client named.
#define EnshwickChatLine_PLAYER 0
// An emote the server posted for a player.
#define EnshwickChatLine_EMOTE 1
// Any other line the server posted itself, such as a player joining
// or leaving.
#define EnshwickChatLine_NOTICE 2

// The buffer a `EnshwickChatFilterFn` writes a rewritten line into.
//
// The loader owns `buffer`, writable for `capacity` bytes during the call
// only; the filter writes UTF-8 into it, sets `len` and answers
// `EnshwickChatRewrite_REWRITTEN`. A `len` over `capacity`, or bytes that are
// not UTF-8, keep the line as it was. The loader owns the struct too and
// hands it to one filter at a time on the game thread, never null; the
// filter keeps no pointer past the call. Layout grows only by appending;
// read a later field once the header's `size` covers it.
typedef struct EnshwickChatRewrite {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // Where the filter writes the new text; never null.
  uint8_t *buffer;
  // Bytes `buffer` holds: the game's limit for a line.
  uint32_t capacity;
  // Bytes the filter wrote.
  uint32_t len;
} EnshwickChatRewrite;
// The line goes on as it is.
#define EnshwickChatRewrite_KEEP 0
// The line is dropped as the chat mute drops it: nobody reads it, and
// the filters after this one do not run.
#define EnshwickChatRewrite_DROP 1
// The line goes on as the text written into the buffer; an empty text
// drops it as `EnshwickChatRewrite_DROP` does.
#define EnshwickChatRewrite_REWRITTEN 2

// The data of the `ChunkLoaded` and
// `ChunkUnloaded` events: a chunk of the
// world's entities, never of its terrain's voxels.
typedef struct EnshwickChunkData {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // Its column, west to east from 0.
  uint32_t x;
  // Its row, from 0 along the other axis.
  uint32_t z;
  // Its side in metres: it spans `x * size` to `(x + 1) * size` on one
  // axis and `z * size` to `(z + 1) * size` on the other, at any height.
  uint32_t size;
} EnshwickChunkData;

// A crafting recipe of the game by its recipe id, to grant, refuse or watch
// the crafting of one recipe.
typedef uint32_t EnshwickRecipeId;

// A request to the client's own interface or input, which
// `EnshwickHostApi.client_command` queues.
//
// The loader runs it in the interface's frame or the input builder's step,
// never inside a mod's callback, and only while the client is still in
// `session` on `expected_screen`. Its outcome is what
// `EnshwickHostApi.client_command_status`
// reports; the call itself answers only that it is queued.
typedef struct EnshwickClientCommand {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // What to do: `EnshwickClientCommand_OPEN_SCREEN` to `EnshwickClientCommand_PRESS`.
  uint32_t kind;
  // A move's hold in milliseconds, 1 to `EnshwickClientCommand_MAX_HOLD_MS`;
  // a press's, 1 to `EnshwickClientCommand_MAX_PRESS_MS`.
  uint32_t hold_ms;
  // The session the command belongs to, as
  // `EnshwickClientSnapshot.session` names it; a command whose session ended
  // is cancelled. 0, the session of a snapshot taken before the
  // interface read a game, takes the session of the first frame that
  // runs the command.
  uint64_t session;
  // The HUD screen the client must show, a `HudScreenType` member; empty
  // for any screen.
  struct EnshwickStr expected_screen;
  // What the command opens or chooses: a screen for
  // `EnshwickClientCommand_OPEN_SCREEN`, a tab for `EnshwickClientCommand_CHOOSE_TAB`, by the
  // `HudScreenType` member it stands for; the server, `host:query_port`,
  // for `EnshwickClientCommand_JOIN`; the game action's reflected
  // `PlayerInputType` name, such as `Jump`, for `EnshwickClientCommand_PRESS`.
  struct EnshwickStr target;
  // A look's turn in radians of yaw and pitch, before the player's
  // camera sensitivity; each at most `EnshwickClientCommand_MAX_LOOK`. A positive yaw
  // turns right.
  float look[2];
  // A move's direction, x right and y forward, of length at most 1.
  float direction[2];
  // The recipe to highlight.
  EnshwickRecipeId recipe;
  // Zero.
  uint32_t reserved;
  // The command `EnshwickClientCommand_CANCEL` cancels.
  uint64_t cancels;
  // The server's password for `EnshwickClientCommand_JOIN`, at most
  // `EnshwickClientCommand_MAX_PASSWORD` bytes, empty for none; borrowed for
  // the call. The loader hands it to the game's own join and never
  // keeps, logs or returns it. ABI 12.11.
  struct EnshwickStr password;
} EnshwickClientCommand;
// Opens `target`, one of the screens the game opens from a key in the
// world.
#define EnshwickClientCommand_OPEN_SCREEN 1
// Closes the open screen, back to the world.
#define EnshwickClientCommand_CLOSE_SCREEN 2
// Shows the in-game menu's tab `target` while the menu shows.
#define EnshwickClientCommand_CHOOSE_TAB 3
// Cancels the mod's command `cancels`.
#define EnshwickClientCommand_CANCEL 4
// Turns the camera by `look` once.
#define EnshwickClientCommand_LOOK 5
// Holds `direction` for `hold_ms`.
#define EnshwickClientCommand_MOVE 6
// Highlights `recipe` in the open crafting menu, crafting nothing.
#define EnshwickClientCommand_HIGHLIGHT_RECIPE 7
// Joins the dedicated server `target`, `host:query_port`, with
// `password` from the main menu with a character selected; `session`
// is 0. Refused while a join or a world runs. ABI 12.11.
#define EnshwickClientCommand_JOIN 8
// Leaves the world back to the main menu. ABI 12.11.
#define EnshwickClientCommand_LEAVE 9
// Holds the game action `target` in the local player's input for
// `hold_ms`, as its key would. ABI 12.21.
#define EnshwickClientCommand_PRESS 10
// The longest password, in bytes, the game's join takes.
#define EnshwickClientCommand_MAX_PASSWORD 1024
// The longest move.
#define EnshwickClientCommand_MAX_HOLD_MS 250
// The longest press. ABI 12.21.
#define EnshwickClientCommand_MAX_PRESS_MS 2000
// The largest turn of one look, in radians, a quarter turn.
#define EnshwickClientCommand_MAX_LOOK 1.5707964

// Where a `EnshwickClientCommand` stands, which
// `EnshwickHostApi.client_command_status`
// writes.
//
// A command is `EnshwickClientCommandStatus_PENDING` until the game took it
// (`EnshwickClientCommandStatus_APPLIED`), then `EnshwickClientCommandStatus_OBSERVED` once the client's own state
// shows its effect. It may end `EnshwickClientCommandStatus_CANCELLED` for `reason`,
// `EnshwickClientCommandStatus_UNAVAILABLE` on a build that lacks what it needs, or,
// for a join, `EnshwickClientCommandStatus_FAILED`. A join follows its `phase` with the
// game's own pace: no two-second limit cancels it. Nothing answers a guessed
// success.
typedef struct EnshwickClientCommandStatus {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // `EnshwickClientCommandStatus_PENDING` to `EnshwickClientCommandStatus_UNAVAILABLE`.
  uint32_t state;
  // Why a cancelled command ended: `EnshwickClientCommandStatus_SESSION_CHANGED` to
  // `EnshwickClientCommandStatus_MOD_OFF`; 0 otherwise.
  uint32_t reason;
  // The command's kind.
  uint32_t kind;
  // Zero.
  uint32_t reserved;
  // Milliseconds since the command was queued.
  uint64_t age_ms;
  // Where the client stands on a `EnshwickClientCommand_JOIN` or
  // `EnshwickClientCommand_LEAVE`: `EnshwickClientCommandStatus_PHASE_QUEUED` to
  // `EnshwickClientCommandStatus_PHASE_LEFT`; 0 for other kinds. ABI 12.11.
  uint32_t phase;
  // Why a `EnshwickClientCommandStatus_FAILED` join failed, as far as the
  // client can tell: `EnshwickClientCommandStatus_WRONG_PASSWORD` to
  // `EnshwickClientCommandStatus_UNKNOWN`; 0 otherwise. ABI 12.11.
  uint32_t failure;
} EnshwickClientCommandStatus;
// Queued; the game has not taken it yet.
#define EnshwickClientCommandStatus_PENDING 0
// The game took it: its queue holds the screen change, the menu its
// tab, or the input builder the look or move.
#define EnshwickClientCommandStatus_APPLIED 1
// The client's own state shows the effect: the screen or tab shows,
// the look was written whole, the move's hold ran out, the recipe is
// highlighted.
#define EnshwickClientCommandStatus_OBSERVED 2
// It ended without its effect, for `reason`.
#define EnshwickClientCommandStatus_CANCELLED 3
// The running build lacks what it needs, or the target is not there,
// such as a recipe the open menu does not list.
#define EnshwickClientCommandStatus_UNAVAILABLE 4
// The client's session changed.
#define EnshwickClientCommandStatus_SESSION_CHANGED 1
// Another screen than the expected one shows.
#define EnshwickClientCommandStatus_OTHER_SCREEN 2
// The player pressed a control or moved the stick or mouse.
#define EnshwickClientCommandStatus_PLAYER_INPUT 3
// The mod cancelled it.
#define EnshwickClientCommandStatus_BY_MOD 4
// Two seconds passed without its effect.
#define EnshwickClientCommandStatus_TIMED_OUT 5
// The game blocks the player's input now.
#define EnshwickClientCommandStatus_INPUT_BLOCKED 6
// The mod or the feature turned off.
#define EnshwickClientCommandStatus_MOD_OFF 7
// The game ended a join without reaching the world, for `failure`.
// ABI 12.11.
#define EnshwickClientCommandStatus_FAILED 5
// Waiting to start: the loader asks the server for its address.
#define EnshwickClientCommandStatus_PHASE_QUEUED 1
// The main menu took the join and starts play.
#define EnshwickClientCommandStatus_PHASE_MENU 2
// Play started; the client connects to the server.
#define EnshwickClientCommandStatus_PHASE_CONNECTING 3
// Connected; the client loads the world.
#define EnshwickClientCommandStatus_PHASE_LOADING 4
// The client is in the server's world.
#define EnshwickClientCommandStatus_PHASE_IN_WORLD 5
// The client left play, back to the main menu.
#define EnshwickClientCommandStatus_PHASE_LEFT 6
// The server refused the password, or asks for one and none was given.
#define EnshwickClientCommandStatus_WRONG_PASSWORD 1
// The server is full.
#define EnshwickClientCommandStatus_FULL 2
// The server was not found.
#define EnshwickClientCommandStatus_NOT_FOUND 3
// The server is reserved for others.
#define EnshwickClientCommandStatus_RESERVED 4
// The server runs another game version.
#define EnshwickClientCommandStatus_VERSION_MISMATCH 5
// The loader's join check or the server refused the client.
#define EnshwickClientCommandStatus_REFUSED 6
// The game did not start or finish the join in time.
#define EnshwickClientCommandStatus_JOIN_TIMED_OUT 7
// The server's query port did not answer with its address.
#define EnshwickClientCommandStatus_UNREACHABLE 8
// The join failed for a reason the client does not show.
#define EnshwickClientCommandStatus_UNKNOWN 9

// The client's state in one copy, which
// `EnshwickHostApi.client_snapshot` writes.
//
// It holds the
// session and world, the open screens and the in-game menu's tab, read in
// the interface's last frame, and the local player's held controls, read
// in its last step.
//
// Each group of fields counts only while its bit of `available` is set; a
// build that lacks what a group reads leaves its bit clear and the rest of
// the copy whole. Names are the game's own: a screen or tab is a member of
// its reflected `HudScreenType`, and empty while none is. The names are
// static text, valid for the process.
typedef struct EnshwickClientSnapshot {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // How old the interface's reads are: `EnshwickClientSnapshot_NEVER` to `EnshwickClientSnapshot_FRESH`.
  uint32_t state;
  // Which field groups count: `EnshwickClientSnapshot_SCREENS` and the other group bits.
  uint32_t available;
  // The loader's count of client sessions, from 1: it grows when the
  // client's game object, the local player's entity or the world
  // changes, so a value read twice names one session. 0 before the
  // interface read a game.
  uint64_t session;
  // Milliseconds since the interface's last read; `UINT64_MAX` before
  // the first.
  uint64_t age_ms;
  // The world's name, as `EnshwickHostApi.world_key`
  // gives it; empty while none is known, as on a client joined to a
  // server.
  struct EnshwickStr scene;
  // The open HUD screen; group `EnshwickClientSnapshot_SCREENS`.
  struct EnshwickStr hud_screen;
  // The screen the open one returns to when it closes, empty when it
  // returns to the world; group `EnshwickClientSnapshot_SCREENS`.
  struct EnshwickStr return_screen;
  // The tab the in-game menu shows, as the screen it stands for
  // (`Social`, `Map`, `Journal`, `PlayerCrafting`, `Backpack`,
  // `Character` or `SkillTree`), empty while no menu shows; group
  // `EnshwickClientSnapshot_MENU_TAB`.
  struct EnshwickStr menu_tab;
  // The local player's held controls, as
  // `EnshwickHostApi.local_held` gives them; group
  // `EnshwickClientSnapshot_HELD`.
  uint64_t held;
  // The local player's entity; group `EnshwickClientSnapshot_PLAYER`.
  EnshwickEntity player;
  // The open screen's host, such as the chest whose screen shows, or 0;
  // group `EnshwickClientSnapshot_SCREENS`.
  EnshwickEntity host;
  // The recipe the crafting menu highlights, 0 for none; group
  // `EnshwickClientSnapshot_RECIPE`.
  EnshwickRecipeId highlighted_recipe;
  // Zero.
  uint32_t reserved;
  // The server a `EnshwickClientCommand_JOIN` of this loader joins or joined,
  // as `host:query_port`, while the client is on its way there or in its
  // world; empty in the menu and in a world the client hosts or joined
  // another way. ABI 12.11.
  struct EnshwickStr server;
} EnshwickClientSnapshot;
// No interface read yet.
#define EnshwickClientSnapshot_NEVER 0
// The interface's reads are off on this build or after a fault.
#define EnshwickClientSnapshot_OFF 1
// The last read is older than a second: no frame drew a game since,
// as while loading.
#define EnshwickClientSnapshot_STALE 2
// The last read is at most a second old.
#define EnshwickClientSnapshot_FRESH 3
// `hud_screen`, `return_screen` and `host`.
#define EnshwickClientSnapshot_SCREENS 1
// `menu_tab`.
#define EnshwickClientSnapshot_MENU_TAB 2
// `held`, a read at most a second old.
#define EnshwickClientSnapshot_HELD 4
// `player`.
#define EnshwickClientSnapshot_PLAYER 8
// `highlighted_recipe`.
#define EnshwickClientSnapshot_RECIPE 16

// A comfort provider in a base registering its comfort for its category,
// as `EnshwickHostApi.comfort_filter` hands it on.
typedef struct EnshwickComfort {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The comfort category; a base keeps the best provider per category.
  uint32_t category;
  // The comfort it registers.
  int32_t value;
  // Where the provider stands, in metres.
  EnshwickMetres position;
} EnshwickComfort;

// A running mod's command names, from
// `EnshwickHostApi.command_list`.
typedef struct EnshwickCommandInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The full name, `admin-tools` in `/admin-tools online`.
  struct EnshwickStr name;
  // The shorter alias, `adm` in `/adm online`; empty when another mod
  // took the word first.
  struct EnshwickStr alias;
  // The id of the mod that named them.
  struct EnshwickStr mod_id;
} EnshwickCommandInfo;

// A stack about to enter one slot of a container, as
// `EnshwickHostApi.container_rule` hands it on.
// ABI 12.9.
typedef struct EnshwickContainerPut {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The container: the placed entity whose inventory holds the slot.
  EnshwickEntity container;
  // The item the container was placed from (`UsedItem`), the one the
  // rule was registered for.
  EnshwickItemId container_item;
  // The item of the stack.
  EnshwickItemId item;
  // How many items enter.
  uint32_t count;
  // The slot, as a sub-inventory entity in the low 32 bits and its slot
  // index above them.
  uint64_t slot;
} EnshwickContainerPut;

// One content rename for `EnshwickHostApi.rename_content`:
// content saved under `from` reads as the content now under `to`.
//
// Frozen, written as an array: a rename is two content GUIDs.
typedef struct EnshwickContentRename {
  // The GUID the content had.
  struct EnshwickGuid from;
  // The GUID the content has now: of a content id the calling mod
  // declared with `declare_content`, of the game's own content, or the
  // `from` of another of its renames.
  struct EnshwickGuid to;
} EnshwickContentRename;

// A control firing, handed to the mod's `EnshwickControlFn`.
typedef struct EnshwickControlEvent {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The control's name in its mod; valid for the call.
  struct EnshwickStr name;
  // What happened: `EnshwickControlSpec_PRESS`, `EnshwickControlSpec_RELEASE` or
  // `EnshwickControlSpec_HOLD`.
  uint32_t kind;
  // 1 when the server fires a control that follows a game action, for
  // `player`; 0 on the client, for its own player.
  uint8_t server;
  // Zero.
  uint8_t reserved[3];
  // The player whose game action fired the control, valid for the call,
  // when `server` is 1; null on the client.
  const struct EnshwickPlayerInfo *player;
} EnshwickControlEvent;

// A control a mod declares with `EnshwickHostApi.control`
// during `enshwick_mod_init`.
//
// A key is the key's scan code in
// bits 0 to 9 (bit 8 for an extended key, bit 9 for a mouse button) and the
// modifiers that must be down with it in bits 16 to 18.
typedef struct EnshwickControlSpec {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The control's name in its mod, 1 to 32 of `[a-z0-9-]`; the player's
  // override names it `<mod id>:<name>`. Valid for the call.
  struct EnshwickStr name;
  // What the mod's key bindings show for it, in the player's language. Valid
  // for the call; the loader copies it.
  struct EnshwickStr label;
  // The default key, or 0 for none.
  uint32_t key;
  // A second default key, or 0 for none.
  uint32_t second;
  // The game action the control follows, by the name the game's input
  // gives it, such as `ContextualAction`; empty for none. Valid for the
  // call.
  struct EnshwickStr link;
  // When the control fires: `EnshwickControlSpec_PRESS`,
  // `EnshwickControlSpec_RELEASE` and `EnshwickControlSpec_HOLD`, at least one of
  // them and no other bit.
  uint32_t fires;
  // How long a key stays down before `EnshwickControlSpec_HOLD` fires, in
  // milliseconds.
  uint32_t hold_ms;
  // The id of the mod's own action the server runs, for the player, each
  // time the control fires on a client; empty for none. A mod of side
  // server needs it, since its controls reach the clients without its
  // code; then `label` is a key of its texts. Valid for the
  // call.
  struct EnshwickStr action;
} EnshwickControlSpec;
// The control fires once when its key goes down.
#define EnshwickControlSpec_PRESS 1
// The control fires once when its key comes up.
#define EnshwickControlSpec_RELEASE 2
// The control fires once when its key stayed down for
// `EnshwickControlSpec.hold_ms`.
#define EnshwickControlSpec_HOLD 4

// A player's craft request, handed to the filters registered with
// `EnshwickHostApi.craft_filter` before the server crafts.
//
// The name is enshwick's; the fields are the game's `CraftRecipeEvent`.
typedef struct EnshwickCraftCheck {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The crafting player's character.
  EnshwickEntity crafting_operator_id;
  // The station, or 0 for crafting by hand.
  EnshwickEntity crafting_station_id;
  // The recipe id.
  EnshwickRecipeId recipe_id;
  // How many.
  uint16_t recipe_amount;
  // Zero.
  uint16_t reserved;
} EnshwickCraftCheck;

// Damage about to reach an entity's health, handed to the filters
// registered with `EnshwickHostApi.damage_filter`, which may change it.
typedef struct EnshwickDamageCheck {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // `EnshwickDamageCheck_IMPACT`, `EnshwickDamageCheck_FALL_DAMAGE`, `EnshwickDamageCheck_FOG`,
  // `EnshwickDamageCheck_DEADLY_FOG`, `EnshwickDamageCheck_SUFFOCATED`,
  // `EnshwickDamageCheck_DROWNING_DEATH`, `EnshwickDamageCheck_FREEZING` or
  // `EnshwickDamageCheck_ADMIN_KILL`.
  uint32_t cause;
  // The entity that takes the damage.
  EnshwickEntity target;
  // What dealt a hit: the attacker, or its projectile; 0 for other
  // causes.
  EnshwickEntity source;
  // Who dealt a hit, the root of the impact that hit (the archer for an
  // arrow), or `source` when the game names none; 0 for other causes.
  EnshwickEntity attacker;
  // The health the damage takes, for causes other than a hit; for a hit,
  // the sum of its damage types but healing, which the loader does not
  // read back.
  float total;
  // How many floats `amounts` points at: the values of the running
  // game's `DamageTypeNew`, read from its reflection; 0 for causes other
  // than a hit.
  uint32_t amount_count;
  // A hit's damage by type, in the game's order of its damage types
  // (its `DamageTypeNew` enum), after the target's resistances and before
  // its armour; healing is what heals. The filter may change them; valid
  // for the call. Null for other causes.
  float *amounts;
} EnshwickDamageCheck;
// A hit from a weapon, a spell, a creature or an explosion.
#define EnshwickDamageCheck_IMPACT 0
// A fall.
#define EnshwickDamageCheck_FALL_DAMAGE 1
// The shroud draining a character whose time in it ran out.
#define EnshwickDamageCheck_FOG 2
// The deadly shroud, which kills at once.
#define EnshwickDamageCheck_DEADLY_FOG 3
// A character out of breath under water.
#define EnshwickDamageCheck_SUFFOCATED 4
// A creature out of breath under water, which dies at once.
#define EnshwickDamageCheck_DROWNING_DEATH 5
// A character frozen through, out of body heat.
#define EnshwickDamageCheck_FREEZING 6
// An administrator's kill command, of chosen enemies or of whole
// factions or players: `total` is 1, and the kill goes ahead unless
// the filters leave 0.
#define EnshwickDamageCheck_ADMIN_KILL 7

// The vanilla value a `EnshwickDataChange` changes; the field says what its
// `entry` is. A later minor may add fields, which an older loader refuses
// with `EnshwickStatus_BAD_ARGS`.
typedef uint32_t EnshwickDataField;
// How many of an item one inventory slot holds, 1 to 65535; the entry
// is an item.
#define EnshwickDataField_MAX_STACK 1
// How many of each item a recipe makes; the entry is a recipe.
#define EnshwickDataField_RECIPE_OUTPUT 2
// How many of each ingredient a recipe takes; the entry is a recipe.
#define EnshwickDataField_RECIPE_COST 3
// How many of the ingredient `item` a recipe takes, 0 taking it out; the
// entry is a recipe.
#define EnshwickDataField_RECIPE_INGREDIENT 4
// How long a recipe takes, in seconds; the entry is a recipe.
#define EnshwickDataField_RECIPE_TIME 5
// How many slots the container an item places holds; the entry is the
// item.
#define EnshwickDataField_CONTAINER_SLOTS 6
// A weapon's damage factor; the entry is the item.
#define EnshwickDataField_DAMAGE_MOD 7
// A weapon's attack speed factor; the entry is the item.
#define EnshwickDataField_SPEED_MOD 8
// Every armour value of an item, scaled only; the entry is the item.
#define EnshwickDataField_ARMOR 9
// How long an item burns as fuel, in seconds; the entry is the item.
#define EnshwickDataField_BURN_TIME 10
// The energy an item gives as fuel; the entry is the item.
#define EnshwickDataField_ENERGY_OUTPUT 11
// The comfort a placed item gives; the entry is the item.
#define EnshwickDataField_COMFORT_AMOUNT 12
// The stack size random loot rolls for an item; the entry is the item.
#define EnshwickDataField_LOOT_STACK_SIZE 13
// The count fill tables give of an item; the entry is the item.
#define EnshwickDataField_LOOT_COUNT 14
// The chance fill tables give an item; the entry is the item.
#define EnshwickDataField_LOOT_SPAWN_PROBABILITY 15
// The weight of an item in fill tables; the entry is the item.
#define EnshwickDataField_LOOT_WEIGHT 16
// The workshop a recipe is made at, by its game id.
#define EnshwickDataField_RECIPE_STATION 17

// Which vanilla items a `EnshwickDataChange` reaches.
typedef uint32_t EnshwickDataSelect;
// The entry the change names.
#define EnshwickDataSelect_ENTRY 0
// Every item of the categories in `EnshwickDataChange.categories`.
#define EnshwickDataSelect_CATEGORY 1
// Every item one slot holds more than one of.
#define EnshwickDataSelect_STACKABLE 2
// Every item that places a container.
#define EnshwickDataSelect_CONTAINERS 3
// Every recipe whose output is of the kind whose
// `EnshwickRecipeKind` code is `EnshwickDataChange.categories`.
#define EnshwickDataSelect_RECIPE_KIND 4
// Every recipe made at the workshop whose GUID is
// `EnshwickDataChange.entry`.
#define EnshwickDataSelect_WORKSHOP 5
// Every backpack.
#define EnshwickDataSelect_BACKPACKS 6

// One change of a vanilla value, as
// `EnshwickHostApi.change_data` takes it. The
// loader copies it; `source` is borrowed for the call.
typedef struct EnshwickDataChange {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The GUID of the vanilla entry.
  struct EnshwickGuid entry;
  // The value changed.
  EnshwickDataField field;
  // Whether `value` replaces it or scales it.
  EnshwickChangeOp op;
  // The new value or the factor.
  double value;
  // Where the change comes from, such as `items.toml:12`, for the reports
  // of conflicts; may be empty.
  struct EnshwickStr source;
  // The item the field names inside its entry, such as a recipe's
  // ingredient; all zero for none.
  struct EnshwickGuid item;
  // Which vanilla entries the change reaches: `entry` alone, or every
  // item or recipe a selection picks, with `entry` all zero but for
  // `EnshwickDataSelect_WORKSHOP`.
  EnshwickDataSelect select;
  // The item categories `EnshwickDataSelect_CATEGORY` picks, a bit per the
  // game's `ItemCategory`, or the `EnshwickRecipeKind` code
  // `EnshwickDataSelect_RECIPE_KIND` picks; 0 for any other selection.
  uint32_t categories;
} EnshwickDataChange;

// The clock of the server's world, written by `EnshwickHostApi.time_of_day`.
//
// Times are game nanoseconds of the day, 0 up to 24 hours; lengths are
// real nanoseconds.
typedef struct EnshwickDaytimeInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The time of day.
  int64_t time_ns;
  // The time of day the day starts at.
  int64_t day_start_ns;
  // The time of day the night starts at.
  int64_t day_end_ns;
  // How long the day lasts in real time at speed 1.
  int64_t day_length_ns;
  // How long the night lasts in real time at speed 1.
  int64_t night_length_ns;
  // How fast the clock runs: 1 normally, faster while players sleep, and
  // times every mod's factor (`EnshwickHostApi.set_time_speed`).
  float speed;
  // `EnshwickDaytimeInfo_DAY` or none.
  uint32_t flags;
} EnshwickDaytimeInfo;
// It is day.
#define EnshwickDaytimeInfo_DAY 1

// How a mod relates to another mod it names.
typedef uint8_t EnshwickRelation;
// The other mod must be present in the version range; it loads first.
#define EnshwickRelation_REQUIRES 0
// When the other mod is present, it loads first; it may be absent.
#define EnshwickRelation_AFTER 1
// This mod does not work beside the other mod in the version range (any
// version when the range is empty): when that one loads, this one is
// skipped.
#define EnshwickRelation_BREAKS 2

// A relation to another mod, by id and SemVer range.
typedef struct EnshwickDependency {
  // The other mod's id.
  struct EnshwickStr id;
  // A Cargo-style version requirement, such as `>=0.1, <1.0`; empty for any.
  struct EnshwickStr range;
  // How the two relate.
  EnshwickRelation relation;
} EnshwickDependency;

// A request to drop a stack of new items on the ground, for
// `EnshwickHostApi.drop_items`.
typedef struct EnshwickDropRequest {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The item.
  EnshwickItemId item;
  // How many, 1 to 65535: one stack lies as one pickup.
  uint32_t count;
  // Where, in metres.
  EnshwickMetres position;
  // The item's level, as the game levels weapons and armour; 0 for none.
  uint32_t level;
  // The game's `ItemRarity` of the item, 0 for common.
  uint32_t rarity;
} EnshwickDropRequest;

// Flags of `EnshwickHostApi.define_entity_data`.
typedef uint32_t EnshwickEntityDataFlags;
// The server keeps the values to itself.
#define EnshwickEntityDataFlags_NONE 0
// The clients that run Enshwick get every value.
#define EnshwickEntityDataFlags_SENT 1

// An entity placed in the world, from `EnshwickHostApi.entities`.
//
// Frozen, written as an array by
// `EnshwickHostApi.entities`: an entity and its place
// are a plain value, as the list rule on `EnshwickHostApi` says.
typedef struct EnshwickEntityPlace {
  // The entity.
  EnshwickEntity entity;
  // Zero.
  uint32_t reserved;
  // Where it is, in metres.
  EnshwickMetres position;
} EnshwickEntityPlace;

// Something the loader tells subscribed mods about, with
// `EnshwickHostApi.subscribe`.
typedef uint32_t EnshwickEvent;
// The mod's settings changed while a world runs: its file on disk, a
// setting set by the console or the Mods screen, or the values the
// server of another's world sent; no data. The thread is not fixed: a
// loader thread about a second after a file edit, the thread that made
// any other change.
#define EnshwickEvent_SETTINGS_CHANGED 1
// The loader's settings changed, log levels among them; no data. Raised
// on a loader thread.
#define EnshwickEvent_LOADER_SETTINGS_CHANGED 2
// A simulation step ran; data is a `EnshwickTickData`. Raised on the server's
// game thread after the step, about 20 times a second with nobody online
// and 60 with players, from before `EnshwickEvent_WORLD_LOADED`. Server only.
#define EnshwickEvent_TICKED 3
// The world finished loading and the server runs; no data. Raised on the
// game thread. Server only.
#define EnshwickEvent_WORLD_LOADED 4
// A save of the world finished writing to disk; no data. Raised on the
// game thread; the final save runs after `EnshwickEvent_SHUTDOWN_STARTED`. Server only.
#define EnshwickEvent_WORLD_SAVED 5
// A player's character entered the world; data is a `EnshwickPlayerData`.
// Raised on the game thread. Server only.
#define EnshwickEvent_PLAYER_JOINED 6
// A player who joined is leaving, with their character still in the
// world; data is a `EnshwickPlayerData`. A player whose slot the game cleared
// or gave another player unseen is told after the step, as recorded at
// join, with the character maybe gone. Raised on the game thread.
// Server only.
#define EnshwickEvent_PLAYER_LEFT 7
// The server begins an orderly stop with the world and players still
// alive; no data. Raised on the game thread; a killed process raises
// nothing. Server only.
#define EnshwickEvent_SHUTDOWN_STARTED 8
// A player placed a building piece or prop; data is a `EnshwickBuildingData`.
// Raised on the game thread after the step that placed it. Server only.
#define EnshwickEvent_BUILDING_PLACED 9
// A player picked up or tore down a placed piece; data is a
// `EnshwickBuildingData` whose entity no longer exists. Raised on the game
// thread after the step. Server only.
#define EnshwickEvent_BUILDING_REMOVED 10
// The join check kept this client out of a server that runs mods of
// side both the client lacks or runs in another version; no data.
// `EnshwickHostApi.join_missing` lists those mods with the versions the
// server runs and their pages. Raised on the game thread once the player
// closed the loader's dialog. Client only.
#define EnshwickEvent_JOIN_REFUSED 11
// The Flame Altars of the server's world changed: one was placed or
// removed, moved, raised a level or had its zone built anew; no data.
// `EnshwickHostApi.altars` lists them as they are now. Raised on the game
// thread after the step that noticed it, while a mod has opened altars
// (`EnshwickHostApi.request_altars`). Server only.
#define EnshwickEvent_ALTARS_CHANGED 12
// The day began in the server's world: the clock passed the hour day
// starts at, or a mod set it into the day; no data. Raised on the game
// thread after the step that noticed it. Server only.
#define EnshwickEvent_DAY_STARTED 13
// The night began in the server's world, as `EnshwickEvent_DAY_STARTED` the
// day; no data. Server only.
#define EnshwickEvent_NIGHT_STARTED 14
// A knowledge value of the server's world or of a connected player
// changed: a recipe learnt, a quest step, a place found; data is a
// `EnshwickKnowledgeData`. Raised on the game thread after the step that
// applied it. Server only.
#define EnshwickEvent_KNOWLEDGE_CHANGED 15
// A player completed a quest: its bool was set for them, and the game
// gave its experience, not its items; data is a `EnshwickQuestData`. Raised
// on the game thread after the step, for every change that sets the
// bool, a mod's grant of it included. Server only.
#define EnshwickEvent_QUEST_COMPLETED 16
// A chunk of the server's entities loaded: players came near, and its
// entities exist now; data is a `EnshwickChunkData`. The terrain's voxels
// load apart from it, without a player, and raise no event. Raised on
// the game thread after the step. Server only.
#define EnshwickEvent_CHUNK_LOADED 17
// A chunk of the server's entities unloaded: no player is near, and its
// entities are gone; data is a `EnshwickChunkData`. The terrain's voxels
// unload apart from it and raise no event. Raised on the game thread
// after the step. Server only.
#define EnshwickEvent_CHUNK_UNLOADED 18
// The settings a mod reads were replaced whole: a world loaded in this
// process (its settings from then on) or stopped (the defaults again),
// or on a client the server of another's world sent its settings or the
// client left it; no data. Raised on the game thread as a world loads
// or stops, else on the thread that took the server's message.
#define EnshwickEvent_SETTINGS_REPLACED 19
// A player died, from a hit, a fall, the shroud or drowning; data is a
// `EnshwickPlayerDeathData`. Raised on the game thread after the step.
// Deaths the game keeps no record of, such as freezing or an
// administrator's kill, raise none. Server only.
#define EnshwickEvent_PLAYER_DIED 20
// A line reached the server's chat: a player's line the chat filters
// kept, as they left it, an emote or another line the server posted;
// data is a `EnshwickChatLine`. Lines the loader posts for
// mods raise none, and a player's lines raise at most eight a second.
// Raised on the game thread after the step. Server only.
#define EnshwickEvent_CHAT_SAID 21
// A save of the world got under way: what a mod writes now beside the
// save belongs to it, and `EnshwickEvent_WORLD_SAVED` follows once it is on
// disk; no data. Raised on the game thread after the step that started
// it. Server only.
#define EnshwickEvent_WORLD_SAVING 22
// The server's voxel editor wrote voxels of the world: a player's tool
// or building, the game itself, or a mod's terrain edit, transaction
// or undo; data is a `EnshwickVoxelEditRequest`. One
// event per write, which an undo or a transaction makes per brick of
// 8 voxels; a write that changed nothing raises none. The name is
// enshwick's. Raised on the game thread after the step. Server only.
#define EnshwickEvent_VOXELS_CHANGED 23
// A page of the terrain came into the server's memory, so its voxels
// usually read and edit; data is a `EnshwickTerrainPageData`. The ground loads and
// drops by page on its own, with players near or not, so a mod that
// keeps its terrain edits outside the save writes them again here.
// Raised on the game thread after the step. Dedicated server only.
#define EnshwickEvent_TERRAIN_PAGE_LOADED 24
// A page of the terrain left the server's memory, the one used least
// lately, and its voxels read as not loaded until a read loads it
// again; data is a `EnshwickTerrainPageData`. Raised on the game thread
// after the step.
// Dedicated server only.
#define EnshwickEvent_TERRAIN_PAGE_UNLOADED 25
// A level or experience write the mod asked for with
// `EnshwickHostApi.progress_write` was
// applied or dropped; data is a
// `EnshwickProgressWriteData`. Raised for that mod
// only, on the game thread after the step that applied the write, or
// after the second step end with no level step that could take it;
// a write still waiting when a hosted game ends is heard as dropped
// then. The name is enshwick's. Server only.
#define EnshwickEvent_PROGRESS_WRITTEN 26
// A player online set, moved or cleared the waypoint all players see;
// data is a `EnshwickWaypointData`. The loader looks at the waypoints every
// 100 ms while a mod subscribed, on the server's game thread after the
// step: a player's first look, as at a join, only notes the waypoint,
// and one set outside the map's frame raises nothing, though a move
// back from there does. A mod's waypoint handler hears it. The name
// is enshwick's. Server only. ABI 12.16.
#define EnshwickEvent_WAYPOINT_CHANGED 27
// A player, or the world for a quest of the world's, started a quest:
// the journal shows its first step now, by the knowledge the step asks
// for, and did not before; data is a `EnshwickQuestData`. A quest whose first
// step always shows raises none. Raised on the game thread after the
// step that changed that knowledge, a mod's change included; what a
// player holds when they join raises nothing. The name is enshwick's.
// Server only. ABI 12.22.
#define EnshwickEvent_QUEST_STARTED 28
// A step of a quest is done for a player or the world: the knowledge
// its completion asks for holds now and did not before; data is a
// `EnshwickQuestStepData`. Raised as `EnshwickEvent_QUEST_STARTED` is. The name is
// enshwick's. Server only. ABI 12.22.
#define EnshwickEvent_QUEST_STEP_DONE 29
// A counter one of a quest step's progress parts counts changed, such
// as a mod's objective; data is a `EnshwickQuestProgressData` with the count
// now and the count the part asks for. Raised as
// `EnshwickEvent_QUEST_STARTED` is, for each change of the counter. The
// name is enshwick's. Server only. ABI 12.22.
#define EnshwickEvent_QUEST_PROGRESSED 30

// A mod event a mod defines, or the one it subscribes to.
typedef struct EnshwickEventSpec {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The event's name, `<defining mod id>:<what>`.
  struct EnshwickStr name;
  // The event data's fields; events grow by appending.
  struct EnshwickLayout layout;
} EnshwickEventSpec;

// One game event type of the running build, as `EnshwickHostApi.event_types`
// writes it. `name` is the loader's, valid for the rest of the process.
typedef struct EnshwickEventTypeInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The reflected name.
  struct EnshwickStr name;
  // The reflected type hash, which each event of the type carries.
  uint32_t hash;
  // The bytes each event of the type carries.
  uint32_t size;
} EnshwickEventTypeInfo;

// One character's experience before the game applies it, in an
// `EnshwickExperiencePass`. Frozen, passed as an array: what concerns the whole
// pass grows in the headed `EnshwickExperiencePass`.
typedef struct EnshwickExperienceRow {
  // The character's entity.
  EnshwickEntity entity;
  // Its level.
  uint32_t level;
  // Experience inside the level.
  uint32_t progress;
  // What the level needs.
  uint32_t need;
  // Gained experience the game applies next, at most one level a tick;
  // a filter may change it.
  uint32_t gained;
} EnshwickExperienceRow;

// Every spawned character's experience before the server applies it, as
// `EnshwickHostApi.experience_filter` hands it on.
typedef struct EnshwickExperiencePass {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The characters, valid and writable for the call.
  struct EnshwickExperienceRow *rows;
  // How many rows there are.
  uint32_t row_count;
  // The highest level.
  uint32_t level_cap;
  // What level `level` (1 or more) needs; valid during the call with
  // `token`.
  uint32_t (*need_for)(void *token, uint32_t level);
  // The argument of `need_for`.
  void *token;
} EnshwickExperiencePass;

// The size, scale and version of the client's fog of war, which
// `EnshwickHostApi.fog_read` writes.
//
// The fog is one image of a byte per pixel, the coverage from 0 (hidden)
// to 255 (uncovered), row 0 at the world's high z edge and x growing with
// world x.
typedef struct EnshwickFogInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The image's width in pixels.
  uint32_t width;
  // The image's height in pixels.
  uint32_t height;
  // The metres one pixel covers along each axis.
  float metres_per_pixel;
  // The world's z size the fog covers, in metres: pixel row
  // `(size_z - z) / metres_per_pixel`.
  float size_z;
  // Grows whenever pixels of the copy changed.
  uint64_t version;
  // Counts the fogs the loader has seen, one per world the client
  // enters; a new one starts from the save of that world.
  uint64_t session;
  // Counts the closed windows of uncovering in this session: the
  // loader closes the pixels uncovered since the last window into a
  // new one at most once a second; 0 before the first.
  uint64_t uncovered;
  // The pixels the last window turned from hidden to shown.
  uint32_t uncovered_pixels;
  // Zero.
  uint32_t reserved;
  // The lowest pixel of the box around the last window's pixels, as
  // x and row.
  uint32_t uncovered_min[2];
  // The box's width and height in pixels.
  uint32_t uncovered_size[2];
} EnshwickFogInfo;

// The keys a game control holds in the client's keyboard and mouse
// bindings, as `EnshwickHostApi.game_binding`
// writes them. ABI 12.20.
//
// Each key is in the form of `EnshwickControlSpec.key`, 0 for none. A key a
// mod control may not take, such as Shift alone or Escape, keeps its scan
// code; input that is no key, such as the mouse wheel, is
// `EnshwickGameBinding_OTHER`.
typedef struct EnshwickGameBinding {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The first key.
  uint32_t first;
  // The second key.
  uint32_t second;
} EnshwickGameBinding;
// A slot that holds input the key form does not name: the mouse
// wheel, a gamepad, or a key without a name.
#define EnshwickGameBinding_OTHER UINT32_MAX

// A game event handed to a `EnshwickGameEventFn`, from the ECS event streams
// of the server's systems.
typedef struct EnshwickGameEventData {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The event's reflected type hash.
  uint32_t type_hash;
  // How many bytes `data` holds: the event type's reflected size.
  uint32_t size;
  // The event as the engine wrote it, valid for the call.
  const uint8_t *data;
} EnshwickGameEventData;

// Flags of a `EnshwickHookSpec`.
typedef uint32_t EnshwickHookFlags;
// No flag.
#define EnshwickHookFlags_NONE 0
// No other mod may link on this target.
#define EnshwickHookFlags_EXCLUSIVE 1
// The link reads the game's call site through
// `EnshwickHostApi.caller_frame`.
#define EnshwickHookFlags_CALLER_FRAME 2

// A request to link a detour into the chain of a game function.
//
// The loader points `next` at the following link or at the original
// function when it arms the chain; until then `next` holds 0 and the detour
// is never called.
typedef struct EnshwickHookSpec {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The absolute address of the game function.
  uint64_t target;
  // The detour, with the target's signature and the `system` ABI.
  size_t detour;
  // A static slot the loader keeps pointing at what comes next.
  const uintptr_t *next;
  // Lower runs earlier; equal priorities run in load order.
  int32_t priority;
  // Hook flags.
  EnshwickHookFlags flags;
} EnshwickHookSpec;

// The outcome of a call across the interface.
//
// Each status has one meaning. `EnshwickStatus_MISSING` names only a thing that
// is absent, `EnshwickStatus_UNAVAILABLE` a call that cannot run in this process
// or on this build, `EnshwickStatus_REFUSED` a refusal by the loader's rules or
// another mod, `EnshwickStatus_BUSY` a full queue or rate that a later call may
// pass, `EnshwickStatus_TOO_SMALL` a buffer too short for an answer whose length
// the call wrote, `EnshwickStatus_BAD_ARGS` an argument that is malformed, and
// `EnshwickStatus_LOADER_FAULT` a fault inside the loader itself.
//
// Values only append in later minors, and a mod treats a status it does
// not know as a failure.
typedef uint32_t EnshwickStatus;
// Success.
#define EnshwickStatus_OK 0
// What the call names is not there: an id, entity, key, file or symbol
// that does not exist.
#define EnshwickStatus_MISSING 1
// More than one candidate answers a lookup.
#define EnshwickStatus_AMBIGUOUS 2
// The loader's rules or another mod refuse the request, and asking
// again unchanged gets the same answer.
#define EnshwickStatus_REFUSED 3
// An argument is malformed: a null pointer the call needs, a value out
// of range, text that is not UTF-8.
#define EnshwickStatus_BAD_ARGS 4
// The call came in a phase where it is not allowed, such as a
// registration after `enshwick_mod_init`.
#define EnshwickStatus_TOO_LATE 5
// The mod has faulted and is off.
#define EnshwickStatus_FAULTED 6
// Called from a thread the function does not serve, such as a game
// read off the server's game thread, or a world change inside a step.
#define EnshwickStatus_WRONG_THREAD 7
// The part of the world asked about is not loaded yet; asking starts
// loading it, so the same call works again in a second or so.
#define EnshwickStatus_NOT_LOADED 8
// The box asked about leaves the world: past its edge across, or above
// the height the loader takes.
#define EnshwickStatus_OUTSIDE_WORLD 9
// The component type is the static half of its ECS type: the set-up
// values the entity's template holds and every entity of that template
// shares, which a write would change for all of them.
#define EnshwickStatus_STATIC_HALF 10
// The entity named is no prop: not placed from an item, or a creature
// such as an animal, a pet or a survivor, which only the general
// entity calls reach.
#define EnshwickStatus_NOT_A_PROP 11
// The entity has nothing the world's next load finds it again by: a
// player's character, or an entity with no place in the world.
#define EnshwickStatus_NOT_KEPT 12
// The call cannot run here: no server runs in this process, as on a
// client in another's game, no world runs, the side is the other one,
// or this game build lacks the pieces the call needs.
#define EnshwickStatus_UNAVAILABLE 13
// The resource asked about is being built on this thread, by a patch
// further up the call, so reading it would build it again without
// end.
#define EnshwickStatus_BUILDING 14
// The loader failed inside its own code; the entry turns off and
// answers this until the process ends, and the mod stays on.
#define EnshwickStatus_LOADER_FAULT 15
// A queue is full or a rate is spent: the same call may pass later.
#define EnshwickStatus_BUSY 16
// The caller's buffer is too short; the call wrote the length the
// answer needs and nothing else.
#define EnshwickStatus_TOO_SMALL 17

// An opaque handle the loader gives each mod; every call names its caller
// with it.
typedef size_t EnshwickModHandle;
// No mod.
#define EnshwickModHandle_NONE 0

// The severity of a log line.
typedef uint8_t EnshwickLevel;
// Something failed and a feature is off.
#define EnshwickLevel_ERROR 0
// Something unexpected that the mod handled.
#define EnshwickLevel_WARN 1
// A milestone worth one line.
#define EnshwickLevel_INFO 2
// Detail for development, off in releases.
#define EnshwickLevel_DEBUG 3

// A service a mod publishes, or the one it asks for.
typedef struct EnshwickServiceSpec {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The service's name, `<publisher id>:<what>`.
  struct EnshwickStr name;
  // The incompatible version: tables of two majors share nothing.
  uint32_t major;
  // The table's fields; tables grow by appending.
  struct EnshwickLayout layout;
  // The `'static` table; null when asking.
  const void *table;
} EnshwickServiceSpec;

// What `EnshwickHostApi.service` found.
typedef struct EnshwickServiceFound {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The publisher's table, valid for the process.
  const void *table;
  // How many fields the publisher's table has.
  uint32_t fields;
  // The publisher's mod id, valid for the process.
  struct EnshwickStr owner;
  // The publisher's version, valid for the process.
  struct EnshwickStr owner_version;
  // Why nothing was found, valid for the process; empty on success.
  struct EnshwickStr reason;
} EnshwickServiceFound;

// Flags of a `EnshwickManifest`.
typedef uint32_t EnshwickManifestFlags;
// No flag.
#define EnshwickManifestFlags_NONE 0
// The mod is a developer tool, never for real play: it runs only while
// `DEVELOPER_TOOLS` exists, and every player reads that developer tools are on.
#define EnshwickManifestFlags_DEVELOPER 4

// A mod the loader loaded, from `EnshwickHostApi.mods`; the loader itself comes
// first, with the id `enshwick`.
//
// Every string stays valid for the life of the process.
typedef struct EnshwickModInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The mod id.
  struct EnshwickStr id;
  // The mod's SemVer version.
  struct EnshwickStr version;
  // Where the mod runs, `EnshwickSide_SERVER` or `EnshwickSide_BOTH`; a
  // `EnshwickSide_BOTH` mod counts in the join check.
  EnshwickSide side;
  // The manifest's flags.
  EnshwickManifestFlags flags;
  // 1 when the mod is off: refused at init or faulted since.
  uint8_t off;
  // The DLL's path relative to the game's folder, with `/` separators,
  // such as `mods/you.trading-post/trading_post.dll`.
  struct EnshwickStr file;
  // The manifest's human metadata, as in `EnshwickManifest`;
  // empty in `EnshwickHostApi.join_missing`,
  // which never passes on text a server chose.
  struct EnshwickStr name;
  // One sentence on what the mod does.
  struct EnshwickStr description;
  // The authors' names.
  const struct EnshwickStr *authors;
  // How many names `authors` points at.
  uint32_t author_count;
  // An SPDX licence expression.
  struct EnshwickStr license;
  // Where players read about the mod.
  struct EnshwickStr homepage;
} EnshwickModInfo;


// How the links of an extension point act on its answer, as the defining
// mod chooses.
typedef uint32_t EnshwickPointMode;
// Links see the answer and change nothing.
#define EnshwickPointMode_OBSERVE 0
// Links may change the answer; the defining mod may ignore the change.
#define EnshwickPointMode_ADVISE 1
// Links may change the answer, and the defining mod takes it.
#define EnshwickPointMode_OVERRIDE 2

// Flags of a `EnshwickPointSpec`.
typedef uint32_t EnshwickPointFlags;
// No flag.
#define EnshwickPointFlags_NONE 0
// A link may refuse: the chain stops and the answer is a refusal.
#define EnshwickPointFlags_REFUSE 1

// An extension point a mod defines, or the one a link attaches to.
typedef struct EnshwickPointSpec {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The point's name, `<defining mod id>:<point>`.
  struct EnshwickStr name;
  // How links act on the answer.
  EnshwickPointMode mode;
  // Point flags.
  EnshwickPointFlags flags;
  // The fields of the input links read; inputs grow by appending.
  struct EnshwickLayout input;
  // The answer, one field: its type, size and alignment.
  struct EnshwickLayout output;
} EnshwickPointSpec;

// What `EnshwickHostApi.define_point` hands back.
typedef struct EnshwickPointHandle {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The id `EnshwickHostApi.run_point` takes.
  uint32_t id;
  // How many links run, valid for the process: 0 until every mod is
  // initialised and whenever none is live, so the defining mod skips
  // `EnshwickHostApi.run_point`.
  const uint32_t *live;
} EnshwickPointHandle;

// A link a mod attaches to another mod's extension point, with its place
// in the chain.
typedef struct EnshwickPointLink {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The point, as the attaching mod declares it.
  const struct EnshwickPointSpec *point;
  // Ids of mods whose links run after this one.
  const struct EnshwickStr *before;
  // Ids of mods whose links run before this one.
  const struct EnshwickStr *after;
  // How many ids `before` points at.
  uint32_t before_count;
  // How many ids `after` points at.
  uint32_t after_count;
  // Among links the ids leave free, lower runs earlier.
  int32_t priority;
} EnshwickPointLink;

// A link's answer, and the chain's.
typedef uint32_t EnshwickPointVerdict;
// The answer stays as it was.
#define EnshwickPointVerdict_PASS 0
// The link wrote a new answer over the value.
#define EnshwickPointVerdict_SET 1
// The link refused: the chain stops.
#define EnshwickPointVerdict_REFUSE 2
// The link could not answer, as when its handler panicked: the loader
// turns this link off, leaves the answer as it was and asks the next.
#define EnshwickPointVerdict_FAILED 3

// Which waits `EnshwickHostApi.loop_wait` found and
// hooked, as bits.
typedef uint32_t EnshwickLoopWaitSites;
// None.
#define EnshwickLoopWaitSites_NONE 0
// The wait on handles, on the server and the client.
#define EnshwickLoopWaitSites_HANDLES 1
// The client's wait on handles and window messages.
#define EnshwickLoopWaitSites_MESSAGES 2

// A periodic tick handler a mod subscribed with
// `EnshwickHostApi.subscribe_tick`, for
// `EnshwickHostApi.set_tick_every`.
//
// Valid for the mod that subscribed it, for the process.
typedef uint32_t EnshwickTickHandle;

// A game resource's key: its GUID, its type hash and part.
typedef struct EnshwickResourceKey {
  // The resource's GUID.
  struct EnshwickGuid guid;
  // The reflected type hash of the resource's root type.
  uint32_t type_hash;
  // The part of a resource split in parts; 0 for most.
  uint32_t part;
} EnshwickResourceKey;

// What a recipe rule of `EnshwickHostApi.recipe_rule` names.
typedef uint32_t EnshwickRecipeRuleKind;
// One recipe: its debug name, content id or GUID.
#define EnshwickRecipeRuleKind_RECIPE 0
// Every recipe that makes an item: the item's debug name, content id
// or GUID.
#define EnshwickRecipeRuleKind_OUTPUT 1
// Every recipe made at a workshop: `hand`, the debug name of the item
// the workshop belongs to, the workshop's content id or GUID.
#define EnshwickRecipeRuleKind_WORKSHOP 2

// What the item a recipe makes is, from
// `EnshwickHostApi.recipe_kind`; a later minor may
// add kinds, which a mod reads as `EnshwickRecipeKind_OTHER`.
typedef uint32_t EnshwickRecipeKind;
// None of the kinds below: equipment, functional props, materials and
// the rest.
#define EnshwickRecipeKind_OTHER 0
// A building block.
#define EnshwickRecipeKind_BUILDING_BLOCK 1
// A roof block.
#define EnshwickRecipeKind_ROOF 2
// An overgrowth material.
#define EnshwickRecipeKind_OVERGROWTH 3
// A terrain material.
#define EnshwickRecipeKind_TERRAIN 4
// A placed prop the game files under its Decorative crafting group.
#define EnshwickRecipeKind_DECORATIVE 5
// A placed prop the game files under its Comfort Furniture crafting
// group.
#define EnshwickRecipeKind_COMFORT_FURNITURE 6

// A game rate of `EnshwickHostApi.rate` and
// `EnshwickHostApi.set_rate`, by the loader's stable
// number.
typedef uint32_t EnshwickRateId;
// Experience from combat.
#define EnshwickRateId_EXPERIENCE_COMBAT 1
// Experience from mining.
#define EnshwickRateId_EXPERIENCE_MINING 2
// Experience from exploration and quests.
#define EnshwickRateId_EXPERIENCE_EXPLORATION_QUESTS 3
// How fast workshops produce.
#define EnshwickRateId_FACTORY_PRODUCTION_SPEED 4
// How many resources a drop stacks.
#define EnshwickRateId_RESOURCE_DROP_STACK_AMOUNT 5
// The comfort furniture gives a base.
#define EnshwickRateId_COMFORT 6
// The skill points a character gains per level.
#define EnshwickRateId_SKILL_POINTS 7
// How many items each stack of random loot holds.
#define EnshwickRateId_LOOT 8

// An attribute of a reflected type or field, in a `EnshwickTypeEntry` or
// `EnshwickTypeField`: the engine's name for it, the type of its value and the
// value as the engine's text.
//
// Frozen, read as an array: the reflection's own attribute record. Its
// strings are the loader's, valid and unchanged for the life of the process.
typedef struct EnshwickTypeAttribute {
  // The attribute's name.
  struct EnshwickStr name;
  // The qualified name of its value's type; empty for a flag.
  struct EnshwickStr value_type;
  // The value as the engine's text; empty for a flag.
  struct EnshwickStr value;
} EnshwickTypeAttribute;

// A field of a reflected struct, in a `EnshwickTypeEntry`.
//
// Frozen, read as an array: the reflection's own field record; more about
// the field is read from its type by name. Its strings and attributes are
// the loader's, valid and unchanged for the life of the process.
typedef struct EnshwickTypeField {
  // The field's name.
  struct EnshwickStr name;
  // The qualified name of its type; empty when the engine does not
  // reflect it.
  struct EnshwickStr type_name;
  // Its offset from the start of the struct.
  uint32_t offset;
  // How many attributes `attributes` points at.
  uint32_t attribute_count;
  // Its attributes; null when it has none.
  const struct EnshwickTypeAttribute *attributes;
} EnshwickTypeField;

// A named constant of a reflected enum, in a `EnshwickTypeEntry`.
//
// Frozen, read as an array: the reflection's own constant record. Its name
// is the loader's, valid and unchanged for the life of the process.
typedef struct EnshwickTypeConstant {
  // The constant's name, without its type.
  struct EnshwickStr name;
  // Its value; for the flag enum of a bitmask, the flag's bit index.
  uint64_t value;
} EnshwickTypeConstant;

// A reflected type of the running game, as `EnshwickHostApi.types`
// lists them: its names, layout, kind and the arrays of its fields,
// constants and attributes.
//
// Every string and array an entry points at is the loader's, valid and
// unchanged for the life of the process; a null array has no elements.
typedef struct EnshwickTypeEntry {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The short name, such as `TemplateResource`.
  struct EnshwickStr name;
  // The name the engine's scripts use, such as `ecs.TemplateResource`.
  struct EnshwickStr impact_name;
  // The full name with its namespaces, as the game reflects it.
  struct EnshwickStr qualified_name;
  // The qualified name of a struct's base or of the type an array,
  // optional, typedef or bitmask holds; empty for none.
  struct EnshwickStr inner;
  // The hash the engine keeps beside the qualified name's.
  uint32_t internal_hash;
  // The size in bytes.
  uint32_t size;
  // The alignment in bytes.
  uint16_t align;
  // The alignment as an array element.
  uint16_t element_align;
  // The descriptor's kind byte, the game's numbering of type kinds
  // (struct, array, enum and the others).
  uint8_t kind;
  // The descriptor's flags byte.
  uint8_t flags;
  // Zero.
  uint8_t reserved[2];
  // The engine's count: of fields, of constants, or a static array's
  // length.
  uint32_t count;
  // How many fields `fields` points at.
  uint32_t field_count;
  // How many constants `constants` points at.
  uint32_t constant_count;
  // How many attributes `attributes` points at.
  uint32_t attribute_count;
  // A struct's own fields; null for none.
  const struct EnshwickTypeField *fields;
  // An enum's constants; null for none.
  const struct EnshwickTypeConstant *constants;
  // The type's attributes; null for none.
  const struct EnshwickTypeAttribute *attributes;
} EnshwickTypeEntry;

// An item's static data, from `EnshwickHostApi.item_info`.
typedef struct EnshwickItemTypeInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The item id: FNV-1a of the item's GUID.
  EnshwickItemId id;
  // How many fit one stack.
  uint16_t max_stack;
  // The game's `ItemCategory`: 1 equipment, 2 weapons, 3 tools and so
  // on.
  uint8_t category;
  // The game's `ItemRarity` of the type.
  uint8_t rarity;
  // `ItemInfo` flags: `EnshwickItemTypeInfo_HAS_PIDE` and the others on this type.
  uint32_t flags;
  // The GUID of the template a player places from the item, its
  // `equipment.placedEntity`; all zero for an item that
  // places nothing.
  struct EnshwickGuid placed_entity;
} EnshwickItemTypeInfo;
// Each item has an entity of its own, with durability or a level.
#define EnshwickItemTypeInfo_HAS_PIDE (1 << 1)
// The item is a container.
#define EnshwickItemTypeInfo_IS_CONTAINER (1 << 2)
// The item is a blueprint.
#define EnshwickItemTypeInfo_IS_BLUEPRINT (1 << 3)
// Each item keeps a count the game shows as a bar on its slot, as the
// water pouch keeps its water (the game's fuel).
#define EnshwickItemTypeInfo_USES_FUEL (1 << 13)

// How the weather weighs at one place, as weights that add up to 1, written
// by `EnshwickHostApi.weather_at`.
typedef struct EnshwickWeatherSample {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // How clear it is.
  float clear;
  // How rainy it is.
  float rain;
  // How snowy it is.
  float snow;
  // How stormy it is, in a blizzard.
  float blizzard;
} EnshwickWeatherSample;

// A front of weather in the server's world: the current phenomenon of one
// of its weather layers, written by `EnshwickHostApi.weather_fronts`.
typedef struct EnshwickWeatherFront {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The weather layer it belongs to.
  uint32_t layer;
  // `EnshwickWeatherFront_NONE`, `EnshwickWeatherFront_RAIN` or
  // `EnshwickWeatherFront_BLIZZARD`.
  uint32_t effect;
  // Where it is, in metres.
  EnshwickMetres position;
  // How far it reaches, in metres.
  float radius;
  // Zero.
  uint32_t reserved;
  // How long it has left, in game nanoseconds.
  int64_t remaining_ns;
  // The GUID of its phenomenon in the weather resource, all zero when
  // the loader did not find it.
  struct EnshwickGuid phenomenon;
} EnshwickWeatherFront;
// Clouds without rain, or nothing.
#define EnshwickWeatherFront_NONE 0
// Rain, snow where it is cold.
#define EnshwickWeatherFront_RAIN 1
// A blizzard.
#define EnshwickWeatherFront_BLIZZARD 2

// A terrain material of the running game, written by
// `EnshwickHostApi.terrain_materials`.
typedef struct EnshwickTerrainMaterialInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The material's voxel id, 1 to 127.
  uint8_t id;
  // `EnshwickTerrainMaterialInfo_FLATTENS` or none.
  uint8_t flags;
  // How hard it is to dig, the game's hardness category from 0.
  uint8_t hardness;
  // Zero.
  uint8_t reserved;
  // The item digging it yields.
  EnshwickItemId item;
  // The health a voxel of it has against tools.
  int32_t health;
} EnshwickTerrainMaterialInfo;
// A player's tool can flatten it.
#define EnshwickTerrainMaterialInfo_FLATTENS 1

// A terrain edit, for `EnshwickHostApi.terrain_edit`.
typedef struct EnshwickTerrainEdit {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // `EnshwickTerrainEdit_ADD`, `EnshwickTerrainEdit_DIG` or
  // `EnshwickTerrainEdit_FLATTEN`.
  uint32_t kind;
  // `EnshwickTerrainEdit_ROUND` or `EnshwickTerrainEdit_SQUARE`.
  uint32_t shape;
  // `EnshwickTerrainEdit_DRY_RUN` or none.
  uint32_t flags;
  // The terrain material added or filled with, 1 to 127; unused by a
  // dig.
  uint32_t material;
  // Where, in metres; every axis 0 or more.
  EnshwickMetres position;
  // How far it reaches, in metres, above 0 and up to 16: a mound's or a
  // flattened disc's radius, a dug sphere's.
  float radius;
  // A mound's height at its middle, in metres, above 0 and up to 16;
  // unused otherwise.
  float height;
  // Terrain materials a dig leaves, a bit for each id (bit `id % 8` of
  // byte `id / 8`): the dig stops at the first shell that meets one.
  uint8_t protect[16];
} EnshwickTerrainEdit;
// Adds a mound of a material, as a player's shovel does.
#define EnshwickTerrainEdit_ADD 0
// Digs out terrain, as a pickaxe does, without its cost.
#define EnshwickTerrainEdit_DIG 1
// Flattens the ground to the level of the position, filling with a
// material.
#define EnshwickTerrainEdit_FLATTEN 2
// A disc or a sphere.
#define EnshwickTerrainEdit_ROUND 0
// A square or a cube.
#define EnshwickTerrainEdit_SQUARE 1
// Counts what the edit would change and changes nothing.
#define EnshwickTerrainEdit_DRY_RUN 1

// A voxel of the server's world, written by
// `EnshwickHostApi.voxel_read`. The loader decodes
// the game's voxel into it, so a mod never reads the game's own encoding.
//
// Frozen, written as an array by
// `EnshwickHostApi.voxel_read`: it is the game's own
// packed four-byte voxel.
typedef struct EnshwickVoxel {
  // `EnshwickVoxel_AIR`, `EnshwickVoxel_TERRAIN` or `EnshwickVoxel_BUILDING`; a reader
  // treats a kind it does not know as neither.
  uint8_t kind;
  // The game's material id: 0 for air, a terrain material's id as
  // `EnshwickTerrainMaterialInfo.id` has it, or a building material's id.
  uint8_t material;
  // How full the voxel is, 0 to 255: enshwick's name for the high byte
  // of the game's voxel, which the game does not name.
  uint8_t fill;
  // Zero.
  uint8_t reserved;
} EnshwickVoxel;
// Empty: no ground and no building.
#define EnshwickVoxel_AIR 0
// Ground of a terrain material.
#define EnshwickVoxel_TERRAIN 1
// A building block's material.
#define EnshwickVoxel_BUILDING 2
// The most voxels one `EnshwickHostApi.voxel_read`
// reads: a box of 64 voxels, 32 metres, a side.
#define EnshwickVoxel_READ_MAX ((64 * 64) * 64)
// The end of the coordinates a box covers on any axis, in voxels:
// every voxel of a box lies below it. 2³⁰, far past any world's edge.
#define EnshwickVoxel_END (1 << 30)

// One change of a transaction, handed to
// `EnshwickHostApi.transaction_commit` in an
// array: its kind names the record `data` points at.
//
// Its layout is frozen: the commit reads an array of it with no entry
// size. A new kind of change gets a new kind, never a new field.
typedef struct EnshwickTransactionOp {
  // One of the kinds below; the loader refuses a kind it does not know.
  uint32_t kind;
  // Zero; the loader refuses another value.
  uint32_t reserved;
  // The change, borrowed for the call: the record its kind names.
  const void *data;
} EnshwickTransactionOp;
// A terrain add, dig or flatten: `data` points at a
// `EnshwickTerrainEdit` without
// `EnshwickTerrainEdit_DRY_RUN`.
#define EnshwickTransactionOp_TERRAIN 1
// Voxels written over a box: `data` points at a `EnshwickVoxelWrite`.
#define EnshwickTransactionOp_VOXELS 2
// The building voxels of a box cleared to air: `data` points at a
// `EnshwickVoxelRegion`.
#define EnshwickTransactionOp_ERASE 3
// A building block placed: `data` points at a `EnshwickBlockPlace`.
#define EnshwickTransactionOp_BLOCK 4
// A prop placed: `data` points at a `EnshwickPropPlace`.
#define EnshwickTransactionOp_PLACE_PROP 5
// A prop removed: `data` points at a `EnshwickPropRemove`.
#define EnshwickTransactionOp_REMOVE_PROP 6

// Where a transaction stands, written by
// `EnshwickHostApi.transaction_status`.
typedef struct EnshwickTransactionInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // One of the states below; a reader treats a state it does not know as
  // `EnshwickTransactionInfo_PENDING`.
  uint32_t state;
  // Why it failed, or why its last undo was refused:
  // `EnshwickStatus_NOT_LOADED` for a part of the world not loaded,
  // `EnshwickStatus_OUTSIDE_WORLD` for a change that leaves the world,
  // `EnshwickStatus_REFUSED` for a region changed since,
  // `EnshwickStatus_UNAVAILABLE` when the world ended or the build lacks a
  // piece; `EnshwickStatus_OK`
  // otherwise.
  EnshwickStatus reason;
  // The changes it holds.
  uint32_t ops;
  // The changes written to the world and not undone yet.
  uint32_t applied;
  // The 8³-voxel bricks whose earlier state the loader keeps for its
  // undo, 1 KiB each.
  uint64_t bricks;
  // When the server refused the player the transaction acts for, its
  // reason, one of the `EnshwickBuildRights` refusals;
  // `EnshwickBuildRights_ALLOWED` otherwise.
  uint32_t refusal;
  // Zero.
  uint32_t reserved;
  // When a prop's state stopped it, or stopped its last undo, the kinds
  // of state the prop holds, a set of the `HOLDS_` bits; 0 otherwise.
  uint32_t holds;
  // The props it placed and did not undo, which
  // `EnshwickHostApi.transaction_entities`
  // lists; kept when its undo data expires, dropped when the world ends.
  uint32_t created;
} EnshwickTransactionInfo;
// Queued, or written in part: the loader spreads a large one over the
// server's steps.
#define EnshwickTransactionInfo_PENDING 0
// Written whole, each prop it placed standing in the world, or still
// absent after the loader's wait limit, or with its start state
// unreadable; it can be undone.
#define EnshwickTransactionInfo_APPLIED 1
// An undo is queued or written in part.
#define EnshwickTransactionInfo_UNDOING 2
// Undone, or dropped before it wrote anything.
#define EnshwickTransactionInfo_UNDONE 3
// It stopped before or right after a change it could not write or
// read back, for `reason`; the changes written stay and can be undone
// while `bricks` is above 0. An undo never fails: it is refused.
#define EnshwickTransactionInfo_FAILED 4
// Written whole, or with its undo refused, but the loader dropped what
// its undo needs, to stay within its memory cap or because the world
// ended; it can no longer be undone.
#define EnshwickTransactionInfo_EXPIRED 5
// The prop's inventory holds items, in any of its linked slots.
#define EnshwickTransactionInfo_HOLDS_INVENTORY 1
// It is a plant (`Growth`): any plant for a removal; for the undo of
// a placement, one whose `EnshwickLevel` differs from the level it first
// stood with.
#define EnshwickTransactionInfo_HOLDS_GROWTH (1 << 1)
// The `FactoryStation` has a recipe running or queued.
#define EnshwickTransactionInfo_HOLDS_FACTORY_STATION (1 << 2)
// A survivor is assigned to the `NpcBed`.
#define EnshwickTransactionInfo_HOLDS_NPC_BED (1 << 3)
// Survivors are assigned to it: `StoreAssociatedNpcIds`.
#define EnshwickTransactionInfo_HOLDS_STORE_ASSOCIATED_NPC_IDS (1 << 4)
// A player wrote its text: `CustomPlayerString`.
#define EnshwickTransactionInfo_HOLDS_CUSTOM_PLAYER_STRING (1 << 5)
// It holds more `Fuel` charges than a fresh one starts with, and does
// not refill itself, as a well or a rain barrel does.
#define EnshwickTransactionInfo_HOLDS_FUEL (1 << 6)
// It is wired to other logic: `LogicGraphNodeConnected`.
#define EnshwickTransactionInfo_HOLDS_LOGIC_GRAPH_NODE_CONNECTED (1 << 7)
// It is a logic trigger, whose settings a player may have changed:
// `LogicGraphTrigger` or `LogicTriggerShape`. Any trigger for a
// removal; for the undo of a placement, one whose settings differ
// from those it first stood with.
#define EnshwickTransactionInfo_HOLDS_LOGIC_GRAPH_TRIGGER (1 << 8)
// Its last undo stopped for `reason`: before writing anything, or at a
// brick changed since or not loaded for the loader's wait limit, with
// what it put back kept. The rest stays written and can be undone
// again.
#define EnshwickTransactionInfo_UNDO_REFUSED 6

// A map marker of the scene the server runs, one of the list
// `EnshwickHostApi.map_markers` writes.
typedef struct EnshwickMapMarkerInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // Where it stands, in metres, y up.
  EnshwickMetres position;
  // Its `markerType` reference; zero for none.
  struct EnshwickGuid marker_type;
  // Its label's text tag (`markerId`); zero for
  // none.
  struct EnshwickGuid label;
} EnshwickMapMarkerInfo;

// The frame of the map of the scene the server runs, as
// `EnshwickHostApi.map_frame` writes it: where the
// map starts and how far it reaches, in metres on the world's x and z.
typedef struct EnshwickMapFrameInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The map's corner of lowest x and z, in metres, x then z.
  double origin[2];
  // How far the map reaches from `origin`, in metres, x then z.
  double size[2];
  // The metres one cell of the game's map covers on each axis.
  double metres_per_cell;
} EnshwickMapFrameInfo;

// A world's save on disk, written by
// `EnshwickHostApi.save_info`.
//
// The mod owns it and passes it as `out`, its header set with
// a header of its own size and ABI version; the loader writes it on the calling thread during the
// call, up to the size the header states, and keeps no pointer to it. It
// holds no pointers. Layout grows only by appending; read a later field
// once the header's `size` covers it.
typedef struct EnshwickSaveInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The world's GUID, which stays the same across its saves.
  struct EnshwickGuid world;
  // When the save was written, in seconds since the Unix epoch.
  uint64_t saved_at;
  // The save file's size in bytes.
  uint64_t size;
  // The backups the game keeps beside it, 0 to 10.
  uint32_t backups;
  // Zero.
  uint32_t reserved;
} EnshwickSaveInfo;

// One operation of an inventory transaction, in `EnshwickInventoryRequest.ops`.
//
// Frozen, with no header: `EnshwickInventoryRequest` reads the operations as an
// array; a kind that needs more than these fields is a new entry.
typedef struct EnshwickInventoryOp {
  // What to do: one of the constants on this type.
  uint32_t kind;
  // `EnshwickInventoryOp_INCLUDE_SPECIAL`, `EnshwickInventoryOp_FILLED` or none.
  uint32_t flags;
  // The inventory entity; for slot operations, the sub-inventory that
  // holds the slot.
  EnshwickEntity target;
  // For slot operations, the slot's index in `target`; 0 otherwise.
  uint32_t slot;
  // The destination inventory of a move, or the inventory a renew gives
  // to; entity 0 otherwise.
  EnshwickEntity other;
  // The fill, 0 to 1, each new item's meter starts with, for a give with
  // `EnshwickInventoryOp_FILLED` and for a renew; 0 otherwise.
  float fill;
  // The item id of a give, take or renew.
  EnshwickItemId item;
  // How many to give or take; for a slot move, how many to move, 0 for the
  // whole stack.
  uint32_t count;
} EnshwickInventoryOp;
// Adds `count` new items `item` to the inventory `target`.
#define EnshwickInventoryOp_GIVE 1
// Removes `count` items `item` from the inventory `target`.
#define EnshwickInventoryOp_TAKE 2
// Destroys the stack in slot `slot` of `target`.
#define EnshwickInventoryOp_DESTROY_SLOT 3
// Moves the stack in slot `slot` of `target`, or `count` of it, into the
// inventory `other`.
#define EnshwickInventoryOp_MOVE_SLOT 4
// Moves every stack of the inventory `target` into the inventory `other`.
#define EnshwickInventoryOp_MOVE_ALL 5
// For `EnshwickInventoryOp_MOVE_ALL`: also move the source's action bar,
// equipment and locked slots.
#define EnshwickInventoryOp_INCLUDE_SPECIAL 1
// Replaces the stack in slot `slot` of `target` with `count` new items
// `item`, each with its own entity and its meter at `fill`, given to
// the inventory `other` and asked for the same slot.
#define EnshwickInventoryOp_RENEW 6
// For `EnshwickInventoryOp_GIVE`: each new item's meter starts at `fill`,
// the game's own per-item fuel fill.
#define EnshwickInventoryOp_FILLED 2

// Inventory operations to run in one transaction, for
// `EnshwickHostApi.inventory_submit`.
typedef struct EnshwickInventoryRequest {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // `EnshwickInventoryRequest_ALLOW_PARTIAL`, `EnshwickInventoryRequest_NOTIFY`.
  uint32_t flags;
  // The player the game credits with the change, for pickup and
  // inventory-full messages; 0 for whichever player the game is serving.
  EnshwickEntity actor;
  // The operations, read during the call.
  const struct EnshwickInventoryOp *ops;
  // How many operations `ops` holds.
  uint32_t op_count;
  // Zero.
  uint32_t reserved;
} EnshwickInventoryRequest;
// Commit even when a give or take leaves some of its count undone.
#define EnshwickInventoryRequest_ALLOW_PARTIAL 1
// Show the actor the pickup and inventory-full messages.
#define EnshwickInventoryRequest_NOTIFY 2
// With `NOTIFY`, raise the pickup event marked hidden,
// so no pickup message shows while the event's readers still hear it.
// ABI 12.9.
#define EnshwickInventoryRequest_HIDE_IN_UI 4

// A request to move an entity, for `EnshwickHostApi.teleport`.
typedef struct EnshwickTeleportRequest {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The entity to move.
  EnshwickEntity entity;
  // `EnshwickTeleportRequest_BEST_SPAWN`, `EnshwickTeleportRequest_ORIENTED` or
  // none.
  uint32_t flags;
  // Where to, in metres.
  EnshwickMetres position;
  // The orientation to arrive in, a quaternion `x, y, z, w`, with
  // `EnshwickTeleportRequest_ORIENTED`; otherwise the entity keeps its own.
  float orientation[4];
} EnshwickTeleportRequest;
// The game looks for a free spot near `position` to stand on, as it
// does for respawns.
#define EnshwickTeleportRequest_BEST_SPAWN 1
// The entity arrives facing `orientation`.
#define EnshwickTeleportRequest_ORIENTED 2

// A request to spawn an entity from a template, for `EnshwickHostApi.spawn`.
typedef struct EnshwickSpawnRequest {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The template GUID.
  struct EnshwickGuid template_;
  // Where, in metres.
  EnshwickMetres position;
  // The orientation, a quaternion `x, y, z, w`.
  float orientation[4];
  // The level the entity spawns at, as the game scales enemies; 0 for
  // the template's own.
  uint32_t level;
  // `EnshwickSpawnRequest_DO_NOT_SAVE`, or 0.
  uint32_t flags;
  // Game time in nanoseconds after which the game removes the entity; 0
  // to keep it.
  uint64_t lifetime_ns;
} EnshwickSpawnRequest;
// The entity stays out of the world's save, as the game's own passing
// animals do: it is gone once the world loads again.
#define EnshwickSpawnRequest_DO_NOT_SAVE 1

// A stat kind's definition, for `EnshwickHostApi.define_stat` and
// `EnshwickHostApi.stat_info`.
typedef struct EnshwickStatSpec {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // `EnshwickStatSpec_ADDITIVE`, `EnshwickStatSpec_MULTIPLICATIVE` or
  // `EnshwickStatSpec_FLAG`.
  uint8_t ty;
  // How the value reads: `EnshwickStatSpec_NUMBER`,
  // `EnshwickStatSpec_SIGNED` or `EnshwickStatSpec_PERCENT`.
  uint8_t format;
  // Zero.
  uint16_t reserved;
  // The value without modifiers.
  float default_;
  // The least value.
  float min;
  // The greatest value.
  float max;
  // The group of the Character screen's attributes it shows in as a row,
  // `EnshwickStatSpec_GROUP_NONE` for none: `EnshwickStatSpec_GROUP_MAIN` to
  // `EnshwickStatSpec_GROUP_PROTECTION`.
  uint8_t group;
  // `EnshwickStatSpec_HIDDEN`.
  uint8_t flags;
  // Zero.
  uint16_t reserved_display;
} EnshwickStatSpec;
// In no group of the Character screen.
#define EnshwickStatSpec_GROUP_NONE 0
// The attribute points column beside the tabs, under the game's
// `groupMain`.
#define EnshwickStatSpec_GROUP_MAIN 1
// The same column, under `groupFogSurvival`.
#define EnshwickStatSpec_GROUP_FOG_SURVIVAL 2
// The Basics tab, under `groupPrimary`.
#define EnshwickStatSpec_GROUP_PRIMARY 3
// The Basics tab, under `groupRegeneration`.
#define EnshwickStatSpec_GROUP_REGENERATION 4
// The Basics tab, under `groupMisc`.
#define EnshwickStatSpec_GROUP_MISC 5
// The Damage tab, under `groupDamage`.
#define EnshwickStatSpec_GROUP_DAMAGE 6
// The Damage tab, under `groupWeaponDamage`.
#define EnshwickStatSpec_GROUP_WEAPON_DAMAGE 7
// The Damage tab, under `groupEnemyDamage`.
#define EnshwickStatSpec_GROUP_ENEMY_DAMAGE 8
// The Protection tab, the game's `groupProtection`.
#define EnshwickStatSpec_GROUP_PROTECTION 9
// Shown nowhere: no row on the Character screen and no line in item
// tooltips.
#define EnshwickStatSpec_HIDDEN 1
// A sum of points, such as extra health.
#define EnshwickStatSpec_ADDITIVE 0
// A factor, 1 without modifiers, such as a damage multiplier.
#define EnshwickStatSpec_MULTIPLICATIVE 1
// On or off: any value but 0 is on.
#define EnshwickStatSpec_FLAG 2
// A plain number.
#define EnshwickStatSpec_NUMBER 0
// A number with its sign.
#define EnshwickStatSpec_SIGNED 1
// A fraction shown as a percentage.
#define EnshwickStatSpec_PERCENT 2

// A stat kind as the loader numbers it: its index for calls and its key,
// the FNV-1a 32 hash of its full id, for what is stored.
typedef struct EnshwickStatRef {
  // The index in this process.
  uint32_t index;
  // The FNV-1a 32 hash of the full id, the same in every process.
  uint32_t key;
} EnshwickStatRef;

// One modifier of a stat, by index; written as arrays.
typedef struct EnshwickStatModifier {
  // The stat's index.
  uint32_t stat;
  // `EnshwickStatModifier_ADD`, `EnshwickStatModifier_MULTIPLY`,
  // `EnshwickStatModifier_MAX` or `EnshwickStatModifier_OVERRIDE`.
  uint8_t op;
  // Zero.
  uint8_t reserved[3];
  // The modifier's value.
  float value;
} EnshwickStatModifier;
// Adds the value.
#define EnshwickStatModifier_ADD 0
// Multiplies by the value.
#define EnshwickStatModifier_MULTIPLY 1
// Raises the result to at least the value.
#define EnshwickStatModifier_MAX 2
// Sets the result.
#define EnshwickStatModifier_OVERRIDE 3

// Sets or removes a mod's modifier on an entity, for
// `EnshwickHostApi.stat_modify`.
typedef struct EnshwickStatModify {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The entity.
  EnshwickEntity entity;
  // The mod's own key; a modifier with the same mod, key and stat
  // replaces the one before.
  uint32_t key;
  // The modifier.
  struct EnshwickStatModifier modifier;
  // `EnshwickStatModify_REMOVE`, `EnshwickStatModify_SAVED`.
  uint32_t flags;
  // How long it lasts in milliseconds; 0 until removed.
  uint64_t duration_ms;
} EnshwickStatModify;
// Removes the modifier instead.
#define EnshwickStatModify_REMOVE 1
// Kept with the world for the player's account, and given back when
// the player joins again; players only.
#define EnshwickStatModify_SAVED (1 << 1)

// One modifier that counts toward an entity's stat, and where it comes
// from, written by `EnshwickHostApi.stat_sources` as an array.
typedef struct EnshwickStatSource {
  // `EnshwickStatSource_ITEM`, `EnshwickStatSource_BUFF` or
  // `EnshwickStatSource_ENTITY`.
  uint8_t kind;
  // The operation, as `EnshwickStatModifier.op`.
  uint8_t op;
  // Zero.
  uint16_t reserved;
  // The stat's index.
  uint32_t stat;
  // The modifier's value.
  float value;
  // The item's entity, the buff id or the modifier's key.
  uint32_t id;
  // The item id, 0, or the index of the mod that set the modifier.
  uint32_t owner;
} EnshwickStatSource;
// An item the entity wears.
#define EnshwickStatSource_ITEM 0
// A buff the entity has.
#define EnshwickStatSource_BUFF 1
// A modifier a mod set on the entity.
#define EnshwickStatSource_ENTITY 2

// One line of what an item gives, written by `EnshwickHostApi.item_lines` as an
// array.
typedef struct EnshwickItemLine {
  // The modifier.
  struct EnshwickStatModifier modifier;
  // The index of the affix it comes from, plus one; 0 for the item's own.
  uint32_t affix;
} EnshwickItemLine;

// A node of the game's skill tree by its id, as
// the knowledge reads name the nodes a character
// unlocked and a skill check names the node it asks about.
typedef uint32_t EnshwickSkillNode;

// A knowledge value of the game by its id: a recipe's, a quest's, or any
// other bool, counter or flags the world or a player keeps.
//
// A recipe's knowledge has the recipe's id, so a `EnshwickRecipeId` converts.
typedef uint32_t EnshwickKnowledgeId;

// A knowledge value, written by `EnshwickHostApi.knowledge`.
typedef struct EnshwickKnowledgeInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The knowledge id.
  EnshwickKnowledgeId id;
  // `EnshwickKnowledgeInfo_BOOL`, `EnshwickKnowledgeInfo_COUNTER` or
  // `EnshwickKnowledgeInfo_FLAGS`.
  uint8_t kind;
  // `EnshwickKnowledgeInfo_WORLD` or `EnshwickKnowledgeInfo_PLAYER`.
  uint8_t scope;
  // Zero.
  uint16_t reserved;
  // The value: 0 or 1, a count, or flag bits.
  uint32_t value;
  // The flag bits the game expects of a flags value; 0 for any.
  uint32_t flags;
} EnshwickKnowledgeInfo;
// Set or not.
#define EnshwickKnowledgeInfo_BOOL 0
// A count.
#define EnshwickKnowledgeInfo_COUNTER 1
// Bits.
#define EnshwickKnowledgeInfo_FLAGS 2
// The world's, shared by every player.
#define EnshwickKnowledgeInfo_WORLD 0
// Each player's own.
#define EnshwickKnowledgeInfo_PLAYER 1

// One grant or revoke of a knowledge value, for
// `EnshwickHostApi.change_knowledge`: of the
// character `entity` in the game, of the account `account` whether it plays
// now or later, or of the world when both are 0.
typedef struct EnshwickKnowledgeChange {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The account whose knowledge changes, by its
  // `EnshwickPlayerInfo.key`: now while its character
  // is in the game, else once its next character loads in this world; 0
  // for none.
  EnshwickPlayerKey account;
  // The character of a player in the game whose knowledge changes now; 0
  // for none.
  EnshwickEntity entity;
  // The knowledge id.
  EnshwickKnowledgeId id;
  // Ignored for `EnshwickKnowledgeInfo_BOOL`, the count to add or take for
  // `EnshwickKnowledgeInfo_COUNTER`, the bit to set or clear for
  // `EnshwickKnowledgeInfo_FLAGS`.
  uint32_t value;
  // The id's kind: `EnshwickKnowledgeInfo_BOOL`, `EnshwickKnowledgeInfo_COUNTER`
  // or `EnshwickKnowledgeInfo_FLAGS`.
  uint8_t kind;
  // `EnshwickKnowledgeChange_GRANT` or `EnshwickKnowledgeChange_REVOKE`.
  uint8_t op;
  // Zero.
  uint16_t reserved;
} EnshwickKnowledgeChange;
// Grants the value, as the game grants it.
#define EnshwickKnowledgeChange_GRANT 0
// Takes the value back.
#define EnshwickKnowledgeChange_REVOKE 1
// A notice of `EnshwickHostApi.knowledge_set`:
// the value written with every bit marked seen, as a save loads it.
#define EnshwickKnowledgeChange_QUIET 0
// A notice of `EnshwickHostApi.knowledge_set`:
// what the value gains granted as the game grants it, what it loses
// written with no bit marked seen.
#define EnshwickKnowledgeChange_SHOWN 1

// One widget, drawn with `EnshwickHostApi.ui_widget`: `kind` says which, and
// which fields it reads; the loader writes `index`, `value` and `result`
// back.
typedef struct EnshwickUiWidget {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // What to draw: one of the kind constants, `EnshwickUiWidget_WINDOW` to
  // `EnshwickUiWidget_INPUT_HINT`.
  uint32_t kind;
  // `EnshwickUiWidget_READ_ONLY`, `EnshwickUiWidget_WRAP`, `EnshwickUiWidget_CENTER`.
  uint32_t flags;
  // A label's style (`EnshwickUiWidget_STYLE_BODY` and on) or a sound
  // (`EnshwickUiWidget_SOUND_CLICK` and on).
  uint32_t style;
  // A slider's stops between `min` and `max`, both included; 0 for a
  // continuous slider.
  uint32_t steps;
  // The text: a label's, a button's, a row's label, a window's id, an
  // input hint's action.
  struct EnshwickStr text;
  // A window's title, an input hint's label.
  struct EnshwickStr detail;
  // A dropdown's options, `option_count` of them, valid for the call.
  const struct EnshwickStr *options;
  // How many options `options` holds.
  uint32_t option_count;
  // In and out: a toggle's state (0 or 1), a dropdown's option.
  uint32_t index;
  // In and out: a slider's value; a progress bar's fill.
  float value;
  // A slider's least value, a progress bar's empty value.
  float min;
  // A slider's greatest value, a progress bar's full value.
  float max;
  // Out: 1 when a button was clicked or a value changed this frame, or
  // when the player pressed Back over a window.
  uint32_t result;
  // Where a HUD widget shows: `EnshwickUiWidget_TOP_LEFT` to
  // `EnshwickUiWidget_BOTTOM_RIGHT`; read in a HUD callback only.
  uint32_t anchor;
  // Zero.
  uint32_t reserved;
  // The sound container a `EnshwickUiWidget_SOUND` widget plays; nil for the
  // game's own sound `style`.
  struct EnshwickGuid sound;
  // The item an `EnshwickUiWidget_ITEM_SLOT` shows.
  EnshwickItemId item;
  // How many of `item` an `EnshwickUiWidget_ITEM_SLOT` shows, at least 1.
  uint32_t count;
} EnshwickUiWidget;
// A window in the game's style, a panel over the screen that takes the
// input while it shows: `text` is its id, unique in the mod, `detail`
// its title. Widgets that follow go into it until `EnshwickUiWidget_END`.
// `result` 1 when the player pressed Back (Escape, B) over it this
// frame, which then does not open the game's pause menu.
#define EnshwickUiWidget_WINDOW 1
// Closes the innermost open window.
#define EnshwickUiWidget_END 2
// A line of text in `style`; `EnshwickUiWidget_WRAP` wraps it at the width
// of the window, `EnshwickUiWidget_CENTER` centres it.
#define EnshwickUiWidget_LABEL 3
// A button labelled `text`; `result` 1 when clicked.
#define EnshwickUiWidget_BUTTON 4
// A settings row labelled `text` with an on and off switch, `index`.
#define EnshwickUiWidget_TOGGLE 5
// A settings row labelled `text` with a slider over `min..=max` in
// `steps` stops, `value`.
#define EnshwickUiWidget_SLIDER 6
// A settings row labelled `text` with a choice among `options`, `index`.
#define EnshwickUiWidget_DROPDOWN 7
// A bar filled from `min` to `max` at `value`.
#define EnshwickUiWidget_PROGRESS 8
// Plays an interface sound, heard only on this client and from no place
// in the world: the sound container `sound`, or the game's own sound
// `style` when `sound` is nil; `EnshwickStatus_MISSING`
// for a container neither the client's archive nor a mod holds. Needs
// no window.
#define EnshwickUiWidget_SOUND 9
// The key or button the player bound to a game action, as the game
// draws it beside its own prompts, so it follows a rebind: `text` is
// the action's reflected `GameplayAction` name, such as
// `ContextualAction`, and `detail` a label beside the key, empty for
// none. `result` 1 when the game drew a key, 0 when the action has no
// binding; `EnshwickStatus_MISSING` for an action
// this build does not name,
// `EnshwickStatus_UNAVAILABLE` on a build where
// the loader does not find the game's hint. Client only.
#define EnshwickUiWidget_INPUT_HINT 10
// An item slot in the game's style, with the icon, count and frame of
// `count` of `item`, as the game's inventory screens draw it. `result`
// holds `EnshwickUiWidget_SLOT_CLICKED` and `EnshwickUiWidget_SLOT_HOVERED`;
// `EnshwickStatus_MISSING` for an item the client
// does not know, `EnshwickStatus_UNAVAILABLE` on
// a build where the loader does not find the game's slot. Client only.
#define EnshwickUiWidget_ITEM_SLOT 11
// An `EnshwickUiWidget_ITEM_SLOT`'s `result` bit: the player clicked the
// slot this frame.
#define EnshwickUiWidget_SLOT_CLICKED 1
// An `EnshwickUiWidget_ITEM_SLOT`'s `result` bit: the cursor is on the slot,
// or, while a gamepad drives the interface, the slot holds the focus.
#define EnshwickUiWidget_SLOT_HOVERED 2
// The row shows its value and does not take changes; a button does not
// take clicks.
#define EnshwickUiWidget_READ_ONLY 1
// A label wraps at the width of its window.
#define EnshwickUiWidget_WRAP 2
// A label is centred.
#define EnshwickUiWidget_CENTER 4
// The game's body text.
#define EnshwickUiWidget_STYLE_BODY 0
// The game's dimmer, secondary text.
#define EnshwickUiWidget_STYLE_SECONDARY 1
// The game's largest heading.
#define EnshwickUiWidget_STYLE_H1 2
// The game's second heading.
#define EnshwickUiWidget_STYLE_H2 3
// The game's third heading.
#define EnshwickUiWidget_STYLE_H3 4
// The game's warning text.
#define EnshwickUiWidget_STYLE_WARNING 5
// The game's text for good news.
#define EnshwickUiWidget_STYLE_POSITIVE 6
// The game's text for bad news.
#define EnshwickUiWidget_STYLE_NEGATIVE 7
// The game's click.
#define EnshwickUiWidget_SOUND_CLICK 0
// The game's confirmation.
#define EnshwickUiWidget_SOUND_CONFIRM 1
// The game's step back.
#define EnshwickUiWidget_SOUND_BACK 2
// The game's refusal.
#define EnshwickUiWidget_SOUND_ERROR 3
// The HUD's top left corner.
#define EnshwickUiWidget_TOP_LEFT 0
// The HUD's top edge, centred.
#define EnshwickUiWidget_TOP_CENTER 1
// The HUD's top right corner.
#define EnshwickUiWidget_TOP_RIGHT 2
// The HUD's bottom left corner.
#define EnshwickUiWidget_BOTTOM_LEFT 3
// The HUD's bottom edge, centred.
#define EnshwickUiWidget_BOTTOM_CENTER 4
// The HUD's bottom right corner.
#define EnshwickUiWidget_BOTTOM_RIGHT 5

// A dialog in the game's own style, queued with `EnshwickHostApi.ui_dialog`.
typedef struct EnshwickUiDialog {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The title.
  struct EnshwickStr title;
  // The message under the title.
  struct EnshwickStr message;
  // Smaller text under the message; empty for none.
  struct EnshwickStr detail;
  // Bit `i` adds the button with the game's label for answer `i`, such
  // as `EnshwickUiDialog_OK`; the buttons show in the order of their bits. At
  // least one.
  uint32_t buttons;
  // `EnshwickUiDialog_WAITING` when the caller blocks until the answer.
  uint32_t flags;
} EnshwickUiDialog;
// "Retry".
#define EnshwickUiDialog_RETRY 0
// "Disable".
#define EnshwickUiDialog_DISABLE 1
// "Erase".
#define EnshwickUiDialog_ERASE 2
// "Reset".
#define EnshwickUiDialog_RESET 3
// "Overwrite".
#define EnshwickUiDialog_OVERWRITE 5
// "Cancel".
#define EnshwickUiDialog_CANCEL 6
// "OK".
#define EnshwickUiDialog_OK 7
// "Ignore".
#define EnshwickUiDialog_IGNORE 8
// The buttons a dialog may carry.
#define EnshwickUiDialog_BUTTONS 495
// The answer when the dialog closed without one, as when the player
// pressed Back over it in game or the game quits.
#define EnshwickUiDialog_CLOSED UINT32_MAX
// The caller blocks until the answer: refused on the thread that
// draws the interface, which would never draw it.
#define EnshwickUiDialog_WAITING 1

// A marker on the client's map, one of the set `EnshwickHostApi.ui_markers`
// hands over.
typedef struct EnshwickUiMarker {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // Where, in metres.
  EnshwickMetres position;
  // The label the map shows beside the icon.
  struct EnshwickStr label;
  // The colour, one of the game's custom marker colours,
  // `EnshwickUiMarker_WHITE` to `EnshwickUiMarker_BLACK`.
  uint32_t color;
  // The registry key of its marker type, one
  // `EnshwickHostApi.map_marker_types`
  // lists; 0 for the loader's own choice, a custom marker type with a
  // label.
  uint32_t marker_type;
} EnshwickUiMarker;
// White.
#define EnshwickUiMarker_WHITE 0
// Red.
#define EnshwickUiMarker_RED 1
// Green.
#define EnshwickUiMarker_GREEN 2
// Blue.
#define EnshwickUiMarker_BLUE 3
// Yellow.
#define EnshwickUiMarker_YELLOW 4
// Purple.
#define EnshwickUiMarker_PURPLE 5
// Turquoise.
#define EnshwickUiMarker_TURQUOISE 6
// Black.
#define EnshwickUiMarker_BLACK 7

// The group the client's map sorts a marker type into, the game's
// `EnshwickMarkerTypeSorting`.
typedef uint8_t EnshwickMarkerTypeSorting;
// In no group.
#define EnshwickMarkerTypeSorting_NONE 0
// The Flame's: altars and shrines.
#define EnshwickMarkerTypeSorting_FLAME_RELATED 1
// Dungeons.
#define EnshwickMarkerTypeSorting_DUNGEONS 2
// Places in the world.
#define EnshwickMarkerTypeSorting_LOCATIONS 3
// Markers players place themselves.
#define EnshwickMarkerTypeSorting_CUSTOM_MARKER 4
// A group of the game's that this loader does not know.
#define EnshwickMarkerTypeSorting_UNKNOWN UINT8_MAX

// A marker type of the client's marker type registry, one of the list
// `EnshwickHostApi.map_marker_types` writes.
typedef struct EnshwickMapMarkerTypeInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // Its name's text tag (`typeName`); zero for
  // none.
  struct EnshwickGuid type_name;
  // Its registry key (`markerId`), as
  // `EnshwickUiMarker.marker_type` takes it.
  uint32_t marker_id;
  // The group the map sorts it into.
  EnshwickMarkerTypeSorting sorting_category;
  // 1 when players can travel to markers of this type, else 0.
  uint8_t is_fast_travel_destination;
  // 1 for a type of the markers players place themselves, else 0.
  uint8_t is_custom_marker_type;
  // 1 when its markers show over the fog of war, else 0.
  uint8_t show_above_fog_of_war;
  // 1 when its markers show their label, else 0.
  uint8_t show_label;
  // Zero.
  uint8_t reserved[3];
} EnshwickMapMarkerTypeInfo;

// An element a mod adds to one of the game's screens, registered with
// `EnshwickHostApi.ui_extension`: a panel at an edge of
// the screen that shows and hides with it.
typedef struct EnshwickUiExtension {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The game's screen, by the name of its member of the game's screen
  // kinds, such as `StorageChest`. Valid for the call.
  struct EnshwickStr screen;
  // The screen's edge the panel sits at: `EnshwickUiExtension_LEFT` to
  // `EnshwickUiExtension_BOTTOM`.
  uint32_t edge;
  // Zero.
  uint32_t reserved;
  // `host_count` template GUIDs: the panel shows only while the open
  // screen's host was made from one of them; null with a count of 0 for
  // a panel of every host. Borrowed for the call: the loader copies
  // them.
  const struct EnshwickGuid *hosts;
  // How many GUIDs `hosts` holds, at most `EnshwickUiExtension_MAX_HOSTS`.
  uint32_t host_count;
  // Zero.
  uint32_t reserved_hosts;
  // Where along its edge the panel sits: `EnshwickUiExtension_CENTER`,
  // `EnshwickUiExtension_START` or `EnshwickUiExtension_END`. A spec of an older
  // minor reads zero, the centre.
  uint32_t align;
  // Layout units the panel moves along its edge from where `align` puts
  // it, toward the edge's end (right or down) when positive; at most
  // `EnshwickUiExtension_MAX_OFFSET` either way. The layout is 1920 units
  // wide and 1080 high, scaled to the window.
  float shift;
  // Layout units the panel moves away from its edge into the screen,
  // from 0 to `EnshwickUiExtension_MAX_OFFSET`.
  float inset;
  // Zero.
  uint32_t reserved_place;
} EnshwickUiExtension;
// The left edge.
#define EnshwickUiExtension_LEFT 0
// The right edge.
#define EnshwickUiExtension_RIGHT 1
// The top edge.
#define EnshwickUiExtension_TOP 2
// The bottom edge.
#define EnshwickUiExtension_BOTTOM 3
// The most host templates one panel names.
#define EnshwickUiExtension_MAX_HOSTS 64
// The panel centred along its edge.
#define EnshwickUiExtension_CENTER 0
// The panel at the start of its edge: the top of a side edge, the left
// of the top or bottom edge.
#define EnshwickUiExtension_START 1
// The panel at the end of its edge: the bottom of a side edge, the
// right of the top or bottom edge.
#define EnshwickUiExtension_END 2
// The largest `shift` either way and the largest `inset`, in layout
// units: the layout's width, past which a panel leaves every screen.
#define EnshwickUiExtension_MAX_OFFSET 1920.0

// Cached diagnostics of the client's local gameplay-input sampler.
// Flags and age are weakly consistent observations, not world or session identity.
// Reading this does not request sampling. ABI 12.1.
typedef struct EnshwickLocalInputInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // A combination of this type's flag constants; unknown flags are ignored.
  uint32_t flags;
  // Reserved; 0.
  uint32_t reserved;
  // Age of the last cached sample in milliseconds; `UINT64_MAX` means never sampled.
  uint64_t age_ms;
} EnshwickLocalInputInfo;
// A mod requested sampling through `local_held`.
#define EnshwickLocalInputInfo_REQUESTED 1
// The sampler's existing layout resolved.
#define EnshwickLocalInputInfo_LAYOUT_AVAILABLE 2
// The existing sampler link succeeded and its fuse remains on.
#define EnshwickLocalInputInfo_LINK_AVAILABLE 4
// The sampler's read fuse remains on; this alone does not mean reads can run.
#define EnshwickLocalInputInfo_READ_ENABLED 8
// At least one sample was cached, including a zero mask.
#define EnshwickLocalInputInfo_SAMPLE_PRESENT 16
// Requested, available, enabled and sampled within the existing 1000 ms window.
#define EnshwickLocalInputInfo_FRESH 32

// A part or feature of the loader that is off, which
// `EnshwickHostApi.loader_off` lists.
//
// The loader turns off only what a build lacks or what faulted, and keeps
// running the rest; every part or feature it does not list runs. The
// names are the ones the log and the Mods screen show, and a new part of
// a later loader appears with no new field.
typedef struct EnshwickLoaderOff {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The part or feature, as its log line names it.
  struct EnshwickStr name;
  // Why it is off, as the log says: the build entry it lacks and why,
  // or the fault that turned it off.
  struct EnshwickStr reason;
  // When it turned off, in seconds since the Unix epoch.
  uint64_t at;
} EnshwickLoaderOff;

// A terrain material of a mod's own, for
// `EnshwickHostApi.terrain_material_add`:
// a copy of a vanilla material, its look included, with the rules
// `changes` names set from this struct.
typedef struct EnshwickTerrainMaterialAdd {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The GUID saves name its voxels by: its content id's, which the
  // calling mod declared with `declare_content`.
  struct EnshwickGuid guid;
  // The item digging it yields, by its GUID, with
  // `EnshwickTerrainMaterialAdd_ITEM`.
  struct EnshwickGuid item;
  // The health a voxel of it has against tools, with
  // `EnshwickTerrainMaterialAdd_HEALTH`.
  int32_t health;
  // The game's terrain material it copies, its look and every rule it
  // does not change, by voxel id.
  uint8_t like;
  // The game's terrain material its voxels load as while no mod adds it,
  // by voxel id.
  uint8_t fallback;
  // The rules it sets: `EnshwickTerrainMaterialAdd_ITEM`,
  // `EnshwickTerrainMaterialAdd_HARDNESS`, `EnshwickTerrainMaterialAdd_HEALTH` and
  // `EnshwickTerrainMaterialAdd_FLATTENS`, or none.
  uint8_t changes;
  // How hard it is to dig, the game's hardness category from 0 (`Soft`)
  // to 5 (`Unbreakable`), with `EnshwickTerrainMaterialAdd_HARDNESS`.
  uint8_t hardness;
  // `EnshwickTerrainMaterialInfo_FLATTENS` or none, with
  // `EnshwickTerrainMaterialAdd_FLATTENS`.
  uint8_t flags;
  // Zero.
  uint8_t reserved[3];
} EnshwickTerrainMaterialAdd;
// Sets the item digging it yields.
#define EnshwickTerrainMaterialAdd_ITEM 1
// Sets its hardness.
#define EnshwickTerrainMaterialAdd_HARDNESS (1 << 1)
// Sets its health.
#define EnshwickTerrainMaterialAdd_HEALTH (1 << 2)
// Sets whether a player's tool can flatten it.
#define EnshwickTerrainMaterialAdd_FLATTENS (1 << 3)
// Every change it may set.
#define EnshwickTerrainMaterialAdd_ALL_CHANGES (((EnshwickTerrainMaterialAdd_ITEM | EnshwickTerrainMaterialAdd_HARDNESS) | EnshwickTerrainMaterialAdd_HEALTH) | EnshwickTerrainMaterialAdd_FLATTENS)

// How the prompt of every object placed from one item reads and what its
// press does, as `EnshwickHostApi.interaction_prompt`
// takes it. ABI 12.3.
//
// The server writes `verb` and the replaced action, so every
// client sees them; a client with Enshwick also shows `text`
// in the verb's place.
typedef struct EnshwickInteractionPrompt {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The item the objects were placed from (`UsedItem`); never 0.
  EnshwickItemId item;
  // The game's `LocaTagId` of the verb every client shows; 0 keeps the
  // object's own verb.
  uint32_t verb;
  // The `LocaTagId` of one of the mod's own texts, shown in the verb's
  // place on clients with Enshwick; 0 for none.
  uint32_t text;
  // `REPLACE_ACTION` or 0.
  uint32_t flags;
} EnshwickInteractionPrompt;
// The press runs the game's empty interaction instead of the object's
// own action, such as opening a chest, so only the mod's action runs.
#define EnshwickInteractionPrompt_REPLACE_ACTION 1

// An item's prop template and optional local placement box, copied by
// `EnshwickHostApi.prop_placement_info`.
// No entity is placed and no permission, collision or persistence is checked.
typedef struct EnshwickPropPlacementInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The template the item places, never `NIL` on success.
  struct EnshwickGuid template_;
  // The declared local minimum corner in metres; read only with `HAS_LOCAL_BOX`.
  float min[3];
  // The declared local maximum corner in metres; read only with `HAS_LOCAL_BOX`.
  float max[3];
  // `HAS_LOCAL_BOX` when the existing resolver supplied finite local corners.
  uint32_t flags;
} EnshwickPropPlacementInfo;
// The resolver supplied local corners, without normalization or a complete
// geometry check. Without it, bounds are unknown,
// not an empty box or the loader's fallback permission box.
#define EnshwickPropPlacementInfo_HAS_LOCAL_BOX 1

// A player's respawn point, which
// `EnshwickHostApi.respawn_point` fills: the spawn
// point entity the character respawns at and where it stands.
typedef struct EnshwickRespawnPointInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The spawn point entity.
  EnshwickEntity entity;
  // The spawn point's kind: `EnshwickRespawnPointInfo_START_LOCATION`,
  // `EnshwickRespawnPointInfo_SAVE_POINT`, `EnshwickRespawnPointInfo_BASE`,
  // `EnshwickRespawnPointInfo_NO_RESPAWN` or `EnshwickRespawnPointInfo_OTHER`.
  uint32_t kind;
  // Where the character respawns, in metres.
  EnshwickMetres position;
  // The orientation it respawns in, a quaternion `x, y, z, w`.
  float orientation[4];
} EnshwickRespawnPointInfo;
// The world's start location.
#define EnshwickRespawnPointInfo_START_LOCATION 0
// A save point out in the world.
#define EnshwickRespawnPointInfo_SAVE_POINT 1
// A Flame Altar's base.
#define EnshwickRespawnPointInfo_BASE 2
// A spawn point nobody respawns at.
#define EnshwickRespawnPointInfo_NO_RESPAWN 3
// A kind the loader does not know on this build.
#define EnshwickRespawnPointInfo_OTHER UINT32_MAX

// The data of `EnshwickEvent_WAYPOINT_CHANGED`, and what
// `EnshwickHostApi.waypoint` reads now.
typedef struct EnshwickWaypointData {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The player's account; 0 when the loader answers for an entity no
  // player online holds.
  EnshwickPlayerKey key;
  // Their character.
  EnshwickEntity entity;
  // Where the waypoint is, in metres, while `set` is 1; zero otherwise.
  EnshwickMetres position;
  // 1 while the waypoint is set, 0 once it is cleared or for an entity
  // without one.
  uint32_t set;
  // Zero.
  uint32_t reserved_tail;
} EnshwickWaypointData;

// A sound a client mod starts, which `EnshwickHostApi.sound_play` queues for the
// client's game thread. ABI 12.17.
typedef struct EnshwickSoundPlay {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The sound container, the game's or a mod's.
  struct EnshwickGuid container;
  // Where it plays: `EnshwickSoundPlay_SCREEN` and the other place constants.
  uint32_t place;
  // The entity of the client's world it follows, for `EnshwickSoundPlay_ON`.
  EnshwickEntity entity;
  // Where it plays in the world, for `EnshwickSoundPlay_AT`.
  EnshwickMetres at;
  // Its volume, 0 to 1, which the game multiplies by the container's.
  float volume;
  // How long it fades in, in milliseconds, at most
  // `EnshwickSoundPlay_MAX_FADE_MS`.
  uint32_t fade_in_ms;
} EnshwickSoundPlay;
// From no place, as an interface sound: the game's `Ui` scene.
#define EnshwickSoundPlay_SCREEN 0
// At `at` in the world: the game's `Game` scene.
#define EnshwickSoundPlay_AT 1
// On `entity`, following it: the game's `Game` scene.
#define EnshwickSoundPlay_ON 2
// As music: the game's `Music` scene.
#define EnshwickSoundPlay_MUSIC 3
// The longest fade a play or a steer takes, in milliseconds.
#define EnshwickSoundPlay_MAX_FADE_MS 60000
// The most sounds one mod starts in one frame of the client.
#define EnshwickSoundPlay_MAX_STARTS 64

// A change to a sound a client mod started, which `EnshwickHostApi.sound_steer`
// queues. ABI 12.17.
typedef struct EnshwickSoundSteer {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // What changes: `EnshwickSoundSteer_STOP` and the other kind constants.
  uint32_t kind;
  // The new volume, 0 to 1, for `EnshwickSoundSteer_VOLUME`.
  float volume;
  // How long the stop or the volume change fades, in milliseconds, at
  // most `EnshwickSoundPlay_MAX_FADE_MS`.
  uint32_t fade_ms;
  // Zero.
  uint32_t reserved;
  // The new place in the world, for `EnshwickSoundSteer_POSITION`.
  EnshwickMetres at;
} EnshwickSoundSteer;
// Stops the sound, fading it out over `fade_ms`.
#define EnshwickSoundSteer_STOP 0
// Changes its volume over `fade_ms`.
#define EnshwickSoundSteer_VOLUME 1
// Moves it to `at`; a sound that followed an entity follows it no more,
// and one on the screen or of music has no place and stays.
#define EnshwickSoundSteer_POSITION 2
// Stops every sound of the mod, fading each out over `fade_ms`; the id
// is not read.
#define EnshwickSoundSteer_STOP_ALL 3

// Where a sound a client mod started stands, which
// `EnshwickHostApi.sound_state` writes. ABI 12.17.
typedef struct EnshwickSoundInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // `EnshwickSoundInfo_PENDING` and the other state constants.
  uint32_t state;
  // Why it failed, for `EnshwickSoundInfo_FAILED`: `EnshwickSoundInfo_GAME_REFUSED`
  // and the other reason constants; 0 otherwise.
  uint32_t reason;
} EnshwickSoundInfo;
// Queued: the client's game thread has not started it yet.
#define EnshwickSoundInfo_PENDING 0
// Playing, or fading in or out.
#define EnshwickSoundInfo_PLAYING 1
// Ended: played out, stopped, its entity gone, its world ended or its
// mod turned off. An id the loader no longer keeps reads as ended.
#define EnshwickSoundInfo_ENDED 2
// Never started: `reason` says why.
#define EnshwickSoundInfo_FAILED 3
// The game did not start it: its sound pool is full, or it found no
// recording of the container.
#define EnshwickSoundInfo_GAME_REFUSED 1
// The place's game scene is not reached on this build.
#define EnshwickSoundInfo_NO_SCENE 2
// The loader's sound playback is off on this build or after a fault.
#define EnshwickSoundInfo_OFF 3
// The entity it was to follow is not in the client's world.
#define EnshwickSoundInfo_NO_ENTITY 4
// The client's sounds already follow the most entities they follow at
// once, 64 over every mod, and this one is not among them.
#define EnshwickSoundInfo_FOLLOW_FULL 5

// The clients a server mod's sound plays on, which
// `EnshwickHostApi.sound_play_for` reads. ABI 12.18.
typedef struct EnshwickSoundAudience {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // Who hears it: `EnshwickSoundAudience_PLAYER` and the other kind constants.
  uint32_t kind;
  // For `EnshwickSoundAudience_NEAR`, how far from `at` a player's character
  // stands at most, in metres, up to `EnshwickSoundAudience_MAX_RADIUS`.
  float radius;
  // The player, for `EnshwickSoundAudience_PLAYER`.
  EnshwickPlayerKey player;
  // The centre, for `EnshwickSoundAudience_NEAR`.
  EnshwickMetres at;
} EnshwickSoundAudience;
// The client of `player`.
#define EnshwickSoundAudience_PLAYER 0
// Every client.
#define EnshwickSoundAudience_EVERYONE 1
// The clients whose player's character stands within
// `radius` of `at` when the sound starts.
#define EnshwickSoundAudience_NEAR 2
// The widest radius, in metres: no sound container carries farther.
#define EnshwickSoundAudience_MAX_RADIUS 500.0
// The most sounds one mod sends clients in one server step.
#define EnshwickSoundAudience_MAX_PLAYS 32


// A player about to interact with an entity, as
// `EnshwickHostApi.interaction_filter` hands it
// on.
//
// Open it, move items in or out of it, sort it, craft or build from it,
// dismantle it, or flip its Quick Stash or upgrade switch.
typedef struct EnshwickInteraction {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // What the player does; one of the constants on this type.
  uint32_t kind;
  // The player's character entity.
  EnshwickEntity character;
  // The entity they act on: a chest, a placed piece, the inventory a slot
  // belongs to.
  EnshwickEntity target;
  // The item the target was placed from (`UsedItem`), 0 when unknown.
  EnshwickItemId item;
  // 1 when `item` and `position` were read from the target.
  uint8_t known;
  // Zero.
  uint8_t reserved[3];
  // Where the target stands, in metres, when `known`.
  EnshwickMetres position;
  // The item of the stack in `from_slot`, when `moving`; 0 for a move
  // that names no source slot. ABI 12.9.
  EnshwickItemId moved;
  // How many items the stack in `from_slot` holds, when `moving`.
  // ABI 12.9.
  uint32_t stack;
  // The amount the player asked for, which counts only for
  // `ACTION_AMOUNT`. ABI 12.9.
  uint32_t amount;
  // The container the items leave: the source entity, or the owner of
  // `from_slot`; 0 when unknown. ABI 12.9.
  EnshwickEntity from;
  // The container the items enter: the target entity, or the owner of
  // `to_slot`; 0 for none, as when items are destroyed. ABI 12.9.
  EnshwickEntity to;
  // What the player moves, one of the `ACTION_` constants; 0 when
  // unknown. ABI 12.9.
  uint8_t action;
  // How the move travels, a mask of the `TRANSFER_` constants; 0 for a
  // plain move. ABI 12.9.
  uint8_t transfer;
  // 1 when the fields of ABI 12.9 were read from a
  // `MOVE`. ABI 12.9.
  uint8_t moving;
  // Zero.
  uint8_t reserved_move;
  // The source slot, as a sub-inventory entity in the low 32 bits and
  // its slot index above them; 0 for a move of a whole inventory.
  // ABI 12.9.
  uint64_t from_slot;
  // The target slot, encoded as `from_slot`; 0 when the game picks the
  // slots. ABI 12.9.
  uint64_t to_slot;
} EnshwickInteraction;
// `action`: one item.
#define EnshwickInteraction_ACTION_ONE 1
// `action`: half the stack.
#define EnshwickInteraction_ACTION_HALF 2
// `action`: the whole stack, or every stack of a
// whole-inventory move.
#define EnshwickInteraction_ACTION_ALL 3
// `action`: `amount` items.
#define EnshwickInteraction_ACTION_AMOUNT 4
// `action`: the stack is destroyed.
#define EnshwickInteraction_ACTION_DESTROY 5
// `transfer`: a gift to another player nearby.
#define EnshwickInteraction_TRANSFER_GIFT 1
// `transfer`: only the stacks of items the target
// already holds move, as Deposit Stacks and Refill Stacks do.
#define EnshwickInteraction_TRANSFER_REFILL_STACKS 2
// `transfer`: every stack moves, as a storage
// window's Take All and Deposit All do.
#define EnshwickInteraction_TRANSFER_MOVE_ALL_STACKS 4
// `transfer`: the stack goes to the action bar.
#define EnshwickInteraction_TRANSFER_TO_ACTIONBAR 8
// `transfer`: the stack goes to the backpack.
#define EnshwickInteraction_TRANSFER_TO_BACKPACK 16
// The game offers to open the target.
#define EnshwickInteraction_OPEN 1
// Items move out of or into the target.
#define EnshwickInteraction_MOVE 2
// The target's inventory is sorted.
#define EnshwickInteraction_SORT 3
// The target lends its stock to crafting or building nearby; asked
// without the target's item or position.
#define EnshwickInteraction_STOCK 4
// The cursor offers to pick up or tear down the target.
#define EnshwickInteraction_DISMANTLE 5
// The player flips the target's Quick Stash switch.
#define EnshwickInteraction_QUICK_STASH 6
// The player upgrades the target into another piece.
#define EnshwickInteraction_UPGRADE 7

// The outcome of one operation in an `EnshwickInventoryResult`.
typedef struct EnshwickInventoryOpResult {
  // The game's error code, 0 for none, or `EnshwickInventoryOpResult_NOT_RUN`.
  uint32_t error;
  // How much of the count the operation could not do.
  uint32_t remaining;
} EnshwickInventoryOpResult;
// The operation did not run: one before it failed.
#define EnshwickInventoryOpResult_NOT_RUN 256

// What became of an `EnshwickInventoryRequest`, passed to its
// `EnshwickInventoryResultFn`.
typedef struct EnshwickInventoryResult {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // One of the constants on this type.
  uint32_t outcome;
  // How many results `ops` holds: one per operation, or none when the
  // request never ran.
  uint32_t op_count;
  // The operations' results, valid for the call.
  const struct EnshwickInventoryOpResult *ops;
} EnshwickInventoryResult;
// Every operation succeeded and the transaction was committed.
#define EnshwickInventoryResult_COMMITTED 0
// An operation failed or fell short; the transaction was rolled back
// and nothing changed.
#define EnshwickInventoryResult_ROLLED_BACK 1
// The request could not run: this game build lacks a piece, or its hook
// was not armed. Nothing changed.
#define EnshwickInventoryResult_UNAVAILABLE 2
// The game did not run inventories in time, as with nobody online.
// Nothing changed.
#define EnshwickInventoryResult_TIMED_OUT 3

// A player's action with an item, handed to the filters registered with
// `EnshwickHostApi.item_filter` in the server's
// world.
//
// The name is the game's `EnshwickItemAction`; the kinds are its `ItemActionType`,
// `ChangeEquipment` and the events that follow a pickup or a placement,
// and `CONSUMED` the loader's word that the game took the
// item of a use every filter allowed.
typedef struct EnshwickItemAction {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // What the player does; one of the constants on this type.
  uint32_t kind;
  // The player's character entity.
  EnshwickEntity character;
  // The item.
  EnshwickItemId item;
  // How many: 1 for a use, the stack for the others.
  uint32_t count;
  // The inventory that holds the item's slot, 0 when none does.
  EnshwickEntity inventory;
  // The slot's index in `inventory`.
  uint32_t slot;
} EnshwickItemAction;
// The player eats, drinks or uses the item from the inventory; a
// refusal leaves it in the slot.
#define EnshwickItemAction_CONSUME 1
// The player throws the stack away from the inventory; a refusal
// keeps it.
#define EnshwickItemAction_TRASH 2
// The player equips the item; a refusal keeps the equipment as it was.
#define EnshwickItemAction_EQUIP 3
// The player takes the item off; a refusal keeps it on.
#define EnshwickItemAction_UNEQUIP 4
// The player picked the item up, after the game moved it; the answer
// is ignored.
#define EnshwickItemAction_PICKUP 5
// The player placed the item as a building piece or a prop, after the
// game placed it; the answer is ignored.
#define EnshwickItemAction_PLACE 6
// The game took the item of a `CONSUME` every filter
// allowed from its slot, at a later step; a use the game refuses, or
// whose player does anything else first, raises none. The answer is ignored.
#define EnshwickItemAction_CONSUMED 7

// An account joining the game, handed to the rules registered with
// `EnshwickHostApi.join_rule`.
//
// The rules see it before any player of its machine enters the world.
// Layout grows only by appending; read a later field once
// the header's `size` covers it.
typedef struct EnshwickJoinCall {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The joining account's key, the one its player and the server's ban
  // list carry; zero for an account the game gives no key.
  EnshwickPlayerKey account;
} EnshwickJoinCall;

// The data of the `KnowledgeChanged` event.
typedef struct EnshwickKnowledgeData {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The player's character, or 0 for the world's knowledge.
  EnshwickEntity entity;
  // The knowledge id.
  EnshwickKnowledgeId id;
  // The value now.
  uint32_t value;
} EnshwickKnowledgeData;

// Knowledge a client asks the server for in its input, handed to the
// filters registered with `EnshwickHostApi.knowledge_input_filter` with the
// verdict so far.
typedef struct EnshwickKnowledgeInput {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The asking player's character.
  EnshwickEntity character;
  // The knowledge id.
  EnshwickKnowledgeId id;
  // The kind the client names: `EnshwickKnowledgeInfo_BOOL`,
  // `EnshwickKnowledgeInfo_COUNTER` or `EnshwickKnowledgeInfo_FLAGS`.
  uint8_t kind;
  // `EnshwickKnowledgeInfo_WORLD` or `EnshwickKnowledgeInfo_PLAYER`, as the id's
  // definition says; `EnshwickKnowledgeInput_NO_SCOPE` for an id the game
  // does not know.
  uint8_t scope;
  // Zero.
  uint16_t reserved;
  // Ignored for a bool, added for a counter, the bit to set for flags.
  uint32_t value;
  // Why the loader's knowledge guard would refuse the request, in any
  // mode: `EnshwickKnowledgeInput_ALLOWED` when it would not.
  uint32_t refusal;
  // 1 when the request goes through so far: after the guard, which
  // refuses only when enforcing, and the filters that ran before.
  uint8_t allowed;
  // Zero.
  uint8_t reserved_tail[3];
} EnshwickKnowledgeInput;
// The scope of an id the game does not know.
#define EnshwickKnowledgeInput_NO_SCOPE 255
// The guard lets it through.
#define EnshwickKnowledgeInput_ALLOWED 0
// An id the game does not know.
#define EnshwickKnowledgeInput_UNKNOWN 1
// A kind that is not the id's.
#define EnshwickKnowledgeInput_KIND 2
// An id of the world's, which every player shares.
#define EnshwickKnowledgeInput_WORLD 3
// A counter.
#define EnshwickKnowledgeInput_COUNTER 4
// A flag bit other than 0, the bit a client sets for what it has seen.
#define EnshwickKnowledgeInput_FLAG_BIT 5
// An id that gives experience.
#define EnshwickKnowledgeInput_EXPERIENCE 6
// An id that gives skill points.
#define EnshwickKnowledgeInput_SKILL_POINTS 7
// A quest's id, set when the game rewards the quest.
#define EnshwickKnowledgeInput_QUEST 8

// A wait of the engine's event loop on its handles, about to happen, as
// `EnshwickHostApi.loop_wait` hands it on.
typedef struct EnshwickLoopWaitData {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // What the game is about to wait, in milliseconds: the time left,
  // truncated.
  uint32_t ms;
  // Zero.
  uint32_t reserved;
  // The time left before the loop's deadline, in nanoseconds.
  int64_t left_ns;
} EnshwickLoopWaitData;

// What a mod is, read by the loader before it runs any of the mod's code.
//
// `enshwick_mod_query` returns it, valid for the process; the loader
// copies what it keeps.
typedef struct EnshwickManifest {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // `<author>.<name>`, see `is_valid_id`; names the settings file.
  struct EnshwickStr id;
  // The mod's SemVer version.
  struct EnshwickStr version;
  // Where the mod runs: `EnshwickSide_SERVER`, only where the world runs, or
  // `EnshwickSide_BOTH`, there and on every client playing in the world, which
  // must have it; the loader refuses any other side.
  EnshwickSide side;
  // Manifest flags.
  EnshwickManifestFlags flags;
  // Relations to other mods.
  const struct EnshwickDependency *dependencies;
  // How many entries `dependencies` points at.
  uint32_t dependency_count;
  // Symbols and constants the mod pins itself, per build.
  const struct EnshwickBuildRow *builds;
  // How many rows `builds` points at.
  uint32_t build_count;
  // The name players read, such as `Survivors per Altar`; empty for the
  // id.
  struct EnshwickStr name;
  // One sentence on what the mod does.
  struct EnshwickStr description;
  // The authors' names.
  const struct EnshwickStr *authors;
  // How many names `authors` points at.
  uint32_t author_count;
  // An SPDX licence expression, such as `MIT OR Apache-2.0`.
  struct EnshwickStr license;
  // Where players read about the mod, an `https://` address.
  struct EnshwickStr homepage;
  // The schemas of the extension points, calls, mod events and services
  // the mod defines, as TOML that `cargo enshwick deps` reads; empty for none.
  // A Rust mod's build collects them from the DLL's `.enshpt` section, where
  // a tool reads them without running the mod.
  struct EnshwickStr points;
  // What other mods may use, each `<kind>:<local name>` with a kind of
  // `point`, `event`, `call`, `service`, `content` or `stat`, such as
  // `call:price-of` for `<id>:price-of`. The loader refuses another mod a
  // reference to a name of this mod unless the list holds it with its
  // kind.
  const struct EnshwickStr *exports;
  // How many names `exports` points at.
  uint32_t export_count;
} EnshwickManifest;

// A message on a mod's channel, handed to the handler registered with
// `EnshwickHostApi.net_channel`.
typedef struct EnshwickNetMessage {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The channel, as `EnshwickHostApi.net_channel` wrote it.
  uint32_t channel;
  // Zero.
  uint32_t reserved;
  // Who sent it: a player's account key on the server,
  // `EnshwickNetMessage_SERVER` on a client.
  EnshwickPlayerKey peer;
  // The message, valid for the call.
  const uint8_t *data;
  // How many bytes `data` holds.
  uint32_t size;
  // Zero.
  uint32_t reserved_tail;
} EnshwickNetMessage;
// The server, as a peer: what a client hears from and sends to.
#define EnshwickNetMessage_SERVER UINT64_MAX

// The data of the `PlayerJoined` and
// `PlayerLeft` events.
typedef struct EnshwickPlayerData {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The server's player slot, 0 to 15, while the player is connected.
  uint32_t slot;
  // The player character's entity id.
  EnshwickEntity entity;
  // The same number for the same player account in every session: a hash
  // the game makes of the online id.
  EnshwickPlayerKey key;
} EnshwickPlayerData;

// The data of the `PlayerDied` event.
typedef struct EnshwickPlayerDeathData {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The player's character; 0 when the account left in the same step.
  // The name is enshwick's: the game's record holds the user instead.
  EnshwickEntity entity;
  // What killed the player, numbered as the damage filter numbers its
  // causes: `EnshwickDamageCheck_IMPACT`,
  // `EnshwickDamageCheck_FALL_DAMAGE`,
  // `EnshwickDamageCheck_FOG`,
  // `EnshwickDamageCheck_DEADLY_FOG` or
  // `EnshwickDamageCheck_SUFFOCATED`, which is
  // drowning; another number is a cause of a later game build.
  uint32_t cause;
  // The account, the same number in every session;
  // the name is enshwick's for the game's account id hash.
  EnshwickPlayerKey key;
  // Where, in metres.
  EnshwickMetres position;
  // For a hit, the template GUID of what dealt it, a creature's or an
  // impact's; zero for other causes.
  struct EnshwickGuid cause_guid;
  // For a hit, the attacker of the last hit the player took, from its
  // `HitHistory`, as `EntityDiedEvent` reads it; 0 for other
  // causes, or when the history names none. The name is enshwick's.
  EnshwickEntity killer_entity;
  // Zero.
  uint32_t reserved_tail;
} EnshwickPlayerDeathData;

// What became of a level or experience write a mod asked for with
// `EnshwickHostApi.progress_write`: the data of
// the `ProgressWritten` event. The
// name is enshwick's.
typedef struct EnshwickProgressWriteData {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The player's character the write was for.
  EnshwickEntity entity;
  // The `what` the mod passed: 1 to set the level
  // (`PROGRESS_SET_LEVEL`), 2 to set the experience
  // (`PROGRESS_SET_EXPERIENCE`) or 3 to add experience
  // (`PROGRESS_ADD_EXPERIENCE`).
  uint32_t what;
  // The `value` the mod passed.
  uint32_t value;
  // 0 when the write was applied (`APPLIED`),
  // else why not: 1 no level step took the character
  // (`NO_LEVEL_STEP`), 2 its
  // experience did not read (`UNREADABLE`),
  // 3 its level did not take the write
  // (`LEVEL_REFUSED`), 4 an earlier
  // write for it was not applied
  // (`BEHIND_DROPPED`), 5 written but
  // not confirmed (`UNCONFIRMED`).
  uint32_t outcome;
  // The character's level as the applied write left it, read back after
  // the write; 0 unless the outcome is 0.
  uint32_t level;
  // The experience inside the level as the applied write left it, read
  // back after the write; 0 unless the outcome is 0.
  uint32_t experience;
  // The experience gained and not applied yet as the write left it, which
  // the game applies in the same step, levelling up from it; 0 unless the
  // outcome is 0.
  uint32_t gained;
} EnshwickProgressWriteData;
// The server's level step applied the write.
#define EnshwickProgressWriteData_APPLIED 0
// No level step took the character by the second step end after the
// write: it left the world, or the step did not run for it.
#define EnshwickProgressWriteData_NO_LEVEL_STEP 1
// The character's experience did not read.
#define EnshwickProgressWriteData_UNREADABLE 2
// The character's level did not take the new level.
#define EnshwickProgressWriteData_LEVEL_REFUSED 3
// An earlier write for the same character could not be applied, and a
// character's writes apply in the order given.
#define EnshwickProgressWriteData_BEHIND_DROPPED 4
// The server wrote the character's level or experience, but a part of
// the write did not take or what it left did not read back; the mod
// reads the character again to know.
#define EnshwickProgressWriteData_UNCONFIRMED 5

// A prop a transaction places, as a player places one from an item, for
// `EnshwickTransactionOp_PLACE_PROP`.
typedef struct EnshwickPropPlace {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The item whose placed template the prop is.
  EnshwickItemId item;
  // Zero.
  uint32_t reserved;
  // Where it stands, in metres.
  EnshwickMetres position;
  // Which way it faces, a quaternion x, y, z, w.
  float orientation[4];
} EnshwickPropPlace;

// A prop a transaction removes, for `EnshwickTransactionOp_REMOVE_PROP`.
typedef struct EnshwickPropRemove {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The prop's entity.
  EnshwickEntity entity;
  // `EnshwickPropRemove_DISCARD_STATE` or none.
  uint32_t flags;
} EnshwickPropRemove;
// Removes the prop even when it holds state a player would lose (see
// `EnshwickTransactionInfo.holds`); without it, such a prop is refused.
#define EnshwickPropRemove_DISCARD_STATE 1

// The data of the `QuestCompleted`
// and `QuestStarted` events.
typedef struct EnshwickQuestData {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The player's character; 0 for a quest of the world's that started.
  EnshwickEntity entity;
  // The quest's id, a player knowledge id.
  EnshwickKnowledgeId quest;
} EnshwickQuestData;

// What the game pays a player for a quest, handed to the filters
// registered with `EnshwickHostApi.quest_reward_filter`
// as the filters before left it.
//
// The game pays a quest's experience and sets the quest's bool; the items
// its journal draws as rewards it never pays, so they are not here.
typedef struct EnshwickQuestPayment {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The player's character.
  EnshwickEntity entity;
  // The quest's id.
  EnshwickKnowledgeId quest;
  // How many of `experience` the game pays, from the first.
  uint32_t count;
  // The most the game pays on this build: the loader pays the first
  // `limit` of `experience` and drops the rest with a warning.
  uint32_t limit;
  // 1 while the game pays it; 0 pays nothing, and the quest still
  // completes.
  uint8_t allowed;
  // Zero.
  uint8_t reserved[3];
  // The experience values the game pays, each an experience value id of
  // the game's, as `ExperienceValue` names them; the same id twice pays
  // it twice.
  uint32_t experience[8];
} EnshwickQuestPayment;
// How many experience values the struct holds.
#define EnshwickQuestPayment_MAX 8

// The data of the `QuestProgressed` event.
typedef struct EnshwickQuestProgressData {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The player's character, or 0 for the world's knowledge.
  EnshwickEntity entity;
  // The quest's id.
  EnshwickKnowledgeId quest;
  // The step's place among the quest's steps, from 0.
  uint32_t step;
  // The part's place among the step's progress parts, from 0.
  uint32_t part;
  // The counter's value now.
  uint32_t value;
  // The count the part asks for: it holds at this count, or above it
  // for a part that asks for at least the count.
  uint32_t target;
} EnshwickQuestProgressData;

// The data of the `QuestStepDone` event.
typedef struct EnshwickQuestStepData {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The player's character, or 0 for the world's knowledge.
  EnshwickEntity entity;
  // The quest's id.
  EnshwickKnowledgeId quest;
  // The step's place among the quest's steps, from 0, as the journal
  // lists them.
  uint32_t step;
} EnshwickQuestStepData;

// The general-purpose registers and flags at a mid-function hook, as the
// interrupted game code had them.
//
// The loader saves them before it calls the handlers of a hook placed with
// `EnshwickHostApi.hook_at` and loads them back afterwards, so a handler that
// writes a field changes what the game code sees next. `rsp` is read-only:
// the stack stays where it was.
//
// Frozen, with no header: it is the whole x64 general-purpose set the
// loader's stub saves; vector registers, when offered, come through an
// entry of their own.
typedef struct EnshwickRegisters {
  // `rax`.
  uint64_t rax;
  // `rcx`.
  uint64_t rcx;
  // `rdx`.
  uint64_t rdx;
  // `rbx`.
  uint64_t rbx;
  // The stack pointer at the hooked instruction; writes are ignored.
  uint64_t rsp;
  // `rbp`.
  uint64_t rbp;
  // `rsi`.
  uint64_t rsi;
  // `rdi`.
  uint64_t rdi;
  // `r8`.
  uint64_t r8;
  // `r9`.
  uint64_t r9;
  // `r10`.
  uint64_t r10;
  // `r11`.
  uint64_t r11;
  // `r12`.
  uint64_t r12;
  // `r13`.
  uint64_t r13;
  // `r14`.
  uint64_t r14;
  // `r15`.
  uint64_t r15;
  // `rflags`.
  uint64_t rflags;
} EnshwickRegisters;

// A player renaming an entity, handed to the filters registered with
// `EnshwickHostApi.rename_filter` before the game applies it.
typedef struct EnshwickRenameCall {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The entity being renamed.
  EnshwickEntity entity;
  // The renaming player, valid for the call; null when the loader could
  // not tell who sent the rename.
  const struct EnshwickPlayerInfo *player;
  // The new name as the client sent it; valid for the call.
  struct EnshwickStr text;
} EnshwickRenameCall;

// The game asking whether a character unlocked a skill node, as
// `EnshwickHostApi.skill_override` hands it on.
typedef struct EnshwickSkillCheck {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The skill node.
  EnshwickSkillNode node;
  // The gate that asks, one of the constants on this type.
  uint32_t gate;
  // 1 when the character unlocked it.
  uint8_t unlocked;
  // Zero.
  uint8_t reserved[3];
} EnshwickSkillCheck;
// A caller the platform does not name.
#define EnshwickSkillCheck_OTHER 0
// The building fly mode gate, on both sides.
#define EnshwickSkillCheck_FLY_MODE 1
// A `EnshwickSkillCheckFn` answer: the game's own.
#define EnshwickSkillCheck_VANILLA 0
// A `EnshwickSkillCheckFn` answer: the skill counts as unlocked.
#define EnshwickSkillCheck_UNLOCKED 1
// A `EnshwickSkillCheckFn` answer: the skill counts as locked.
#define EnshwickSkillCheck_LOCKED 2

// A stat whose value changed on an entity, for the handlers registered
// with `EnshwickHostApi.stats_changed`.
typedef struct EnshwickStatChange {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The entity.
  EnshwickEntity entity;
  // The stat.
  struct EnshwickStatRef stat;
  // The value before.
  float old;
  // The value now.
  float new_;
  // Zero.
  uint32_t reserved;
} EnshwickStatChange;

// A summoning staff's use about to spawn or move a survivor, as
// `EnshwickHostApi.summon_filter` hands it on, and once it worked, as
// `EnshwickHostApi.summoned` does.
typedef struct EnshwickSummon {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The item used.
  EnshwickItemId item;
  // The ammunition it used, 0 for none.
  EnshwickItemId ammo;
  // The template GUID it places.
  struct EnshwickGuid template_;
  // The target cell.
  EnshwickCell target;
  // The slot the used item lies in, `sub-inventory | index << 32`.
  uint64_t slot;
  // The used item's own entity, 0 for an item without one.
  EnshwickEntity pide;
  // The used item's meter as clients see it, current and maximum;
  // `[0, 0]` for an item without one.
  uint16_t meter[2];
  // After the use: 1 when it took the item.
  uint8_t consumed;
  // Zero.
  uint8_t reserved[3];
} EnshwickSummon;
// A filter's answer: the use takes the item.
#define EnshwickSummon_TAKE 1
// A filter's answer: the use is refused; nothing spawns, moves or is
// taken.
#define EnshwickSummon_REFUSE 2

// A game system asking which entity is a survivor, as
// `EnshwickHostApi.survivor_lookup` hands it to the mod that answers.
//
// The game keeps one entity per survivor; a mod that keeps several answers
// each asker with the one that suits it. Cells are
// whole 0.5 m cells.
typedef struct EnshwickSurvivorLookup {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The survivor's id.
  EnshwickNpcId survivor;
  // The game's own answer, 0 for none.
  EnshwickEntity vanilla;
  // Who asks; one of the constants on this type.
  uint32_t purpose;
  // The entity the asker works on, as its purpose says; 0 for none.
  EnshwickEntity entity;
  // The cell its purpose names.
  EnshwickCell cell;
  // The bed an entity sleeps in, 0 for none; valid during the call with
  // `token`.
  EnshwickEntity (*bed_of)(void *token, EnshwickEntity entity);
  // The argument of `bed_of`.
  void *token;
} EnshwickSurvivorLookup;
// The pass that keeps one entity per survivor: `entity` is the one it
// looks at, `cell` its home. Answer `entity` to keep it, or another
// entity, which makes the game remove `entity`.
#define EnshwickSurvivorLookup_UNIQUENESS 1
// A summoning staff used at `cell`: answer the entity to move there, or
// 0 to spawn a new one.
#define EnshwickSurvivorLookup_SPAWN 2
// A bed at `cell` binding its sleeper: answer the entity that may sleep
// there, 0 for none.
#define EnshwickSurvivorLookup_BED_OWNER 3
// A player giving bed `entity` at `cell` to the survivor with its staff:
// answer the entity that gets it.
#define EnshwickSurvivorLookup_ASSIGN_BED 4
// The same assignment asking who slept in bed `entity` at `cell` before:
// answer that entity, 0 for none.
#define EnshwickSurvivorLookup_PREVIOUS_OCCUPANT 5
// A survivor's idle behaviour: `entity` is the survivor, `cell` where it
// stands; answer the entity whose base it keeps to.
#define EnshwickSurvivorLookup_ROAMING 6
// The first binding of beds after a load, before beds bind their
// sleepers.
#define EnshwickSurvivorLookup_BED_BINDING 7
// The cursor measuring how far a bed lies from the survivor before it
// offers to assign it; 0 skips the measure.
#define EnshwickSurvivorLookup_CURSOR 8

// The data of the `TerrainPageLoaded`
// and `TerrainPageUnloaded` events: a
// page of the finest voxels, the ones
// `Voxels` reads. "Page" is enshwick's name.
typedef struct EnshwickTerrainPageData {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // Its column in the world's grid of pages, from 0 at the world's
  // corner along the voxels' x axis.
  uint32_t x;
  // Its row, from 0 along the voxels' z axis.
  uint32_t z;
  // Its side in metres, 128 on server 26430253: it spans `x * size` to
  // `(x + 1) * size` metres along x and `z * size` to `(z + 1) * size`
  // along z, at every height.
  uint32_t size;
} EnshwickTerrainPageData;

// The data of the `Ticked` event.
//
// A handler on a period sees the steps and the time since its own last
// call; an every-step handler sees one step and the time since the step
// before.
typedef struct EnshwickTickData {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // Steps since the loader started counting, from 1.
  uint64_t step;
  // Server steps since this handler's last call: 1 for an every-step
  // handler and for a handler's first call.
  uint64_t elapsed_steps;
  // Wall time since this handler's last call in nanoseconds; for its
  // first call, and for an every-step handler, the time since the step
  // before. 0 on the loader's first step.
  uint64_t elapsed_ns;
  // The average time between server steps in nanoseconds, over about the
  // last 16 steps; 0 until a step has been measured.
  uint64_t step_ns;
  // What one call of this mod's event handlers cost on average since
  // this handler's last call, in nanoseconds; 0 for an every-step
  // handler, unless the loader's diagnostics time callbacks, and for
  // about a second after they start, while the loader measures its
  // clock.
  uint64_t cost_ns;
  // Which game the server steps, from 1: a dedicated server runs one,
  // and a client counts one more for each game it hosts after the first.
  uint64_t game;
} EnshwickTickData;

// A frame of the client's interface, handed to the callbacks registered with
// `EnshwickHostApi.ui_frame`; widgets drawn with `EnshwickHostApi.ui_widget` name its
// `token`.
typedef struct EnshwickUiFrame {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // Names this frame in `EnshwickHostApi.ui_widget`; valid until the callback
  // returns.
  uint64_t token;
  // Where the player is: `EnshwickUiFrame_MENU`, `EnshwickUiFrame_LOADING` or
  // `EnshwickUiFrame_IN_GAME`.
  uint32_t screen;
  // Zero.
  uint32_t reserved;
  // The game's name of its screen the player has open, such as
  // `StorageChest`, a member of its `HudScreenType`; empty outside a game
  // and on a build where the loader does not find it. The loader keeps
  // the text for the life of the process.
  struct EnshwickStr hud_screen;
  // The title of what the player's interaction prompt offers now, such
  // as a chest's name, as the game shows it; empty while it offers
  // nothing, outside a game and on a build where the loader does not
  // find it. The game's own text, borrowed for the callback.
  struct EnshwickStr prompt_title;
  // The game's text id (`LocaTagId`) of the prompt's verb, such as the
  // one for "Open"; 0 while the prompt offers nothing.
  uint32_t prompt_verb;
  // The entity of the client's world the open screen belongs to, such
  // as the chest whose inventory shows; entity 0 for a screen of no
  // object, outside a game and on a build where the
  // loader does not find it.
  EnshwickEntity screen_host;
  // The template GUID of `screen_host` in the client's world, as the
  // client's last step read it; all zero for no host, before a step read
  // this host, and on a build where the loader does not find it.
  struct EnshwickGuid screen_host_template;
  // Where `screen_host` stands in the client's world, in
  // metres, from the same step; meaningful only while
  // `screen_host_template` is not all zero.
  EnshwickMetres screen_host_position;
  // The slot of the game's inventory screen under the cursor in this
  // frame, as the game packs it, `inventory | index << 32`, the
  // inventory an entity of the client's world; 0 while the cursor is on
  // none, outside an inventory screen and on a build where the loader
  // does not find the game's slot widget. While a gamepad drives the
  // interface, the slot that holds the screen's focus.
  uint64_t hovered_slot;
  // The item id in `hovered_slot`; 0 for an empty slot.
  EnshwickItemId hovered_item;
  // How many `hovered_slot` holds.
  uint32_t hovered_count;
  // The entity of the item in `hovered_slot`, for an item with one of
  // its own; entity 0 for none.
  EnshwickEntity hovered_pide;
  // Zero.
  uint32_t reserved_hovered;
} EnshwickUiFrame;
// The title screen and the menus before a game.
#define EnshwickUiFrame_MENU 0
// A game loading, or the intro and outro.
#define EnshwickUiFrame_LOADING 1
// In a game.
#define EnshwickUiFrame_IN_GAME 2

// The item whose tooltip opened, handed to the callbacks registered with
// `EnshwickHostApi.ui_item_tooltip`; the lines
// a callback adds are `EnshwickUiWidget_LABEL` widgets drawn with
// `EnshwickHostApi.ui_widget` naming its `token`.
typedef struct EnshwickUiTooltipItem {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // Names this tooltip in `EnshwickHostApi.ui_widget`;
  // valid until the callback returns.
  uint64_t token;
  // The item id.
  EnshwickItemId item;
  // The item's own entity in the client's world.
  EnshwickEntity pide;
} EnshwickUiTooltipItem;
// The most lines one callback adds to a tooltip; the loader refuses
// more with `EnshwickStatus_BUSY`.
#define EnshwickUiTooltipItem_MAX_LINES 16

// One voxel a committed edit changed, as `EnshwickVoxelEditRequest` lists it: where
// it is and what it was and is.
//
// Frozen, read as an array from `EnshwickVoxelEditRequest`: a cell and its voxel
// before and after are all a change is.
typedef struct EnshwickVoxelChange {
  // The voxel's place, in voxels from the world's corner.
  uint32_t at[3];
  // The voxel before the edit.
  struct EnshwickVoxel before;
  // The voxel after it.
  struct EnshwickVoxel after;
} EnshwickVoxelChange;

// The data of `EnshwickEvent_VOXELS_CHANGED`: one
// write of the server's voxel editor and the voxels it changed, borrowed
// for the call that hands it over.
//
// The game writes a whole box, aligned to its bricks of 8 voxels and
// larger than the edit, so `changes` lists only the voxels that differ
// from the box as it was. Layout grows only by appending; read a later
// field once the header's `size` covers it.
typedef struct EnshwickVoxelEditRequest {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The mod whose call through the API made the edit, such as a
  // transaction's write or a terrain edit; `EnshwickModHandle_NONE` for the
  // game's own edits, a player's tools and buildings among them, and
  // for the data packs the loader runs.
  EnshwickModHandle by;
  // The lowest corner of the box the game wrote, in voxels.
  uint32_t min[3];
  // The box's voxels along each axis.
  uint32_t size[3];
  // The voxels that changed, `count` of them, in the box's order: x
  // fastest, then y, then z. Valid for the call only.
  const struct EnshwickVoxelChange *changes;
  // How many voxels changed, at least one.
  uint32_t count;
} EnshwickVoxelEditRequest;

// A box of voxels, for `EnshwickTransactionOp_ERASE`.
typedef struct EnshwickVoxelRegion {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The box's lowest voxel, in voxels of half a metre.
  uint32_t min[3];
  // The box's voxels along each axis.
  uint32_t size[3];
} EnshwickVoxelRegion;

// Voxels a transaction writes over a box, for
// `EnshwickTransactionOp_VOXELS`.
typedef struct EnshwickVoxelWrite {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The box's lowest voxel, in voxels of half a metre from the world's
  // corner.
  uint32_t min[3];
  // The box's voxels along each axis.
  uint32_t size[3];
  // The box's voxels, x fastest, then y, then z, borrowed for the call;
  // each kind must be the one its material names.
  const struct EnshwickVoxel *voxels;
  // The voxels at `voxels`: `size[0] * size[1] * size[2]`.
  uint32_t count;
} EnshwickVoxelWrite;

// A mod's answer to the loader's questions about its player actions: the
// call, the pointer given at registration and where to write the reply,
// which stays valid until the callback runs again.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the server's game thread for a player, and on the thread that runs
// the line for the console or a remote caller.
typedef void (*EnshwickActionFn)(const struct EnshwickActionCall *call,
                                 void *user,
                                 struct EnshwickStr *reply);

// An altar limit filter: the limit so far, writable, and the pointer given
// at registration. It must not unwind.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the thread of the game system that asks, the game thread or a task
// worker, so several calls may run at once.
typedef void (*EnshwickAltarLimitFilterFn)(struct EnshwickAltarLimit *limit, void *user);

// An altar zone filter: the zone so far, writable, and the pointer given
// at registration. It must not unwind.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the thread of the game system that asks, the game thread or a task
// worker, so several calls may run at once.
typedef void (*EnshwickAltarZoneFilterFn)(struct EnshwickAltarZone *zone, void *user);

// A filter of survivors' beds: returns `FILTER_REFUSE` to make the game clear the bed.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the thread of the game system that asks, the game thread or a task
// worker, so several calls may run at once.
typedef uint8_t (*EnshwickBedCheckFn)(const struct EnshwickBedCheck *check, void *user);

// A mod's build filter: the placement and the pointer given at
// registration; returns `FILTER_REFUSE` to refuse it, else `FILTER_PASS`.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the worker threads of the game's cursor check, on the server and on
// a client for its own prediction, so several calls may run at once.
typedef uint8_t (*EnshwickBuildFilterFn)(const struct EnshwickBuildCheck *call, void *user);

// A mod's zone check filter: the check and the pointer given at
// registration; returns `FILTER_YES` to accept the
// placement after all, and any other answer keeps the game's refusal.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the thread of the game system that asks, the game thread or a task
// worker, so several calls may run at once.
typedef uint8_t (*EnshwickBuildingZoneFilterFn)(const struct EnshwickBuildingZoneCheck *check,
                                                void *user);

// A mod's handler for a call it serves.
//
// It gets the input, valid for the call, the answer to write, as large as
// the spec's output, and the pointer given at serving, and answers
// `EnshwickStatus_OK` only once the whole answer is written.
// It runs on the caller's thread, any thread, possibly on several at once
// and inside another call, and must not unwind.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
typedef EnshwickStatus (*EnshwickCallFn)(const void *input, void *output, void *user);

// A mod's chat filter, registered with
// `EnshwickHostApi.chat_filter`.
//
// It is called on the server's game thread, inside the game's chat post,
// for a player's line after the chat mute; it answers `EnshwickChatRewrite_KEEP`,
// `EnshwickChatRewrite_DROP` or `EnshwickChatRewrite_REWRITTEN`, any other answer
// keeping the line. `line` and `rewrite` are borrowed for the call and
// never null; `line.text` is the text after the filters before this one.
// Never re-entered; it must not unwind.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
typedef uint8_t (*EnshwickChatFilterFn)(const struct EnshwickChatLine *line,
                                        struct EnshwickChatRewrite *rewrite,
                                        void *user);

// A comfort filter: the registration and the pointer given at
// registration; returns the comfort to register.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the thread of the game system that asks, the game thread or a task
// worker, so several calls may run at once.
typedef int32_t (*EnshwickComfortFilterFn)(const struct EnshwickComfort *comfort, void *user);

// A mod's check of a new text of its settings file, before the loader writes
// it: `EnshwickStatus_OK` to accept it; else the reason, written to `why` and valid
// until the mod's next call, and `EnshwickStatus_REFUSED`.
//
// A reason that starts with a key of the mod's texts, followed by
// tab-separated `name=value` pairs (`"config.min\tmin=2"`), shows as that
// text in the reader's language with each `{name}` filled; any other
// reason shows as written. A name holds no `=`; neither holds a tab.
//
// It must not unwind.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Its thread is not fixed, so it must be thread-safe.
typedef EnshwickStatus (*EnshwickConfigCheckFn)(struct EnshwickStr text,
                                                struct EnshwickStr *why,
                                                void *user);

// A mod's container rule: the stack and the pointer given at registration;
// returns `FILTER_REFUSE` to keep the stack out.
//
// The game handles a refusal as a slot that does not take the stack, so it
// stays where it was or goes to another slot.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the thread of the game system that moves the items, the game
// thread or a task worker, so several calls may run at once.
typedef uint8_t (*EnshwickContainerRuleFn)(const struct EnshwickContainerPut *put, void *user);

// A mod's control callback: the event and the pointer given with
// `EnshwickHostApi.control`.
//
// The client calls it on its interface thread for a key, the server on its
// game thread for a game action; a client hosting a game does both, so a
// control with a key and a game action may run on the two threads at once.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
typedef void (*EnshwickControlFn)(const struct EnshwickControlEvent *event, void *user);

// A mod's craft filter: the request and the pointer given at registration;
// returns `FILTER_REFUSE` to refuse it, else `FILTER_PASS`.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the server's game thread, at the start of a step.
typedef uint8_t (*EnshwickCraftFilterFn)(const struct EnshwickCraftCheck *call, void *user);

// A mod's damage filter: the damage, which it may change, and the pointer
// given at registration. It runs on the game's worker threads.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
typedef void (*EnshwickDamageFilterFn)(struct EnshwickDamageCheck *check, void *user);

// A mod's handler for an `EnshwickEvent`: the event, its data (null for events
// without data, valid for the call, and no other thread writes it during
// the call) and the pointer given at subscription.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
typedef void (*EnshwickEventFn)(EnshwickEvent event, const void *data, void *user);

// An experience filter: the pass and the pointer given at registration.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the thread of the game system that asks, the game thread or a task
// worker, so several calls may run at once.
typedef void (*EnshwickExperienceFilterFn)(struct EnshwickExperiencePass *pass, void *user);

// A mod's handler for game events: the event and the pointer given at
// subscription. It must not unwind.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the thread that steps the event's world, at the start of a step.
typedef void (*EnshwickGameEventFn)(const struct EnshwickGameEventData *event, void *user);

// A mod's interaction filter: the call and the pointer given at
// registration; returns `FILTER_REFUSE` to refuse, which the game handles as its own
// refusal, so nothing moves and nothing is lost.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the thread of the game system that asks, the game thread or a task
// worker, so several calls may run at once.
typedef uint8_t (*EnshwickInteractionFilterFn)(const struct EnshwickInteraction *call, void *user);

// Receives the `EnshwickInventoryResult` of a request, on the server's game thread
// after a simulation step; it must not unwind.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
typedef void (*EnshwickInventoryResultFn)(const struct EnshwickInventoryResult *result, void *user);

// A mod's item filter: the action and the pointer given at registration;
// returns `FILTER_REFUSE` to refuse an action that can be refused.
//
// The game handles a refused action as one it already ran, so nothing
// moves and nothing is lost.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the thread of the game system that asks, the game thread or a task
// worker, so several calls may run at once.
typedef uint8_t (*EnshwickItemFilterFn)(const struct EnshwickItemAction *call, void *user);

// A mod's join rule, registered with
// `EnshwickHostApi.join_rule`.
//
// It is called on the thread the session reads its messages on, not the
// game thread, once for each account a joining machine brings; it returns
// `FILTER_REFUSE` to refuse the account, any other answer letting it
// in. `call` is borrowed for the call and never null; `user` is the
// pointer given at registration, passed as it is. Never re-entered; it must
// not unwind.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
typedef uint8_t (*EnshwickJoinRuleFn)(const struct EnshwickJoinCall *call, void *user);

// A mod's knowledge input filter: the request and the pointer given at
// registration; returns `FILTER_REFUSE` to refuse it, else `FILTER_PASS`.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the thread of the game system that asks, the game thread or a task
// worker, so several calls may run at once.
typedef uint8_t (*EnshwickKnowledgeInputFilterFn)(const struct EnshwickKnowledgeInput *call,
                                                  void *user);

// A handler of the event loop's waits: the wait, borrowed for the call,
// and the pointer given at registration. It must not unwind.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the thread that waits, inside its wait.
typedef void (*EnshwickLoopWaitFn)(const struct EnshwickLoopWaitData *wait, void *user);

// A handler of a mid-function hook: the saved registers, writable, and the
// pointer it was registered with. It must not unwind.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on whichever thread reaches the hooked instruction, so several calls
// may run at once.
typedef void (*EnshwickMidHookFn)(struct EnshwickRegisters *registers, void *user);

// A mod's handler for a mod event: the event's data, valid for the call,
// and the pointer given at subscription.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the thread that sends the event.
typedef void (*EnshwickModEventFn)(const void *data, void *user);

// A mod's handler for messages on its channel: the message and the pointer
// given at registration. It must not unwind.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Its thread is not fixed, so it must be thread-safe.
typedef void (*EnshwickNetMessageFn)(const struct EnshwickNetMessage *message, void *user);

// A mod's link on an extension point: the input, valid for the call, the
// answer so far, which the link may overwrite and then answers
// `EnshwickPointVerdict_SET`, and the pointer given at attaching.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the thread that asks the point.
typedef EnshwickPointVerdict (*EnshwickPointFn)(const void *input, void *value, void *user);

// A mod's quest reward filter, which may change the payment in place.
//
// It gets the payment and the pointer given at registration, and returns
// `FILTER_REFUSE` to pay nothing,
// `FILTER_YES` to pay again what a filter before
// refused, else `FILTER_PASS`.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the thread of the game's quest reward system, the game thread or
// a task worker. A payment the game puts off, as when it cannot queue the
// quest's bool, comes again on a later step, and the filters see it again.
typedef uint8_t (*EnshwickQuestRewardFilterFn)(struct EnshwickQuestPayment *call, void *user);

// A mod's rename filter: the call and the pointer given at registration;
// returns `FILTER_REFUSE` to refuse the rename, else `FILTER_PASS`.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the thread the game changes the name on.
typedef uint8_t (*EnshwickRenameFilterFn)(const struct EnshwickRenameCall *call, void *user);

// Builds a served resource from its base: `base` is the base resource's
// `base_len` bytes, readable and unchanged for the call.
//
// On `EnshwickStatus_OK` the function writes to `out` and `out_len` where its
// output lies: bytes inside one buffer it took during this call from
// `EnshwickHostApi.resource_buffer`, the only
// place an output may lie. The loader owns the buffer: the game's first
// build keeps it for the process, as the game holds those bytes, and frees
// every other buffer when the build that lent it ends, a rebuild's and a
// served read's included. An output anywhere else is a failure, as is a
// null one. A rebuild
// (`EnshwickHostApi.rebuild_resource`) writes
// the bytes of its output that differ over those the first build kept.
//
// The loader calls it on the thread that builds: the game's resource
// thread for the game's first build, the calling thread for a rebuild or a
// served read (`EnshwickHostApi.resource_served`).
// Builds of one key on different threads run at once, so a patch may run
// on several threads at a time, each call with its own base and buffers;
// a rebuild of a key waits for the key's other rebuild. On one thread it
// may run nested inside another key's patch, never inside a build of its
// own key. A failure returned from the game's build turns the mod off,
// but `EnshwickStatus_FAULTED`, the answer of a
// patch whose builder panicked: that turns off only this patch for the
// process, and its mod reports the resource as a feature off. Later builds
// of the key run without it but a rebuild, which runs every patch the
// game's build applied. A panic must not leave the function. `user` is
// the pointer the mod served with.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
typedef EnshwickStatus (*EnshwickResourcePatchFn)(const uint8_t *base,
                                                  uint32_t base_len,
                                                  void *user,
                                                  uint8_t **out,
                                                  uint32_t *out_len);

// A handler of the server loop's wait between two passes: the wait in
// nanoseconds, which it may change, and the pointer given at registration.
// It must not unwind.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the server loop's thread, between two passes.
typedef void (*EnshwickServerWaitFn)(uint64_t *wait_ns, void *user);

// A skill override: the check and the pointer given at registration;
// returns `EnshwickSkillCheck_VANILLA`, `EnshwickSkillCheck_UNLOCKED` or
// `EnshwickSkillCheck_LOCKED`.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the thread of the game system that asks, the game thread or a task
// worker, so several calls may run at once.
typedef uint8_t (*EnshwickSkillCheckFn)(const struct EnshwickSkillCheck *check, void *user);

// A handler of stat changes: the change and the pointer given at
// registration. It runs on the game thread, after a step.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
typedef void (*EnshwickStatChangeFn)(const struct EnshwickStatChange *change, void *user);

// A mod's status source: the pointer given at registration and where to
// write the mod's lines for the loader's status summary. The text stays
// valid until the source runs again; empty for none.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the thread that asks for the loader's status summary, such as the
// one running a console line.
typedef void (*EnshwickStatusFn)(void *user, struct EnshwickStr *out);

// A summon filter: returns `EnshwickSummon_TAKE` to make the game take the
// item in the same transaction, and only when the spawn or move works,
// `EnshwickSummon_REFUSE` to refuse the use, or 0 to leave it.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the thread of the game system that asks, the game thread or a task
// worker, so several calls may run at once.
typedef uint8_t (*EnshwickSummonFilterFn)(const struct EnshwickSummon *summon, void *user);

// The mod's survivor lookup: the call and the pointer given at
// registration; returns 1 with the answer in `out`, or 0 for the game's own.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the thread of the game system that asks, the game thread or a task
// worker, so several calls may run at once.
typedef uint8_t (*EnshwickSurvivorLookupFn)(const struct EnshwickSurvivorLookup *call,
                                            void *user,
                                            EnshwickEntity *out);

// The client's survivor menu merging the statuses of every entity of one
// survivor.
//
// The survivor, `count` status bytes at `statuses`, valid for the call, and
// the pointer given at registration; returns `SURVIVOR_STATUS_SET` or-ed
// with the byte to show, or 0 to leave the game's.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the client, on the thread of the game system that builds the
// survivor menu.
typedef uint32_t (*EnshwickSurvivorStatusFn)(EnshwickNpcId survivor,
                                             const uint8_t *statuses,
                                             uint32_t count,
                                             void *user);

// A handler of a transaction's end: the transaction's id, where it stands
// (borrowed for the call, null once the loader no longer remembers it) and
// the pointer given at registration. It must not unwind.
//
// `EnshwickHostApi.transaction_settled`
// calls it once, on the server's game thread after a step.
typedef void (*EnshwickTransactionSettledFn)(uint64_t id,
                                             const struct EnshwickTransactionInfo *info,
                                             void *user);

// A mod's callback for the answer to its dialog: the answer, such as
// `EnshwickUiDialog_OK` or `EnshwickUiDialog_CLOSED`, and the pointer given with the
// dialog.
//
// Called once, on the thread that draws the interface, or on a thread of the
// loader's when the dialog shows as a system window. It must not unwind.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
typedef void (*EnshwickUiAnswerFn)(uint32_t answer, void *user);

// A mod's callback for each frame of the client's interface: the frame and
// the pointer given at registration. It must not unwind.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the client's main thread, which draws the interface.
typedef void (*EnshwickUiFrameFn)(const struct EnshwickUiFrame *frame, void *user);

// A mod's callback for an item's tooltip opening: the item and the pointer
// given at registration. It adds its lines with
// `EnshwickHostApi.ui_widget` and must not unwind.
//
// Null is never a callback: the loader refuses a null one with
// `EnshwickStatus_BAD_ARGS` and keeps nothing.
// Runs on the client's main thread, which draws the interface, once each
// time the tooltip shows another item, never once per frame.
typedef void (*EnshwickUiTooltipFn)(const struct EnshwickUiTooltipItem *item, void *user);

// A request to run a handler at an instruction inside a game function.
//
// Every mod's handler on one address runs, by priority and then load order,
// each seeing the writes of the ones before; then the instructions the jump
// replaced run and the function goes on.
typedef struct EnshwickMidHookSpec {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The absolute address of the instruction to run the handler before.
  uint64_t address;
  // The handler.
  EnshwickMidHookFn handler;
  // Passed to the handler on every call.
  void *user;
  // Lower runs earlier; equal priorities run in load order.
  int32_t priority;
  // `EnshwickHookFlags_EXCLUSIVE` or none.
  EnshwickHookFlags flags;
} EnshwickMidHookSpec;

// The table of services the loader offers a mod.
//
// A function appended at its end is a new
// `ABI_MINOR`, any other change a new
// `ABI_MAJOR`. A mod starts only on a table of its
// major and its minor or a newer one, whose header's `size` covers it whole.
//
// Every list answers the same way. The caller passes `out`, room for
// `capacity` entries `entry_size` bytes apart, the size of the entry as it
// knows it, at least a `EnshwickHeader` for a headed entry; the loader fills what
// both sides know of each entry, its own header in each, and writes how
// many there are to `count`. A null `out` with `capacity` 0 asks only for
// the count. A null `count`, an `entry_size` shorter than the entry's first
// version or a null `out` with a nonzero `capacity` is
// `EnshwickStatus_BAD_ARGS`, with nothing written.
//
// A list of plain values (ids, hashes, keys, an entity and its place, a
// stat modifier and its source) takes no `entry_size`: its entries are
// packed, of a type frozen at its size, since a value never gains a field;
// a value that needs more is a new entry. Otherwise it answers as every list does.
//
// No entry unwinds into the mod. A panic in the loader's own code answers
// `EnshwickStatus_LOADER_FAULT` and turns that entry off, which then answers
// `EnshwickStatus_LOADER_FAULT` to every mod; a panic in a mod's callback that
// an entry runs turns that mod off. An entry allowed during
// `enshwick_mod_init` only answers `EnshwickStatus_TOO_LATE` after it.
//
// Every callback a mod hands the loader, and the `user` pointer beside it,
// stays registered for the rest of the process: nothing unregisters it, and
// the loader never frees `user`. A callback must not unwind into the
// loader: a C++ exception or a `longjmp` out of it is undefined behaviour.
// Each callback's comment names the threads it runs on; one that names
// several threads, or worker threads, may run on them at once, so its
// state needs locks or atomics. A callback may call the entries allowed on
// its thread; one that sends an event, asks a point or makes a call may
// see its own callback, or another mod's, run inside that entry.
//
// Every `EnshwickStr` the loader writes, as an `out` or as a field of an entry
// it fills, names how long it stays valid: for the call, until the next
// call on the same thread, or for the process. Text kept for the process
// is never freed, so a mod asks for it once rather than every frame.
typedef struct EnshwickHostApi {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The loader's own SemVer version, valid for the process: a release's,
  // or a development build's, such as `1.0.0-alpha.15+dev.5f647f46`,
  // which sorts with the release it follows. Compatibility is the ABI
  // version's alone (`EnshwickHeader.abi_major`, `EnshwickHeader.abi_minor`), never
  // this text's.
  struct EnshwickStr host_version;
  // Writes the running game's identity, its side, image base, build
  // fingerprint and whether the loader found every entry it knows, as far as the
  // `EnshwickHeader` `out` starts with says the caller's struct reaches.
  // `EnshwickStatus_BAD_ARGS` for a null `out`. Any thread.
  EnshwickStatus (*build)(EnshwickModHandle me, struct EnshwickBuildInfo *out);
  // Resolves a symbolic id for the running build: the mod's own rows first,
  // then what the loader found. Any thread.
  EnshwickStatus (*symbol)(EnshwickModHandle me, struct EnshwickStr name, uint64_t *out);
  // Resolves an ECS system's update function by its reflected name.
  // `EnshwickStatus_MISSING` for a system the running build lacks. Any thread.
  EnshwickStatus (*system)(EnshwickModHandle me, struct EnshwickStr name, uint64_t *out);
  // Resolves a field's offset in a reflected type. `EnshwickStatus_MISSING`
  // for a type or field the running build lacks. Any thread.
  EnshwickStatus (*field_offset)(EnshwickModHandle me,
                                 struct EnshwickStr type_name,
                                 struct EnshwickStr field,
                                 uint64_t *out);
  // Resolves a reflected type's size. `EnshwickStatus_MISSING` for a type the
  // running build lacks. Any thread.
  EnshwickStatus (*type_size)(EnshwickModHandle me, struct EnshwickStr type_name, uint32_t *out);
  // Links a detour; allowed during `enshwick_mod_init` only.
  EnshwickStatus (*hook)(EnshwickModHandle me, const struct EnshwickHookSpec *spec);
  // Writes the game caller's context at entry of the current chain, for
  // links with `EnshwickHookFlags_CALLER_FRAME`; valid until the link calls
  // anything hooked; all zero outside a chain. On the thread the chain
  // runs on.
  EnshwickStatus (*caller_frame)(EnshwickModHandle me, struct EnshwickCallerFrame *out);
  // Reports that the mod failed at run time; the loader unlinks every link
  // of the mod and turns it off. Any thread; a mod already off gets
  // `EnshwickStatus_OK` too.
  EnshwickStatus (*fault)(EnshwickModHandle me, struct EnshwickStr reason);
  // Writes a line to the log under the mod's id, as long as the mod is
  // loaded, on or off. `EnshwickStatus_BAD_ARGS` for text that is not UTF-8.
  // Any thread.
  EnshwickStatus (*log)(EnshwickModHandle me, EnshwickLevel level, struct EnshwickStr message);
  // The text of the mod's settings file `file` (its name without
  // `.toml`) in force: the running world's, which the loader made or
  // brought in line with the schema `EnshwickHostApi.config_schema` handed it
  // as the world loaded; on a client in another's world, the values that
  // world's server sent; empty while no world runs. The text stays valid
  // for the life of the process, so a thread may read it while another
  // asks again. `EnshwickStatus_MISSING` for a file no schema names.
  EnshwickStatus (*settings)(EnshwickModHandle me, struct EnshwickStr file, struct EnshwickStr *out);
  // Publishes the service `spec` describes, table and layout, for other
  // mods; `EnshwickStatus_REFUSED` when another mod publishes its name and
  // major. Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*publish)(EnshwickModHandle me, const struct EnshwickServiceSpec *spec);
  // Finds the service `wanted` describes, its table null, and checks
  // that the published layout starts with the wanted one; writes the
  // table and its publisher to `out`, and on `EnshwickStatus_MISSING` or
  // `EnshwickStatus_REFUSED` the reason; refused for a service of another loaded
  // mod whose `EnshwickManifest.exports` lack its kind and name. Any thread.
  EnshwickStatus (*service)(EnshwickModHandle me,
                            const struct EnshwickServiceSpec *wanted,
                            struct EnshwickServiceFound *out);
  // Resolves an engine constant of the running build, such as a field
  // offset reflection does not describe, by its symbolic id. Any thread.
  EnshwickStatus (*constant)(EnshwickModHandle me, struct EnshwickStr name, uint64_t *out);
  // The mod's data folder in the world the server runs,
  // `<stem>-mod-data/<id>` beside the world's save, created on first use;
  // the path stays valid for the process. `EnshwickStatus_UNAVAILABLE` while
  // no world runs, as during init. Any thread.
  EnshwickStatus (*data_dir)(EnshwickModHandle me, struct EnshwickStr *out);
  // The most detailed level kept for the mod's lines; the mod skips
  // formatting anything more detailed. Any thread, without locks.
  EnshwickStatus (*log_level)(EnshwickModHandle me, EnshwickLevel *out);
  // Calls `callback` with `user` on every `event` for the mod, on the
  // thread that raises it, until the mod is off; allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*subscribe)(EnshwickModHandle me,
                              EnshwickEvent event,
                              EnshwickEventFn callback,
                              void *user);
  // The ECS type hash of a reflected component type, by name.
  // `EnshwickStatus_MISSING` for a type the running build lacks. Any thread.
  EnshwickStatus (*type_hash)(EnshwickModHandle me, struct EnshwickStr type_name, uint32_t *out);
  // A read-only pointer to `entity`'s component of type `type_hash`, valid
  // until the game thread moves on; on the server's game thread only.
  // For the static half of an ECS type, such as `FuelSetup`, it is the
  // set-up data the entity's template holds.
  EnshwickStatus (*component)(EnshwickModHandle me,
                              EnshwickEntity entity,
                              uint32_t type_hash,
                              const void **out);
  // The template GUID of `entity`; on the server's game
  // thread only.
  EnshwickStatus (*entity_template)(EnshwickModHandle me,
                                    EnshwickEntity entity,
                                    struct EnshwickGuid *out);
  // Up to `capacity` loaded mods, the loader first, and how many there
  // are. Entries are `entry_size` bytes apart, the caller's
  // `sizeof(EnshwickModInfo)`; the loader fills what both sides know of each
  // and its own `EnshwickHeader`. The strings stay valid for the process, since
  // no mod unloads before it ends. Any thread.
  EnshwickStatus (*mods)(EnshwickModHandle me,
                         struct EnshwickModInfo *out,
                         uint32_t entry_size,
                         uint32_t capacity,
                         uint32_t *count);
  // Writes where the game's code section (`.text`) is loaded and how many
  // bytes it spans. `EnshwickStatus_UNAVAILABLE` when the loader found no code
  // section. Any thread.
  EnshwickStatus (*code_range)(EnshwickModHandle me, uint64_t *start, uint32_t *len);
  // Replaces `len` bytes of game code at `address`, which must read
  // `expected` at arming, with `replacement`; both point at `len` bytes
  // valid for the call. The loader writes the patch with the hooks, refuses
  // one that overlaps another patch, and skips one that overlaps hooked
  // bytes or no longer reads `expected`, turning the mod off. Allowed
  // during `enshwick_mod_init` only.
  EnshwickStatus (*patch_code)(EnshwickModHandle me,
                               uint64_t address,
                               const uint8_t *expected,
                               const uint8_t *replacement,
                               uint32_t len);
  // The most recipes one workshop takes on the running game: 512, the
  // game's own list size, or `UINT32_MAX` once the loader has lifted that
  // limit at arming. Any thread.
  EnshwickStatus (*workshop_recipes)(EnshwickModHandle me, uint32_t *out);
  // Writes 1 while this process runs a game server: always on the
  // dedicated server, on the client while it hosts a game, solo play
  // included; else 0. Any thread.
  EnshwickStatus (*hosting)(EnshwickModHandle me, uint8_t *out);
  // Writes the entity world of the game server this process runs, the
  // value a system context of that world holds, a pointer whose meaning
  // belongs to the running game build; 0 while none runs or before its
  // first step. Any thread.
  EnshwickStatus (*server_world)(EnshwickModHandle me, uint64_t *out);
  // Up to `capacity` of the server's `EnshwickSide_BOTH`
  // mods that the last refused join found missing on this client or in
  // another version, and how many there are; laid out as
  // `EnshwickHostApi.mods` lays out its entries. Each entry has the id and
  // version the server advertises, `EnshwickSide_BOTH`,
  // `off` 1 and the client's DLL path when it has the mod (empty
  // otherwise); name, homepage and the rest stay empty, since the server
  // chose none of them. The strings stay valid for the process; once
  // 64 KiB of advertised text is kept, an entry with new text is left
  // out. Empty before a refused join; see `EnshwickEvent_JOIN_REFUSED`. Any thread.
  EnshwickStatus (*join_missing)(EnshwickModHandle me,
                                 struct EnshwickModInfo *out,
                                 uint32_t entry_size,
                                 uint32_t capacity,
                                 uint32_t *count);
  // Runs a handler before the instruction at an address inside a game
  // function, with the registers there; allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*hook_at)(EnshwickModHandle me, const struct EnshwickMidHookSpec *spec);
  // Adds `callback` with `user` to the mod's status sources: the loader
  // calls it, on a thread of its own or the game thread, whenever it
  // writes its status summary (`enshwick status` on the console, into
  // the loader's log) and shows its lines under the mod. Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*status)(EnshwickModHandle me, EnshwickStatusFn callback, void *user);
  // Calls `callback` with `user` on every game event of the reflected
  // type whose hash is `type_hash`, or on every game event for 0, on
  // the server's game thread before the systems of the next step run.
  // Allowed during `enshwick_mod_init` only, `EnshwickStatus_TOO_LATE` after.
  // `callback` is non-null and stays callable while the mod is loaded;
  // the loader keeps `user` and never reads or frees it. The event and
  // its bytes are borrowed for the call. `callback` may call the loader
  // again, and must not unwind: the Rust layer catches its handler's
  // panic and turns the mod off.
  EnshwickStatus (*subscribe_game_event)(EnshwickModHandle me,
                                         uint32_t type_hash,
                                         EnshwickGameEventFn callback,
                                         void *user);
  // Opens the mod's channel `name`, 1 to 64 bytes, between its server
  // and client parts, and writes its id to `out`; `callback` gets each
  // message on it with `user`: on the server's game thread, on a client
  // on the thread that takes the server's chat. Mods of
  // `EnshwickSide_BOTH` only (`EnshwickStatus_REFUSED` for a
  // server mod), since both ends must run the mod.
  // Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*net_channel)(EnshwickModHandle me,
                                struct EnshwickStr name,
                                EnshwickNetMessageFn callback,
                                void *user,
                                uint32_t *out);
  // Sends `size` bytes at `data`, at most 8192, on the mod's `channel`:
  // to the server for `EnshwickNetMessage_SERVER`, else from the server to the
  // client of the player with account key `peer`, or to every client
  // that runs Enshwick for 0. Any thread. `EnshwickStatus_MISSING` while the
  // peer cannot take messages, `EnshwickStatus_BUSY` over the peer's rate,
  // `EnshwickStatus_BAD_ARGS` for too many bytes or another mod's channel.
  EnshwickStatus (*net_send)(EnshwickModHandle me,
                             uint32_t channel,
                             EnshwickPlayerKey peer,
                             const uint8_t *data,
                             uint32_t size);
  // `EnshwickStatus_OK` when `peer` takes messages: a player's client that
  // runs Enshwick, on the server; the server, for `EnshwickNetMessage_SERVER`
  // on a client. `EnshwickStatus_MISSING` otherwise. Any thread.
  EnshwickStatus (*net_ready)(EnshwickModHandle me, EnshwickPlayerKey peer);
  // Hands the loader the schema of the mod's settings file `file`, its
  // name without `.toml`: `schema`, the TOML text of a
  // settings schema (the format the book's settings chapter gives), and
  // `defaults`, the file
  // written from the defaults with their comments. As each world loads,
  // the loader writes the file from `defaults` beside the save, or adds
  // the keys it lacks and removes the ones the schema lacks. The console
  // and the Mods screen read and check values against `schema`.
  // `EnshwickStatus_BAD_ARGS` for text that is not a schema or a file name
  // that is not lowercase letters, digits, `-` and `_`;
  // `EnshwickStatus_REFUSED` for a file another schema names. Any thread, any time; a
  // schema handed while a world runs is written as the next world loads.
  EnshwickStatus (*config_schema)(EnshwickModHandle me,
                                  struct EnshwickStr file,
                                  struct EnshwickStr schema,
                                  struct EnshwickStr defaults);
  // Makes `callback` with `user` the mod's check of a new text of its
  // settings file `file`, which the loader runs before it writes a change
  // from the console or the Mods screen; a refusal keeps the file.
  // `EnshwickStatus_REFUSED` for a file that has a check. Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*config_check)(EnshwickModHandle me,
                                 struct EnshwickStr file,
                                 EnshwickConfigCheckFn callback,
                                 void *user);
  // Hands the loader the mod's texts, a TOML table per language code as
  // the book's texts chapter gives them, for the loader's own screens: the
  // Mods screen labels the mod's settings with them. A later call
  // replaces them. Any thread.
  EnshwickStatus (*texts)(EnshwickModHandle me, struct EnshwickStr toml);
  // Defines the mod event `spec` describes, named `<id>:<name>` after
  // the defining mod, and writes the id `EnshwickHostApi.send_event` takes;
  // `EnshwickStatus_REFUSED` for a name outside the mod's id or defined
  // twice. Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*define_event)(EnshwickModHandle me,
                                 const struct EnshwickEventSpec *spec,
                                 uint32_t *out);
  // Calls `callback` with `user` and the event's data on every mod event
  // `spec` names, once its definition, checked against `spec`'s layout
  // when every mod is initialised, matches. `EnshwickStatus_REFUSED` for an event
  // of another loaded mod whose `EnshwickManifest.exports`
  // lack its name. Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*subscribe_mod_event)(EnshwickModHandle me,
                                        const struct EnshwickEventSpec *spec,
                                        EnshwickModEventFn callback,
                                        void *user);
  // Hands `data` to every subscriber of the mod's own event `event`, on
  // this thread, before it returns; reaches no one before every mod is
  // initialised. `EnshwickStatus_REFUSED` for an event another mod defined.
  // Any thread.
  EnshwickStatus (*send_event)(EnshwickModHandle me, uint32_t event, const void *data);
  // Declares the mod's content renames, the `count` entries at `renames`,
  // copied: content saved under a rename's `from` reads as the content
  // now under its `to`. Each `to` is the GUID of a content id the mod
  // declared with `declare_content` before, of
  // the game's own content, or the `from` of another of its renames, as
  // in a chain. No `from` is a GUID another mod declared. `EnshwickStatus_REFUSED`
  // for a second call, a rename to itself, renames in a cycle, a `from`
  // another mod declared, or any other `to`, with a warning naming the mod
  // that declared it; `EnshwickStatus_BAD_ARGS` for a null `renames` with a nonzero
  // `count`. Allowed during `enshwick_mod_init` only. A mod that declares
  // a `from` later keeps it: this mod's rename from it is dropped, a chain
  // of its renames through it skips it, the resources this mod serves
  // under it from another GUID's base are dropped, and a warning names
  // both mods.
  EnshwickStatus (*rename_content)(EnshwickModHandle me,
                                   const struct EnshwickContentRename *renames,
                                   uint32_t count);
  // The mod's folder in `mods/`, which holds its installation; the path
  // stays valid for the process. Any thread.
  EnshwickStatus (*mod_dir)(EnshwickModHandle me, struct EnshwickStr *out);
  // The game language of the reader, as the game numbers them. On a
  // server, while the loader calls the
  // mod about one player (an action, a settings check, the player's mod
  // message, a building event of that player's character) on that
  // thread, the language that player's game named when
  // it joined, always one the game knows; else `0`, the fallback. On a
  // client, the language it last asked a text in, `0` before its first
  // text. Any thread; the answer holds for the calling thread.
  EnshwickStatus (*language)(EnshwickModHandle me, uint32_t *out);
  // Defines the extension point `spec` describes, named
  // `<id>:<point>` after the defining mod, and writes its id and the
  // count of its live links to `out`; `EnshwickStatus_REFUSED` for a name
  // outside the mod's id or defined twice. Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*define_point)(EnshwickModHandle me,
                                 const struct EnshwickPointSpec *spec,
                                 struct EnshwickPointHandle *out);
  // Attaches `callback` with `user` to the extension point `link`
  // names, in the order it asks for. Matched against the point's
  // definition once every mod is initialised; a point no running mod
  // defines, or one whose layout or mode differs, leaves the link inert.
  // `EnshwickStatus_REFUSED` for a point of another loaded mod whose exports
  // (`EnshwickManifest.exports`) lack its kind and name, or of
  // a required mod that does not define it as `link` declares it. Allowed
  // during `enshwick_mod_init` only.
  EnshwickStatus (*attach_point)(EnshwickModHandle me,
                                 const struct EnshwickPointLink *link,
                                 EnshwickPointFn callback,
                                 void *user);
  // Runs the live links of the mod's own extension point `point` over
  // `value`, which holds the mod's own answer, on this thread, and writes
  // to `verdict` whether the answer was kept, changed or refused.
  // `EnshwickStatus_REFUSED` for a point another mod defined. Any thread.
  EnshwickStatus (*run_point)(EnshwickModHandle me,
                              uint32_t point,
                              const void *input,
                              void *value,
                              EnshwickPointVerdict *verdict);
  // Up to `capacity` of the running build's game event types, and how
  // many there are, as every list answers: every reflected type that
  // derives from the engine's event base, in no set order. The names are
  // the loader's, valid for the rest of the process. A handle the loader
  // never gave out gives `EnshwickStatus_BAD_ARGS` and a mod that is off
  // `EnshwickStatus_FAULTED`, with nothing written. Any thread.
  EnshwickStatus (*event_types)(EnshwickModHandle me,
                                struct EnshwickEventTypeInfo *out,
                                uint32_t entry_size,
                                uint32_t capacity,
                                uint32_t *count);
  // Reports whether the mod's feature `name` runs on this game build:
  // off for the reason `off`, or on when `off` is empty. The loader
  // logs it and shows it in the status summary and on the Mods screen; a
  // later report of the same name replaces the earlier one. The name
  // `commands` is the mod's typed lines: while it is off,
  // `command_list` leaves the mod's commands out.
  // `EnshwickStatus_BAD_ARGS` for an empty name. Both strings are borrowed for
  // the call. Any thread.
  EnshwickStatus (*feature)(EnshwickModHandle me, struct EnshwickStr name, struct EnshwickStr off);
  // Calls `callback` with `user` before every wait of the engine's event
  // loop on its handles, on the thread that waits, and writes to `found`
  // which waits it hooked (`EnshwickLoopWaitSites`); a wait this build lacks,
  // whose code is not what its table expects, or that the mod hooked
  // already is left out. `EnshwickStatus_BAD_ARGS` for a null `found`, before
  // anything is hooked. Handlers run in load order. Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*loop_wait)(EnshwickModHandle me,
                              EnshwickLoopWaitFn callback,
                              void *user,
                              EnshwickLoopWaitSites *found);
  // Calls `callback` with `user` once a pass where the server's game loop
  // has picked its wait before the next pass, in nanoseconds, which the
  // handler may change; writes 1 to `found` when the loader hooked the
  // place, else 0. `EnshwickStatus_BAD_ARGS` for a null `found`, before
  // anything is hooked. Handlers run in load order, each on the last one's wait. On
  // the game thread of the server loop. Allowed during `enshwick_mod_init`
  // only.
  EnshwickStatus (*server_wait)(EnshwickModHandle me,
                                EnshwickServerWaitFn callback,
                                void *user,
                                uint8_t *found);
  // Caps the worker threads of the task system's default and background
  // groups (`foreground` and `background`) as it starts, 0 leaving a group as the game sizes it; with
  // several mods the lowest cap wins. Writes 1 to `found` when a cap is
  // hooked and the task system starts after the hooks arm, which only the
  // dedicated server does, else 0. `EnshwickStatus_BAD_ARGS` for a null
  // `found`, before anything is hooked. Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*task_workers)(EnshwickModHandle me,
                                 uint32_t foreground,
                                 uint32_t background,
                                 uint8_t *found);
  // Serves the call `spec` describes with `callback` and `user`, named
  // `<id>:<call>` after the serving mod; `EnshwickStatus_REFUSED` for another
  // name, or a name and major served twice, `EnshwickStatus_BAD_ARGS` for a
  // spec that does not read. Allowed during `enshwick_mod_init` only:
  // `EnshwickStatus_TOO_LATE` once every mod is initialised.
  EnshwickStatus (*serve_call)(EnshwickModHandle me,
                               const struct EnshwickCallSpec *spec,
                               EnshwickCallFn callback,
                               void *user);
  // Asks for the call `spec` describes and writes its id to `out`.
  // Matched against the served call by name and major once every mod is
  // initialised: the input and the answer must agree field by field and
  // in size, or the call stays unmatched with the reason in the log.
  // `EnshwickStatus_REFUSED` when the mod asked for the name and major already
  // with another spec, for a call of another loaded mod whose exports
  // (`EnshwickManifest.exports`) lack its kind and name, or of
  // a required mod that does not serve it as `spec` declares it;
  // `EnshwickStatus_BAD_ARGS` for a spec that does not read or a null `out`.
  // Allowed during `enshwick_mod_init` only: `EnshwickStatus_TOO_LATE` once
  // every mod is initialised.
  EnshwickStatus (*find_call)(EnshwickModHandle me,
                              const struct EnshwickCallSpec *spec,
                              uint32_t *out);
  // Runs the call `id` from `EnshwickHostApi.find_call` on this thread: the
  // server's handler reads `input` and writes the answer to `output`,
  // both as large as the spec says. `EnshwickStatus_MISSING` while the call
  // is unmatched, `EnshwickStatus_UNAVAILABLE` before every mod is initialised, `EnshwickStatus_REFUSED`
  // when the serving mod or its handler is off, `EnshwickStatus_BAD_ARGS` for
  // a null pointer or an id the mod was not given, or what the handler
  // answers. Any thread.
  EnshwickStatus (*call)(EnshwickModHandle me, uint32_t id, const void *input, void *output);
  // Calls `callback` with `user`, `EnshwickEvent_TICKED` and a `EnshwickTickData`
  // on the server's game thread after a step, once `every_ns` of wall
  // time have passed since its last call, and on the first step after it
  // subscribed; 0 calls it every step. It is also called once at each
  // world's start, before `EnshwickEvent_WORLD_LOADED`, with that game in
  // `EnshwickTickData.game`, unless its period is
  // `UINT64_MAX`. Steps the handler skips cost it nothing. The data lives for the call; the loader keeps `user` and
  // never frees it. Writes the handler's handle to `out`, the caller's,
  // writable for four bytes, on `EnshwickStatus_OK` only. `EnshwickStatus_BAD_ARGS`
  // for a null `out`; `EnshwickStatus_TOO_LATE` outside `enshwick_mod_init`.
  // The handler runs until the mod is off; a panic in it turns the mod
  // off. It may call
  // `EnshwickHostApi.set_tick_every` on itself. `EnshwickStatus_REFUSED` once the
  // loader holds `UINT32_MAX` handlers.
  EnshwickStatus (*subscribe_tick)(EnshwickModHandle me,
                                   uint64_t every_ns,
                                   EnshwickEventFn callback,
                                   void *user,
                                   EnshwickTickHandle *out);
  // Sets the period of the mod's periodic tick handler `handle` to
  // `every_ns`, 0 for every step, from the next step on: the next
  // call comes `every_ns` after its last one, or on the next step when
  // that time has passed. `EnshwickStatus_REFUSED` for another mod's handle;
  // `EnshwickStatus_MISSING` for a handle the loader never gave out. Any
  // thread, the handler itself included.
  EnshwickStatus (*set_tick_every)(EnshwickModHandle me,
                                   EnshwickTickHandle handle,
                                   uint64_t every_ns);
  // Writes `value`, one TOML value, as the setting `key` of the mod's own
  // settings file `file` in the world this process runs, as the console
  // does: checked against the file's schema and by the mod's
  // `EnshwickHostApi.config_check`, written atomically with the file's
  // comments kept, then raised as a settings change. A secret setting
  // too, whose value the log never shows. Any thread; writes of settings
  // wait for each other, and a write from the mod's settings handler
  // raises the change again. The strings are borrowed for the call.
  // `EnshwickStatus_UNAVAILABLE` while no world runs here, as on a client in
  // another's world; `EnshwickStatus_MISSING` for a file or key the mod's
  // settings lack; `EnshwickStatus_BAD_ARGS` for a value the setting does not
  // accept or a string that is not UTF-8; `EnshwickStatus_REFUSED` when the
  // mod's check refuses the file with the value, the file does not read
  // or write, or the write comes from inside the mod's own check or
  // nested more than four deep through its settings handler. On an error the file stays as it was and the loader
  // logs why.
  EnshwickStatus (*set_config)(EnshwickModHandle me,
                               struct EnshwickStr file,
                               struct EnshwickStr key,
                               struct EnshwickStr value);
  // As `EnshwickHostApi.subscribe_game_event` for the events of the client's own world,
  // on the thread that steps it, right after the engine moved them; no
  // filter, veto or item action runs on them. One move hands the
  // client's callbacks at most 256 events and 64 KiB of them, the rest
  // dropped with a warning logged once. On a dedicated server
  // `EnshwickStatus_OK` without keeping `callback`, which never runs there.
  EnshwickStatus (*subscribe_client_game_event)(EnshwickModHandle me,
                                                uint32_t type_hash,
                                                EnshwickGameEventFn callback,
                                                void *user);
  // Serves the resource `key` as `patch` makes it from the resource
  // `base`, built the first time the game reads it: a new resource when
  // the keys differ, a changed vanilla one when they are equal. Allowed
  // during `enshwick_mod_init` only. Several mods may serve one key:
  // their patches run in load order, each on the one before's result.
  // A `key` whose GUID differs from `base`'s names new content: its GUID
  // is that of a content id the mod declared with
  // `declare_content`, unless the game's archive
  // holds `key` or the GUID is the `from` of one of the mod's
  // `rename_content` renames; `EnshwickStatus_REFUSED`
  // for any other GUID, with a warning naming the mod that declared it.
  EnshwickStatus (*serve_resource)(EnshwickModHandle me,
                                   const struct EnshwickResourceKey *key,
                                   const struct EnshwickResourceKey *base,
                                   EnshwickResourcePatchFn patch,
                                   void *user);
  // Adds the text of `LocaTag` `id` in `language` (`LanguageId`, 0 for any
  // language without its own); the client shows it wherever the game asks
  // for that tag. Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*add_text)(EnshwickModHandle me,
                             uint32_t id,
                             uint32_t language,
                             struct EnshwickStr text);
  // Opens the item data kind `id`, `name` for the mod's own
  // (`<mod>:<name>`) or `mod:name` for any mod's, each side 1 to 64 of
  // `[a-z0-9_-]`, and writes its index to `out`; every mod gets the same
  // index for one id. Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*define_item_data)(EnshwickModHandle me, struct EnshwickStr id, uint32_t *out);
  // Copies the value of kind `kind` on the item entity `pide` holding
  // item `item` into `out`, at most `capacity` bytes, and writes its
  // length to `len`; `EnshwickStatus_MISSING` when it has none or its entity
  // holds another item, `EnshwickStatus_TOO_SMALL` when `capacity` is short,
  // with `len` written. On the server the server's values, on a client
  // what the server sent. Any thread.
  EnshwickStatus (*item_data_get)(EnshwickModHandle me,
                                  uint32_t kind,
                                  EnshwickEntity pide,
                                  EnshwickItemId item,
                                  uint8_t *out,
                                  uint32_t capacity,
                                  uint32_t *len);
  // Sets the value of kind `kind` on the item entity `pide` holding item
  // `item` to `len` bytes at `data`, at most 512, or removes it for 0;
  // the clients that run Enshwick get the change. `EnshwickStatus_UNAVAILABLE`
  // on a client alone or before the world has loaded.
  // Any thread.
  EnshwickStatus (*set_item_data)(EnshwickModHandle me,
                                  uint32_t kind,
                                  EnshwickEntity pide,
                                  EnshwickItemId item,
                                  const uint8_t *data,
                                  uint32_t len);
  // Serves `len` bytes at `data` as game content, the pixels of a
  // texture the mod serves, and writes the content hash a resource names
  // them by to `out`; the same bytes give the same hash. The loader
  // keeps the bytes for the life of the process: bytes served before
  // answer their hash without a second copy, and every other serve adds
  // to memory until the process ends. Any thread.
  EnshwickStatus (*content_serve)(EnshwickModHandle me,
                                  const uint8_t *data,
                                  uint32_t len,
                                  uint32_t (*out)[4]);
  // Copies the game's own content `hash` from its archive into `out`, at
  // most `capacity` bytes, and writes its length to `len`;
  // `EnshwickStatus_MISSING` for a hash the archive lacks or keeps
  // compressed, `EnshwickStatus_TOO_SMALL` when `capacity` is short or `out`
  // null, with `len` written, so a null `out` asks for the length. Reads the archive's files, never the game's memory.
  // Any thread.
  EnshwickStatus (*content_read)(EnshwickModHandle me,
                                 const uint32_t (*hash)[4],
                                 uint8_t *out,
                                 uint32_t capacity,
                                 uint32_t *len);
  // `EnshwickStatus_OK` when this client shows the UI images mods serve: the
  // content and UI registry links run and the registry knows the served
  // images; `EnshwickStatus_UNAVAILABLE` otherwise, and always on a server. Any
  // thread.
  EnshwickStatus (*icons_ready)(EnshwickModHandle me);
  // Declares the content id `id`, `<mod>:<name>` in the mod's own
  // namespace, and writes its GUID to `out`; `EnshwickStatus_REFUSED` for a
  // malformed id, one outside the namespace, or one whose game id
  // another content id has, which turns the mod off with both ids and
  // both mods named. Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*declare_content)(EnshwickModHandle me,
                                    struct EnshwickStr id,
                                    struct EnshwickGuid *out);
  // Opens the entity data kind `id`, as `EnshwickHostApi.define_item_data` opens
  // an item kind, and writes its index to `out`; with
  // `EnshwickEntityDataFlags_SENT` the clients that run Enshwick get its
  // values. `EnshwickHostApi.item_data_get` and `EnshwickHostApi.set_item_data`
  // read and write it with `item` 0; a set runs on the server's game
  // thread. `EnshwickStatus_REFUSED` for an id opened as an item kind. Allowed
  // during `enshwick_mod_init` only.
  EnshwickStatus (*define_entity_data)(EnshwickModHandle me,
                                       struct EnshwickStr id,
                                       EnshwickEntityDataFlags flags,
                                       uint32_t *out);
  // Copies the game's own resource `key` from its archive into `out`, at
  // most `capacity` bytes, as the game reads it before it resolves its
  // references, and writes its length to `len`; `EnshwickStatus_MISSING` for
  // a key the archive lacks, `EnshwickStatus_TOO_SMALL` when `capacity` is
  // short, with `len` written. Reads the archive's files, never the
  // game's memory. Any thread.
  EnshwickStatus (*resource_read)(EnshwickModHandle me,
                                  const struct EnshwickResourceKey *key,
                                  uint8_t *out,
                                  uint32_t capacity,
                                  uint32_t *len);
  // Writes the game's own text of tag `id` in `language`, as the game
  // numbers them (0 for English), to `out`, valid for the life of the
  // process; a text missing in `language` falls back to English as the
  // game's does. Read from the game's archive, the first call for a
  // language reads its table. `EnshwickStatus_MISSING` for a tag the game
  // lacks, and always on a server, whose archive has no texts. Any
  // thread.
  EnshwickStatus (*game_text)(EnshwickModHandle me,
                              uint32_t id,
                              uint32_t language,
                              struct EnshwickStr *out);
  // During `enshwick_mod_init`, by a mod that serves `key`: keeps the
  // base of `key` when it is first built, so it can be built again.
  // Afterwards: builds `key` again from that base with the patches of
  // every mod that built the bytes the game holds, a mod turned off
  // since included, as values they read changed, and writes the
  // bytes that differ over the resource the game holds; `EnshwickStatus_OK`
  // with `key` itself left while it was not built yet, as its first
  // build takes the values in force. Then every key whose build read
  // `key` as served is built again the same way. Any thread.
  // `EnshwickStatus_MISSING` when its base was not kept, `EnshwickStatus_REFUSED`
  // when the new build differs in length, which the game's copy cannot
  // take, `EnshwickStatus_BAD_ARGS` for a key the mod does not serve,
  // `EnshwickStatus_BUILDING` from a patch of `key` itself, whose build would
  // wait on its own. The
  // loader frees what each rebuild's patches built in a
  // `resource_buffer`.
  EnshwickStatus (*rebuild_resource)(EnshwickModHandle me, const struct EnshwickResourceKey *key);
  // Adds the calling mod's rule that disables recipes (`disable` 1), or
  // takes its own rule of that kind and name back (`disable` 0). `name`
  // names a recipe, an item every recipe of which is disabled, or a
  // workshop, as `kind` says, in the forms the loader's recipe file
  // takes; the loader resolves it against the recipe registry and logs a
  // name it cannot resolve. The server's recipe file wins over every
  // mod's rules. Any thread; applied within a second.
  EnshwickStatus (*recipe_rule)(EnshwickModHandle me,
                                EnshwickRecipeRuleKind kind,
                                struct EnshwickStr name,
                                uint8_t disable);
  // Writes 1 to `out` when the recipe with game id `recipe` is disabled
  // in this process now, else 0: on a client joined to a server, by the
  // server's rules. Any thread.
  EnshwickStatus (*recipe_disabled)(EnshwickModHandle me, EnshwickRecipeId recipe, uint8_t *out);
  // Writes to `out` the kind of the recipes that make the item `item`,
  // from the item's static data; `EnshwickStatus_MISSING` while the game has
  // not loaded the item, which on a dedicated server comes after its
  // recipe registry is first built, and while the loader serves no
  // resource. Any thread.
  EnshwickStatus (*recipe_kind)(EnshwickModHandle me, EnshwickItemId item, EnshwickRecipeKind *out);
  // Declares that the calling mod multiplies `rate`, so the loader arms
  // what the rate needs; its factor is 1 until
  // `EnshwickHostApi.set_rate`. `EnshwickStatus_UNAVAILABLE` on a build that lacks
  // what the rate needs, `EnshwickStatus_BAD_ARGS` for a rate this loader
  // does not know, `EnshwickStatus_REFUSED` for a rate clients count too in a
  // `EnshwickSide_SERVER` mod. A second declaration of the same rate is
  // `EnshwickStatus_OK`. `EnshwickStatus_TOO_LATE` after `enshwick_mod_init`,
  // `EnshwickStatus_FAULTED` for a mod that is off. Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*rate)(EnshwickModHandle me, EnshwickRateId rate);
  // Puts `factor` in force as the calling mod's factor for `rate`: the
  // game uses the product of every declaring mod's factor, held to 0 to
  // 100, from the next time it reads the rate. `EnshwickStatus_BAD_ARGS` for
  // a factor that is not finite or lies outside 0 to 100, or a rate the
  // mod did not declare, `EnshwickStatus_FAULTED` for a mod that is off. Any
  // thread; neither allocates nor locks once every mod is initialised,
  // except for a recipe output rate, whose change builds the recipe
  // registry again.
  EnshwickStatus (*set_rate)(EnshwickModHandle me, EnshwickRateId rate, float factor);
  // Declares a change of a vanilla value, of one entry or of each item or
  // recipe a selection picks. The loader merges the changes
  // of every mod once their `init` ran: a set in load order, the last
  // winning and the overridden one reported in both mods' logs and the
  // conflict report; then every factor, multiplied. It builds the changed
  // resource the first time the game reads it, on each side from its own
  // mods. `EnshwickStatus_BAD_ARGS` for a null `change`, an unknown field or
  // operation and a value outside the field's range;
  // `EnshwickStatus_UNAVAILABLE` when the build does not reflect the field.
  //
  // With `out`, the change is live: the mod sets its value later with
  // `EnshwickHostApi.set_change`, and until then it changes nothing;
  // `change.op` and `change.value` are ignored, and the change's number
  // goes to `out`. The loader builds every resource a live change reaches
  // from the first read, so a later set takes effect in place.
  // `EnshwickStatus_BAD_ARGS` for an ingredient, whose change alters a
  // recipe's length, and `EnshwickStatus_REFUSED` for a field clients read in
  // a `EnshwickSide_SERVER` mod. During
  // `enshwick_mod_init` only, on its thread.
  EnshwickStatus (*change_data)(EnshwickModHandle me,
                                const struct EnshwickDataChange *change,
                                EnshwickChangeId *out);
  // Puts `value` in force for the calling mod's change `id` by `op`:
  // `EnshwickChangeOp_SET`, `EnshwickChangeOp_TIMES`, or `EnshwickChangeOp_RESET` to
  // take the mod's value out. The loader merges every mod's changes
  // again and builds each resource it patches anew in place; the game
  // reads the new value from then on. During `enshwick_mod_init` the
  // value counts from the first read. `EnshwickStatus_BAD_ARGS` for an id
  // the mod did not declare, an unknown operation, or a value the field
  // does not take; `EnshwickStatus_FAULTED` for a mod that is off;
  // `EnshwickStatus_TOO_LATE` when the value takes a restart, and the log says
  // why: it reaches something the loader did not patch at start (the
  // value is dropped and the old one stays in force), or a resource does
  // not build again in place (the value is in force for what did build
  // and for every first read, the rest until a restart). Any
  // thread; it allocates and builds resources, so it is no per-frame
  // call.
  EnshwickStatus (*set_change)(EnshwickModHandle me,
                               EnshwickChangeId id,
                               EnshwickChangeOp op,
                               double value);
  // Checks that the mod may name the content id `id`, `<mod>:<name>`:
  // `EnshwickStatus_OK` for one of its own, one of a mod that is not loaded,
  // and one whose mod's manifest exports `<name>` or the name it is a
  // part of (`locker` for `locker.recipe`); `EnshwickStatus_REFUSED` for any
  // other, with the id and the owner's exports logged;
  // `EnshwickStatus_BAD_ARGS` for a malformed id. Any thread.
  EnshwickStatus (*use_content)(EnshwickModHandle me, struct EnshwickStr id);
  // Up to `capacity` keys of the game's own resources of type
  // `type_hash`, or of every resource for 0, in the order their bytes lie
  // in the archive, and how many there are; read from the archive's files
  // at the first call and kept, as every list of plain values answers.
  // `EnshwickStatus_UNAVAILABLE` when the archive does not read. Any thread.
  EnshwickStatus (*resource_keys)(EnshwickModHandle me,
                                  uint32_t type_hash,
                                  struct EnshwickResourceKey *out,
                                  uint32_t capacity,
                                  uint32_t *count);
  // Up to `capacity` hashes of the game's own contents, in the archive's
  // order, and how many there are; `EnshwickHostApi.content_read` reads each,
  // as every list of plain values answers.
  // `EnshwickStatus_UNAVAILABLE` when the archive does not read. Any thread.
  EnshwickStatus (*content_hashes)(EnshwickModHandle me,
                                   uint32_t (*out)[4],
                                   uint32_t capacity,
                                   uint32_t *count);
  // Up to `capacity` reflected types of the running game, each a name
  // carries alone, and how many there are; built at the first call and
  // kept, as every list answers. The strings and arrays an
  // entry points at are the loader's, valid and unchanged for the life of
  // the process. `EnshwickStatus_UNAVAILABLE` when the game holds no reflection
  // data. Any thread.
  EnshwickStatus (*types)(EnshwickModHandle me,
                          struct EnshwickTypeEntry *out,
                          uint32_t entry_size,
                          uint32_t capacity,
                          uint32_t *count);
  // Writes the version string of the game's archive to `out`, such as
  // `1283531|^/game38/branches/release_1.0|2026-10-30T09:15:50.868196Z`:
  // the build number, the branch and when the build was made. The string
  // is the loader's, valid and unchanged for the life of the process.
  // `EnshwickStatus_UNAVAILABLE` when the archive does not read. Any thread.
  EnshwickStatus (*game_version)(EnshwickModHandle me, struct EnshwickStr *out);
  // Gives the crafting prop `prop` its index in the game's crafting prop
  // mask and writes it to `out`: the first bit the game's own props leave
  // free for the first prop any mod adds, then one more for each new prop
  // in call order, and the same index again for a prop given one before.
  // The loader reads the mask's bits and the game's props from the
  // running game, so a game patch that adds a prop needs no new mod.
  // `EnshwickStatus_REFUSED` when the mask is full, when the game's mask or
  // props do not read, or for a mod not of side both, whose bit clients
  // would lack, each of which turns the mod off naming the prop, and for
  // a `prop` whose content id the mod did not declare with
  // `declare_content`, with a warning naming the
  // mod that declared it; `EnshwickStatus_BAD_ARGS` for a null `prop` or
  // `out`. Allowed during
  // `enshwick_mod_init` only (`EnshwickStatus_TOO_LATE` after); `prop` and
  // `out` are borrowed for the call.
  EnshwickStatus (*crafting_prop)(EnshwickModHandle me,
                                  const struct EnshwickGuid *prop,
                                  uint8_t *out);
  // Writes to `out` how many bytes from `address` on, at most `max`,
  // lie in committed, unguarded, read-only pages: the run of such
  // regions that `address` starts, cut at `max`; 0 when `address` does
  // not lie in one. It reads no byte of that memory, so any `address` is
  // allowed. No thread writes those pages without changing their
  // protection, and the count holds only while nothing frees or protects
  // them, which the caller knows of the memory it asks about. For a
  // private run of 16 MiB or more the loader may answer from what it
  // found earlier on the same thread in the same world, without asking
  // Windows again, until the world goes away; smaller runs and mapped
  // views are asked afresh. `EnshwickStatus_BAD_ARGS` for a null `out`. Any thread,
  // re-entrant; `out` is borrowed for the call.
  EnshwickStatus (*read_only)(EnshwickModHandle me, uint64_t address, uint32_t max, uint32_t *out);
  // Copies resource `key` as the game's first read would build it now
  // into `out`, at most `capacity` bytes, and writes its length to
  // `len`. A key mods serve, or the key a mod renamed it to, is built
  // from its base in the archive with the patches of every mod on, on
  // the calling thread; a patch that fails is skipped and its mod stays
  // on. The build is kept nowhere, so a mod turned off since the game's
  // own build is left out, and the loader frees each output a patch
  // built in a `resource_buffer` once it has
  // copied the bytes. When every patch fails, a changed vanilla key
  // copies as the archive has it. Any other key, and every key while
  // served resources are off, copies as the game opens it from the
  // archive. A game build that reads a served key here is built again
  // whenever that key is rebuilt. `EnshwickStatus_MISSING` for a key neither
  // served nor in the archive, or a new key whose every patch failed;
  // `EnshwickStatus_TOO_SMALL` when `capacity` is short or `out` null, with
  // `len` written, and for a short `out` the build kept for this
  // thread's next call with the same key; `EnshwickStatus_REFUSED` during `enshwick_mod_init`, while
  // not every patch is in; `EnshwickStatus_BUILDING` from a patch for a key
  // whose patches run on this thread, which would build it again without
  // end. Any thread; re-entrant from a patch for another key. `key` and
  // `len` are borrowed for the call.
  EnshwickStatus (*resource_served)(EnshwickModHandle me,
                                    const struct EnshwickResourceKey *key,
                                    uint8_t *out,
                                    uint32_t capacity,
                                    uint32_t *len);
  // Writes to `out` a pointer to `len` zeroed, writable bytes the loader
  // owns, for the output of the `EnshwickResourcePatchFn`
  // running on the calling thread: the patch builds its output there and
  // returns where it lies from the same call, the only place an output
  // may lie. The bytes stay valid until the build that runs the patch
  // ends; the game's first build keeps the buffer of the last patch's
  // output for the process, as the game holds it, and the loader frees
  // every other buffer, a rebuild's and a served read's
  // (`resource_served`) included, when the
  // build ends. A mod that is off gets one too, as a rebuild runs the
  // patches of the mods that built what the game holds.
  // `EnshwickStatus_REFUSED` outside a patch or when `len` bytes do not
  // allocate; `EnshwickStatus_BAD_ARGS` for a null `out` or a handle the
  // loader never gave out. The patch's thread only, re-entrant from a
  // patch for another key; `out` is borrowed for the call.
  EnshwickStatus (*resource_buffer)(EnshwickModHandle me, uint32_t len, uint8_t **out);
  // Writes to `out` the content id, `<mod>:<name>`, that a loaded mod
  // declared for exactly the GUID at `guid`, not only for its game id: a
  // mod's item, recipe or other content. The text lives for the process.
  // Any thread. `EnshwickStatus_MISSING` for a GUID no mod declared, such as a
  // vanilla item's; `EnshwickStatus_BAD_ARGS` for a null `guid` or `out`.
  EnshwickStatus (*content_name)(EnshwickModHandle me,
                                 const struct EnshwickGuid *guid,
                                 struct EnshwickStr *out);
  // The connected player whose character is `entity`; on the server's game
  // thread only.
  EnshwickStatus (*player)(EnshwickModHandle me,
                           EnshwickEntity entity,
                           struct EnshwickPlayerInfo *out);
  // Up to `capacity` connected players with characters in the world, and
  // how many there are, as every list answers (see `EnshwickHostApi`); on the
  // server's game thread only.
  EnshwickStatus (*players)(EnshwickModHandle me,
                            struct EnshwickPlayerInfo *out,
                            uint32_t entry_size,
                            uint32_t capacity,
                            uint32_t *count);
  // Sends `text` to the chat of the player whose account key is `to`, from
  // any thread; the server posts it after the next step. Lines past 1024
  // bytes are cut. Key 0, which names the console or a remote caller in
  // a request and no player, answers `EnshwickStatus_BAD_ARGS` and sends
  // nothing: answer such a request in its outcome, and send to everyone
  // with `EnshwickHostApi.chat_all`.
  EnshwickStatus (*chat)(EnshwickModHandle me, EnshwickPlayerKey to, struct EnshwickStr text);
  // Sends `text` to the chat of every connected player, from any thread;
  // the server posts it after the next step. Lines past 1024 bytes are
  // cut.
  EnshwickStatus (*chat_all)(EnshwickModHandle me, struct EnshwickStr text);
  // Gives `entity` the name `text`, as a player's rename does: shown on
  // hover, sent to every client and saved with the world; cut at 255
  // bytes, removed when empty. On the server's game thread only.
  EnshwickStatus (*set_label)(EnshwickModHandle me, EnshwickEntity entity, struct EnshwickStr text);
  // Calls `callback` with `user` before the server applies a player's
  // rename of an entity; any filter returning `FILTER_REFUSE` refuses it. Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*rename_filter)(EnshwickModHandle me,
                                  EnshwickRenameFilterFn callback,
                                  void *user);
  // Writes the static data of the item `item`, as far as the `EnshwickHeader`
  // `out` starts with says the caller's struct reaches;
  // `EnshwickStatus_MISSING` for an unknown item, `EnshwickStatus_UNAVAILABLE` while
  // no world runs.
  // Any thread.
  EnshwickStatus (*item_info)(EnshwickModHandle me,
                              EnshwickItemId item,
                              struct EnshwickItemTypeInfo *out);
  // Asks the loader to follow the Flame Altars of the server's world, for
  // `EnshwickHostApi.altars` and `EnshwickEvent_ALTARS_CHANGED`. Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*request_altars)(EnshwickModHandle me);
  // Up to `capacity` altars of the server's world and how many there are.
  // Entries are `entry_size` bytes apart, the caller's
  // `sizeof(EnshwickAltarInfo)`; the loader fills what both sides know of
  // each and its own `EnshwickHeader`. Any thread; empty while no world runs.
  // `EnshwickStatus_REFUSED` for a mod that did not open altars.
  EnshwickStatus (*altars)(EnshwickModHandle me,
                           struct EnshwickAltarInfo *out,
                           uint32_t entry_size,
                           uint32_t capacity,
                           uint32_t *count);
  // Calls `callback` with `user` when the game refuses a placement whose
  // box does not lie in one building zone; any filter returning
  // `FILTER_YES` accepts it after all. Runs on both sides, since a client predicts placement.
  // Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*building_zone_filter)(EnshwickModHandle me,
                                         EnshwickBuildingZoneFilterFn callback,
                                         void *user);
  // Writes how many altar zones and bases the game's last grouping found;
  // `EnshwickStatus_UNAVAILABLE` before the first one or without the raised
  // limit.
  // Any thread.
  EnshwickStatus (*base_zones)(EnshwickModHandle me, uint32_t *zones, uint32_t *bases);
  // Up to `capacity` entities of the server's world that have a component
  // of the reflected type `type_hash` and a place in the world, and how
  // many there are. On the server's game thread only.
  EnshwickStatus (*entities)(EnshwickModHandle me,
                             uint32_t type_hash,
                             struct EnshwickEntityPlace *out,
                             uint32_t capacity,
                             uint32_t *count);
  // Asks the server to destroy `entity` after the systems of the next
  // step; `EnshwickStatus_REFUSED` for a player's character,
  // `EnshwickStatus_MISSING` for an entity not in the world,
  // `EnshwickStatus_UNAVAILABLE` where no server runs or on a build without the
  // command. On the server's game thread only.
  EnshwickStatus (*destroy)(EnshwickModHandle me, EnshwickEntity entity);
  // Asks the server to save the world now, as its timed saves do;
  // `EnshwickEvent_WORLD_SAVED` follows once the save is on disk. A save under
  // way counts as asked; `EnshwickStatus_REFUSED` while the world may not be
  // saved, as before it has loaded. On the server's game thread only.
  EnshwickStatus (*save_world)(EnshwickModHandle me);
  // Writes the key of the world the server runs: its save file's name
  // without `.ksd`, such as `world0`, `[A-Za-z0-9_-]`, valid for the
  // process.
  // `EnshwickStatus_UNAVAILABLE` before the server has read or written the
  // world.
  // Any thread.
  EnshwickStatus (*world_key)(EnshwickModHandle me, struct EnshwickStr *out);
  // A writable pointer to `entity`'s component of the reflected type
  // `type_hash`, valid until the game thread returns to the game; the
  // loader marks the component changed, as the game's own writes do, so
  // the game sends it to clients and saves it. `EnshwickStatus_REFUSED` for a
  // copy the game keeps for clients (`Network*`, `Client*`), which it
  // overwrites; `EnshwickStatus_STATIC_HALF` for the static half of an ECS
  // type, the template's set-up data;
  // `EnshwickStatus_MISSING` when the entity lacks the component or is not in
  // the world; `EnshwickStatus_UNAVAILABLE` where no server runs or on a build
  // without writes. On the server's game thread only.
  EnshwickStatus (*component_write)(EnshwickModHandle me,
                                    EnshwickEntity entity,
                                    uint32_t type_hash,
                                    void **out);
  // Shows `text`, cut at 255 bytes, on the screen of the player with
  // account key `key`, as the game shows its own "can't do that"
  // messages; `EnshwickStatus_UNAVAILABLE` when the player's client lacks
  // Enshwick and cannot show it, `EnshwickStatus_BUSY` when the server's line
  // queue is full, `EnshwickStatus_REFUSED` when it sends no lines to one
  // player, and nothing is sent after any of them. On the server's game
  // thread only.
  EnshwickStatus (*notify)(EnshwickModHandle me, EnshwickPlayerKey key, struct EnshwickStr text);
  // Writes the clock of the server's world, as far as the `EnshwickHeader`
  // `out` starts with says the caller's struct reaches. On the server's
  // game thread only.
  EnshwickStatus (*time_of_day)(EnshwickModHandle me, struct EnshwickDaytimeInfo *out);
  // Sets the clock of the server's world to `time_ns`, game nanoseconds of
  // the day, taken modulo 24 hours; clients follow after the next step.
  // On the server's game thread only.
  EnshwickStatus (*set_time_of_day)(EnshwickModHandle me, int64_t time_ns);
  // Lets the mod change how fast the clock runs, with
  // `EnshwickHostApi.set_time_speed`; every mod may, each with a factor of
  // its own. Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*request_time_speed)(EnshwickModHandle me);
  // Sets the mod's factor on the clock's speed, from 0 (stopped) to 1000;
  // the clock runs at the game's own speed times every mod's factor.
  // `EnshwickStatus_REFUSED` for a mod without
  // `EnshwickHostApi.request_time_speed`, `EnshwickStatus_BAD_ARGS` for a factor
  // out of range. A hosted game's end puts every factor back at 1. On
  // the server's game thread only.
  EnshwickStatus (*set_time_speed)(EnshwickModHandle me, float factor);
  // Copies the server's live game settings, the game's `GameSettings` type,
  // into `out`, `size` bytes, which must be the type's reflected size
  // (`EnshwickStatus_BAD_ARGS` otherwise), and writes the preset the server
  // runs (`GameSettingsPreset`) to `preset`. On the server's game thread
  // only.
  EnshwickStatus (*game_settings)(EnshwickModHandle me,
                                  uint8_t *out,
                                  uint32_t size,
                                  uint32_t *preset);
  // Replaces the server's live game settings with `data`, `size` bytes of
  // the type's whole reflected layout, as the game's admin command does:
  // systems see them from the next step and every client gets them; the
  // preset becomes custom. Until the server stops: nothing writes them to
  // disk.
  //
  // With `mask`, `size` bytes as well, the fields with a byte set in it
  // stay with the running world: the loader applies them again each time
  // the world loads, by the end of its first step;
  // `EnshwickStatus_UNAVAILABLE` while no world runs. A null `data`, with a
  // null `mask` and a `size` of 0, forgets the fields kept for the running
  // world and puts back the values it loaded with.
  //
  // `EnshwickStatus_BAD_ARGS` for a wrong size, a day or night length of zero
  // or less, or a `mask` or `size` with a null `data`. On the server's
  // game thread only.
  EnshwickStatus (*set_game_settings)(EnshwickModHandle me,
                                      const uint8_t *data,
                                      const uint8_t *mask,
                                      uint32_t size);
  // Up to `capacity` buff and debuff types of the running game, and how
  // many there are, as every list answers. On the server's game thread
  // only.
  EnshwickStatus (*buff_types)(EnshwickModHandle me,
                               struct EnshwickBuffTypeInfo *out,
                               uint32_t entry_size,
                               uint32_t capacity,
                               uint32_t *count);
  // Up to `capacity` buffs `entity` has, and how many there are, as every
  // list answers; `EnshwickStatus_MISSING` for an entity that holds no buffs.
  // On the server's game thread only.
  EnshwickStatus (*buffs)(EnshwickModHandle me,
                          EnshwickEntity entity,
                          struct EnshwickActiveBuff *out,
                          uint32_t entry_size,
                          uint32_t capacity,
                          uint32_t *count);
  // Gives an entity a buff, as the game restores a character's saved
  // buffs: at once, for the duration asked. One the entity has already
  // is renewed to that duration. `EnshwickStatus_MISSING` for an entity that
  // holds no buffs, `EnshwickStatus_BAD_ARGS` for an unknown type,
  // `EnshwickStatus_REFUSED` for a type removed in this step, which the game
  // clears only in the next. On the server's game thread only.
  EnshwickStatus (*apply_buff)(EnshwickModHandle me, const struct EnshwickBuffRequest *request);
  // Removes every buff of type `id` from `entity`, as the game cleanses
  // one: it goes in the next step. Writes 1 to `removed` when the entity
  // had it, else 0; `EnshwickStatus_MISSING` for an entity that holds no
  // buffs. On the server's game thread only.
  EnshwickStatus (*remove_buff)(EnshwickModHandle me,
                                EnshwickEntity entity,
                                EnshwickBuffTypeId id,
                                uint8_t *removed);
  // Writes how the weather weighs at `position`, in
  // metres, as the game's systems weigh it, as far as the `EnshwickHeader`
  // `out` starts with says the caller's struct reaches. On the server's
  // game thread only.
  EnshwickStatus (*weather_at)(EnshwickModHandle me,
                               const EnshwickMetres *position,
                               struct EnshwickWeatherSample *out);
  // Up to `capacity` current weather fronts of the server's world, one per
  // layer that has one, and how many there are, as every list answers.
  // On the server's game thread only.
  EnshwickStatus (*weather_fronts)(EnshwickModHandle me,
                                   struct EnshwickWeatherFront *out,
                                   uint32_t entry_size,
                                   uint32_t capacity,
                                   uint32_t *count);
  // Starts new weather in every layer of the server's world: fronts of
  // `effect` (`EnshwickWeatherFront_NONE` for clear skies) where a layer has
  // one, clear skies elsewhere, for `duration_ns` game nanoseconds or the
  // game's own for 0; `UINT32_MAX` lets the game choose anew. Clients
  // follow. On the server's game thread only.
  EnshwickStatus (*set_weather)(EnshwickModHandle me, uint32_t effect, int64_t duration_ns);
  // Up to `capacity` terrain materials of the running game, and how many
  // there are, as every list answers. On the server's game thread only.
  EnshwickStatus (*terrain_materials)(EnshwickModHandle me,
                                      struct EnshwickTerrainMaterialInfo *out,
                                      uint32_t entry_size,
                                      uint32_t capacity,
                                      uint32_t *count);
  // Runs a terrain edit in the server's world as the game's terraforming
  // does, and writes the volume it changed in cubic metres (for a
  // flatten, 1 when anything changed) to `amount`; clients see it, and
  // the save keeps it inside a base's zone only. Building voxels are left
  // alone.
  // `EnshwickStatus_BAD_ARGS` for an edit out of range;
  // `EnshwickStatus_OUTSIDE_WORLD` for one that reaches past the world's edge
  // or above the height the loader takes, `EnshwickStatus_NOT_LOADED` unless
  // every column of the box it can write is loaded, writing nothing. On
  // the server's game thread only.
  EnshwickStatus (*terrain_edit)(EnshwickModHandle me,
                                 const struct EnshwickTerrainEdit *edit,
                                 float *amount);
  // Calls `callback` with `user` for every altar of the server's world
  // when the loader sizes the altar zones: after altars change and after
  // `EnshwickHostApi.altar_zones_changed`. Each filter, in load order, sees the
  // size so far, the vanilla one first, and may change it: a `grow` below
  // 0 counts as 0, a `height` below one cell keeps the height before it,
  // and the altar and level stay. It opens altars too, and may call the
  // host, such as `EnshwickHostApi.altars`. On the server's game thread.
  // Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*altar_zone_filter)(EnshwickModHandle me,
                                      EnshwickAltarZoneFilterFn callback,
                                      void *user);
  // Asks the loader to size the altar zones again on the server's next
  // step, as when a mod's settings change what its filter answers. Any
  // thread, also during init. `EnshwickStatus_REFUSED` for a mod without an
  // altar zone filter.
  EnshwickStatus (*altar_zones_changed)(EnshwickModHandle me);
  // Calls `callback` with `user` when the game asks how many altars a
  // Flame level allows, on whichever thread asks: the placement cursor
  // while a player aims an altar and the server's placement. Each filter,
  // in load order, sees the game's own count and the count so far and may
  // change `allowed`; the level and the game's count stay. The loader keeps
  // the answer within the zones the game can group
  // (`EnshwickHostApi.base_limit`). A filter runs often: keep it cheap; it may
  // call the host. `EnshwickStatus_TOO_LATE` after `enshwick_mod_init`,
  // `EnshwickStatus_UNAVAILABLE` on a build without the game's lookup.
  EnshwickStatus (*altar_limit_filter)(EnshwickModHandle me,
                                       EnshwickAltarLimitFilterFn callback,
                                       void *user);
  // Writes to `out` the most altar zones and bases the game groups now:
  // its own, or the loader's maximum once `EnshwickHostApi.request_base_limits` is
  // in force, from `enshwick_mod_ready` on, with the zones its own
  // grouping takes. `EnshwickStatus_BAD_ARGS` for a null `out`. Any thread.
  EnshwickStatus (*base_limit)(EnshwickModHandle me, struct EnshwickBaseLimitInfo *out);
  // Reads the voxels of the server's world, decoded at full detail, in the
  // box of `size` voxels
  // from `min`, in voxels of half a metre from the world's corner, into
  // `out`: x fastest, then y, then z, `size[0] * size[1] * size[2]`
  // entries. `min` and `size` are borrowed for the call; `out` points at
  // `capacity` writable entries, untouched unless the call returns
  // `EnshwickStatus_OK`. `EnshwickStatus_BAD_ARGS` for a null pointer, an empty
  // box, one past `EnshwickVoxel_END`, more than `EnshwickVoxel_READ_MAX` voxels or
  // a `capacity` below them; `EnshwickStatus_OUTSIDE_WORLD` for a box past
  // the world's edge or above the height the loader takes;
  // `EnshwickStatus_NOT_LOADED` while part of the box is not loaded
  // yet, which starts loading it; `EnshwickStatus_UNAVAILABLE` before a world
  // runs or on a build without the read. On the server's game thread at
  // a quiet moment only, as the world's writes: `EnshwickStatus_WRONG_THREAD`
  // inside a step.
  EnshwickStatus (*voxel_read)(EnshwickModHandle me,
                               const uint32_t (*min)[3],
                               const uint32_t (*size)[3],
                               struct EnshwickVoxel *out,
                               uint32_t capacity);
  // Fills `check.refusal` with whether the server lets the player whose
  // character is `check.character` edit the box from `check.min` to
  // `check.max`. The permissions follow the game's terrain tool:
  // `CanEditWorld` for every edit, `CanEditBase` where the box overlaps a
  // building zone. The loader adds two tests the building cursor runs for
  // items: both corners inside the world's building area, and no strict
  // no-build zone the world places. A player not in game has no
  // permissions. `check` is borrowed for the call. `EnshwickStatus_BAD_ARGS`
  // for a null `check`, an old header, a corner that is not finite or a
  // `max` below `min`; `EnshwickStatus_UNAVAILABLE` while no world is loaded or
  // on a build without the checks. On the server's game thread at a quiet
  // moment only, outside a step or in a game event:
  // `EnshwickStatus_WRONG_THREAD` inside a step.
  EnshwickStatus (*build_rights)(EnshwickModHandle me, struct EnshwickBuildRights *check);
  // Queues the `count` changes at `ops` as one transaction of the
  // server's world and writes its id to `id`: the loader writes them in
  // order on the server's game thread at quiet moments, a budget of
  // bricks a step, keeping the earlier state of every brick a change can
  // touch so `EnshwickHostApi.transaction_undo` can put it back. `ops` and the
  // records they point at are borrowed for the call. `EnshwickStatus_BAD_ARGS`
  // for a null pointer, no changes, a kind or record the loader refuses
  // (as `EnshwickHostApi.terrain_edit` does, and a dry run), or more than its
  // cap of changes or bricks; `EnshwickStatus_OUTSIDE_WORLD` for a change that
  // leaves the world, once the loader knows its extent (a transaction
  // committed earlier fails with it before writing); `EnshwickStatus_BUSY`
  // while the queue is full; `EnshwickStatus_UNAVAILABLE` while no world runs
  // or on a build without the edits, the voxel read or the undo write.
  //
  // `actor` names the character of the player the transaction acts for,
  // 0 for none: before it writes anything the loader checks that the
  // server lets the player edit the box of every change, as
  // `EnshwickHostApi.build_rights` does, and marks building events with the
  // player. A refusal fails it with `EnshwickStatus_REFUSED` and the reason in
  // `EnshwickTransactionInfo.refusal`; nothing is written. The check runs at
  // its first step on the server's game thread, never at the commit. Any
  // thread.
  EnshwickStatus (*transaction_commit)(EnshwickModHandle me,
                                       EnshwickEntity actor,
                                       const struct EnshwickTransactionOp *ops,
                                       uint32_t count,
                                       uint64_t *id);
  // Queues the undo of transaction `id` of this mod: a queued one is
  // dropped, one written in whole or in part gets each brick it touched
  // back as it was, in reverse order, on the server's game thread at
  // quiet moments. Before writing, the undo checks that every brick still
  // holds what the transaction left there and that all of them are
  // loaded, and it checks each brick again right before writing it back;
  // a brick changed meanwhile, or a column that stays unloaded for the
  // loader's wait limit, stops it, and the state turns
  // `EnshwickTransactionInfo_UNDO_REFUSED` with what was put back kept. A
  // refused undo may be asked again. `EnshwickStatus_OK` when queued or
  // already undoing; `EnshwickStatus_REFUSED` for an id this mod did not get,
  // one the loader forgot, one undone, one whose undo data is gone, or
  // while the transactions in work already claim the loader's cap of
  // undo data. Any thread.
  EnshwickStatus (*transaction_undo)(EnshwickModHandle me, uint64_t id);
  // Writes where transaction `id` of this mod stands to `out`, a
  // `EnshwickTransactionInfo` with its header. `EnshwickStatus_BAD_ARGS` for a null
  // `out`; `EnshwickStatus_MISSING` for an id this mod did not get or one the
  // loader forgot (it keeps the newest few thousand). Any thread.
  EnshwickStatus (*transaction_status)(EnshwickModHandle me,
                                       uint64_t id,
                                       struct EnshwickTransactionInfo *out);
  // `EnshwickStatus_OK` on the server's game thread at a quiet moment while a
  // world runs, where a mod may read live components that no system
  // writes meanwhile; `EnshwickStatus_WRONG_THREAD` on another thread or
  // inside a step, `EnshwickStatus_UNAVAILABLE` while no world runs.
  EnshwickStatus (*world_quiet)(EnshwickModHandle me);
  // Writes the entities of the props transaction `id` of this mod placed
  // and did not undo, kept when its undo data expires, oldest first, up to `capacity` of them at `out`,
  // and their number to `count`; a prop is in the world from the step
  // after the one that placed it. `out` may be null with `capacity` 0 to
  // learn the number. `EnshwickStatus_BAD_ARGS` for a null `count`;
  // `EnshwickStatus_MISSING` for an id this mod did not get or one the loader
  // forgot. Any thread.
  EnshwickStatus (*transaction_entities)(EnshwickModHandle me,
                                         uint64_t id,
                                         EnshwickEntity *out,
                                         uint32_t capacity,
                                         uint32_t *count);
  // Writes to `kept` 1 when a save keeps changes in the box from `min`
  // to `max`, in metres, and 0 when not: the game saves a world only
  // inside its bases' zones, so the box must lie wholly inside one, half
  // a metre in from its edges. `EnshwickStatus_BAD_ARGS` for a null pointer,
  // a corner that is not finite or a `max` below `min`;
  // `EnshwickStatus_UNAVAILABLE` while no world is loaded or on a build without
  // the zone list. On the server's game thread at a quiet moment only:
  // `EnshwickStatus_WRONG_THREAD` elsewhere or inside a step. The mod passes
  // `min` and `max` readable for three `f64` each and `kept` writable for
  // one byte, all three borrowed for the call and untouched by other
  // threads during it; none needs alignment.
  EnshwickStatus (*world_persists)(EnshwickModHandle me,
                                   const EnshwickMetres *min,
                                   const EnshwickMetres *max,
                                   uint8_t *kept);
  // Up to `capacity` map markers of the scene the server runs and how
  // many there are: what the scene's global chunk places, read from the
  // game's archive once per scene on a thread of the loader's;
  // `EnshwickStatus_NOT_LOADED` until that read is done; as every list answers. On the server's game
  // thread at a quiet moment (`EnshwickStatus_WRONG_THREAD` elsewhere or
  // inside a step); `EnshwickStatus_UNAVAILABLE` while no world runs or when the
  // scene does not read on this build.
  EnshwickStatus (*map_markers)(EnshwickModHandle me,
                                struct EnshwickMapMarkerInfo *out,
                                uint32_t entry_size,
                                uint32_t capacity,
                                uint32_t *count);
  // Asks the game for the account action `action` on `account`, as its
  // administrators' account actions do: `EnshwickAccountActionType_KICK`
  // removes a player online, `EnshwickAccountActionType_BAN` puts the account
  // on the server's ban list and removes its player when online,
  // `EnshwickAccountActionType_UNBAN` takes it off the list; the dedicated
  // server writes the list to `bannedAccounts` of `enshrouded_server.json`.
  // `EnshwickStatus_REFUSED` for a kick or a ban of an administrator online,
  // whom the game never removes, for a kick or a ban of a hosted game's
  // host, and for a kick or a ban of an account not online; an account on
  // the list already is no error. `EnshwickStatus_BAD_ARGS` for account 0 or an
  // action this loader does not know; `EnshwickStatus_UNAVAILABLE` while no server runs or on a build that
  // lacks the action. On the server's game thread between steps only:
  // `EnshwickStatus_WRONG_THREAD` elsewhere.
  EnshwickStatus (*account_action)(EnshwickModHandle me,
                                   EnshwickAccountActionType action,
                                   EnshwickPlayerKey account);
  // Writes to `out` the frame of the map of the scene the server runs:
  // the scene's biome map, read from the game's archive once per scene
  // on a thread of the loader's; `EnshwickStatus_NOT_LOADED` until that read
  // is done. `out` is the caller's, at least a `EnshwickHeader` and writable
  // for the size its header states; `EnshwickStatus_BAD_ARGS` for a null or
  // smaller one. On the server's game thread at a quiet moment
  // (`EnshwickStatus_WRONG_THREAD` elsewhere or inside a step);
  // `EnshwickStatus_UNAVAILABLE` while no world runs, for a scene without a biome
  // map, or when the scene does not read on this build.
  EnshwickStatus (*map_frame)(EnshwickModHandle me, struct EnshwickMapFrameInfo *out);
  // Asks the game to group up to `zones` altar zones into up to `bases`
  // bases. Raised, the game groups every listed zone into its base;
  // `zones` caps the altars a Flame level allows, and an altar that would
  // start a base past `bases` is refused. Values below the game's own
  // count as the game's, and the loader takes the largest request of
  // every mod for each. The loader owns the hooks; when one cannot be
  // armed after init the game keeps its own limits although the call
  // returned `EnshwickStatus_OK`, so read `EnshwickHostApi.base_limit` from
  // `enshwick_mod_ready` on. `EnshwickStatus_BAD_ARGS` for a value past the
  // most the loader takes (`EnshwickBaseLimitInfo.max_zones`,
  // `EnshwickBaseLimitInfo.max_bases`); `EnshwickStatus_UNAVAILABLE` when this build's
  // code is not the one the loader knows, and the game keeps its own.
  // Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*request_base_limits)(EnshwickModHandle me, uint32_t zones, uint32_t bases);
  // Mutes `account` in chat when `muted` is 1, or lifts its mute when 0.
  // The server drops every line the account's player types before any
  // other player reads it, and tells that player with one of the game's
  // notices at most every 30 seconds; lines mods send and mod messages
  // pass. The mutes belong to the world: the file
  // `<stem>-mod-data/enshwick/muted.txt` beside its save, written at once
  // and read again as the world loads. `EnshwickStatus_OK` also when the
  // list already says so; `EnshwickStatus_BAD_ARGS` for account 0 or `muted`
  // above 1; `EnshwickStatus_REFUSED` for a mute of an administrator online
  // or of a hosted game's host, whose lines pass even when muted before,
  // and when the file is not written;
  // `EnshwickStatus_UNAVAILABLE` outside a server, the dedicated one or the one a
  // client hosts, and while no world runs. Any thread, also while another call changes the list:
  // changes take turns and the file is replaced whole.
  EnshwickStatus (*set_chat_mute)(EnshwickModHandle me, EnshwickPlayerKey account, uint8_t muted);
  // Writes 1 to `muted` when `account` is muted in chat, 0 when not.
  // `muted` is the caller's, writable for one byte for the call, and
  // written only on `EnshwickStatus_OK`. `EnshwickStatus_BAD_ARGS` for account 0
  // or a null `muted`; `EnshwickStatus_UNAVAILABLE` outside a server, the
  // dedicated one or the one a client hosts. Any thread.
  EnshwickStatus (*chat_muted)(EnshwickModHandle me, EnshwickPlayerKey account, uint8_t *muted);
  // Caps the bases the server holds at `bases`, below the largest
  // `EnshwickHostApi.request_base_limits`: an altar that would start a base
  // past it is refused. Each call replaces the mod's own cap, and the
  // lowest cap of every mod holds, so a mod asks for its most at init
  // and sets each world's cap from its settings as the world loads; a
  // cap at or past the request leaves the request; a hosted game's end
  // drops every cap. `EnshwickHostApi.base_limit`
  // reads the capped bases. `EnshwickStatus_BAD_ARGS` below the game's 16.
  // Any time, any thread.
  EnshwickStatus (*set_base_limit)(EnshwickModHandle me, uint32_t bases);
  // Writes the save of the world the server runs to `out`, which the
  // mod starts with a header of its own size and ABI version: `EnshwickStatus_MISSING` before its first
  // save, `EnshwickStatus_UNAVAILABLE` while no world runs or on a client that
  // hosts none, and
  // `EnshwickStatus_REFUSED` when the file does not read. Any thread; `out` is
  // borrowed for the call.
  EnshwickStatus (*save_info)(EnshwickModHandle me, struct EnshwickSaveInfo *out);
  // Holds the world's saves while `held` is 1, or lets them go for 0:
  // the game saves only while no running mod holds them, the timed save
  // waiting and `EnshwickHostApi.save_world` refused meanwhile, and the log names each
  // mod that holds or lets go. A mod that turns off lets go. At shutdown
  // every hold lets go before `EnshwickEvent_SHUTDOWN_STARTED`; a hold set
  // while the mod hears that event refuses the final save, and a hold
  // asked for after it is not taken. Server only
  // (`EnshwickStatus_UNAVAILABLE` on a build without the save layout); any thread,
  // taking effect after the next step, or at once while the mod hears
  // `EnshwickEvent_SHUTDOWN_STARTED`; `EnshwickStatus_BAD_ARGS` for another
  // `held`.
  EnshwickStatus (*hold_saves)(EnshwickModHandle me, uint8_t held);
  // Keeps `entity`, which a mod spawned or placed, out of the world's
  // saves (`saved` 0) or puts it back in them (any other `saved`), from
  // the next step on; the next save after that keeps it or leaves it
  // out. `EnshwickStatus_REFUSED` for an entity no mod spawned since the world
  // loaded, such as a vanilla one, or a full command buffer; `EnshwickStatus_MISSING` for an
  // entity not in the world; `EnshwickStatus_UNAVAILABLE` where no server
  // runs, as on a client in another's game, or on a build without the
  // commands; `EnshwickStatus_BAD_ARGS` for entity 0. On the server's game
  // thread only (`EnshwickStatus_WRONG_THREAD` elsewhere).
  EnshwickStatus (*set_saved)(EnshwickModHandle me, EnshwickEntity entity, uint8_t saved);
  // Writes the level or experience of the player whose character is
  // `entity` in the server's next level step, where the game raises
  // levels itself; `what` says which. `PROGRESS_SET_LEVEL`: the level
  // becomes `value`, kept between 1 and the game's level cap, with no
  // experience inside it and what that level needs to go on, as the
  // game leaves a level it raised, and a gain not applied yet is
  // dropped; the game raises no level-up event.
  // `PROGRESS_SET_EXPERIENCE`: the experience inside the level becomes
  // `value`, kept below what the level needs.
  // `PROGRESS_ADD_EXPERIENCE`: `value` is gained, and the game levels
  // up from it, one level a step, with its own events; experience
  // filters do not see it. Skill points follow the level, as the game
  // counts them from it. A mod asks for
  // the writes during `init` with `PROGRESS_OPEN` and `EnshwickEntity(0)`;
  // `EnshwickStatus_TOO_LATE` for that after `init`. `EnshwickStatus_OK` queues
  // the write; the step applies writes in the order given, and
  // `EnshwickEvent_PROGRESS_WRITTEN` tells the
  // mod after the step whether it applied the write or why it was
  // dropped.
  // `EnshwickStatus_REFUSED` for a mod that did not ask, an entity that is
  // not a player's character in the game; `EnshwickStatus_BUSY` while 256
  // writes wait;
  // `EnshwickStatus_BAD_ARGS` for another `what`; `EnshwickStatus_WRONG_THREAD`
  // off the server's game thread; `EnshwickStatus_UNAVAILABLE` for a write during
  // `init`, on a client, a
  // build without the level step, or once a panic turned the writes
  // off.
  EnshwickStatus (*progress_write)(EnshwickModHandle me,
                                   EnshwickEntity entity,
                                   uint32_t what,
                                   uint32_t value);
  // Asks the dedicated server to stop the way the game stops on Ctrl+C:
  // after the current step the server shuts down, `ShutdownStarted` is
  // raised and the save holds let go, the final save runs, and the
  // process exits with code 0. Returns at once; the stop runs on a
  // thread of the loader's. Any thread. `EnshwickStatus_REFUSED` when a stop
  // is under way already, asked by a mod or by the game, or the game has
  // not started yet;
  // `EnshwickStatus_UNAVAILABLE` off a dedicated server, as in single player
  // or a hosted game; `EnshwickStatus_UNAVAILABLE` when the loader did not find
  // the game's stop on this build.
  EnshwickStatus (*server_stop)(EnshwickModHandle me);
  // Uncovers the fog of war of the player with account key `account` in
  // the circle of `radius` metres around world `x` and `z`, as exploring
  // there does: the server's loader sends it to the loader on the
  // player's client, which hands it to the game's own reveal, and the
  // character's save keeps it. Returns once it is sent. The server holds
  // no fog, so a client without Enshwick is not reached. Any thread, on
  // a dedicated server or a client that hosts. `EnshwickStatus_UNAVAILABLE`
  // for a player whose client takes no messages from the loader, as one
  // without Enshwick or not online, and on a client in another's game;
  // `EnshwickStatus_BAD_ARGS` for a place that is not finite or a radius that
  // is not above 0 and at most 100 000; `EnshwickStatus_REFUSED` when the
  // link to the player has no room.
  EnshwickStatus (*fog_uncover)(EnshwickModHandle me,
                                EnshwickPlayerKey account,
                                float x,
                                float z,
                                float radius);
  // The game language the player of account `key` reads, as the game
  // numbers them: on a
  // server, the one that player's game named when it joined, always one
  // the game knows; `0`, the fallback, for a player whose game runs no
  // Enshwick, for a key no connected player holds and on a client. Any
  // thread, re-entrant.
  EnshwickStatus (*language_of)(EnshwickModHandle me, EnshwickPlayerKey key, uint32_t *out);
  // Adds `callback` with `user` to the join rules, which the server asks
  // in load order, on the thread the game reads a machine's join on, once
  // for each account the machine brings and before any of its players is
  // added. The first `FILTER_REFUSE` refuses the machine: it gets the game's own reply
  // for an account on the ban list, which its player reads as the host
  // denying access, and loads no world. The loader writes nothing to the
  // ban list, and the rules of a mod that is off are not asked. A machine
  // that already joined keeps playing: the game drops its join message.
  // Server only: on a client it runs while it hosts a game. A build
  // without the join path answers `EnshwickStatus_UNAVAILABLE`. Allowed during
  // `enshwick_mod_init` only (`EnshwickStatus_TOO_LATE` after it);
  // `callback` and `user` are kept for the life of the process and never
  // freed by the loader.
  EnshwickStatus (*join_rule)(EnshwickModHandle me,
                              EnshwickJoinRuleFn callback,
                              void *user);
  // Asks for inventory transactions and item data; allowed during
  // `enshwick_mod_init` only. `EnshwickStatus_UNAVAILABLE` when this game build
  // lacks a piece of them. Requests run in the game server: the dedicated
  // server, or the client while it hosts.
  EnshwickStatus (*request_inventory)(EnshwickModHandle me);
  // Queues `request`, from any thread, for the game's inventory system,
  // which runs it as one transaction: all or nothing. `done` gets the
  // result with `user` on the game thread after a later step, unless the
  // mod is off by then. On an error status nothing is queued and `done` is
  // never called.
  EnshwickStatus (*inventory_submit)(EnshwickModHandle me,
                                     const struct EnshwickInventoryRequest *request,
                                     EnshwickInventoryResultFn done,
                                     void *user);
  // Calls `callback` with `user` to answer the game's survivor lookups
  // that `EnshwickSurvivorLookup` names: the lookups of
  // every mod run in load order, the first that answers deciding, and
  // the game answers when all pass on. A second call replaces the mod's
  // own lookup. Runs on the server's worker
  // threads, in the dedicated server or a hosting client, and for
  // `EnshwickSurvivorLookup_CURSOR` on a client alone too. Allowed during
  // `enshwick_mod_init` only; `EnshwickStatus_UNAVAILABLE` on a build that
  // lacks a piece.
  EnshwickStatus (*survivor_lookup)(EnshwickModHandle me,
                                    EnshwickSurvivorLookupFn callback,
                                    void *user);
  // Calls `callback` with `user` after a survivor's own bed check kept its
  // bed; any filter returning `FILTER_REFUSE` clears it. Server's worker threads.
  // Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*survivor_bed_check)(EnshwickModHandle me,
                                       EnshwickBedCheckFn callback,
                                       void *user);
  // Calls `callback` with `user` before a summoning staff spawns or moves
  // a survivor; any filter returning 1 makes the use take the item.
  // Server's game thread. Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*summon_filter)(EnshwickModHandle me,
                                  EnshwickSummonFilterFn callback,
                                  void *user);
  // Files item `item` as a summoning staff of the survivor `survivor`, so
  // the game treats it as that survivor's staff, on both sides. Allowed
  // during `enshwick_mod_init` only.
  EnshwickStatus (*summon_item)(EnshwickModHandle me, EnshwickItemId item, EnshwickNpcId survivor);
  // Calls `callback` with `user` to merge the statuses the client's
  // survivor menu shows for a survivor with several entities: the merges
  // of every mod run in load order, the first that sets a status
  // deciding, and the game's stays when all pass on. A second call
  // replaces the mod's own merge. Client only. Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*survivor_status)(EnshwickModHandle me,
                                    EnshwickSurvivorStatusFn callback,
                                    void *user);
  // Raises the game event at `data`, `size` bytes of the reflected type
  // `type_hash`, its whole layout; the loader stamps it with the game
  // time and the systems of the next step see it. Only types the loader
  // allows, which the server's own systems raise: `EnshwickStatus_REFUSED`
  // for any other, when the step's stream is full or past a mod's 32
  // notify events in one step, `EnshwickStatus_BAD_ARGS` for a size that is
  // not the type's. On the server's game thread only.
  EnshwickStatus (*emit)(EnshwickModHandle me,
                         uint32_t type_hash,
                         const uint8_t *data,
                         uint32_t size);
  // Asks the server to move an entity as its admin teleport does, after
  // the next step; `EnshwickStatus_MISSING` for an entity not in the world,
  // `EnshwickStatus_REFUSED` for one the game's teleport does not move, as a
  // prop: it moves actors and characters; `EnshwickStatus_UNAVAILABLE` where
  // no server runs or on a build without teleports.
  // On the server's game thread only.
  EnshwickStatus (*teleport)(EnshwickModHandle me, const struct EnshwickTeleportRequest *request);
  // Asks the server to spawn an entity from a template, as its admin
  // spawn does, and writes the new entity's id to `out`; the entity is in
  // the world after the next step. `EnshwickStatus_BAD_ARGS` for a zero
  // template; a template the game does not know spawns nothing. On the
  // server's game thread only.
  EnshwickStatus (*spawn)(EnshwickModHandle me,
                          const struct EnshwickSpawnRequest *request,
                          EnshwickEntity *out);
  // Calls `callback` with `user` on damage about to reach an entity's
  // health in the server's world, before the game applies it: hits,
  // falls and the shroud; what the filters leave in the
  // `EnshwickDamageCheck` is what the game applies. Runs on the game's worker
  // threads, in load order. Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*damage_filter)(EnshwickModHandle me,
                                  EnshwickDamageFilterFn callback,
                                  void *user);
  // Calls `callback` with `user` after a summoning staff's use spawned or
  // moved a survivor, with `EnshwickSummon.consumed` set; its return value is
  // ignored. On the thread of the use, as `EnshwickHostApi.summon_filter`.
  // Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*summoned)(EnshwickModHandle me,
                             EnshwickSummonFilterFn callback,
                             void *user);
  // Defines the stat kind `id` with `def`, or opens it with a null
  // `def`, and writes its index and key to `out`. `id` is a full content
  // id, `<mod>:<name>`; the text that names the kind is `<name>.name` in
  // its owner's texts, which the loader derives. A mod defines kinds in
  // its own namespace and opens the loader's built-in ones
  // (`enshwick:<name>`) and those of other mods. Every mod gets the same
  // index for one id. `EnshwickStatus_BAD_ARGS` for a malformed id or
  // definition; `EnshwickStatus_REFUSED` for a kind defined twice or in
  // another mod's namespace, or a kind of another loaded mod whose
  // `EnshwickManifest.exports` lack its kind and name and the
  // name it is a part of, with the id and the owner's exports logged.
  // Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*define_stat)(EnshwickModHandle me,
                                struct EnshwickStr id,
                                const struct EnshwickStatSpec *def,
                                struct EnshwickStatRef *out);
  // Writes the definition of stat `stat` to `def` and its full id to
  // `id`, a string the loader keeps for the process; `EnshwickStatus_MISSING`
  // for a kind opened but never defined. Any thread.
  EnshwickStatus (*stat_info)(EnshwickModHandle me,
                              uint32_t stat,
                              struct EnshwickStatSpec *def,
                              struct EnshwickStr *id);
  // Writes the value of stat `stat` on `entity` to `out`: its default
  // when nothing modifies it. Values are worked out on the server after
  // each step a source changed; a client answers defaults. Any thread,
  // without locks.
  EnshwickStatus (*stat_value)(EnshwickModHandle me,
                               EnshwickEntity entity,
                               uint32_t stat,
                               float *out);
  // Sets or removes a modifier the mod keeps on an entity; the value
  // follows after the next step. `EnshwickStatus_REFUSED` on a client alone,
  // or for a saved modifier on an entity that is no player. Any thread.
  EnshwickStatus (*stat_modify)(EnshwickModHandle me, const struct EnshwickStatModify *request);
  // Gives the buff type `buff` the `count` modifiers at `modifiers`:
  // they count toward every watched entity that has the buff. Allowed
  // during `enshwick_mod_init` only.
  EnshwickStatus (*stat_buff)(EnshwickModHandle me,
                              uint32_t buff,
                              const struct EnshwickStatModifier *modifiers,
                              uint32_t count);
  // Calls `callback` with `user` for every stat value that changed on a
  // watched entity, on the game thread after the step that changed it.
  // Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*stats_changed)(EnshwickModHandle me,
                                  EnshwickStatChangeFn callback,
                                  void *user);
  // Writes up to `capacity` modifiers that count toward stat `stat` on
  // `entity`, in the order they apply, and how many there are to
  // `count`; every stat for `UINT32_MAX`. On the server's game thread at a
  // quiet moment (`EnshwickStatus_WRONG_THREAD` inside a step).
  EnshwickStatus (*stat_sources)(EnshwickModHandle me,
                                 EnshwickEntity entity,
                                 uint32_t stat,
                                 struct EnshwickStatSource *out,
                                 uint32_t capacity,
                                 uint32_t *count);
  // Watches `entity` for the items it wears and the buffs it has, or
  // stops `me`'s watch for `watch` 0: the entity stays watched while any
  // mod watches it, players are always watched, and so is every entity a
  // mod keeps a modifier on. Any thread.
  EnshwickStatus (*stat_watch)(EnshwickModHandle me, EnshwickEntity entity, uint8_t watch);
  // Defines the affix `id` with `def` and writes its index and key to
  // `out`. `id` is a full content id in the mod's namespace,
  // `<mod>:<name>`; the text that names the affix is `<name>.name` in the
  // mod's texts, which the loader derives. `EnshwickStatus_BAD_ARGS` for a
  // malformed id or definition, `EnshwickStatus_REFUSED` for an affix defined
  // twice or in another mod's namespace. Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*define_affix)(EnshwickModHandle me,
                                 struct EnshwickStr id,
                                 const struct EnshwickAffixSpec *def,
                                 struct EnshwickStatRef *out);
  // Rolls affixes onto an item from every mod's affixes that fit its
  // category, replacing those it had, and writes how many to `count`;
  // `EnshwickStatus_REFUSED` for a `EnshwickAffixRoll_ONCE` roll of an item that
  // rolled before, or off the server. On the server's game thread only.
  EnshwickStatus (*affix_roll)(EnshwickModHandle me,
                               const struct EnshwickAffixRoll *roll,
                               uint32_t *count);
  // Writes up to `capacity` lines of what the item gives, its own
  // modifiers first and then each affix's, and how many to `count`. On a
  // client, from what the server sent. Any thread.
  EnshwickStatus (*item_lines)(EnshwickModHandle me,
                               EnshwickEntity pide,
                               EnshwickItemId item,
                               struct EnshwickItemLine *out,
                               uint32_t capacity,
                               uint32_t *count);
  // Writes the text key and rarity of affix `affix` (an index) to `text`
  // and `rarity`; the text is the loader's, valid for the process. Any
  // thread.
  EnshwickStatus (*affix_info)(EnshwickModHandle me,
                               uint32_t affix,
                               struct EnshwickStr *text,
                               uint8_t *rarity);
  // Calls `callback` with `user` for each player's action with an item
  // in the server's world, on the thread of the system that runs it:
  // before the game uses, throws away, equips or takes off an item, where
  // `FILTER_REFUSE` refuses it as an action already run, after a pickup or a
  // placement, and after the game took the item of a use every filter
  // allowed (`EnshwickItemAction_CONSUMED`, so a
  // use reaches a filter twice), whose answers are ignored. A
  // filter ignores a kind it does not know. Allowed during
  // `enshwick_mod_init` only: `EnshwickStatus_TOO_LATE` after,
  // `EnshwickStatus_UNAVAILABLE` on a build without the player input it
  // reads.
  EnshwickStatus (*item_filter)(EnshwickModHandle me,
                                EnshwickItemFilterFn callback,
                                void *user);
  // Drops `request.count` new items of `request.item` on the ground at
  // `request.position`, as one loose stack any player can pick up, as
  // the game spills loot; writes the new pickup's entity to `out`. The
  // game falls it to the ground and does not save it.
  // `EnshwickStatus_BAD_ARGS` for a null `request` or `out`, a count of 0 or over
  // 65535, or a rarity the game lacks; `EnshwickStatus_MISSING` for an item
  // the game does not know; `EnshwickStatus_WRONG_THREAD` off the server's
  // game thread; `EnshwickStatus_UNAVAILABLE` on a client, a build without
  // the pickup or the item lookup, or while no item registry or world
  // exists yet, which a later call may find. The loader
  // reads the pickup off the game thread as the server starts, before its
  // world loads, in about a tenth of a second; a call before that read
  // ends waits for the rest of it. `request` and `out` are borrowed for
  // the call.
  EnshwickStatus (*drop_items)(EnshwickModHandle me,
                               const struct EnshwickDropRequest *request,
                               EnshwickEntity *out);
  // Calls `callback` with `user` when the game asks whether a character
  // unlocked skill node `node`; the first filter that answers decides.
  // Runs on both sides, on the asking thread. Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*skill_override)(EnshwickModHandle me,
                                   EnshwickSkillNode node,
                                   EnshwickSkillCheckFn callback,
                                   void *user);
  // Calls `callback` with `user` when a comfort provider registers its
  // comfort; filters run in load order, each on the last one's value.
  // Server's worker threads. Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*comfort_filter)(EnshwickModHandle me,
                                   EnshwickComfortFilterFn callback,
                                   void *user);
  // Calls `callback` with `user` before the server applies the
  // characters' gained experience, once per batch; filters run in load
  // order. Server's worker threads. Allowed during `enshwick_mod_init`
  // only.
  EnshwickStatus (*experience_filter)(EnshwickModHandle me,
                                      EnshwickExperienceFilterFn callback,
                                      void *user);
  // Writes knowledge `id` of the player whose character is `entity`, or
  // of the world for 0 or for an id of the world's;
  // `EnshwickStatus_MISSING` for an id the game does not know,
  // `EnshwickStatus_REFUSED` for a player not in the game. On the server's
  // game thread only.
  EnshwickStatus (*knowledge)(EnshwickModHandle me,
                              EnshwickEntity entity,
                              EnshwickKnowledgeId id,
                              struct EnshwickKnowledgeInfo *out);
  // Grants or takes back the knowledge value `change` names.
  //
  // A grant to a character or the world works as the game grants: the
  // next step applies it with what follows (experience, unlocks, clients,
  // the save); `EnshwickStatus_BAD_ARGS` for a bit the game does not expect,
  // `EnshwickStatus_BUSY` for a full queue. A revoke clears a
  // `EnshwickKnowledgeInfo_BOOL`, takes `value` from a
  // `EnshwickKnowledgeInfo_COUNTER` (down to 0) or clears bit `value` of
  // `EnshwickKnowledgeInfo_FLAGS`, sends it to the clients again and is saved
  // with the next save; what followed the grant, such as experience,
  // stays. A change of an account applies now when its character is in
  // the game, else once its next character has loaded in this world; the
  // loader keeps waiting changes with the world, and refuses an id of the
  // world's with `EnshwickStatus_BAD_ARGS`.
  //
  // `change` is borrowed for the call. `EnshwickStatus_BAD_ARGS` for a null or
  // short `change`, both `account` and `entity` set, an unknown id or
  // `op`, a kind that is not the id's or a bit past 31;
  // `EnshwickStatus_REFUSED` for a character not in the game, or whose save has
  // not loaded yet, or a player's id named for the world;
  // `EnshwickStatus_UNAVAILABLE` for an account while no world runs. On the
  // server's game thread only.
  EnshwickStatus (*change_knowledge)(EnshwickModHandle me,
                                     const struct EnshwickKnowledgeChange *change);
  // Up to `capacity` quest ids of the running game, and how many there
  // are. On the server's game thread only.
  EnshwickStatus (*quests)(EnshwickModHandle me,
                           EnshwickKnowledgeId *out,
                           uint32_t capacity,
                           uint32_t *count);
  // Registers a filter on the knowledge clients ask the server for in
  // their input, which a vanilla game grants unchecked. Filters run in
  // load order after the loader's guard, each with the verdict so far,
  // and the last answer decides. It runs on the server's worker threads,
  // in the dedicated server or a hosting client. `EnshwickStatus_UNAVAILABLE` on
  // a build without the input site. Allowed during `enshwick_mod_init`
  // only.
  EnshwickStatus (*knowledge_input_filter)(EnshwickModHandle me,
                                           EnshwickKnowledgeInputFilterFn callback,
                                           void *user);
  // Writes to `holds` 1 when the game query `query` (a query of the
  // game's query database, which quest steps and recipes name by id)
  // holds for the player whose character is `entity`, as the game reads
  // it when it pays a quest (the player's own state), or for the world
  // when `entity` is `EnshwickEntity(0)`, and 0 when it does not.
  // `EnshwickStatus_UNAVAILABLE` on a build without the game's query calls or
  // while no world runs, `EnshwickStatus_MISSING` for a query the game does
  // not know or has not evaluated yet; `EnshwickStatus_REFUSED` for a player not in the game;
  // `EnshwickStatus_WRONG_THREAD` off the game thread; `EnshwickStatus_BAD_ARGS`
  // for a null `holds`. `holds` is borrowed for the call.
  EnshwickStatus (*game_query)(EnshwickModHandle me,
                               EnshwickEntity entity,
                               uint32_t query,
                               uint32_t *holds);
  // Runs the console line `line` as the server's console, an
  // administrator with no character, and writes its reply to `out`, valid
  // until the next call on this thread: `enshwick ...` for the loader's
  // own tools, or a mod's id and the text form of one of its
  // actions. On the server's game thread only;
  // `EnshwickStatus_MISSING` when nothing answers the line's first word.
  EnshwickStatus (*run_console)(EnshwickModHandle me,
                                struct EnshwickStr line,
                                struct EnshwickStr *out);
  // Calls `callback` with `user` before a player's interaction that
  // `EnshwickInteraction` names; any filter returning `FILTER_REFUSE` refuses it. Runs in the
  // server's world only, on the thread of the system that checks it.
  // Allowed during `enshwick_mod_init` only; `EnshwickStatus_UNAVAILABLE` on a
  // build that lacks a piece.
  EnshwickStatus (*interaction_filter)(EnshwickModHandle me,
                                       EnshwickInteractionFilterFn callback,
                                       void *user);
  // Calls `callback` with `user` on each player's craft request in the
  // server's world, on the game thread before the crafting system reads
  // it; any filter returning `FILTER_REFUSE` refuses it, and nothing is made or taken.
  // Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*craft_filter)(EnshwickModHandle me,
                                 EnshwickCraftFilterFn callback,
                                 void *user);
  // Calls `callback` with `user` after the game's cursor check allows a
  // placement; any filter returning `FILTER_REFUSE` refuses it as the game refuses a
  // placement without permission. Runs on both sides, on the cursor
  // systems' worker threads: the server's check decides, a client's
  // shows. Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*build_filter)(EnshwickModHandle me,
                                 EnshwickBuildFilterFn callback,
                                 void *user);
  // Registers the mod's player actions: the loader asks `callback` with
  // `user`, on the server's game thread, for the actions a player may
  // use, to run one a player picked in the interface or the console
  // named. One registration per
  // mod. Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*actions)(EnshwickModHandle me,
                            EnshwickActionFn callback,
                            void *user);
  // Declares a control of the calling mod from `spec`, with `callback`
  // and `user` for its events; allowed during `enshwick_mod_init` only.
  // The client fires it from its keys while the player is in the world
  // with no menu, chat or inventory open, and never takes the key from
  // the game; the server fires one with a `link` when that player's game
  // action fires. `EnshwickStatus_BAD_ARGS` for a malformed name, key or
  // unknown `link`, `EnshwickStatus_REFUSED` for a name the mod declared
  // before, `EnshwickStatus_TOO_LATE` after init.
  EnshwickStatus (*control)(EnshwickModHandle me,
                            const struct EnshwickControlSpec *spec,
                            EnshwickControlFn callback,
                            void *user);
  // On a client, asks the server to run the calling mod's player action
  // `action` with `args` in the sheet's encoding, as the interface does,
  // on the object `target` names, null for none: the server hands the
  // target to the action's `at` and handler in
  // `EnshwickActionCall.target`, checked as any
  // object a client names, runs the handler and shows the reply as a
  // notice. `EnshwickStatus_UNAVAILABLE` on a server or while the server takes
  // no messages; `EnshwickStatus_BAD_ARGS` for unreadable strings or a target
  // position that is not finite. Any thread; strings and `target` are
  // borrowed for the call.
  EnshwickStatus (*run_action)(EnshwickModHandle me,
                               struct EnshwickStr action,
                               struct EnshwickStr args,
                               const struct EnshwickActionTarget *target);
  // Adds `callback` with `user` to the chat filters, which run in load
  // order on the server's game thread, inside the game's chat post, on
  // every line a player typed that the chat mute let through; the first
  // `EnshwickChatRewrite_DROP` drops the line,
  // and a rewrite hands the new text to the filters after it. Lines the
  // loader posts for mods pass no filter. Server only: on a client it
  // runs while it hosts a game (`EnshwickStatus_UNAVAILABLE` on a build without
  // the chat post). Allowed during `enshwick_mod_init` only
  // (`EnshwickStatus_TOO_LATE` after it); `callback` and `user` are kept for
  // the life of the process and never freed by the loader.
  EnshwickStatus (*chat_filter)(EnshwickModHandle me,
                                EnshwickChatFilterFn callback,
                                void *user);
  // Names the calling mod's actions as commands: a
  // line `/<name> <action> <values>`, or the same with `alias`, runs
  // one of them from a player's chat, the server's console or
  // `run_command`, with the caller's rights. The
  // name is the part of the mod's id after the author. `alias` is 1 to
  // 32 of `[a-z0-9-]`, shorter
  // than the name, and neither `enshwick` nor `help`.
  // `EnshwickStatus_BAD_ARGS` for a malformed or unreadable alias, or a mod
  // whose name is `enshwick` or `help`; `EnshwickStatus_REFUSED` for a second
  // call, or when another mod's commands have the same name or another
  // mod's whole id is that name, which the
  // loader logs and reports naming both mods, the mod running on without
  // commands. An alias that another mod's commands, name or id hold goes
  // to that mod: the call returns `EnshwickStatus_OK`, the mod's commands run
  // by their name alone, and the loader logs and reports it once naming
  // both mods. Allowed during `enshwick_mod_init` only; `alias` is
  // borrowed for the call.
  EnshwickStatus (*commands)(EnshwickModHandle me, struct EnshwickStr alias);
  // Runs the command line `line`, such as `/adm online`, as a remote
  // caller named `label` (for the log and the handler, at most 64 bytes)
  // with the game's permission bits `permissions` (as
  // `EnshwickPlayerInfo.permissions`), never as
  // a player, and writes the mod's `EnshwickActionReply` to
  // `out` as far as the header the caller filled reaches, its text valid
  // until the next call on this thread. The
  // leading `/` is optional. On the server's game thread only
  // (`EnshwickStatus_WRONG_THREAD`); `EnshwickStatus_MISSING` when no running mod
  // has the line's first word as its command name or alias;
  // `EnshwickStatus_BAD_ARGS` for a line over 1,024 bytes, an empty line or
  // an unreadable string.
  EnshwickStatus (*run_command)(EnshwickModHandle me,
                                struct EnshwickStr line,
                                struct EnshwickStr label,
                                uint32_t permissions,
                                struct EnshwickActionReply *out);
  // Up to `capacity` command sets of the running mods, in load order, and
  // how many there are; a mod that reported its feature `commands` off
  // (`feature`) has none. Entries are `entry_size`
  // bytes apart, the caller's `sizeof(EnshwickCommandInfo)`; the loader
  // fills what both sides
  // know of each and its own `EnshwickHeader`. The strings stay valid until the
  // next call on this thread. `EnshwickStatus_BAD_ARGS` for a non-null `out`
  // whose entries are shorter than a header. Any thread.
  EnshwickStatus (*command_list)(EnshwickModHandle me,
                                 struct EnshwickCommandInfo *out,
                                 uint32_t entry_size,
                                 uint32_t capacity,
                                 uint32_t *count);
  // Tells the loader that who may run the calling mod's actions changed,
  // such as when a rule reads settings that just changed: the server
  // asks the mod again and carries each player the controls they may
  // run now, within two seconds. Any thread. `EnshwickStatus_MISSING` for a
  // mod without actions.
  EnshwickStatus (*actions_changed)(EnshwickModHandle me);
  // Shows the calling mod's window `id`, 1 to 32 of `[a-z0-9-]`, to the
  // `count` players whose account keys `players` holds; closes it on
  // their clients when `change` is `WINDOW_CLOSE`; or, when it is `WINDOW_UPDATE`, updates it
  // only on the clients that still show it, opening none. When a client closes
  // a window or its player leaves, the loader tells the mod with
  // `EnshwickActionCall_WINDOW_CLOSED`.
  // After the server's step the
  // loader asks the mod for the window as each player sees it
  // (`EnshwickActionCall_WINDOW`) and carries the
  // text to that player's client, which draws it; an empty answer closes
  // it there. A player not online is skipped. On the server only
  // (`EnshwickStatus_UNAVAILABLE` on a client); `EnshwickStatus_BAD_ARGS` for a
  // malformed id, `count` over 64 or a null `players` with a non-zero
  // `count`; `EnshwickStatus_REFUSED` for a mod without
  // actions, or while 256 shows or closes wait.
  // Any thread; `id` and `players` are borrowed for the call.
  EnshwickStatus (*action_window)(EnshwickModHandle me,
                                  struct EnshwickStr id,
                                  const EnshwickPlayerKey *players,
                                  uint32_t count,
                                  uint32_t change);
  // Calls `callback` with `user` once per frame of the client's
  // interface, on the thread that draws it, while the mod is on; the
  // callback draws with `EnshwickHostApi.ui_widget`. Client only
  // (`EnshwickStatus_UNAVAILABLE` on a server); when the interface could not be
  // linked, the callback never runs. Allowed during `enshwick_mod_init`
  // only.
  EnshwickStatus (*ui_frame)(EnshwickModHandle me,
                             EnshwickUiFrameFn callback,
                             void *user);
  // Draws `widget` in the frame `frame` names and writes back its
  // `index`, `value` and `result`. Only inside the mod's frame callback,
  // or inside its tooltip callback with the `EnshwickUiTooltipItem`'s `token` as
  // `frame`, where a label adds a line to the item's tooltip
  // (`EnshwickStatus_WRONG_THREAD` elsewhere); `EnshwickStatus_REFUSED` for a
  // widget outside a window, an `EnshwickUiWidget_END` without one or a tooltip
  // line that is not a label; `EnshwickStatus_BUSY` for a tooltip line past
  // `EnshwickUiTooltipItem_MAX_LINES`; `EnshwickStatus_MISSING` for a widget this game
  // build lacks or an item slot of an item the client does not know;
  // `EnshwickStatus_BAD_ARGS` for a malformed widget or an item slot of count 0.
  EnshwickStatus (*ui_widget)(EnshwickModHandle me, uint64_t frame, struct EnshwickUiWidget *widget);
  // Queues `dialog` and calls `callback` with the answer and `user` once
  // the player gives it, on the thread that draws the interface; dialogs
  // show one at a time, in order, in the game's style. Any thread. Client
  // only (`EnshwickStatus_UNAVAILABLE` on a server); when the interface could not
  // be linked, the dialog shows as a system message box.
  // `EnshwickStatus_BUSY` when too many wait; `EnshwickStatus_WRONG_THREAD` for
  // `EnshwickUiDialog_WAITING` on the thread that draws the interface. On an
  // error status `callback` is never called.
  EnshwickStatus (*ui_dialog)(EnshwickModHandle me,
                              const struct EnshwickUiDialog *dialog,
                              EnshwickUiAnswerFn callback,
                              void *user);
  // Calls `callback` with `user` once per frame while the game draws its
  // HUD in game, hidden with it; the callback draws labels and bars with
  // `EnshwickHostApi.ui_widget` at their `anchor`. Client only
  // (`EnshwickStatus_UNAVAILABLE` on a server). Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*ui_hud)(EnshwickModHandle me,
                           EnshwickUiFrameFn callback,
                           void *user);
  // Replaces the mod's markers on the client's map with the `count` at
  // `markers`, copied; none clears them. Any thread. Client only
  // (`EnshwickStatus_UNAVAILABLE` on a server); `EnshwickStatus_BAD_ARGS` past 256
  // markers. A marker with a non-zero `EnshwickUiMarker.marker_type` has the
  // loader read the registry as `EnshwickHostApi.map_marker_types` does, on
  // the calling thread at the first such call: `EnshwickStatus_BAD_ARGS` for
  // a type it does not list, `EnshwickStatus_UNAVAILABLE` when it does not
  // read.
  // The mod's markers stay as they were after either. A type the
  // registry the map runs on lacks shows as the loader's own choice.
  // Every entry is as large as the header of the first one says.
  EnshwickStatus (*ui_markers)(EnshwickModHandle me,
                               const struct EnshwickUiMarker *markers,
                               uint32_t count);
  // Asks the loader to keep a copy of the client's fog of war for this
  // mod. Client only (`EnshwickStatus_UNAVAILABLE` on a dedicated server).
  // Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*request_fog)(EnshwickModHandle me);
  // Writes the fog's `EnshwickFogInfo` to `info`, then copies the box of
  // `width` × `height` pixels at (`x`, `y`) into `out`, row by row, at
  // most `capacity` bytes; a box of no pixels copies nothing. Any thread.
  // `EnshwickStatus_UNAVAILABLE` while the client is in no world, when this build
  // lacks the fog, or after the loader stopped copying it; `EnshwickStatus_REFUSED` for a mod that did not
  // open it; `EnshwickStatus_OUTSIDE_WORLD` for a box past the image;
  // `EnshwickStatus_BAD_ARGS` for a null `info`, or a null or short `out`
  // for a box of pixels. The loader may refresh the copy meanwhile:
  // compare `version`.
  EnshwickStatus (*fog_read)(EnshwickModHandle me,
                             struct EnshwickFogInfo *info,
                             uint32_t x,
                             uint32_t y,
                             uint32_t width,
                             uint32_t height,
                             uint8_t *out,
                             uint32_t capacity);
  // Up to `capacity` marker types of the client's marker type registry,
  // in the registry's order, and how many there are, read from the
  // game's archive at the first call and kept, as every list answers.
  // `EnshwickStatus_UNAVAILABLE` on a dedicated
  // server, whose archive has no registry, and when the registry does
  // not read on this build. Any thread.
  EnshwickStatus (*map_marker_types)(EnshwickModHandle me,
                                     struct EnshwickMapMarkerTypeInfo *out,
                                     uint32_t entry_size,
                                     uint32_t capacity,
                                     uint32_t *count);
  // Adds a panel at an edge of the game's screen `spec` names and calls
  // `callback` with `user` once per frame while that screen is open, on
  // the thread that draws the interface; the callback draws into the
  // panel with `EnshwickHostApi.ui_widget`, without a window of its own
  // (`EnshwickStatus_REFUSED` for `EnshwickUiWidget_WINDOW` and `EnshwickUiWidget_END`).
  // Panels at one placement share one window, which opens only once a
  // panel draws a widget that needs it. The panel never changes the game
  // screen's own layout. Client only (`EnshwickStatus_UNAVAILABLE` on a server,
  // `EnshwickStatus_MISSING` for a screen this game build lacks); `EnshwickStatus_BAD_ARGS` for a null
  // or short `spec`, an unknown edge or alignment, a `shift` or `inset`
  // past its bounds or not a number, a non-zero `reserved_place`, an
  // unreadable screen name, or a
  // screen on which the player is in the world with no window open
  // (`InGame`, `NoUi`, `TakeScreenshot`, the multitools' building
  // screens); allowed during `enshwick_mod_init` only, on the thread that
  // runs it, once per panel; the loader never calls `callback` inside this
  // call. `spec` and its screen name are borrowed for
  // the call; `callback` and `user` are kept for the life of the process
  // and never freed by the loader.
  EnshwickStatus (*ui_extension)(EnshwickModHandle me,
                                 const struct EnshwickUiExtension *spec,
                                 EnshwickUiFrameFn callback,
                                 void *user);
  // Writes to `out` the game controls the local player holds, the
  // `Bitmask64` of the game's `PlayerInputType` as the client's
  // interface read it in its last step: gameplay controls only, not the
  // client-only ones such as menu keys. The bits are the game's, which a
  // game build may renumber: find each by its name among the constants
  // `EnshwickHostApi.types` lists for `PlayerInputType`, never as a fixed
  // value. The loader reads the mask only once a mod asked for it: the
  // first call, of any mod, starts the reads, and the interface's next
  // step reads it; the safe layer calls it when a mod opens its input.
  // Client only; `EnshwickStatus_UNAVAILABLE` on a server, outside a game,
  // from the first call until that step, while no step read it for a
  // second, or on a build without the read; `EnshwickStatus_BAD_ARGS` for a
  // null `out`. Any thread; `out` is borrowed for the call.
  EnshwickStatus (*local_held)(EnshwickModHandle me, uint64_t *out);
  // Copies the local input sampler's cached diagnostics without requesting reads.
  // Client only (`EnshwickStatus_UNAVAILABLE` on a server); callable on any thread.
  // No game memory is read. Flags are weakly consistent and age is not session identity.
  // `out` is a headed, writable `EnshwickLocalInputInfo`. ABI 12.1.
  EnshwickStatus (*local_input_status)(EnshwickModHandle me, struct EnshwickLocalInputInfo *out);
  // Writes to `out`, a headed `EnshwickClientSnapshot`, the client's state in
  // one copy: its session, the open screens and the in-game menu's tab
  // from the interface's last frame, and the local player's held
  // controls. The first call, of any mod, starts the interface's reads
  // and the held controls' reads, so it answers `EnshwickClientSnapshot_NEVER`;
  // the safe layer calls it when a mod opens its input. Every call after
  // a frame drew a game answers its copy; until one did, as in the main
  // menu before the first world, it answers `NEVER`. Client only:
  // `EnshwickStatus_UNAVAILABLE` on a server; `EnshwickStatus_BAD_ARGS` for a null
  // or unheaded `out`. Any thread; it runs no mod callback, and `out` is
  // borrowed for the call. ABI 12.1.
  EnshwickStatus (*client_snapshot)(EnshwickModHandle me, struct EnshwickClientSnapshot *out);
  // Up to `capacity` parts and features of the loader that are off, in
  // the order they turned off, and how many there are. Entries are
  // `entry_size` bytes apart, the caller's `sizeof(EnshwickLoaderOff)`; the
  // loader fills what both sides know of each and its own `EnshwickHeader`.
  // Everything it does not list runs. The strings stay valid for the
  // process, since the loader keeps each entry's first reason and drops
  // none. Any thread; it runs no mod callback. ABI 12.1.
  EnshwickStatus (*loader_off)(EnshwickModHandle me,
                               struct EnshwickLoaderOff *out,
                               uint32_t entry_size,
                               uint32_t capacity,
                               uint32_t *count);
  // Queues `spec`, a headed `EnshwickClientCommand`, and writes its id to `id`:
  // the call answers only that it is queued, and
  // `EnshwickHostApi.client_command_status` tells what came of it. One command
  // of each kind waits per mod: a second answers `EnshwickStatus_REFUSED`
  // until the first ends. `EnshwickStatus_BAD_ARGS` for an unknown kind, a
  // target the kind does not take, a look past
  // `EnshwickClientCommand_MAX_LOOK`, a direction longer than 1 or a hold past
  // `EnshwickClientCommand_MAX_HOLD_MS`; `EnshwickStatus_UNAVAILABLE` on a server
  // and on a build without what the kind needs. Any thread; the command
  // runs later on the client's game thread, never inside a mod callback.
  // ABI 12.1.
  EnshwickStatus (*client_command)(EnshwickModHandle me,
                                   const struct EnshwickClientCommand *spec,
                                   uint64_t *id);
  // Writes to `out`, a headed `EnshwickClientCommandStatus`, where the mod's
  // command `id` stands. The loader keeps the last 64 commands' outcomes;
  // `EnshwickStatus_UNAVAILABLE` for an id it no longer keeps or another mod's.
  // Any thread. ABI 12.1.
  EnshwickStatus (*client_command_status)(EnshwickModHandle me,
                                          uint64_t id,
                                          struct EnshwickClientCommandStatus *out);
  // Adds `material`, a headed `EnshwickTerrainMaterialAdd`, as a terrain
  // material of the game, and writes the voxel id it has for this run to
  // `id`. The loader gives mods' materials the terrain ids no palette of
  // the game names, in load order, and serves the material's palette
  // entry, rules and look on both sides; saves name its voxels by its
  // GUID. `EnshwickStatus_BAD_ARGS` for a `like` or `fallback` that is no
  // terrain material of the game or a change it does not know;
  // `EnshwickStatus_REFUSED` for a GUID whose content id the calling mod did
  // not declare with `declare_content`, for a
  // mod not of side both, for a GUID added before and once every free id
  // is taken; `EnshwickStatus_UNAVAILABLE` on a build whose archive lacks the
  // palette or the rules. Allowed during `enshwick_mod_init` only.
  // ABI 12.2.
  EnshwickStatus (*terrain_material_add)(EnshwickModHandle me,
                                         const struct EnshwickTerrainMaterialAdd *material,
                                         uint8_t *id);
  // Gives every object placed from `spec`'s item, a headed
  // `EnshwickInteractionPrompt`, its prompt: the game verb and the replaced
  // action in the server's world, the mod's text on this client. One
  // prompt per item across all mods. Allowed during `enshwick_mod_init`
  // only; `EnshwickStatus_BAD_ARGS` for a null or short `spec`, item 0, an
  // unknown flag or a prompt that changes nothing; `EnshwickStatus_REFUSED`
  // for an item another prompt has; `EnshwickStatus_UNAVAILABLE` for a verb
  // or an action on a build without the server's writes, while a build
  // without the client's text site turns the text off alone. ABI 12.3.
  EnshwickStatus (*interaction_prompt)(EnshwickModHandle me,
                                       const struct EnshwickInteractionPrompt *spec);
  // Registers `callback` to add lines to the tooltip and detail panel of
  // every item with an entity of its own, after the game's rows. It runs
  // on the client's main thread, once each time the tooltip shows another
  // item, with a `EnshwickUiTooltipItem` whose token takes up to
  // `EnshwickUiTooltipItem_MAX_LINES` `EnshwickUiWidget_LABEL` widgets through
  // `EnshwickHostApi.ui_widget`; the loader keeps the lines and draws them
  // each frame the tooltip shows. Client only, during init:
  // `EnshwickStatus_UNAVAILABLE` on a server, `EnshwickStatus_BAD_ARGS` for a null
  // callback. ABI 12.4.
  EnshwickStatus (*ui_item_tooltip)(EnshwickModHandle me,
                                    EnshwickUiTooltipFn callback,
                                    void *user);
  // Replaces the mod's change to the permissions of the player in game
  // with `account`: `deny` takes bits from the player, `grant` gives
  // bits, both masks of `EnshwickPlayerInfo`'s permission
  // bits; both 0 ends the change. The server holds the player to
  // `(group | every grant) & !every deny`, with `group` the bits of the
  // player's user group, from its next step on, so the game's own checks
  // refuse what a taken bit guards, and the loader's own reads of the
  // player's bits give the bits in force; the player's client still acts
  // by its group's bits. A change ends when the mod ends it or turns
  // off, when the player leaves and when the world ends; ending one
  // needs no player in game. `EnshwickStatus_BAD_ARGS` for account 0 or a bit
  // past the permissions; `EnshwickStatus_REFUSED` for a new change to an
  // account not in game; `EnshwickStatus_WRONG_THREAD` off the server's game
  // thread; `EnshwickStatus_UNAVAILABLE` outside a server and on a build
  // without the permission table, the step link or the player removal
  // link. ABI 12.5.
  EnshwickStatus (*change_permissions)(EnshwickModHandle me,
                                       EnshwickPlayerKey account,
                                       uint32_t deny,
                                       uint32_t grant);
  // Calls `callback` once for transaction `id` of this mod, with where it
  // stands and `user`, on the server's game thread after the first step
  // that ends with it no longer `EnshwickTransactionInfo_PENDING`: written
  // whole (`EnshwickTransactionInfo_APPLIED`, which says when a prop may still
  // be absent), or failed, refused, undoing or undone. A step at least
  // `within_ns` nanoseconds after the call runs it as it stands, still
  // pending perhaps. The `info` is null when the loader forgot the
  // transaction meanwhile. A step runs its due callbacks in the order
  // they were asked, and a callback may ask again. When a client's hosted
  // game ends, every callback kept runs with its transaction failed or
  // expired, never in the next game; a dedicated server that stops runs
  // none. A mod the loader turned off loses its callbacks unrun. `EnshwickStatus_BAD_ARGS` for a null `callback`;
  // `EnshwickStatus_MISSING` for an id this mod did not get or one the loader
  // forgot; `EnshwickStatus_UNAVAILABLE` once the transactions are off, which
  // runs every callback kept at once. Nothing is kept on an error. Any
  // thread. ABI 12.7.
  EnshwickStatus (*transaction_settled)(EnshwickModHandle me,
                                        uint64_t id,
                                        uint64_t within_ns,
                                        EnshwickTransactionSettledFn callback,
                                        void *user);
  // Copies the prop template and optional local placement box for `item`
  // into headed `out`. `BAD_ARGS` for an unknown item or one placing nothing;
  // `NOT_A_PROP` for creatures and flame altars; `UNAVAILABLE` without a running
  // server, registry or required reads. Any thread, read-only. ABI 12.8.
  EnshwickStatus (*prop_placement_info)(EnshwickModHandle me,
                                        EnshwickItemId item,
                                        struct EnshwickPropPlacementInfo *out);
  // Registers `callback` to rule which stacks enter the containers placed
  // from `item`, vanilla or a mod's own. The loader asks the rules of the
  // container's item in load order, until one refuses, before a stack
  // enters one of its slots, whatever moves it: a player's move, a swap, Take All, Deposit
  // All or Stacks, an area deposit; a stack a rule refuses stays where it
  // was, or goes to another slot when the game picks the slots. Gives
  // through `EnshwickHostApi.inventory_submit` and the game's own loot are not
  // asked. Allowed during `enshwick_mod_init` only; `EnshwickStatus_BAD_ARGS`
  // for item 0 or a null callback; `EnshwickStatus_UNAVAILABLE` on a build
  // without the slot link. ABI 12.9.
  EnshwickStatus (*container_rule)(EnshwickModHandle me,
                                   EnshwickItemId item,
                                   EnshwickContainerRuleFn callback,
                                   void *user);
  // Shows `character`'s player the game's pickup message for `count` of
  // `item`, as if the items had entered the player's inventory, which
  // they do not: the loader raises the game's pickup event on the server,
  // and every reader of that event, but the loader's item filters, hears
  // it. `count` above 65535 shows 65535, the game's most. On the server's
  // game thread between steps only; the systems of the next step see the
  // event. `EnshwickStatus_BAD_ARGS` for item 0, count 0 or an entity that is
  // no player; `EnshwickStatus_WRONG_THREAD` off the game thread or during a
  // step; `EnshwickStatus_UNAVAILABLE` outside a server and on a build without
  // the event. ABI 12.9.
  EnshwickStatus (*show_pickup)(EnshwickModHandle me,
                                EnshwickEntity character,
                                EnshwickItemId item,
                                uint32_t count);
  // Copies the respawn point of the player whose character is `character`
  // into headed `out`: the spawn point entity, its kind and where the
  // character respawns. `EnshwickStatus_MISSING` for an entity that is no
  // player's character, or a character whose respawn point is not in the
  // world, where the game falls back to the world's start;
  // `EnshwickStatus_WRONG_THREAD` off the server's game thread;
  // `EnshwickStatus_UNAVAILABLE` outside a server and on a build without the
  // layout. ABI 12.12.
  EnshwickStatus (*respawn_point)(EnshwickModHandle me,
                                  EnshwickEntity character,
                                  struct EnshwickRespawnPointInfo *out);
  // Makes `spawn_point` the respawn point of the player whose character
  // is `character`, as the game does when the character stands in a
  // spawn point, and raises the game's `PlayerSpawnInfoChangeEvent` for
  // it when the point names a base or a scene. While the character
  // stands in another spawn point, the game sets that one back at its
  // next step. Nothing changes when a call fails. `EnshwickStatus_BAD_ARGS` for an entity that is no
  // spawn point, or one of kind `EnshwickRespawnPointInfo_NO_RESPAWN`;
  // `EnshwickStatus_MISSING` for an entity that is no player's character;
  // `EnshwickStatus_WRONG_THREAD` off the server's game thread or during a
  // step; `EnshwickStatus_UNAVAILABLE` outside a server and on a build without
  // the layout. ABI 12.12.
  EnshwickStatus (*set_respawn_point)(EnshwickModHandle me,
                                      EnshwickEntity character,
                                      EnshwickEntity spawn_point);
  // Adds `buff`, a headed `EnshwickBuffTypeAdd`, as a buff type of the game,
  // and writes its game id to `id`. The loader serves one patch of the
  // game's buff collection on both sides with every mod's types after the
  // vanilla ones, each a copy of its `like` with its changes, and names
  // a type's icon on a client only once that client shows served UI
  // images. `EnshwickStatus_BAD_ARGS` for a change or flag it does not know, a
  // negative life time, or a `like` that is no buff type of the game;
  // `EnshwickStatus_REFUSED` for a GUID whose content id the calling mod did
  // not declare with `declare_content`, for a mod not of side both and
  // for an id the game or a mod loaded before has;
  // `EnshwickStatus_UNAVAILABLE` on a build whose
  // archive or layout lacks the collection. Allowed during
  // `enshwick_mod_init` only. ABI 12.13.
  EnshwickStatus (*buff_type_add)(EnshwickModHandle me,
                                  const struct EnshwickBuffTypeAdd *buff,
                                  EnshwickBuffTypeId *id);
  // Up to `capacity` damage causes, as `EnshwickDamageCheck`
  // numbers them, whose sites are not armed, and how many there are, as
  // every list answers: the causes no damage filter gets on this build
  // and run. A site arms when a mod registered a damage filter and the
  // build has it, and stops when it faults; with no filter, only the hit
  // site arms, and only for stats. `EnshwickStatus_UNAVAILABLE` until the
  // loader has linked and armed its hooks, after every mod's init. Any
  // thread. ABI 12.14.
  EnshwickStatus (*damage_causes_off)(EnshwickModHandle me,
                                      uint32_t *out,
                                      uint32_t capacity,
                                      uint32_t *count);
  // Writes to `out` the waypoint all players see that the character
  // `entity` has set now, read as `EnshwickEvent_WAYPOINT_CHANGED`
  // reads it: `set` 0 while it is off or for an entity without one, and
  // `key` 0 when no player online plays `entity`. `out` is the caller's,
  // at least a `EnshwickHeader` and writable for the size its header states;
  // `EnshwickStatus_BAD_ARGS` for a null or smaller one. On the server's game
  // thread at a quiet moment (`EnshwickStatus_WRONG_THREAD` elsewhere or
  // inside a step); `EnshwickStatus_UNAVAILABLE` before a world runs or on a
  // build whose reflection lacks the waypoint. ABI 12.16.
  EnshwickStatus (*waypoint)(EnshwickModHandle me,
                             EnshwickEntity entity,
                             struct EnshwickWaypointData *out);
  // Queues `spec`, a headed `EnshwickSoundPlay`, and writes the sound's id to
  // `id`: the client's game thread starts it in its next interface frame,
  // and `EnshwickHostApi.sound_state` tells what came of it. Any thread, also
  // outside a frame. `EnshwickStatus_MISSING` for a container neither the
  // game's archive nor a mod holds; `EnshwickStatus_BAD_ARGS` for an unknown
  // place, a volume outside 0 to 1, a fade past
  // `EnshwickSoundPlay_MAX_FADE_MS`, a non-finite position or entity 0 on
  // `EnshwickSoundPlay_ON`; `EnshwickStatus_BUSY` past
  // `EnshwickSoundPlay_MAX_STARTS` starts of the mod waiting for one frame;
  // `EnshwickStatus_UNAVAILABLE` on a server and on a build where the
  // loader's playback is off. ABI 12.17.
  EnshwickStatus (*sound_play)(EnshwickModHandle me,
                               const struct EnshwickSoundPlay *spec,
                               uint64_t *id);
  // Queues `steer`, a headed `EnshwickSoundSteer`, for the mod's sound `id`, or
  // for all of the mod's sounds with `EnshwickSoundSteer_STOP_ALL`. An id of a
  // sound that ended changes nothing; ids count per mod, so another
  // mod's id names at most one of this mod's own sounds. Any thread. `EnshwickStatus_BAD_ARGS` as for
  // `EnshwickHostApi.sound_play`; `EnshwickStatus_BUSY` when the mod's queue is
  // full; `EnshwickStatus_UNAVAILABLE` on a server. ABI 12.17.
  EnshwickStatus (*sound_steer)(EnshwickModHandle me,
                                uint64_t id,
                                const struct EnshwickSoundSteer *steer);
  // Writes to `out`, a headed `EnshwickSoundInfo`, where the mod's sound `id`
  // stands; an id the loader no longer keeps reads as
  // `EnshwickSoundInfo_ENDED`, and another mod's id at most one of this mod's
  // own sounds, since ids count per mod. Any thread. `EnshwickStatus_BAD_ARGS` for a null or
  // smaller `out`; `EnshwickStatus_UNAVAILABLE` on a server. ABI 12.17.
  EnshwickStatus (*sound_state)(EnshwickModHandle me, uint64_t id, struct EnshwickSoundInfo *out);
  // Sends `play`, a headed `EnshwickSoundPlay`, from the server to the clients
  // of `audience`, a headed `EnshwickSoundAudience`, that run Enshwick and the
  // mod, and writes the sound's id to `id`; each client plays it as the
  // mod's client part's own `EnshwickHostApi.sound_play` would, so a vanilla
  // client hears nothing. A hosting client's own player plays it in
  // process. The mod runs on both sides. Entity ids are the server
  // world's, which name no client entity, so `EnshwickSoundPlay_ON` plays
  // where the entity stands when it is sent, without following it.
  // Clients send nothing back, so the id only steers
  // (`EnshwickHostApi.sound_steer_for`). Any thread; a
  // `EnshwickSoundAudience_NEAR` audience or `EnshwickSoundPlay_ON` on the
  // server's game thread, where it reads where entities stand.
  // `EnshwickStatus_BAD_ARGS` for an audience or play out of bounds;
  // `EnshwickStatus_MISSING` for an entity of `EnshwickSoundPlay_ON` not in the
  // world; `EnshwickStatus_BUSY` past `EnshwickSoundAudience_MAX_PLAYS` sounds of
  // the mod in one server step; `EnshwickStatus_WRONG_THREAD` off the game
  // thread where it must read; `EnshwickStatus_REFUSED` for a mod that does
  // not run on both sides; `EnshwickStatus_UNAVAILABLE` where no server runs
  // in the process. ABI 12.18.
  EnshwickStatus (*sound_play_for)(EnshwickModHandle me,
                                   const struct EnshwickSoundAudience *audience,
                                   const struct EnshwickSoundPlay *play,
                                   uint64_t *id);
  // Sends `steer`, a headed `EnshwickSoundSteer`, for the mod's sound `id` from
  // `EnshwickHostApi.sound_play_for` to the clients that got it. An id of no
  // such sound changes nothing. `EnshwickSoundSteer_STOP_ALL` is not taken.
  // Any thread. `EnshwickStatus_BAD_ARGS` out of bounds and for
  // `EnshwickSoundSteer_STOP_ALL`; `EnshwickStatus_UNAVAILABLE` where no server
  // runs in the process. ABI 12.18.
  EnshwickStatus (*sound_steer_for)(EnshwickModHandle me,
                                    uint64_t id,
                                    const struct EnshwickSoundSteer *steer);
  // As `serve_resource` from `key` itself, for a
  // patch that serves `key` with the data of the resource `from`, a
  // re-key: inside `patch`, a `resource_served`
  // read of `from` builds it without its own re-keys, so the re-keys of
  // every mod apply at once, each key taking the data its `from` has
  // before any re-key, and keys re-keyed in a ring rotate.
  // `EnshwickStatus_BAD_ARGS` when `from` is `key` or of another type.
  // Allowed during `enshwick_mod_init` only. ABI 12.19.
  EnshwickStatus (*rekey_resource)(EnshwickModHandle me,
                                   const struct EnshwickResourceKey *key,
                                   const struct EnshwickResourceKey *from,
                                   EnshwickResourcePatchFn patch,
                                   void *user);
  // Writes to `out`, a headed `EnshwickGameBinding`, the keys the client's
  // keyboard and mouse bindings give the game action named `action`, as
  // its reflection names it (`ContextualAction`), a mod's rebind
  // included. The interface reads the bindings every few frames, so a
  // change the player applies shows within a quarter second.
  // `EnshwickStatus_UNAVAILABLE` on a server, before the interface drew its
  // first frame and on a build where the loader does not find the
  // bindings; `EnshwickStatus_MISSING` for an action the build does not name;
  // `EnshwickStatus_BAD_ARGS` for a null or unheaded `out`. Any thread; the
  // strings are borrowed for the call. ABI 12.20.
  EnshwickStatus (*game_binding)(EnshwickModHandle me,
                                 struct EnshwickStr action,
                                 struct EnshwickGameBinding *out);
  // Binds `key`, in the form of `EnshwickControlSpec.key`, as the first key of
  // the game action named `action` in the client's keyboard and mouse
  // bindings for the mod `me`, or with 0 ends `me`'s rebind of it. The
  // interface writes it at its next frame and again after each change
  // the player applies, which the game rebuilds its bindings from, so the
  // rebind holds until the mod ends it or turns off; the game's Controls
  // page never shows it. Of the mods that rebind an action, the one that
  // rebound it last wins; asking again for the same rebind keeps a mod's
  // place, and an action no mod rebinds holds the game's own key.
  // `EnshwickStatus_UNAVAILABLE` as for `EnshwickHostApi.game_binding`;
  // `EnshwickStatus_MISSING` for an action the build does not name;
  // `EnshwickStatus_BAD_ARGS` for a key a mod control may not take, or a
  // modifier the game's bindings do not list. Any thread. ABI 12.20.
  EnshwickStatus (*game_rebind)(EnshwickModHandle me, struct EnshwickStr action, uint32_t key);
  // Registers a filter on what the game pays a player for a quest, the
  // quest's experience, before it pays it and sets the quest's bool.
  // Filters run in load order, each with the payment as the filters
  // before left it, on the thread of the game's quest reward system, in
  // the dedicated server or a hosting client. `EnshwickStatus_UNAVAILABLE` on
  // a build where the loader does not find the payment, or where its
  // code is not what the loader expects; `EnshwickStatus_BAD_ARGS` for a null
  // callback. Allowed during `enshwick_mod_init` only. ABI 12.22.
  EnshwickStatus (*quest_reward_filter)(EnshwickModHandle me,
                                        EnshwickQuestRewardFilterFn callback,
                                        void *user);
  // Sets knowledge `id` of the player whose character is `entity`, or of
  // the world for 0 or an id of the world's, to exactly `value`: 0 or 1
  // for a `EnshwickKnowledgeInfo_BOOL`, any count or bits otherwise.
  //
  // The game keeps a mark beside each value, one bit per flag (all of
  // them for a bool or a counter), set for what the player has seen.
  // `EnshwickKnowledgeChange_QUIET` writes the value at once with every bit
  // marked, as a save loads it, sends it to the clients again and is
  // saved with the next save; nothing a grant brings follows.
  // `EnshwickKnowledgeChange_SHOWN` grants what the value gains as
  // `EnshwickHostApi.change_knowledge` does (the next step applies it with
  // what follows) and writes what it loses at once with no bit marked.
  //
  // `EnshwickStatus_BAD_ARGS` for an unknown id or `notice`, a bool above 1,
  // or, for `SHOWN`, a gained bit the game does not expect;
  // `EnshwickStatus_BUSY` when the game's queue fills while `SHOWN` grants;
  // `EnshwickStatus_REFUSED` as for `EnshwickHostApi.change_knowledge`. On the
  // server's game thread only. ABI 12.22.
  EnshwickStatus (*knowledge_set)(EnshwickModHandle me,
                                  EnshwickEntity entity,
                                  EnshwickKnowledgeId id,
                                  uint32_t value,
                                  uint8_t notice);
} EnshwickHostApi;

// The layout the loader was built with, from its golden layout file.
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickAccountActionType) == 1, "size of AccountActionType");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickAccountActionType) == 1, "alignment of AccountActionType");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickActionCall) == 96, "size of ActionCall");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickActionCall) == 8, "alignment of ActionCall");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActionCall, header) == 0, "offset of ActionCall.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActionCall, kind) == 8, "offset of ActionCall.kind");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActionCall, language) == 12, "offset of ActionCall.language");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActionCall, player) == 16, "offset of ActionCall.player");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActionCall, action) == 24, "offset of ActionCall.action");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActionCall, args) == 40, "offset of ActionCall.args");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActionCall, target) == 56, "offset of ActionCall.target");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActionCall, reply) == 64, "offset of ActionCall.reply");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActionCall, word) == 72, "offset of ActionCall.word");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActionCall, via) == 88, "offset of ActionCall.via");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActionCall, reason) == 92, "offset of ActionCall.reason");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickActionReply) == 32, "size of ActionReply");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickActionReply) == 8, "alignment of ActionReply");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActionReply, header) == 0, "offset of ActionReply.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActionReply, done) == 8, "offset of ActionReply.done");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActionReply, text) == 16, "offset of ActionReply.text");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickActionTarget) == 40, "size of ActionTarget");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickActionTarget) == 8, "alignment of ActionTarget");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActionTarget, header) == 0, "offset of ActionTarget.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActionTarget, flags) == 8, "offset of ActionTarget.flags");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActionTarget, entity) == 12, "offset of ActionTarget.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActionTarget, position) == 16, "offset of ActionTarget.position");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickActiveBuff) == 40, "size of ActiveBuff");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickActiveBuff) == 8, "alignment of ActiveBuff");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActiveBuff, header) == 0, "offset of ActiveBuff.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActiveBuff, id) == 8, "offset of ActiveBuff.id");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActiveBuff, entity) == 12, "offset of ActiveBuff.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActiveBuff, slot) == 16, "offset of ActiveBuff.slot");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActiveBuff, level) == 20, "offset of ActiveBuff.level");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActiveBuff, duration_ns) == 24, "offset of ActiveBuff.duration_ns");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickActiveBuff, remaining_ns) == 32, "offset of ActiveBuff.remaining_ns");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickAffixModifierSpec) == 20, "size of AffixModifierSpec");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickAffixModifierSpec) == 4, "alignment of AffixModifierSpec");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAffixModifierSpec, stat) == 0, "offset of AffixModifierSpec.stat");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAffixModifierSpec, op) == 4, "offset of AffixModifierSpec.op");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAffixModifierSpec, reserved) == 5, "offset of AffixModifierSpec.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAffixModifierSpec, min) == 8, "offset of AffixModifierSpec.min");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAffixModifierSpec, max) == 12, "offset of AffixModifierSpec.max");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAffixModifierSpec, step) == 16, "offset of AffixModifierSpec.step");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickAffixRoll) == 32, "size of AffixRoll");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickAffixRoll) == 8, "alignment of AffixRoll");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAffixRoll, header) == 0, "offset of AffixRoll.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAffixRoll, pide) == 8, "offset of AffixRoll.pide");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAffixRoll, item) == 12, "offset of AffixRoll.item");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAffixRoll, min) == 16, "offset of AffixRoll.min");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAffixRoll, max) == 17, "offset of AffixRoll.max");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAffixRoll, max_rarity) == 18, "offset of AffixRoll.max_rarity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAffixRoll, flags) == 19, "offset of AffixRoll.flags");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAffixRoll, seed) == 24, "offset of AffixRoll.seed");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickAffixSpec) == 48, "size of AffixSpec");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickAffixSpec) == 8, "alignment of AffixSpec");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAffixSpec, header) == 0, "offset of AffixSpec.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAffixSpec, group) == 8, "offset of AffixSpec.group");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAffixSpec, modifiers) == 24, "offset of AffixSpec.modifiers");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAffixSpec, modifier_count) == 32, "offset of AffixSpec.modifier_count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAffixSpec, weight) == 36, "offset of AffixSpec.weight");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAffixSpec, categories) == 40, "offset of AffixSpec.categories");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAffixSpec, rarity) == 44, "offset of AffixSpec.rarity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAffixSpec, reserved) == 45, "offset of AffixSpec.reserved");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickAltarInfo) == 856, "size of AltarInfo");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickAltarInfo) == 8, "alignment of AltarInfo");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAltarInfo, header) == 0, "offset of AltarInfo.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAltarInfo, entity) == 8, "offset of AltarInfo.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAltarInfo, level) == 12, "offset of AltarInfo.level");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAltarInfo, tilted) == 13, "offset of AltarInfo.tilted");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAltarInfo, level_count) == 14, "offset of AltarInfo.level_count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAltarInfo, reserved) == 15, "offset of AltarInfo.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAltarInfo, zone) == 16, "offset of AltarInfo.zone");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAltarInfo, levels) == 64, "offset of AltarInfo.levels");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAltarInfo, position) == 832, "offset of AltarInfo.position");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickAltarLimit) == 24, "size of AltarLimit");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickAltarLimit) == 4, "alignment of AltarLimit");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAltarLimit, header) == 0, "offset of AltarLimit.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAltarLimit, flame_level) == 8, "offset of AltarLimit.flame_level");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAltarLimit, vanilla) == 12, "offset of AltarLimit.vanilla");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAltarLimit, allowed) == 16, "offset of AltarLimit.allowed");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAltarLimit, reserved) == 20, "offset of AltarLimit.reserved");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickAltarZone) == 32, "size of AltarZone");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickAltarZone) == 8, "alignment of AltarZone");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAltarZone, header) == 0, "offset of AltarZone.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAltarZone, altar) == 8, "offset of AltarZone.altar");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAltarZone, level) == 12, "offset of AltarZone.level");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAltarZone, reserved) == 13, "offset of AltarZone.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAltarZone, grow) == 16, "offset of AltarZone.grow");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickAltarZone, height) == 24, "offset of AltarZone.height");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickBaseLimitInfo) == 36, "size of BaseLimitInfo");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickBaseLimitInfo) == 4, "alignment of BaseLimitInfo");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBaseLimitInfo, header) == 0, "offset of BaseLimitInfo.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBaseLimitInfo, zones) == 8, "offset of BaseLimitInfo.zones");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBaseLimitInfo, bases) == 12, "offset of BaseLimitInfo.bases");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBaseLimitInfo, vanilla_zones) == 16, "offset of BaseLimitInfo.vanilla_zones");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBaseLimitInfo, reserved) == 20, "offset of BaseLimitInfo.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBaseLimitInfo, vanilla_bases) == 24, "offset of BaseLimitInfo.vanilla_bases");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBaseLimitInfo, max_zones) == 28, "offset of BaseLimitInfo.max_zones");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBaseLimitInfo, max_bases) == 32, "offset of BaseLimitInfo.max_bases");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickBedCheck) == 72, "size of BedCheck");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickBedCheck) == 8, "alignment of BedCheck");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBedCheck, header) == 0, "offset of BedCheck.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBedCheck, npc_id) == 8, "offset of BedCheck.npc_id");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBedCheck, entity) == 12, "offset of BedCheck.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBedCheck, bed_id) == 16, "offset of BedCheck.bed_id");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBedCheck, home) == 24, "offset of BedCheck.home");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBedCheck, bed_at) == 48, "offset of BedCheck.bed_at");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickBlockPlace) == 56, "size of BlockPlace");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickBlockPlace) == 8, "alignment of BlockPlace");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBlockPlace, header) == 0, "offset of BlockPlace.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBlockPlace, shape) == 8, "offset of BlockPlace.shape");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBlockPlace, material) == 12, "offset of BlockPlace.material");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBlockPlace, position) == 16, "offset of BlockPlace.position");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBlockPlace, rotation) == 40, "offset of BlockPlace.rotation");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickBounds) == 48, "size of Bounds");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickBounds) == 8, "alignment of Bounds");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBounds, min) == 0, "offset of Bounds.min");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBounds, max) == 24, "offset of Bounds.max");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickBuffRequest) == 32, "size of BuffRequest");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickBuffRequest) == 8, "alignment of BuffRequest");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuffRequest, header) == 0, "offset of BuffRequest.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuffRequest, entity) == 8, "offset of BuffRequest.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuffRequest, id) == 12, "offset of BuffRequest.id");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuffRequest, level) == 16, "offset of BuffRequest.level");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuffRequest, reserved) == 20, "offset of BuffRequest.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuffRequest, duration_ns) == 24, "offset of BuffRequest.duration_ns");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickBuffTypeAdd) == 88, "size of BuffTypeAdd");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickBuffTypeAdd) == 8, "alignment of BuffTypeAdd");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuffTypeAdd, header) == 0, "offset of BuffTypeAdd.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuffTypeAdd, guid) == 8, "offset of BuffTypeAdd.guid");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuffTypeAdd, name) == 24, "offset of BuffTypeAdd.name");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuffTypeAdd, description) == 40, "offset of BuffTypeAdd.description");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuffTypeAdd, icon) == 56, "offset of BuffTypeAdd.icon");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuffTypeAdd, life_time_ns) == 72, "offset of BuffTypeAdd.life_time_ns");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuffTypeAdd, like) == 80, "offset of BuffTypeAdd.like");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuffTypeAdd, changes) == 84, "offset of BuffTypeAdd.changes");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuffTypeAdd, slot) == 86, "offset of BuffTypeAdd.slot");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuffTypeAdd, flags) == 87, "offset of BuffTypeAdd.flags");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickBuffTypeId) == 4, "size of BuffTypeId");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickBuffTypeId) == 4, "alignment of BuffTypeId");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickBuffTypeInfo) == 32, "size of BuffTypeInfo");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickBuffTypeInfo) == 8, "alignment of BuffTypeInfo");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuffTypeInfo, header) == 0, "offset of BuffTypeInfo.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuffTypeInfo, id) == 8, "offset of BuffTypeInfo.id");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuffTypeInfo, slot) == 12, "offset of BuffTypeInfo.slot");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuffTypeInfo, flags) == 14, "offset of BuffTypeInfo.flags");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuffTypeInfo, name) == 16, "offset of BuffTypeInfo.name");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuffTypeInfo, duration_ns) == 24, "offset of BuffTypeInfo.duration_ns");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickBuildCheck) == 56, "size of BuildCheck");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickBuildCheck) == 8, "alignment of BuildCheck");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildCheck, header) == 0, "offset of BuildCheck.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildCheck, character) == 8, "offset of BuildCheck.character");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildCheck, item) == 12, "offset of BuildCheck.item");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildCheck, position) == 16, "offset of BuildCheck.position");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildCheck, reason) == 40, "offset of BuildCheck.reason");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildCheck, account) == 48, "offset of BuildCheck.account");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickBuildInfo) == 48, "size of BuildInfo");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickBuildInfo) == 8, "alignment of BuildInfo");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildInfo, header) == 0, "offset of BuildInfo.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildInfo, side) == 8, "offset of BuildInfo.side");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildInfo, base) == 16, "offset of BuildInfo.base");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildInfo, fingerprint) == 24, "offset of BuildInfo.fingerprint");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildInfo, complete) == 44, "offset of BuildInfo.complete");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickBuildRights) == 64, "size of BuildRights");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickBuildRights) == 8, "alignment of BuildRights");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildRights, header) == 0, "offset of BuildRights.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildRights, character) == 8, "offset of BuildRights.character");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildRights, refusal) == 12, "offset of BuildRights.refusal");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildRights, min) == 16, "offset of BuildRights.min");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildRights, max) == 40, "offset of BuildRights.max");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickBuildRow) == 56, "size of BuildRow");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickBuildRow) == 8, "alignment of BuildRow");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildRow, side) == 0, "offset of BuildRow.side");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildRow, fingerprint) == 4, "offset of BuildRow.fingerprint");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildRow, symbols) == 24, "offset of BuildRow.symbols");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildRow, symbol_count) == 32, "offset of BuildRow.symbol_count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildRow, constants) == 40, "offset of BuildRow.constants");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildRow, constant_count) == 48, "offset of BuildRow.constant_count");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickBuildingData) == 64, "size of BuildingData");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickBuildingData) == 8, "alignment of BuildingData");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildingData, header) == 0, "offset of BuildingData.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildingData, entity) == 8, "offset of BuildingData.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildingData, character) == 12, "offset of BuildingData.character");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildingData, item) == 16, "offset of BuildingData.item");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildingData, kind) == 20, "offset of BuildingData.kind");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildingData, replay) == 21, "offset of BuildingData.replay");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildingData, template_) == 22, "offset of BuildingData.template");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildingData, position) == 40, "offset of BuildingData.position");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickBuildingZoneCheck) == 80, "size of BuildingZoneCheck");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickBuildingZoneCheck) == 8, "alignment of BuildingZoneCheck");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildingZoneCheck, header) == 0, "offset of BuildingZoneCheck.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildingZoneCheck, zones) == 8, "offset of BuildingZoneCheck.zones");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildingZoneCheck, zone_count) == 16, "offset of BuildingZoneCheck.zone_count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildingZoneCheck, object) == 24, "offset of BuildingZoneCheck.object");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickBuildingZoneCheck, margin) == 72, "offset of BuildingZoneCheck.margin");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickCallSpec) == 64, "size of CallSpec");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickCallSpec) == 8, "alignment of CallSpec");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickCallSpec, header) == 0, "offset of CallSpec.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickCallSpec, name) == 8, "offset of CallSpec.name");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickCallSpec, major) == 24, "offset of CallSpec.major");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickCallSpec, input) == 32, "offset of CallSpec.input");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickCallSpec, output) == 48, "offset of CallSpec.output");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickCallerFrame) == 72, "size of CallerFrame");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickCallerFrame) == 8, "alignment of CallerFrame");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickCallerFrame, rsp) == 0, "offset of CallerFrame.rsp");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickCallerFrame, rbx) == 8, "offset of CallerFrame.rbx");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickCallerFrame, rbp) == 16, "offset of CallerFrame.rbp");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickCallerFrame, rdi) == 24, "offset of CallerFrame.rdi");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickCallerFrame, rsi) == 32, "offset of CallerFrame.rsi");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickCallerFrame, r12) == 40, "offset of CallerFrame.r12");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickCallerFrame, r13) == 48, "offset of CallerFrame.r13");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickCallerFrame, r14) == 56, "offset of CallerFrame.r14");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickCallerFrame, r15) == 64, "offset of CallerFrame.r15");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickChangeId) == 4, "size of ChangeId");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickChangeId) == 4, "alignment of ChangeId");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickChangeOp) == 4, "size of ChangeOp");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickChangeOp) == 4, "alignment of ChangeOp");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickChatLine) == 48, "size of ChatLine");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickChatLine) == 8, "alignment of ChatLine");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickChatLine, header) == 0, "offset of ChatLine.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickChatLine, account) == 8, "offset of ChatLine.account");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickChatLine, entity) == 16, "offset of ChatLine.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickChatLine, kind) == 20, "offset of ChatLine.kind");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickChatLine, loca) == 24, "offset of ChatLine.loca");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickChatLine, text) == 32, "offset of ChatLine.text");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickChatRewrite) == 24, "size of ChatRewrite");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickChatRewrite) == 8, "alignment of ChatRewrite");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickChatRewrite, header) == 0, "offset of ChatRewrite.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickChatRewrite, buffer) == 8, "offset of ChatRewrite.buffer");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickChatRewrite, capacity) == 16, "offset of ChatRewrite.capacity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickChatRewrite, len) == 20, "offset of ChatRewrite.len");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickChunkData) == 20, "size of ChunkData");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickChunkData) == 4, "alignment of ChunkData");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickChunkData, header) == 0, "offset of ChunkData.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickChunkData, x) == 8, "offset of ChunkData.x");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickChunkData, z) == 12, "offset of ChunkData.z");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickChunkData, size) == 16, "offset of ChunkData.size");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickClientCommand) == 104, "size of ClientCommand");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickClientCommand) == 8, "alignment of ClientCommand");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientCommand, header) == 0, "offset of ClientCommand.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientCommand, kind) == 8, "offset of ClientCommand.kind");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientCommand, hold_ms) == 12, "offset of ClientCommand.hold_ms");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientCommand, session) == 16, "offset of ClientCommand.session");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientCommand, expected_screen) == 24, "offset of ClientCommand.expected_screen");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientCommand, target) == 40, "offset of ClientCommand.target");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientCommand, look) == 56, "offset of ClientCommand.look");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientCommand, direction) == 64, "offset of ClientCommand.direction");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientCommand, recipe) == 72, "offset of ClientCommand.recipe");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientCommand, reserved) == 76, "offset of ClientCommand.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientCommand, cancels) == 80, "offset of ClientCommand.cancels");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientCommand, password) == 88, "offset of ClientCommand.password");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickClientCommandStatus) == 40, "size of ClientCommandStatus");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickClientCommandStatus) == 8, "alignment of ClientCommandStatus");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientCommandStatus, header) == 0, "offset of ClientCommandStatus.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientCommandStatus, state) == 8, "offset of ClientCommandStatus.state");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientCommandStatus, reason) == 12, "offset of ClientCommandStatus.reason");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientCommandStatus, kind) == 16, "offset of ClientCommandStatus.kind");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientCommandStatus, reserved) == 20, "offset of ClientCommandStatus.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientCommandStatus, age_ms) == 24, "offset of ClientCommandStatus.age_ms");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientCommandStatus, phase) == 32, "offset of ClientCommandStatus.phase");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientCommandStatus, failure) == 36, "offset of ClientCommandStatus.failure");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickClientSnapshot) == 136, "size of ClientSnapshot");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickClientSnapshot) == 8, "alignment of ClientSnapshot");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientSnapshot, header) == 0, "offset of ClientSnapshot.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientSnapshot, state) == 8, "offset of ClientSnapshot.state");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientSnapshot, available) == 12, "offset of ClientSnapshot.available");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientSnapshot, session) == 16, "offset of ClientSnapshot.session");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientSnapshot, age_ms) == 24, "offset of ClientSnapshot.age_ms");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientSnapshot, scene) == 32, "offset of ClientSnapshot.scene");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientSnapshot, hud_screen) == 48, "offset of ClientSnapshot.hud_screen");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientSnapshot, return_screen) == 64, "offset of ClientSnapshot.return_screen");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientSnapshot, menu_tab) == 80, "offset of ClientSnapshot.menu_tab");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientSnapshot, held) == 96, "offset of ClientSnapshot.held");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientSnapshot, player) == 104, "offset of ClientSnapshot.player");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientSnapshot, host) == 108, "offset of ClientSnapshot.host");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientSnapshot, highlighted_recipe) == 112, "offset of ClientSnapshot.highlighted_recipe");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientSnapshot, reserved) == 116, "offset of ClientSnapshot.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickClientSnapshot, server) == 120, "offset of ClientSnapshot.server");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickComfort) == 40, "size of Comfort");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickComfort) == 8, "alignment of Comfort");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickComfort, header) == 0, "offset of Comfort.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickComfort, category) == 8, "offset of Comfort.category");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickComfort, value) == 12, "offset of Comfort.value");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickComfort, position) == 16, "offset of Comfort.position");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickCommandInfo) == 56, "size of CommandInfo");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickCommandInfo) == 8, "alignment of CommandInfo");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickCommandInfo, header) == 0, "offset of CommandInfo.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickCommandInfo, name) == 8, "offset of CommandInfo.name");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickCommandInfo, alias) == 24, "offset of CommandInfo.alias");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickCommandInfo, mod_id) == 40, "offset of CommandInfo.mod_id");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickContainerPut) == 32, "size of ContainerPut");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickContainerPut) == 8, "alignment of ContainerPut");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickContainerPut, header) == 0, "offset of ContainerPut.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickContainerPut, container) == 8, "offset of ContainerPut.container");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickContainerPut, container_item) == 12, "offset of ContainerPut.container_item");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickContainerPut, item) == 16, "offset of ContainerPut.item");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickContainerPut, count) == 20, "offset of ContainerPut.count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickContainerPut, slot) == 24, "offset of ContainerPut.slot");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickContentRename) == 32, "size of ContentRename");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickContentRename) == 1, "alignment of ContentRename");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickContentRename, from) == 0, "offset of ContentRename.from");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickContentRename, to) == 16, "offset of ContentRename.to");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickControlEvent) == 40, "size of ControlEvent");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickControlEvent) == 8, "alignment of ControlEvent");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickControlEvent, header) == 0, "offset of ControlEvent.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickControlEvent, name) == 8, "offset of ControlEvent.name");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickControlEvent, kind) == 24, "offset of ControlEvent.kind");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickControlEvent, server) == 28, "offset of ControlEvent.server");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickControlEvent, reserved) == 29, "offset of ControlEvent.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickControlEvent, player) == 32, "offset of ControlEvent.player");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickControlSpec) == 88, "size of ControlSpec");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickControlSpec) == 8, "alignment of ControlSpec");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickControlSpec, header) == 0, "offset of ControlSpec.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickControlSpec, name) == 8, "offset of ControlSpec.name");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickControlSpec, label) == 24, "offset of ControlSpec.label");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickControlSpec, key) == 40, "offset of ControlSpec.key");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickControlSpec, second) == 44, "offset of ControlSpec.second");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickControlSpec, link) == 48, "offset of ControlSpec.link");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickControlSpec, fires) == 64, "offset of ControlSpec.fires");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickControlSpec, hold_ms) == 68, "offset of ControlSpec.hold_ms");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickControlSpec, action) == 72, "offset of ControlSpec.action");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickCraftCheck) == 24, "size of CraftCheck");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickCraftCheck) == 4, "alignment of CraftCheck");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickCraftCheck, header) == 0, "offset of CraftCheck.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickCraftCheck, crafting_operator_id) == 8, "offset of CraftCheck.crafting_operator_id");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickCraftCheck, crafting_station_id) == 12, "offset of CraftCheck.crafting_station_id");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickCraftCheck, recipe_id) == 16, "offset of CraftCheck.recipe_id");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickCraftCheck, recipe_amount) == 20, "offset of CraftCheck.recipe_amount");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickCraftCheck, reserved) == 22, "offset of CraftCheck.reserved");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickDamageCheck) == 40, "size of DamageCheck");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickDamageCheck) == 8, "alignment of DamageCheck");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDamageCheck, header) == 0, "offset of DamageCheck.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDamageCheck, cause) == 8, "offset of DamageCheck.cause");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDamageCheck, target) == 12, "offset of DamageCheck.target");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDamageCheck, source) == 16, "offset of DamageCheck.source");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDamageCheck, attacker) == 20, "offset of DamageCheck.attacker");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDamageCheck, total) == 24, "offset of DamageCheck.total");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDamageCheck, amount_count) == 28, "offset of DamageCheck.amount_count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDamageCheck, amounts) == 32, "offset of DamageCheck.amounts");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickDataChange) == 80, "size of DataChange");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickDataChange) == 8, "alignment of DataChange");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDataChange, header) == 0, "offset of DataChange.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDataChange, entry) == 8, "offset of DataChange.entry");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDataChange, field) == 24, "offset of DataChange.field");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDataChange, op) == 28, "offset of DataChange.op");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDataChange, value) == 32, "offset of DataChange.value");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDataChange, source) == 40, "offset of DataChange.source");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDataChange, item) == 56, "offset of DataChange.item");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDataChange, select) == 72, "offset of DataChange.select");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDataChange, categories) == 76, "offset of DataChange.categories");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickDataField) == 4, "size of DataField");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickDataField) == 4, "alignment of DataField");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickDataSelect) == 4, "size of DataSelect");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickDataSelect) == 4, "alignment of DataSelect");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickDaytimeInfo) == 56, "size of DaytimeInfo");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickDaytimeInfo) == 8, "alignment of DaytimeInfo");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDaytimeInfo, header) == 0, "offset of DaytimeInfo.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDaytimeInfo, time_ns) == 8, "offset of DaytimeInfo.time_ns");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDaytimeInfo, day_start_ns) == 16, "offset of DaytimeInfo.day_start_ns");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDaytimeInfo, day_end_ns) == 24, "offset of DaytimeInfo.day_end_ns");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDaytimeInfo, day_length_ns) == 32, "offset of DaytimeInfo.day_length_ns");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDaytimeInfo, night_length_ns) == 40, "offset of DaytimeInfo.night_length_ns");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDaytimeInfo, speed) == 48, "offset of DaytimeInfo.speed");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDaytimeInfo, flags) == 52, "offset of DaytimeInfo.flags");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickDependency) == 40, "size of Dependency");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickDependency) == 8, "alignment of Dependency");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDependency, id) == 0, "offset of Dependency.id");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDependency, range) == 16, "offset of Dependency.range");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDependency, relation) == 32, "offset of Dependency.relation");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickDropRequest) == 48, "size of DropRequest");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickDropRequest) == 8, "alignment of DropRequest");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDropRequest, header) == 0, "offset of DropRequest.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDropRequest, item) == 8, "offset of DropRequest.item");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDropRequest, count) == 12, "offset of DropRequest.count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDropRequest, position) == 16, "offset of DropRequest.position");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDropRequest, level) == 40, "offset of DropRequest.level");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickDropRequest, rarity) == 44, "offset of DropRequest.rarity");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickEntity) == 4, "size of Entity");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickEntity) == 4, "alignment of Entity");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickEntityDataFlags) == 4, "size of EntityDataFlags");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickEntityDataFlags) == 4, "alignment of EntityDataFlags");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickEntityPlace) == 32, "size of EntityPlace");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickEntityPlace) == 8, "alignment of EntityPlace");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickEntityPlace, entity) == 0, "offset of EntityPlace.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickEntityPlace, reserved) == 4, "offset of EntityPlace.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickEntityPlace, position) == 8, "offset of EntityPlace.position");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickEvent) == 4, "size of Event");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickEvent) == 4, "alignment of Event");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickEventSpec) == 40, "size of EventSpec");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickEventSpec) == 8, "alignment of EventSpec");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickEventSpec, header) == 0, "offset of EventSpec.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickEventSpec, name) == 8, "offset of EventSpec.name");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickEventSpec, layout) == 24, "offset of EventSpec.layout");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickEventTypeInfo) == 32, "size of EventTypeInfo");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickEventTypeInfo) == 8, "alignment of EventTypeInfo");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickEventTypeInfo, header) == 0, "offset of EventTypeInfo.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickEventTypeInfo, name) == 8, "offset of EventTypeInfo.name");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickEventTypeInfo, hash) == 24, "offset of EventTypeInfo.hash");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickEventTypeInfo, size) == 28, "offset of EventTypeInfo.size");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickExperiencePass) == 40, "size of ExperiencePass");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickExperiencePass) == 8, "alignment of ExperiencePass");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickExperiencePass, header) == 0, "offset of ExperiencePass.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickExperiencePass, rows) == 8, "offset of ExperiencePass.rows");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickExperiencePass, row_count) == 16, "offset of ExperiencePass.row_count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickExperiencePass, level_cap) == 20, "offset of ExperiencePass.level_cap");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickExperiencePass, need_for) == 24, "offset of ExperiencePass.need_for");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickExperiencePass, token) == 32, "offset of ExperiencePass.token");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickExperienceRow) == 20, "size of ExperienceRow");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickExperienceRow) == 4, "alignment of ExperienceRow");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickExperienceRow, entity) == 0, "offset of ExperienceRow.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickExperienceRow, level) == 4, "offset of ExperienceRow.level");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickExperienceRow, progress) == 8, "offset of ExperienceRow.progress");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickExperienceRow, need) == 12, "offset of ExperienceRow.need");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickExperienceRow, gained) == 16, "offset of ExperienceRow.gained");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickExtent) == 48, "size of Extent");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickExtent) == 8, "alignment of Extent");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickExtent, min) == 0, "offset of Extent.min");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickExtent, max) == 24, "offset of Extent.max");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickFingerprint) == 20, "size of Fingerprint");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickFingerprint) == 4, "alignment of Fingerprint");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickFingerprint, guid) == 0, "offset of Fingerprint.guid");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickFingerprint, age) == 16, "offset of Fingerprint.age");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickFogInfo) == 72, "size of FogInfo");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickFogInfo) == 8, "alignment of FogInfo");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickFogInfo, header) == 0, "offset of FogInfo.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickFogInfo, width) == 8, "offset of FogInfo.width");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickFogInfo, height) == 12, "offset of FogInfo.height");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickFogInfo, metres_per_pixel) == 16, "offset of FogInfo.metres_per_pixel");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickFogInfo, size_z) == 20, "offset of FogInfo.size_z");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickFogInfo, version) == 24, "offset of FogInfo.version");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickFogInfo, session) == 32, "offset of FogInfo.session");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickFogInfo, uncovered) == 40, "offset of FogInfo.uncovered");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickFogInfo, uncovered_pixels) == 48, "offset of FogInfo.uncovered_pixels");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickFogInfo, reserved) == 52, "offset of FogInfo.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickFogInfo, uncovered_min) == 56, "offset of FogInfo.uncovered_min");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickFogInfo, uncovered_size) == 64, "offset of FogInfo.uncovered_size");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickGameBinding) == 16, "size of GameBinding");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickGameBinding) == 4, "alignment of GameBinding");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickGameBinding, header) == 0, "offset of GameBinding.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickGameBinding, first) == 8, "offset of GameBinding.first");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickGameBinding, second) == 12, "offset of GameBinding.second");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickGameEventData) == 24, "size of GameEventData");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickGameEventData) == 8, "alignment of GameEventData");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickGameEventData, header) == 0, "offset of GameEventData.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickGameEventData, type_hash) == 8, "offset of GameEventData.type_hash");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickGameEventData, size) == 12, "offset of GameEventData.size");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickGameEventData, data) == 16, "offset of GameEventData.data");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickGuid) == 16, "size of Guid");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickGuid) == 1, "alignment of Guid");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickHeader) == 8, "size of Header");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickHeader) == 4, "alignment of Header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHeader, size) == 0, "offset of Header.size");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHeader, abi_major) == 4, "offset of Header.abi_major");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHeader, abi_minor) == 6, "offset of Header.abi_minor");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickHookFlags) == 4, "size of HookFlags");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickHookFlags) == 4, "alignment of HookFlags");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickHookSpec) == 40, "size of HookSpec");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickHookSpec) == 8, "alignment of HookSpec");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHookSpec, header) == 0, "offset of HookSpec.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHookSpec, target) == 8, "offset of HookSpec.target");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHookSpec, detour) == 16, "offset of HookSpec.detour");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHookSpec, next) == 24, "offset of HookSpec.next");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHookSpec, priority) == 32, "offset of HookSpec.priority");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHookSpec, flags) == 36, "offset of HookSpec.flags");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickHostApi) == 1864, "size of HostApi");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickHostApi) == 8, "alignment of HostApi");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, header) == 0, "offset of HostApi.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, host_version) == 8, "offset of HostApi.host_version");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, build) == 24, "offset of HostApi.build");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, symbol) == 32, "offset of HostApi.symbol");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, system) == 40, "offset of HostApi.system");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, field_offset) == 48, "offset of HostApi.field_offset");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, type_size) == 56, "offset of HostApi.type_size");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, hook) == 64, "offset of HostApi.hook");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, caller_frame) == 72, "offset of HostApi.caller_frame");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, fault) == 80, "offset of HostApi.fault");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, log) == 88, "offset of HostApi.log");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, settings) == 96, "offset of HostApi.settings");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, publish) == 104, "offset of HostApi.publish");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, service) == 112, "offset of HostApi.service");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, constant) == 120, "offset of HostApi.constant");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, data_dir) == 128, "offset of HostApi.data_dir");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, log_level) == 136, "offset of HostApi.log_level");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, subscribe) == 144, "offset of HostApi.subscribe");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, type_hash) == 152, "offset of HostApi.type_hash");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, component) == 160, "offset of HostApi.component");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, entity_template) == 168, "offset of HostApi.entity_template");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, mods) == 176, "offset of HostApi.mods");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, code_range) == 184, "offset of HostApi.code_range");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, patch_code) == 192, "offset of HostApi.patch_code");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, workshop_recipes) == 200, "offset of HostApi.workshop_recipes");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, hosting) == 208, "offset of HostApi.hosting");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, server_world) == 216, "offset of HostApi.server_world");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, join_missing) == 224, "offset of HostApi.join_missing");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, hook_at) == 232, "offset of HostApi.hook_at");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, status) == 240, "offset of HostApi.status");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, subscribe_game_event) == 248, "offset of HostApi.subscribe_game_event");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, net_channel) == 256, "offset of HostApi.net_channel");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, net_send) == 264, "offset of HostApi.net_send");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, net_ready) == 272, "offset of HostApi.net_ready");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, config_schema) == 280, "offset of HostApi.config_schema");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, config_check) == 288, "offset of HostApi.config_check");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, texts) == 296, "offset of HostApi.texts");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, define_event) == 304, "offset of HostApi.define_event");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, subscribe_mod_event) == 312, "offset of HostApi.subscribe_mod_event");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, send_event) == 320, "offset of HostApi.send_event");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, rename_content) == 328, "offset of HostApi.rename_content");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, mod_dir) == 336, "offset of HostApi.mod_dir");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, language) == 344, "offset of HostApi.language");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, define_point) == 352, "offset of HostApi.define_point");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, attach_point) == 360, "offset of HostApi.attach_point");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, run_point) == 368, "offset of HostApi.run_point");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, event_types) == 376, "offset of HostApi.event_types");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, feature) == 384, "offset of HostApi.feature");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, loop_wait) == 392, "offset of HostApi.loop_wait");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, server_wait) == 400, "offset of HostApi.server_wait");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, task_workers) == 408, "offset of HostApi.task_workers");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, serve_call) == 416, "offset of HostApi.serve_call");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, find_call) == 424, "offset of HostApi.find_call");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, call) == 432, "offset of HostApi.call");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, subscribe_tick) == 440, "offset of HostApi.subscribe_tick");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, set_tick_every) == 448, "offset of HostApi.set_tick_every");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, set_config) == 456, "offset of HostApi.set_config");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, subscribe_client_game_event) == 464, "offset of HostApi.subscribe_client_game_event");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, serve_resource) == 472, "offset of HostApi.serve_resource");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, add_text) == 480, "offset of HostApi.add_text");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, define_item_data) == 488, "offset of HostApi.define_item_data");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, item_data_get) == 496, "offset of HostApi.item_data_get");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, set_item_data) == 504, "offset of HostApi.set_item_data");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, content_serve) == 512, "offset of HostApi.content_serve");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, content_read) == 520, "offset of HostApi.content_read");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, icons_ready) == 528, "offset of HostApi.icons_ready");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, declare_content) == 536, "offset of HostApi.declare_content");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, define_entity_data) == 544, "offset of HostApi.define_entity_data");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, resource_read) == 552, "offset of HostApi.resource_read");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, game_text) == 560, "offset of HostApi.game_text");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, rebuild_resource) == 568, "offset of HostApi.rebuild_resource");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, recipe_rule) == 576, "offset of HostApi.recipe_rule");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, recipe_disabled) == 584, "offset of HostApi.recipe_disabled");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, recipe_kind) == 592, "offset of HostApi.recipe_kind");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, rate) == 600, "offset of HostApi.rate");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, set_rate) == 608, "offset of HostApi.set_rate");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, change_data) == 616, "offset of HostApi.change_data");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, set_change) == 624, "offset of HostApi.set_change");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, use_content) == 632, "offset of HostApi.use_content");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, resource_keys) == 640, "offset of HostApi.resource_keys");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, content_hashes) == 648, "offset of HostApi.content_hashes");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, types) == 656, "offset of HostApi.types");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, game_version) == 664, "offset of HostApi.game_version");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, crafting_prop) == 672, "offset of HostApi.crafting_prop");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, read_only) == 680, "offset of HostApi.read_only");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, resource_served) == 688, "offset of HostApi.resource_served");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, resource_buffer) == 696, "offset of HostApi.resource_buffer");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, content_name) == 704, "offset of HostApi.content_name");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, player) == 712, "offset of HostApi.player");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, players) == 720, "offset of HostApi.players");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, chat) == 728, "offset of HostApi.chat");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, chat_all) == 736, "offset of HostApi.chat_all");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, set_label) == 744, "offset of HostApi.set_label");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, rename_filter) == 752, "offset of HostApi.rename_filter");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, item_info) == 760, "offset of HostApi.item_info");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, request_altars) == 768, "offset of HostApi.request_altars");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, altars) == 776, "offset of HostApi.altars");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, building_zone_filter) == 784, "offset of HostApi.building_zone_filter");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, base_zones) == 792, "offset of HostApi.base_zones");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, entities) == 800, "offset of HostApi.entities");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, destroy) == 808, "offset of HostApi.destroy");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, save_world) == 816, "offset of HostApi.save_world");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, world_key) == 824, "offset of HostApi.world_key");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, component_write) == 832, "offset of HostApi.component_write");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, notify) == 840, "offset of HostApi.notify");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, time_of_day) == 848, "offset of HostApi.time_of_day");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, set_time_of_day) == 856, "offset of HostApi.set_time_of_day");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, request_time_speed) == 864, "offset of HostApi.request_time_speed");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, set_time_speed) == 872, "offset of HostApi.set_time_speed");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, game_settings) == 880, "offset of HostApi.game_settings");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, set_game_settings) == 888, "offset of HostApi.set_game_settings");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, buff_types) == 896, "offset of HostApi.buff_types");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, buffs) == 904, "offset of HostApi.buffs");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, apply_buff) == 912, "offset of HostApi.apply_buff");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, remove_buff) == 920, "offset of HostApi.remove_buff");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, weather_at) == 928, "offset of HostApi.weather_at");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, weather_fronts) == 936, "offset of HostApi.weather_fronts");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, set_weather) == 944, "offset of HostApi.set_weather");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, terrain_materials) == 952, "offset of HostApi.terrain_materials");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, terrain_edit) == 960, "offset of HostApi.terrain_edit");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, altar_zone_filter) == 968, "offset of HostApi.altar_zone_filter");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, altar_zones_changed) == 976, "offset of HostApi.altar_zones_changed");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, altar_limit_filter) == 984, "offset of HostApi.altar_limit_filter");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, base_limit) == 992, "offset of HostApi.base_limit");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, voxel_read) == 1000, "offset of HostApi.voxel_read");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, build_rights) == 1008, "offset of HostApi.build_rights");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, transaction_commit) == 1016, "offset of HostApi.transaction_commit");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, transaction_undo) == 1024, "offset of HostApi.transaction_undo");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, transaction_status) == 1032, "offset of HostApi.transaction_status");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, world_quiet) == 1040, "offset of HostApi.world_quiet");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, transaction_entities) == 1048, "offset of HostApi.transaction_entities");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, world_persists) == 1056, "offset of HostApi.world_persists");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, map_markers) == 1064, "offset of HostApi.map_markers");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, account_action) == 1072, "offset of HostApi.account_action");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, map_frame) == 1080, "offset of HostApi.map_frame");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, request_base_limits) == 1088, "offset of HostApi.request_base_limits");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, set_chat_mute) == 1096, "offset of HostApi.set_chat_mute");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, chat_muted) == 1104, "offset of HostApi.chat_muted");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, set_base_limit) == 1112, "offset of HostApi.set_base_limit");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, save_info) == 1120, "offset of HostApi.save_info");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, hold_saves) == 1128, "offset of HostApi.hold_saves");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, set_saved) == 1136, "offset of HostApi.set_saved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, progress_write) == 1144, "offset of HostApi.progress_write");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, server_stop) == 1152, "offset of HostApi.server_stop");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, fog_uncover) == 1160, "offset of HostApi.fog_uncover");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, language_of) == 1168, "offset of HostApi.language_of");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, join_rule) == 1176, "offset of HostApi.join_rule");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, request_inventory) == 1184, "offset of HostApi.request_inventory");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, inventory_submit) == 1192, "offset of HostApi.inventory_submit");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, survivor_lookup) == 1200, "offset of HostApi.survivor_lookup");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, survivor_bed_check) == 1208, "offset of HostApi.survivor_bed_check");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, summon_filter) == 1216, "offset of HostApi.summon_filter");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, summon_item) == 1224, "offset of HostApi.summon_item");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, survivor_status) == 1232, "offset of HostApi.survivor_status");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, emit) == 1240, "offset of HostApi.emit");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, teleport) == 1248, "offset of HostApi.teleport");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, spawn) == 1256, "offset of HostApi.spawn");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, damage_filter) == 1264, "offset of HostApi.damage_filter");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, summoned) == 1272, "offset of HostApi.summoned");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, define_stat) == 1280, "offset of HostApi.define_stat");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, stat_info) == 1288, "offset of HostApi.stat_info");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, stat_value) == 1296, "offset of HostApi.stat_value");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, stat_modify) == 1304, "offset of HostApi.stat_modify");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, stat_buff) == 1312, "offset of HostApi.stat_buff");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, stats_changed) == 1320, "offset of HostApi.stats_changed");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, stat_sources) == 1328, "offset of HostApi.stat_sources");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, stat_watch) == 1336, "offset of HostApi.stat_watch");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, define_affix) == 1344, "offset of HostApi.define_affix");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, affix_roll) == 1352, "offset of HostApi.affix_roll");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, item_lines) == 1360, "offset of HostApi.item_lines");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, affix_info) == 1368, "offset of HostApi.affix_info");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, item_filter) == 1376, "offset of HostApi.item_filter");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, drop_items) == 1384, "offset of HostApi.drop_items");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, skill_override) == 1392, "offset of HostApi.skill_override");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, comfort_filter) == 1400, "offset of HostApi.comfort_filter");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, experience_filter) == 1408, "offset of HostApi.experience_filter");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, knowledge) == 1416, "offset of HostApi.knowledge");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, change_knowledge) == 1424, "offset of HostApi.change_knowledge");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, quests) == 1432, "offset of HostApi.quests");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, knowledge_input_filter) == 1440, "offset of HostApi.knowledge_input_filter");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, game_query) == 1448, "offset of HostApi.game_query");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, run_console) == 1456, "offset of HostApi.run_console");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, interaction_filter) == 1464, "offset of HostApi.interaction_filter");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, craft_filter) == 1472, "offset of HostApi.craft_filter");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, build_filter) == 1480, "offset of HostApi.build_filter");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, actions) == 1488, "offset of HostApi.actions");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, control) == 1496, "offset of HostApi.control");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, run_action) == 1504, "offset of HostApi.run_action");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, chat_filter) == 1512, "offset of HostApi.chat_filter");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, commands) == 1520, "offset of HostApi.commands");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, run_command) == 1528, "offset of HostApi.run_command");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, command_list) == 1536, "offset of HostApi.command_list");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, actions_changed) == 1544, "offset of HostApi.actions_changed");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, action_window) == 1552, "offset of HostApi.action_window");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, ui_frame) == 1560, "offset of HostApi.ui_frame");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, ui_widget) == 1568, "offset of HostApi.ui_widget");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, ui_dialog) == 1576, "offset of HostApi.ui_dialog");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, ui_hud) == 1584, "offset of HostApi.ui_hud");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, ui_markers) == 1592, "offset of HostApi.ui_markers");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, request_fog) == 1600, "offset of HostApi.request_fog");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, fog_read) == 1608, "offset of HostApi.fog_read");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, map_marker_types) == 1616, "offset of HostApi.map_marker_types");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, ui_extension) == 1624, "offset of HostApi.ui_extension");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, local_held) == 1632, "offset of HostApi.local_held");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, local_input_status) == 1640, "offset of HostApi.local_input_status");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, client_snapshot) == 1648, "offset of HostApi.client_snapshot");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, loader_off) == 1656, "offset of HostApi.loader_off");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, client_command) == 1664, "offset of HostApi.client_command");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, client_command_status) == 1672, "offset of HostApi.client_command_status");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, terrain_material_add) == 1680, "offset of HostApi.terrain_material_add");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, interaction_prompt) == 1688, "offset of HostApi.interaction_prompt");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, ui_item_tooltip) == 1696, "offset of HostApi.ui_item_tooltip");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, change_permissions) == 1704, "offset of HostApi.change_permissions");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, transaction_settled) == 1712, "offset of HostApi.transaction_settled");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, prop_placement_info) == 1720, "offset of HostApi.prop_placement_info");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, container_rule) == 1728, "offset of HostApi.container_rule");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, show_pickup) == 1736, "offset of HostApi.show_pickup");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, respawn_point) == 1744, "offset of HostApi.respawn_point");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, set_respawn_point) == 1752, "offset of HostApi.set_respawn_point");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, buff_type_add) == 1760, "offset of HostApi.buff_type_add");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, damage_causes_off) == 1768, "offset of HostApi.damage_causes_off");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, waypoint) == 1776, "offset of HostApi.waypoint");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, sound_play) == 1784, "offset of HostApi.sound_play");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, sound_steer) == 1792, "offset of HostApi.sound_steer");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, sound_state) == 1800, "offset of HostApi.sound_state");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, sound_play_for) == 1808, "offset of HostApi.sound_play_for");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, sound_steer_for) == 1816, "offset of HostApi.sound_steer_for");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, rekey_resource) == 1824, "offset of HostApi.rekey_resource");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, game_binding) == 1832, "offset of HostApi.game_binding");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, game_rebind) == 1840, "offset of HostApi.game_rebind");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, quest_reward_filter) == 1848, "offset of HostApi.quest_reward_filter");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickHostApi, knowledge_set) == 1856, "offset of HostApi.knowledge_set");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickInteraction) == 96, "size of Interaction");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickInteraction) == 8, "alignment of Interaction");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInteraction, header) == 0, "offset of Interaction.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInteraction, kind) == 8, "offset of Interaction.kind");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInteraction, character) == 12, "offset of Interaction.character");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInteraction, target) == 16, "offset of Interaction.target");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInteraction, item) == 20, "offset of Interaction.item");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInteraction, known) == 24, "offset of Interaction.known");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInteraction, reserved) == 25, "offset of Interaction.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInteraction, position) == 32, "offset of Interaction.position");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInteraction, moved) == 56, "offset of Interaction.moved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInteraction, stack) == 60, "offset of Interaction.stack");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInteraction, amount) == 64, "offset of Interaction.amount");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInteraction, from) == 68, "offset of Interaction.from");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInteraction, to) == 72, "offset of Interaction.to");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInteraction, action) == 76, "offset of Interaction.action");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInteraction, transfer) == 77, "offset of Interaction.transfer");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInteraction, moving) == 78, "offset of Interaction.moving");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInteraction, reserved_move) == 79, "offset of Interaction.reserved_move");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInteraction, from_slot) == 80, "offset of Interaction.from_slot");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInteraction, to_slot) == 88, "offset of Interaction.to_slot");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickInteractionPrompt) == 24, "size of InteractionPrompt");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickInteractionPrompt) == 4, "alignment of InteractionPrompt");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInteractionPrompt, header) == 0, "offset of InteractionPrompt.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInteractionPrompt, item) == 8, "offset of InteractionPrompt.item");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInteractionPrompt, verb) == 12, "offset of InteractionPrompt.verb");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInteractionPrompt, text) == 16, "offset of InteractionPrompt.text");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInteractionPrompt, flags) == 20, "offset of InteractionPrompt.flags");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickInventoryOp) == 32, "size of InventoryOp");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickInventoryOp) == 4, "alignment of InventoryOp");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInventoryOp, kind) == 0, "offset of InventoryOp.kind");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInventoryOp, flags) == 4, "offset of InventoryOp.flags");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInventoryOp, target) == 8, "offset of InventoryOp.target");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInventoryOp, slot) == 12, "offset of InventoryOp.slot");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInventoryOp, other) == 16, "offset of InventoryOp.other");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInventoryOp, fill) == 20, "offset of InventoryOp.fill");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInventoryOp, item) == 24, "offset of InventoryOp.item");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInventoryOp, count) == 28, "offset of InventoryOp.count");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickInventoryOpResult) == 8, "size of InventoryOpResult");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickInventoryOpResult) == 4, "alignment of InventoryOpResult");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInventoryOpResult, error) == 0, "offset of InventoryOpResult.error");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInventoryOpResult, remaining) == 4, "offset of InventoryOpResult.remaining");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickInventoryRequest) == 32, "size of InventoryRequest");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickInventoryRequest) == 8, "alignment of InventoryRequest");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInventoryRequest, header) == 0, "offset of InventoryRequest.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInventoryRequest, flags) == 8, "offset of InventoryRequest.flags");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInventoryRequest, actor) == 12, "offset of InventoryRequest.actor");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInventoryRequest, ops) == 16, "offset of InventoryRequest.ops");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInventoryRequest, op_count) == 24, "offset of InventoryRequest.op_count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInventoryRequest, reserved) == 28, "offset of InventoryRequest.reserved");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickInventoryResult) == 24, "size of InventoryResult");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickInventoryResult) == 8, "alignment of InventoryResult");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInventoryResult, header) == 0, "offset of InventoryResult.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInventoryResult, outcome) == 8, "offset of InventoryResult.outcome");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInventoryResult, op_count) == 12, "offset of InventoryResult.op_count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickInventoryResult, ops) == 16, "offset of InventoryResult.ops");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickItemAction) == 32, "size of ItemAction");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickItemAction) == 4, "alignment of ItemAction");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickItemAction, header) == 0, "offset of ItemAction.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickItemAction, kind) == 8, "offset of ItemAction.kind");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickItemAction, character) == 12, "offset of ItemAction.character");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickItemAction, item) == 16, "offset of ItemAction.item");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickItemAction, count) == 20, "offset of ItemAction.count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickItemAction, inventory) == 24, "offset of ItemAction.inventory");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickItemAction, slot) == 28, "offset of ItemAction.slot");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickItemId) == 4, "size of ItemId");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickItemId) == 4, "alignment of ItemId");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickItemLine) == 16, "size of ItemLine");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickItemLine) == 4, "alignment of ItemLine");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickItemLine, modifier) == 0, "offset of ItemLine.modifier");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickItemLine, affix) == 12, "offset of ItemLine.affix");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickItemTypeInfo) == 36, "size of ItemTypeInfo");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickItemTypeInfo) == 4, "alignment of ItemTypeInfo");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickItemTypeInfo, header) == 0, "offset of ItemTypeInfo.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickItemTypeInfo, id) == 8, "offset of ItemTypeInfo.id");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickItemTypeInfo, max_stack) == 12, "offset of ItemTypeInfo.max_stack");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickItemTypeInfo, category) == 14, "offset of ItemTypeInfo.category");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickItemTypeInfo, rarity) == 15, "offset of ItemTypeInfo.rarity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickItemTypeInfo, flags) == 16, "offset of ItemTypeInfo.flags");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickItemTypeInfo, placed_entity) == 20, "offset of ItemTypeInfo.placed_entity");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickJoinCall) == 16, "size of JoinCall");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickJoinCall) == 8, "alignment of JoinCall");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickJoinCall, header) == 0, "offset of JoinCall.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickJoinCall, account) == 8, "offset of JoinCall.account");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickKnowledgeChange) == 32, "size of KnowledgeChange");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickKnowledgeChange) == 8, "alignment of KnowledgeChange");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeChange, header) == 0, "offset of KnowledgeChange.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeChange, account) == 8, "offset of KnowledgeChange.account");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeChange, entity) == 16, "offset of KnowledgeChange.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeChange, id) == 20, "offset of KnowledgeChange.id");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeChange, value) == 24, "offset of KnowledgeChange.value");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeChange, kind) == 28, "offset of KnowledgeChange.kind");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeChange, op) == 29, "offset of KnowledgeChange.op");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeChange, reserved) == 30, "offset of KnowledgeChange.reserved");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickKnowledgeData) == 20, "size of KnowledgeData");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickKnowledgeData) == 4, "alignment of KnowledgeData");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeData, header) == 0, "offset of KnowledgeData.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeData, entity) == 8, "offset of KnowledgeData.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeData, id) == 12, "offset of KnowledgeData.id");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeData, value) == 16, "offset of KnowledgeData.value");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickKnowledgeId) == 4, "size of KnowledgeId");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickKnowledgeId) == 4, "alignment of KnowledgeId");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickKnowledgeInfo) == 24, "size of KnowledgeInfo");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickKnowledgeInfo) == 4, "alignment of KnowledgeInfo");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeInfo, header) == 0, "offset of KnowledgeInfo.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeInfo, id) == 8, "offset of KnowledgeInfo.id");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeInfo, kind) == 12, "offset of KnowledgeInfo.kind");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeInfo, scope) == 13, "offset of KnowledgeInfo.scope");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeInfo, reserved) == 14, "offset of KnowledgeInfo.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeInfo, value) == 16, "offset of KnowledgeInfo.value");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeInfo, flags) == 20, "offset of KnowledgeInfo.flags");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickKnowledgeInput) == 32, "size of KnowledgeInput");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickKnowledgeInput) == 4, "alignment of KnowledgeInput");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeInput, header) == 0, "offset of KnowledgeInput.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeInput, character) == 8, "offset of KnowledgeInput.character");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeInput, id) == 12, "offset of KnowledgeInput.id");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeInput, kind) == 16, "offset of KnowledgeInput.kind");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeInput, scope) == 17, "offset of KnowledgeInput.scope");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeInput, reserved) == 18, "offset of KnowledgeInput.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeInput, value) == 20, "offset of KnowledgeInput.value");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeInput, refusal) == 24, "offset of KnowledgeInput.refusal");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeInput, allowed) == 28, "offset of KnowledgeInput.allowed");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickKnowledgeInput, reserved_tail) == 29, "offset of KnowledgeInput.reserved_tail");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickLayout) == 16, "size of Layout");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickLayout) == 8, "alignment of Layout");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickLayout, fields) == 0, "offset of Layout.fields");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickLayout, count) == 8, "offset of Layout.count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickLayout, size) == 12, "offset of Layout.size");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickLayoutField) == 24, "size of LayoutField");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickLayoutField) == 8, "alignment of LayoutField");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickLayoutField, name) == 0, "offset of LayoutField.name");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickLayoutField, hash) == 16, "offset of LayoutField.hash");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickLevel) == 1, "size of Level");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickLevel) == 1, "alignment of Level");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickLoaderOff) == 48, "size of LoaderOff");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickLoaderOff) == 8, "alignment of LoaderOff");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickLoaderOff, header) == 0, "offset of LoaderOff.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickLoaderOff, name) == 8, "offset of LoaderOff.name");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickLoaderOff, reason) == 24, "offset of LoaderOff.reason");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickLoaderOff, at) == 40, "offset of LoaderOff.at");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickLocalInputInfo) == 24, "size of LocalInputInfo");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickLocalInputInfo) == 8, "alignment of LocalInputInfo");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickLocalInputInfo, header) == 0, "offset of LocalInputInfo.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickLocalInputInfo, flags) == 8, "offset of LocalInputInfo.flags");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickLocalInputInfo, reserved) == 12, "offset of LocalInputInfo.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickLocalInputInfo, age_ms) == 16, "offset of LocalInputInfo.age_ms");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickLoopWaitData) == 24, "size of LoopWaitData");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickLoopWaitData) == 8, "alignment of LoopWaitData");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickLoopWaitData, header) == 0, "offset of LoopWaitData.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickLoopWaitData, ms) == 8, "offset of LoopWaitData.ms");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickLoopWaitData, reserved) == 12, "offset of LoopWaitData.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickLoopWaitData, left_ns) == 16, "offset of LoopWaitData.left_ns");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickLoopWaitSites) == 4, "size of LoopWaitSites");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickLoopWaitSites) == 4, "alignment of LoopWaitSites");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickManifest) == 192, "size of Manifest");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickManifest) == 8, "alignment of Manifest");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickManifest, header) == 0, "offset of Manifest.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickManifest, id) == 8, "offset of Manifest.id");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickManifest, version) == 24, "offset of Manifest.version");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickManifest, side) == 40, "offset of Manifest.side");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickManifest, flags) == 44, "offset of Manifest.flags");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickManifest, dependencies) == 48, "offset of Manifest.dependencies");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickManifest, dependency_count) == 56, "offset of Manifest.dependency_count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickManifest, builds) == 64, "offset of Manifest.builds");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickManifest, build_count) == 72, "offset of Manifest.build_count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickManifest, name) == 80, "offset of Manifest.name");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickManifest, description) == 96, "offset of Manifest.description");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickManifest, authors) == 112, "offset of Manifest.authors");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickManifest, author_count) == 120, "offset of Manifest.author_count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickManifest, license) == 128, "offset of Manifest.license");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickManifest, homepage) == 144, "offset of Manifest.homepage");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickManifest, points) == 160, "offset of Manifest.points");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickManifest, exports) == 176, "offset of Manifest.exports");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickManifest, export_count) == 184, "offset of Manifest.export_count");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickManifestFlags) == 4, "size of ManifestFlags");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickManifestFlags) == 4, "alignment of ManifestFlags");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickMapFrameInfo) == 48, "size of MapFrameInfo");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickMapFrameInfo) == 8, "alignment of MapFrameInfo");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickMapFrameInfo, header) == 0, "offset of MapFrameInfo.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickMapFrameInfo, origin) == 8, "offset of MapFrameInfo.origin");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickMapFrameInfo, size) == 24, "offset of MapFrameInfo.size");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickMapFrameInfo, metres_per_cell) == 40, "offset of MapFrameInfo.metres_per_cell");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickMapMarkerInfo) == 64, "size of MapMarkerInfo");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickMapMarkerInfo) == 8, "alignment of MapMarkerInfo");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickMapMarkerInfo, header) == 0, "offset of MapMarkerInfo.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickMapMarkerInfo, position) == 8, "offset of MapMarkerInfo.position");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickMapMarkerInfo, marker_type) == 32, "offset of MapMarkerInfo.marker_type");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickMapMarkerInfo, label) == 48, "offset of MapMarkerInfo.label");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickMapMarkerTypeInfo) == 36, "size of MapMarkerTypeInfo");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickMapMarkerTypeInfo) == 4, "alignment of MapMarkerTypeInfo");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickMapMarkerTypeInfo, header) == 0, "offset of MapMarkerTypeInfo.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickMapMarkerTypeInfo, type_name) == 8, "offset of MapMarkerTypeInfo.type_name");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickMapMarkerTypeInfo, marker_id) == 24, "offset of MapMarkerTypeInfo.marker_id");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickMapMarkerTypeInfo, sorting_category) == 28, "offset of MapMarkerTypeInfo.sorting_category");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickMapMarkerTypeInfo, is_fast_travel_destination) == 29, "offset of MapMarkerTypeInfo.is_fast_travel_destination");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickMapMarkerTypeInfo, is_custom_marker_type) == 30, "offset of MapMarkerTypeInfo.is_custom_marker_type");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickMapMarkerTypeInfo, show_above_fog_of_war) == 31, "offset of MapMarkerTypeInfo.show_above_fog_of_war");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickMapMarkerTypeInfo, show_label) == 32, "offset of MapMarkerTypeInfo.show_label");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickMapMarkerTypeInfo, reserved) == 33, "offset of MapMarkerTypeInfo.reserved");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickMarkerTypeSorting) == 1, "size of MarkerTypeSorting");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickMarkerTypeSorting) == 1, "alignment of MarkerTypeSorting");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickMidHookSpec) == 40, "size of MidHookSpec");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickMidHookSpec) == 8, "alignment of MidHookSpec");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickMidHookSpec, header) == 0, "offset of MidHookSpec.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickMidHookSpec, address) == 8, "offset of MidHookSpec.address");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickMidHookSpec, handler) == 16, "offset of MidHookSpec.handler");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickMidHookSpec, user) == 24, "offset of MidHookSpec.user");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickMidHookSpec, priority) == 32, "offset of MidHookSpec.priority");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickMidHookSpec, flags) == 36, "offset of MidHookSpec.flags");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickModHandle) == 8, "size of ModHandle");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickModHandle) == 8, "alignment of ModHandle");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickModInfo) == 152, "size of ModInfo");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickModInfo) == 8, "alignment of ModInfo");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickModInfo, header) == 0, "offset of ModInfo.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickModInfo, id) == 8, "offset of ModInfo.id");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickModInfo, version) == 24, "offset of ModInfo.version");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickModInfo, side) == 40, "offset of ModInfo.side");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickModInfo, flags) == 44, "offset of ModInfo.flags");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickModInfo, off) == 48, "offset of ModInfo.off");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickModInfo, file) == 56, "offset of ModInfo.file");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickModInfo, name) == 72, "offset of ModInfo.name");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickModInfo, description) == 88, "offset of ModInfo.description");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickModInfo, authors) == 104, "offset of ModInfo.authors");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickModInfo, author_count) == 112, "offset of ModInfo.author_count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickModInfo, license) == 120, "offset of ModInfo.license");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickModInfo, homepage) == 136, "offset of ModInfo.homepage");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickNetMessage) == 40, "size of NetMessage");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickNetMessage) == 8, "alignment of NetMessage");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickNetMessage, header) == 0, "offset of NetMessage.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickNetMessage, channel) == 8, "offset of NetMessage.channel");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickNetMessage, reserved) == 12, "offset of NetMessage.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickNetMessage, peer) == 16, "offset of NetMessage.peer");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickNetMessage, data) == 24, "offset of NetMessage.data");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickNetMessage, size) == 32, "offset of NetMessage.size");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickNetMessage, reserved_tail) == 36, "offset of NetMessage.reserved_tail");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickNpcId) == 4, "size of NpcId");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickNpcId) == 4, "alignment of NpcId");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickPlayerData) == 24, "size of PlayerData");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickPlayerData) == 8, "alignment of PlayerData");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPlayerData, header) == 0, "offset of PlayerData.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPlayerData, slot) == 8, "offset of PlayerData.slot");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPlayerData, entity) == 12, "offset of PlayerData.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPlayerData, key) == 16, "offset of PlayerData.key");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickPlayerDeathData) == 72, "size of PlayerDeathData");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickPlayerDeathData) == 8, "alignment of PlayerDeathData");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPlayerDeathData, header) == 0, "offset of PlayerDeathData.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPlayerDeathData, entity) == 8, "offset of PlayerDeathData.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPlayerDeathData, cause) == 12, "offset of PlayerDeathData.cause");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPlayerDeathData, key) == 16, "offset of PlayerDeathData.key");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPlayerDeathData, position) == 24, "offset of PlayerDeathData.position");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPlayerDeathData, cause_guid) == 48, "offset of PlayerDeathData.cause_guid");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPlayerDeathData, killer_entity) == 64, "offset of PlayerDeathData.killer_entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPlayerDeathData, reserved_tail) == 68, "offset of PlayerDeathData.reserved_tail");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickPlayerInfo) == 160, "size of PlayerInfo");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickPlayerInfo) == 8, "alignment of PlayerInfo");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPlayerInfo, header) == 0, "offset of PlayerInfo.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPlayerInfo, slot) == 8, "offset of PlayerInfo.slot");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPlayerInfo, entity) == 12, "offset of PlayerInfo.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPlayerInfo, key) == 16, "offset of PlayerInfo.key");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPlayerInfo, permissions) == 24, "offset of PlayerInfo.permissions");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPlayerInfo, name_len) == 28, "offset of PlayerInfo.name_len");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPlayerInfo, name) == 32, "offset of PlayerInfo.name");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickPlayerKey) == 8, "size of PlayerKey");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickPlayerKey) == 8, "alignment of PlayerKey");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickPointFlags) == 4, "size of PointFlags");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickPointFlags) == 4, "alignment of PointFlags");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickPointHandle) == 24, "size of PointHandle");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickPointHandle) == 8, "alignment of PointHandle");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPointHandle, header) == 0, "offset of PointHandle.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPointHandle, id) == 8, "offset of PointHandle.id");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPointHandle, live) == 16, "offset of PointHandle.live");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickPointLink) == 48, "size of PointLink");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickPointLink) == 8, "alignment of PointLink");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPointLink, header) == 0, "offset of PointLink.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPointLink, point) == 8, "offset of PointLink.point");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPointLink, before) == 16, "offset of PointLink.before");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPointLink, after) == 24, "offset of PointLink.after");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPointLink, before_count) == 32, "offset of PointLink.before_count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPointLink, after_count) == 36, "offset of PointLink.after_count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPointLink, priority) == 40, "offset of PointLink.priority");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickPointMode) == 4, "size of PointMode");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickPointMode) == 4, "alignment of PointMode");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickPointSpec) == 64, "size of PointSpec");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickPointSpec) == 8, "alignment of PointSpec");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPointSpec, header) == 0, "offset of PointSpec.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPointSpec, name) == 8, "offset of PointSpec.name");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPointSpec, mode) == 24, "offset of PointSpec.mode");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPointSpec, flags) == 28, "offset of PointSpec.flags");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPointSpec, input) == 32, "offset of PointSpec.input");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPointSpec, output) == 48, "offset of PointSpec.output");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickPointVerdict) == 4, "size of PointVerdict");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickPointVerdict) == 4, "alignment of PointVerdict");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickProgressWriteData) == 36, "size of ProgressWriteData");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickProgressWriteData) == 4, "alignment of ProgressWriteData");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickProgressWriteData, header) == 0, "offset of ProgressWriteData.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickProgressWriteData, entity) == 8, "offset of ProgressWriteData.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickProgressWriteData, what) == 12, "offset of ProgressWriteData.what");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickProgressWriteData, value) == 16, "offset of ProgressWriteData.value");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickProgressWriteData, outcome) == 20, "offset of ProgressWriteData.outcome");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickProgressWriteData, level) == 24, "offset of ProgressWriteData.level");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickProgressWriteData, experience) == 28, "offset of ProgressWriteData.experience");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickProgressWriteData, gained) == 32, "offset of ProgressWriteData.gained");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickPropPlace) == 56, "size of PropPlace");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickPropPlace) == 8, "alignment of PropPlace");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPropPlace, header) == 0, "offset of PropPlace.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPropPlace, item) == 8, "offset of PropPlace.item");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPropPlace, reserved) == 12, "offset of PropPlace.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPropPlace, position) == 16, "offset of PropPlace.position");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPropPlace, orientation) == 40, "offset of PropPlace.orientation");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickPropPlacementInfo) == 52, "size of PropPlacementInfo");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickPropPlacementInfo) == 4, "alignment of PropPlacementInfo");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPropPlacementInfo, header) == 0, "offset of PropPlacementInfo.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPropPlacementInfo, template_) == 8, "offset of PropPlacementInfo.template");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPropPlacementInfo, min) == 24, "offset of PropPlacementInfo.min");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPropPlacementInfo, max) == 36, "offset of PropPlacementInfo.max");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPropPlacementInfo, flags) == 48, "offset of PropPlacementInfo.flags");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickPropRemove) == 16, "size of PropRemove");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickPropRemove) == 4, "alignment of PropRemove");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPropRemove, header) == 0, "offset of PropRemove.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPropRemove, entity) == 8, "offset of PropRemove.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickPropRemove, flags) == 12, "offset of PropRemove.flags");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickQuestData) == 16, "size of QuestData");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickQuestData) == 4, "alignment of QuestData");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickQuestData, header) == 0, "offset of QuestData.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickQuestData, entity) == 8, "offset of QuestData.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickQuestData, quest) == 12, "offset of QuestData.quest");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickQuestPayment) == 60, "size of QuestPayment");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickQuestPayment) == 4, "alignment of QuestPayment");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickQuestPayment, header) == 0, "offset of QuestPayment.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickQuestPayment, entity) == 8, "offset of QuestPayment.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickQuestPayment, quest) == 12, "offset of QuestPayment.quest");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickQuestPayment, count) == 16, "offset of QuestPayment.count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickQuestPayment, limit) == 20, "offset of QuestPayment.limit");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickQuestPayment, allowed) == 24, "offset of QuestPayment.allowed");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickQuestPayment, reserved) == 25, "offset of QuestPayment.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickQuestPayment, experience) == 28, "offset of QuestPayment.experience");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickQuestProgressData) == 32, "size of QuestProgressData");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickQuestProgressData) == 4, "alignment of QuestProgressData");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickQuestProgressData, header) == 0, "offset of QuestProgressData.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickQuestProgressData, entity) == 8, "offset of QuestProgressData.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickQuestProgressData, quest) == 12, "offset of QuestProgressData.quest");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickQuestProgressData, step) == 16, "offset of QuestProgressData.step");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickQuestProgressData, part) == 20, "offset of QuestProgressData.part");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickQuestProgressData, value) == 24, "offset of QuestProgressData.value");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickQuestProgressData, target) == 28, "offset of QuestProgressData.target");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickQuestStepData) == 20, "size of QuestStepData");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickQuestStepData) == 4, "alignment of QuestStepData");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickQuestStepData, header) == 0, "offset of QuestStepData.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickQuestStepData, entity) == 8, "offset of QuestStepData.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickQuestStepData, quest) == 12, "offset of QuestStepData.quest");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickQuestStepData, step) == 16, "offset of QuestStepData.step");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickRateId) == 4, "size of RateId");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickRateId) == 4, "alignment of RateId");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickRecipeId) == 4, "size of RecipeId");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickRecipeId) == 4, "alignment of RecipeId");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickRecipeKind) == 4, "size of RecipeKind");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickRecipeKind) == 4, "alignment of RecipeKind");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickRecipeRuleKind) == 4, "size of RecipeRuleKind");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickRecipeRuleKind) == 4, "alignment of RecipeRuleKind");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickRegisters) == 136, "size of Registers");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickRegisters) == 8, "alignment of Registers");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRegisters, rax) == 0, "offset of Registers.rax");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRegisters, rcx) == 8, "offset of Registers.rcx");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRegisters, rdx) == 16, "offset of Registers.rdx");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRegisters, rbx) == 24, "offset of Registers.rbx");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRegisters, rsp) == 32, "offset of Registers.rsp");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRegisters, rbp) == 40, "offset of Registers.rbp");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRegisters, rsi) == 48, "offset of Registers.rsi");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRegisters, rdi) == 56, "offset of Registers.rdi");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRegisters, r8) == 64, "offset of Registers.r8");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRegisters, r9) == 72, "offset of Registers.r9");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRegisters, r10) == 80, "offset of Registers.r10");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRegisters, r11) == 88, "offset of Registers.r11");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRegisters, r12) == 96, "offset of Registers.r12");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRegisters, r13) == 104, "offset of Registers.r13");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRegisters, r14) == 112, "offset of Registers.r14");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRegisters, r15) == 120, "offset of Registers.r15");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRegisters, rflags) == 128, "offset of Registers.rflags");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickRelation) == 1, "size of Relation");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickRelation) == 1, "alignment of Relation");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickRenameCall) == 40, "size of RenameCall");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickRenameCall) == 8, "alignment of RenameCall");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRenameCall, header) == 0, "offset of RenameCall.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRenameCall, entity) == 8, "offset of RenameCall.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRenameCall, player) == 16, "offset of RenameCall.player");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRenameCall, text) == 24, "offset of RenameCall.text");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickResourceKey) == 24, "size of ResourceKey");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickResourceKey) == 4, "alignment of ResourceKey");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickResourceKey, guid) == 0, "offset of ResourceKey.guid");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickResourceKey, type_hash) == 16, "offset of ResourceKey.type_hash");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickResourceKey, part) == 20, "offset of ResourceKey.part");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickRespawnPointInfo) == 56, "size of RespawnPointInfo");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickRespawnPointInfo) == 8, "alignment of RespawnPointInfo");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRespawnPointInfo, header) == 0, "offset of RespawnPointInfo.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRespawnPointInfo, entity) == 8, "offset of RespawnPointInfo.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRespawnPointInfo, kind) == 12, "offset of RespawnPointInfo.kind");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRespawnPointInfo, position) == 16, "offset of RespawnPointInfo.position");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickRespawnPointInfo, orientation) == 40, "offset of RespawnPointInfo.orientation");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickSaveInfo) == 48, "size of SaveInfo");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickSaveInfo) == 8, "alignment of SaveInfo");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSaveInfo, header) == 0, "offset of SaveInfo.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSaveInfo, world) == 8, "offset of SaveInfo.world");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSaveInfo, saved_at) == 24, "offset of SaveInfo.saved_at");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSaveInfo, size) == 32, "offset of SaveInfo.size");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSaveInfo, backups) == 40, "offset of SaveInfo.backups");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSaveInfo, reserved) == 44, "offset of SaveInfo.reserved");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickServiceFound) == 72, "size of ServiceFound");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickServiceFound) == 8, "alignment of ServiceFound");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickServiceFound, header) == 0, "offset of ServiceFound.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickServiceFound, table) == 8, "offset of ServiceFound.table");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickServiceFound, fields) == 16, "offset of ServiceFound.fields");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickServiceFound, owner) == 24, "offset of ServiceFound.owner");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickServiceFound, owner_version) == 40, "offset of ServiceFound.owner_version");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickServiceFound, reason) == 56, "offset of ServiceFound.reason");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickServiceSpec) == 56, "size of ServiceSpec");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickServiceSpec) == 8, "alignment of ServiceSpec");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickServiceSpec, header) == 0, "offset of ServiceSpec.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickServiceSpec, name) == 8, "offset of ServiceSpec.name");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickServiceSpec, major) == 24, "offset of ServiceSpec.major");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickServiceSpec, layout) == 32, "offset of ServiceSpec.layout");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickServiceSpec, table) == 48, "offset of ServiceSpec.table");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickSide) == 1, "size of Side");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickSide) == 1, "alignment of Side");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickSkillCheck) == 20, "size of SkillCheck");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickSkillCheck) == 4, "alignment of SkillCheck");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSkillCheck, header) == 0, "offset of SkillCheck.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSkillCheck, node) == 8, "offset of SkillCheck.node");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSkillCheck, gate) == 12, "offset of SkillCheck.gate");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSkillCheck, unlocked) == 16, "offset of SkillCheck.unlocked");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSkillCheck, reserved) == 17, "offset of SkillCheck.reserved");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickSkillNode) == 4, "size of SkillNode");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickSkillNode) == 4, "alignment of SkillNode");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickSoundAudience) == 48, "size of SoundAudience");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickSoundAudience) == 8, "alignment of SoundAudience");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSoundAudience, header) == 0, "offset of SoundAudience.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSoundAudience, kind) == 8, "offset of SoundAudience.kind");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSoundAudience, radius) == 12, "offset of SoundAudience.radius");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSoundAudience, player) == 16, "offset of SoundAudience.player");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSoundAudience, at) == 24, "offset of SoundAudience.at");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickSoundInfo) == 16, "size of SoundInfo");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickSoundInfo) == 4, "alignment of SoundInfo");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSoundInfo, header) == 0, "offset of SoundInfo.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSoundInfo, state) == 8, "offset of SoundInfo.state");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSoundInfo, reason) == 12, "offset of SoundInfo.reason");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickSoundPlay) == 64, "size of SoundPlay");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickSoundPlay) == 8, "alignment of SoundPlay");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSoundPlay, header) == 0, "offset of SoundPlay.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSoundPlay, container) == 8, "offset of SoundPlay.container");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSoundPlay, place) == 24, "offset of SoundPlay.place");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSoundPlay, entity) == 28, "offset of SoundPlay.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSoundPlay, at) == 32, "offset of SoundPlay.at");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSoundPlay, volume) == 56, "offset of SoundPlay.volume");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSoundPlay, fade_in_ms) == 60, "offset of SoundPlay.fade_in_ms");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickSoundSteer) == 48, "size of SoundSteer");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickSoundSteer) == 8, "alignment of SoundSteer");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSoundSteer, header) == 0, "offset of SoundSteer.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSoundSteer, kind) == 8, "offset of SoundSteer.kind");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSoundSteer, volume) == 12, "offset of SoundSteer.volume");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSoundSteer, fade_ms) == 16, "offset of SoundSteer.fade_ms");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSoundSteer, reserved) == 20, "offset of SoundSteer.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSoundSteer, at) == 24, "offset of SoundSteer.at");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickSpawnRequest) == 80, "size of SpawnRequest");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickSpawnRequest) == 8, "alignment of SpawnRequest");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSpawnRequest, header) == 0, "offset of SpawnRequest.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSpawnRequest, template_) == 8, "offset of SpawnRequest.template");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSpawnRequest, position) == 24, "offset of SpawnRequest.position");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSpawnRequest, orientation) == 48, "offset of SpawnRequest.orientation");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSpawnRequest, level) == 64, "offset of SpawnRequest.level");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSpawnRequest, flags) == 68, "offset of SpawnRequest.flags");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSpawnRequest, lifetime_ns) == 72, "offset of SpawnRequest.lifetime_ns");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickStatChange) == 32, "size of StatChange");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickStatChange) == 4, "alignment of StatChange");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatChange, header) == 0, "offset of StatChange.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatChange, entity) == 8, "offset of StatChange.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatChange, stat) == 12, "offset of StatChange.stat");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatChange, old) == 20, "offset of StatChange.old");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatChange, new_) == 24, "offset of StatChange.new");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatChange, reserved) == 28, "offset of StatChange.reserved");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickStatModifier) == 12, "size of StatModifier");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickStatModifier) == 4, "alignment of StatModifier");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatModifier, stat) == 0, "offset of StatModifier.stat");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatModifier, op) == 4, "offset of StatModifier.op");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatModifier, reserved) == 5, "offset of StatModifier.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatModifier, value) == 8, "offset of StatModifier.value");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickStatModify) == 40, "size of StatModify");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickStatModify) == 8, "alignment of StatModify");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatModify, header) == 0, "offset of StatModify.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatModify, entity) == 8, "offset of StatModify.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatModify, key) == 12, "offset of StatModify.key");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatModify, modifier) == 16, "offset of StatModify.modifier");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatModify, flags) == 28, "offset of StatModify.flags");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatModify, duration_ms) == 32, "offset of StatModify.duration_ms");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickStatRef) == 8, "size of StatRef");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickStatRef) == 4, "alignment of StatRef");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatRef, index) == 0, "offset of StatRef.index");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatRef, key) == 4, "offset of StatRef.key");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickStatSource) == 20, "size of StatSource");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickStatSource) == 4, "alignment of StatSource");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatSource, kind) == 0, "offset of StatSource.kind");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatSource, op) == 1, "offset of StatSource.op");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatSource, reserved) == 2, "offset of StatSource.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatSource, stat) == 4, "offset of StatSource.stat");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatSource, value) == 8, "offset of StatSource.value");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatSource, id) == 12, "offset of StatSource.id");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatSource, owner) == 16, "offset of StatSource.owner");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickStatSpec) == 28, "size of StatSpec");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickStatSpec) == 4, "alignment of StatSpec");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatSpec, header) == 0, "offset of StatSpec.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatSpec, ty) == 8, "offset of StatSpec.ty");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatSpec, format) == 9, "offset of StatSpec.format");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatSpec, reserved) == 10, "offset of StatSpec.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatSpec, default_) == 12, "offset of StatSpec.default");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatSpec, min) == 16, "offset of StatSpec.min");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatSpec, max) == 20, "offset of StatSpec.max");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatSpec, group) == 24, "offset of StatSpec.group");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatSpec, flags) == 25, "offset of StatSpec.flags");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStatSpec, reserved_display) == 26, "offset of StatSpec.reserved_display");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickStatus) == 4, "size of Status");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickStatus) == 4, "alignment of Status");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickStr) == 16, "size of Str");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickStr) == 8, "alignment of Str");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStr, ptr) == 0, "offset of Str.ptr");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickStr, len) == 8, "offset of Str.len");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickSummon) == 80, "size of Summon");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickSummon) == 8, "alignment of Summon");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSummon, header) == 0, "offset of Summon.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSummon, item) == 8, "offset of Summon.item");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSummon, ammo) == 12, "offset of Summon.ammo");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSummon, template_) == 16, "offset of Summon.template");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSummon, target) == 32, "offset of Summon.target");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSummon, slot) == 56, "offset of Summon.slot");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSummon, pide) == 64, "offset of Summon.pide");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSummon, meter) == 68, "offset of Summon.meter");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSummon, consumed) == 72, "offset of Summon.consumed");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSummon, reserved) == 73, "offset of Summon.reserved");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickSurvivorLookup) == 64, "size of SurvivorLookup");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickSurvivorLookup) == 8, "alignment of SurvivorLookup");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSurvivorLookup, header) == 0, "offset of SurvivorLookup.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSurvivorLookup, survivor) == 8, "offset of SurvivorLookup.survivor");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSurvivorLookup, vanilla) == 12, "offset of SurvivorLookup.vanilla");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSurvivorLookup, purpose) == 16, "offset of SurvivorLookup.purpose");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSurvivorLookup, entity) == 20, "offset of SurvivorLookup.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSurvivorLookup, cell) == 24, "offset of SurvivorLookup.cell");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSurvivorLookup, bed_of) == 48, "offset of SurvivorLookup.bed_of");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSurvivorLookup, token) == 56, "offset of SurvivorLookup.token");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickSymbol) == 24, "size of Symbol");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickSymbol) == 8, "alignment of Symbol");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSymbol, name) == 0, "offset of Symbol.name");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickSymbol, rva) == 16, "offset of Symbol.rva");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickTeleportRequest) == 56, "size of TeleportRequest");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickTeleportRequest) == 8, "alignment of TeleportRequest");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTeleportRequest, header) == 0, "offset of TeleportRequest.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTeleportRequest, entity) == 8, "offset of TeleportRequest.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTeleportRequest, flags) == 12, "offset of TeleportRequest.flags");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTeleportRequest, position) == 16, "offset of TeleportRequest.position");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTeleportRequest, orientation) == 40, "offset of TeleportRequest.orientation");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickTerrainEdit) == 72, "size of TerrainEdit");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickTerrainEdit) == 8, "alignment of TerrainEdit");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainEdit, header) == 0, "offset of TerrainEdit.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainEdit, kind) == 8, "offset of TerrainEdit.kind");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainEdit, shape) == 12, "offset of TerrainEdit.shape");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainEdit, flags) == 16, "offset of TerrainEdit.flags");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainEdit, material) == 20, "offset of TerrainEdit.material");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainEdit, position) == 24, "offset of TerrainEdit.position");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainEdit, radius) == 48, "offset of TerrainEdit.radius");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainEdit, height) == 52, "offset of TerrainEdit.height");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainEdit, protect) == 56, "offset of TerrainEdit.protect");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickTerrainMaterialAdd) == 52, "size of TerrainMaterialAdd");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickTerrainMaterialAdd) == 4, "alignment of TerrainMaterialAdd");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainMaterialAdd, header) == 0, "offset of TerrainMaterialAdd.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainMaterialAdd, guid) == 8, "offset of TerrainMaterialAdd.guid");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainMaterialAdd, item) == 24, "offset of TerrainMaterialAdd.item");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainMaterialAdd, health) == 40, "offset of TerrainMaterialAdd.health");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainMaterialAdd, like) == 44, "offset of TerrainMaterialAdd.like");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainMaterialAdd, fallback) == 45, "offset of TerrainMaterialAdd.fallback");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainMaterialAdd, changes) == 46, "offset of TerrainMaterialAdd.changes");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainMaterialAdd, hardness) == 47, "offset of TerrainMaterialAdd.hardness");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainMaterialAdd, flags) == 48, "offset of TerrainMaterialAdd.flags");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainMaterialAdd, reserved) == 49, "offset of TerrainMaterialAdd.reserved");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickTerrainMaterialInfo) == 20, "size of TerrainMaterialInfo");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickTerrainMaterialInfo) == 4, "alignment of TerrainMaterialInfo");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainMaterialInfo, header) == 0, "offset of TerrainMaterialInfo.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainMaterialInfo, id) == 8, "offset of TerrainMaterialInfo.id");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainMaterialInfo, flags) == 9, "offset of TerrainMaterialInfo.flags");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainMaterialInfo, hardness) == 10, "offset of TerrainMaterialInfo.hardness");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainMaterialInfo, reserved) == 11, "offset of TerrainMaterialInfo.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainMaterialInfo, item) == 12, "offset of TerrainMaterialInfo.item");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainMaterialInfo, health) == 16, "offset of TerrainMaterialInfo.health");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickTerrainPageData) == 20, "size of TerrainPageData");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickTerrainPageData) == 4, "alignment of TerrainPageData");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainPageData, header) == 0, "offset of TerrainPageData.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainPageData, x) == 8, "offset of TerrainPageData.x");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainPageData, z) == 12, "offset of TerrainPageData.z");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTerrainPageData, size) == 16, "offset of TerrainPageData.size");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickTickData) == 56, "size of TickData");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickTickData) == 8, "alignment of TickData");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTickData, header) == 0, "offset of TickData.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTickData, step) == 8, "offset of TickData.step");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTickData, elapsed_steps) == 16, "offset of TickData.elapsed_steps");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTickData, elapsed_ns) == 24, "offset of TickData.elapsed_ns");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTickData, step_ns) == 32, "offset of TickData.step_ns");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTickData, cost_ns) == 40, "offset of TickData.cost_ns");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTickData, game) == 48, "offset of TickData.game");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickTickHandle) == 4, "size of TickHandle");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickTickHandle) == 4, "alignment of TickHandle");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickTransactionInfo) == 48, "size of TransactionInfo");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickTransactionInfo) == 8, "alignment of TransactionInfo");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTransactionInfo, header) == 0, "offset of TransactionInfo.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTransactionInfo, state) == 8, "offset of TransactionInfo.state");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTransactionInfo, reason) == 12, "offset of TransactionInfo.reason");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTransactionInfo, ops) == 16, "offset of TransactionInfo.ops");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTransactionInfo, applied) == 20, "offset of TransactionInfo.applied");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTransactionInfo, bricks) == 24, "offset of TransactionInfo.bricks");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTransactionInfo, refusal) == 32, "offset of TransactionInfo.refusal");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTransactionInfo, reserved) == 36, "offset of TransactionInfo.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTransactionInfo, holds) == 40, "offset of TransactionInfo.holds");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTransactionInfo, created) == 44, "offset of TransactionInfo.created");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickTransactionOp) == 16, "size of TransactionOp");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickTransactionOp) == 8, "alignment of TransactionOp");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTransactionOp, kind) == 0, "offset of TransactionOp.kind");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTransactionOp, reserved) == 4, "offset of TransactionOp.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTransactionOp, data) == 8, "offset of TransactionOp.data");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickTypeAttribute) == 48, "size of TypeAttribute");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickTypeAttribute) == 8, "alignment of TypeAttribute");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeAttribute, name) == 0, "offset of TypeAttribute.name");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeAttribute, value_type) == 16, "offset of TypeAttribute.value_type");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeAttribute, value) == 32, "offset of TypeAttribute.value");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickTypeConstant) == 24, "size of TypeConstant");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickTypeConstant) == 8, "alignment of TypeConstant");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeConstant, name) == 0, "offset of TypeConstant.name");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeConstant, value) == 16, "offset of TypeConstant.value");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickTypeEntry) == 128, "size of TypeEntry");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickTypeEntry) == 8, "alignment of TypeEntry");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeEntry, header) == 0, "offset of TypeEntry.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeEntry, name) == 8, "offset of TypeEntry.name");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeEntry, impact_name) == 24, "offset of TypeEntry.impact_name");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeEntry, qualified_name) == 40, "offset of TypeEntry.qualified_name");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeEntry, inner) == 56, "offset of TypeEntry.inner");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeEntry, internal_hash) == 72, "offset of TypeEntry.internal_hash");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeEntry, size) == 76, "offset of TypeEntry.size");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeEntry, align) == 80, "offset of TypeEntry.align");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeEntry, element_align) == 82, "offset of TypeEntry.element_align");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeEntry, kind) == 84, "offset of TypeEntry.kind");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeEntry, flags) == 85, "offset of TypeEntry.flags");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeEntry, reserved) == 86, "offset of TypeEntry.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeEntry, count) == 88, "offset of TypeEntry.count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeEntry, field_count) == 92, "offset of TypeEntry.field_count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeEntry, constant_count) == 96, "offset of TypeEntry.constant_count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeEntry, attribute_count) == 100, "offset of TypeEntry.attribute_count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeEntry, fields) == 104, "offset of TypeEntry.fields");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeEntry, constants) == 112, "offset of TypeEntry.constants");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeEntry, attributes) == 120, "offset of TypeEntry.attributes");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickTypeField) == 48, "size of TypeField");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickTypeField) == 8, "alignment of TypeField");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeField, name) == 0, "offset of TypeField.name");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeField, type_name) == 16, "offset of TypeField.type_name");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeField, offset) == 32, "offset of TypeField.offset");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeField, attribute_count) == 36, "offset of TypeField.attribute_count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickTypeField, attributes) == 40, "offset of TypeField.attributes");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickUiDialog) == 64, "size of UiDialog");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickUiDialog) == 8, "alignment of UiDialog");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiDialog, header) == 0, "offset of UiDialog.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiDialog, title) == 8, "offset of UiDialog.title");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiDialog, message) == 24, "offset of UiDialog.message");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiDialog, detail) == 40, "offset of UiDialog.detail");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiDialog, buttons) == 56, "offset of UiDialog.buttons");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiDialog, flags) == 60, "offset of UiDialog.flags");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickUiExtension) == 64, "size of UiExtension");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickUiExtension) == 8, "alignment of UiExtension");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiExtension, header) == 0, "offset of UiExtension.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiExtension, screen) == 8, "offset of UiExtension.screen");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiExtension, edge) == 24, "offset of UiExtension.edge");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiExtension, reserved) == 28, "offset of UiExtension.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiExtension, hosts) == 32, "offset of UiExtension.hosts");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiExtension, host_count) == 40, "offset of UiExtension.host_count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiExtension, reserved_hosts) == 44, "offset of UiExtension.reserved_hosts");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiExtension, align) == 48, "offset of UiExtension.align");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiExtension, shift) == 52, "offset of UiExtension.shift");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiExtension, inset) == 56, "offset of UiExtension.inset");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiExtension, reserved_place) == 60, "offset of UiExtension.reserved_place");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickUiFrame) == 128, "size of UiFrame");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickUiFrame) == 8, "alignment of UiFrame");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiFrame, header) == 0, "offset of UiFrame.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiFrame, token) == 8, "offset of UiFrame.token");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiFrame, screen) == 16, "offset of UiFrame.screen");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiFrame, reserved) == 20, "offset of UiFrame.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiFrame, hud_screen) == 24, "offset of UiFrame.hud_screen");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiFrame, prompt_title) == 40, "offset of UiFrame.prompt_title");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiFrame, prompt_verb) == 56, "offset of UiFrame.prompt_verb");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiFrame, screen_host) == 60, "offset of UiFrame.screen_host");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiFrame, screen_host_template) == 64, "offset of UiFrame.screen_host_template");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiFrame, screen_host_position) == 80, "offset of UiFrame.screen_host_position");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiFrame, hovered_slot) == 104, "offset of UiFrame.hovered_slot");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiFrame, hovered_item) == 112, "offset of UiFrame.hovered_item");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiFrame, hovered_count) == 116, "offset of UiFrame.hovered_count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiFrame, hovered_pide) == 120, "offset of UiFrame.hovered_pide");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiFrame, reserved_hovered) == 124, "offset of UiFrame.reserved_hovered");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickUiMarker) == 56, "size of UiMarker");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickUiMarker) == 8, "alignment of UiMarker");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiMarker, header) == 0, "offset of UiMarker.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiMarker, position) == 8, "offset of UiMarker.position");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiMarker, label) == 32, "offset of UiMarker.label");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiMarker, color) == 48, "offset of UiMarker.color");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiMarker, marker_type) == 52, "offset of UiMarker.marker_type");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickUiTooltipItem) == 24, "size of UiTooltipItem");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickUiTooltipItem) == 8, "alignment of UiTooltipItem");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiTooltipItem, header) == 0, "offset of UiTooltipItem.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiTooltipItem, token) == 8, "offset of UiTooltipItem.token");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiTooltipItem, item) == 16, "offset of UiTooltipItem.item");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiTooltipItem, pide) == 20, "offset of UiTooltipItem.pide");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickUiWidget) == 120, "size of UiWidget");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickUiWidget) == 8, "alignment of UiWidget");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiWidget, header) == 0, "offset of UiWidget.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiWidget, kind) == 8, "offset of UiWidget.kind");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiWidget, flags) == 12, "offset of UiWidget.flags");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiWidget, style) == 16, "offset of UiWidget.style");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiWidget, steps) == 20, "offset of UiWidget.steps");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiWidget, text) == 24, "offset of UiWidget.text");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiWidget, detail) == 40, "offset of UiWidget.detail");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiWidget, options) == 56, "offset of UiWidget.options");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiWidget, option_count) == 64, "offset of UiWidget.option_count");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiWidget, index) == 68, "offset of UiWidget.index");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiWidget, value) == 72, "offset of UiWidget.value");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiWidget, min) == 76, "offset of UiWidget.min");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiWidget, max) == 80, "offset of UiWidget.max");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiWidget, result) == 84, "offset of UiWidget.result");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiWidget, anchor) == 88, "offset of UiWidget.anchor");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiWidget, reserved) == 92, "offset of UiWidget.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiWidget, sound) == 96, "offset of UiWidget.sound");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiWidget, item) == 112, "offset of UiWidget.item");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickUiWidget, count) == 116, "offset of UiWidget.count");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickVoxel) == 4, "size of Voxel");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickVoxel) == 1, "alignment of Voxel");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickVoxel, kind) == 0, "offset of Voxel.kind");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickVoxel, material) == 1, "offset of Voxel.material");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickVoxel, fill) == 2, "offset of Voxel.fill");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickVoxel, reserved) == 3, "offset of Voxel.reserved");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickVoxelChange) == 20, "size of VoxelChange");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickVoxelChange) == 4, "alignment of VoxelChange");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickVoxelChange, at) == 0, "offset of VoxelChange.at");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickVoxelChange, before) == 12, "offset of VoxelChange.before");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickVoxelChange, after) == 16, "offset of VoxelChange.after");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickVoxelEditRequest) == 56, "size of VoxelEditRequest");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickVoxelEditRequest) == 8, "alignment of VoxelEditRequest");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickVoxelEditRequest, header) == 0, "offset of VoxelEditRequest.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickVoxelEditRequest, by) == 8, "offset of VoxelEditRequest.by");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickVoxelEditRequest, min) == 16, "offset of VoxelEditRequest.min");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickVoxelEditRequest, size) == 28, "offset of VoxelEditRequest.size");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickVoxelEditRequest, changes) == 40, "offset of VoxelEditRequest.changes");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickVoxelEditRequest, count) == 48, "offset of VoxelEditRequest.count");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickVoxelRegion) == 32, "size of VoxelRegion");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickVoxelRegion) == 4, "alignment of VoxelRegion");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickVoxelRegion, header) == 0, "offset of VoxelRegion.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickVoxelRegion, min) == 8, "offset of VoxelRegion.min");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickVoxelRegion, size) == 20, "offset of VoxelRegion.size");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickVoxelWrite) == 48, "size of VoxelWrite");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickVoxelWrite) == 8, "alignment of VoxelWrite");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickVoxelWrite, header) == 0, "offset of VoxelWrite.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickVoxelWrite, min) == 8, "offset of VoxelWrite.min");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickVoxelWrite, size) == 20, "offset of VoxelWrite.size");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickVoxelWrite, voxels) == 32, "offset of VoxelWrite.voxels");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickVoxelWrite, count) == 40, "offset of VoxelWrite.count");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickWaypointData) == 56, "size of WaypointData");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickWaypointData) == 8, "alignment of WaypointData");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickWaypointData, header) == 0, "offset of WaypointData.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickWaypointData, key) == 8, "offset of WaypointData.key");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickWaypointData, entity) == 16, "offset of WaypointData.entity");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickWaypointData, position) == 24, "offset of WaypointData.position");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickWaypointData, set) == 48, "offset of WaypointData.set");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickWaypointData, reserved_tail) == 52, "offset of WaypointData.reserved_tail");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickWeatherFront) == 72, "size of WeatherFront");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickWeatherFront) == 8, "alignment of WeatherFront");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickWeatherFront, header) == 0, "offset of WeatherFront.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickWeatherFront, layer) == 8, "offset of WeatherFront.layer");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickWeatherFront, effect) == 12, "offset of WeatherFront.effect");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickWeatherFront, position) == 16, "offset of WeatherFront.position");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickWeatherFront, radius) == 40, "offset of WeatherFront.radius");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickWeatherFront, reserved) == 44, "offset of WeatherFront.reserved");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickWeatherFront, remaining_ns) == 48, "offset of WeatherFront.remaining_ns");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickWeatherFront, phenomenon) == 56, "offset of WeatherFront.phenomenon");
ENSHWICK_STATIC_ASSERT(sizeof(EnshwickWeatherSample) == 24, "size of WeatherSample");
ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF(EnshwickWeatherSample) == 4, "alignment of WeatherSample");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickWeatherSample, header) == 0, "offset of WeatherSample.header");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickWeatherSample, clear) == 8, "offset of WeatherSample.clear");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickWeatherSample, rain) == 12, "offset of WeatherSample.rain");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickWeatherSample, snow) == 16, "offset of WeatherSample.snow");
ENSHWICK_STATIC_ASSERT(offsetof(EnshwickWeatherSample, blizzard) == 20, "offset of WeatherSample.blizzard");

#pragma pack(pop)

#if defined(_WIN32)
#define ENSHWICK_EXPORT __declspec(dllexport)
#else
#define ENSHWICK_EXPORT
#endif

// An `EnshwickStr` initializer for a string literal, without its final zero.
// In C++ the pointer is `const char *`, so a manifest built from it is a
// constant expression (`constinit`) and needs no C++ runtime to start.
#ifdef __cplusplus
#define ENSHWICK_STR_INIT(literal) { (literal), sizeof(literal) - 1 }
#else
#define ENSHWICK_STR_INIT(literal) { (const uint8_t *)(literal), sizeof(literal) - 1 }
#endif

#ifdef __cplusplus
extern "C" {
#endif

// The ENSHWICK_ABI_MAJOR the mod was built for; its manifest's header holds
// the minor.
ENSHWICK_EXPORT uint16_t enshwick_mod_abi(void);
// The mod's manifest; static, no side effects.
ENSHWICK_EXPORT const EnshwickManifest *enshwick_mod_query(void);
// Registers hooks, services and settings.
ENSHWICK_EXPORT EnshwickStatus enshwick_mod_init(const EnshwickHostApi *host, EnshwickModHandle me);
// Runs after every chain is armed; any status but OK turns the mod off.
ENSHWICK_EXPORT EnshwickStatus enshwick_mod_ready(const EnshwickHostApi *host, EnshwickModHandle me);

#ifdef __cplusplus
}
#endif

#endif  /* ENSHWICK_H */
