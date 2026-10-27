#ifndef ENSHWICK_H
#define ENSHWICK_H

/* Written by cbindgen from the enshwick crate's ABI module: run `cargo xtask header`, never edit by hand. */

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

// The major version of this interface. A loader refuses a mod built for
// another major.
#define ENSHWICK_ABI_MAJOR 3

// The minor version of this interface. A newer minor only appends fields.
#define ENSHWICK_ABI_MINOR 2

// The bit of a `SurvivorStatusFn` answer that says its low byte is the
// status to show.
#define ENSHWICK_SURVIVOR_STATUS_SET 256

// An account action of `HostApi::account_action`,
// with the values of the game's own `AccountActionType`.
typedef uint8_t EnshwickAccountActionType;
// Removes a player online from the game.
#define EnshwickAccountActionType_KICK 0
// Puts the account on the ban list, and removes its player when
// online.
#define EnshwickAccountActionType_BAN 1
// Takes the account off the ban list.
#define EnshwickAccountActionType_UN_BAN 2

// The start of every struct the other side may know an older or newer
// version of.
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
// The engine's id 0 stands for no entity; the safe API hands it over as
// `None` in an `Option<Entity>`, never as an `Entity`.
typedef uint32_t EnshwickEntity;

// A player's account by the number the game keeps for it, the same in every
// session, to remember a player across reconnects and restarts.
typedef uint64_t EnshwickPlayerKey;

// A connected player, from `HostApi::player` or `HostApi::players`.
//
// Unlike other structs with a `Header`, its layout is frozen and it never
// grows: `ActionCall` and `RenameCall` carry it before fields of their
// own, and `HostApi::players` writes an array of it with no entry size.
// Player data a later minor adds goes into a new struct. The caller still
// starts each one it passes with `Header::of`.
typedef struct EnshwickPlayerInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The server's player slot.
  uint32_t slot;
  // The player character's entity.
  EnshwickEntity entity;
  // The same number for the same account in every session.
  EnshwickPlayerKey key;
  // The server's permission bits for the player: 1
  // kick and ban, 2
  // open inventories, 4
  // edit the base, 8
  // edit the world, 16
  // extend the base, 32
  // receive experience.
  uint8_t permissions;
  // Bytes of `name` in use.
  uint8_t name_len;
  // The name the client sent, UTF-8 as sent, cut at 64 bytes.
  uint8_t name[64];
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

// A UTF-8 string borrowed for the duration of a call, or for as long as its
// owner documents.
typedef struct EnshwickStr {
  const uint8_t *ptr;
  size_t len;
} EnshwickStr;

// A question the loader asks a mod about its player actions, registered
// with `HostApi::actions`.
typedef struct EnshwickActionCall {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // What the loader asks: `ActionCall::SHEET`, `ActionCall::RUN`,
  // `ActionCall::HINTS` or `ActionCall::CONSOLE`.
  uint32_t kind;
  // The language the player reads the game in, a
  // `Language` value.
  uint32_t language;
  // The player the question is about.
  struct EnshwickPlayerInfo player;
  // The action's id, for `ActionCall::RUN`; the console line after the
  // mod's id, for `ActionCall::CONSOLE`; valid for the call.
  struct EnshwickStr action;
  // The action's arguments in the sheet's encoding, for
  // `ActionCall::RUN`; valid for the call.
  struct EnshwickStr args;
} EnshwickActionCall;
// The actions the player may use now, as a sheet: the reply is the
// sheet's text.
#define EnshwickActionCall_SHEET 0
// The player runs an action from the interface: the reply is `1` or
// `0`, whether it was done, and the text the player reads.
#define EnshwickActionCall_RUN 1
// The actions with a hint that the player has at hand now: a line
// `<id><TAB><label><TAB><subject>` each.
#define EnshwickActionCall_HINTS 2
// The server's console runs an action from its text form, such as
// `access Ann on`, in `action`: the action's id and its values as
// words; `help` or nothing lists the actions. `player` is the console,
// an administrator with no character. The reply is as for
// `ActionCall::RUN`.
#define EnshwickActionCall_CONSOLE 3

// A buff an entity has, written by `HostApi::buffs`.
typedef struct EnshwickActiveBuff {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // Its type's id.
  uint32_t id;
  // The buff's own entity, which carries its effect.
  EnshwickEntity entity;
  // Where the entity holds it, as `BuffTypeInfo::slot`.
  uint32_t slot;
  // Its level; 0 for none.
  uint32_t level;
  // How long it lasts in all, in game nanoseconds.
  int64_t duration;
  // How long it has left, in game nanoseconds.
  int64_t remaining;
} EnshwickActiveBuff;

// One modifier of an affix with the range its value rolls in; read as
// arrays.
typedef struct EnshwickAffixModifierData {
  // The stat's index.
  uint32_t stat;
  // The operation, as `StatModifierData::op`.
  uint8_t op;
  // Zero.
  uint8_t reserved[3];
  // The least value.
  float min;
  // The greatest value.
  float max;
  // The value rolls in steps of this from `min`; 0 for any value.
  float step;
} EnshwickAffixModifierData;

// An affix's definition, for `HostApi::affix_open`.
typedef struct EnshwickAffixDefData {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The text key that names the affix, in its mod's texts.
  struct EnshwickStr text;
  // Affixes of one group never roll together on an item; empty for none.
  struct EnshwickStr group;
  // Its modifiers' ranges.
  const struct EnshwickAffixModifierData *modifiers;
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
} EnshwickAffixDefData;

// An item of the game by the number its item data gives it, as a prop's
// `UsedItem` names the item the prop was placed from.
typedef uint32_t EnshwickItemId;

// Rolls affixes onto an item, for `HostApi::affix_roll`.
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
  // `AffixRoll::ONCE`.
  uint8_t flags;
  // The seed of the roll: the same seed, item and affixes give the same
  // result.
  uint64_t seed;
} EnshwickAffixRoll;
// Rolls only an item that never rolled; others answer
// `Status::REFUSED`.
#define EnshwickAffixRoll_ONCE 1

// A box of whole cells or of 32.32 fixed-point metres, as its user says:
// `min` inclusive, `max` exclusive on each axis; axis 1 is up.
typedef struct EnshwickBounds {
  // The inclusive lower corner.
  int64_t min[3];
  // The exclusive upper corner.
  int64_t max[3];
} EnshwickBounds;

// One Flame Altar of the server's world, from `HostApi::altars`.
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
  // The vanilla zone at each level from 1 to `level_count`.
  struct EnshwickBounds levels[16];
  // Where the altar stands, in metres.
  double position[3];
} EnshwickAltarInfo;

// How many altars a Flame level allows, as the loader asks every
// `HostApi::altar_limit_filter` in
// load order.
typedef struct EnshwickAltarLimit {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The Flame level.
  uint32_t flame_level;
  // The altars the game itself allows at that level.
  uint8_t vanilla;
  // The altars allowed so far, as the filters before have left it; the
  // filter's answer.
  uint8_t allowed;
  // Zero.
  uint8_t reserved[2];
} EnshwickAltarLimit;

// One altar's zone as the loader sizes it, handed to every altar zone
// filter in load order.
//
// The zone is the altar's vanilla zone at `level` grown by `grow` cells on
// every horizontal side and made `height` cells tall, as the filters before
// have left it; see
// `HostApi::altar_zone_filter`.
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
// `HostApi::base_limit` writes it.
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
} EnshwickBaseLimitInfo;

// A survivor's own check that its bed is still its, after the game kept it,
// as `HostApi::survivor_bed_check` hands it on. Cells are whole 0.5 m
// cells.
typedef struct EnshwickBedCheck {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The survivor's id.
  uint32_t survivor;
  // The survivor's entity.
  EnshwickEntity entity;
  // The bed.
  EnshwickEntity bed;
  // Where the survivor belongs: its saved spawn cell, or where it stands.
  int64_t home[3];
  // Where the bed stands.
  int64_t bed_at[3];
} EnshwickBedCheck;

// A building block a transaction places, as a player's building tool
// does, for `TransactionOp::BLOCK`.
typedef struct EnshwickBlockPlace {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The shape's item: one with a `VoxelBlueprintItem`.
  EnshwickItemId shape;
  // The material's item: one whose `ChangeVoxelData` places a building
  // material.
  EnshwickItemId material;
  // Where the block's middle lies, in metres.
  double position[3];
  // The block's rotation, a quaternion x, y, z, w; the game keeps the
  // nearest quarter turns.
  float rotation[4];
} EnshwickBlockPlace;

// A buff to give an entity, for `HostApi::apply_buff`.
typedef struct EnshwickBuffRequest {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The entity to give it.
  EnshwickEntity entity;
  // The type's id.
  uint32_t id;
  // Its level, 1 to 255; 0 for none.
  uint32_t level;
  // Zero.
  uint32_t reserved;
  // How long it lasts, in game nanoseconds; 0 for the type's own.
  int64_t duration;
} EnshwickBuffRequest;

// A buff or debuff type of the running game, written by
// `HostApi::buff_types`.
typedef struct EnshwickBuffTypeInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The type's id: FNV-1a 32 of its GUID's bytes.
  uint32_t id;
  // Where an entity holds it: `BuffTypeInfo::FOOD`,
  // `BuffTypeInfo::BUFF`, `BuffTypeInfo::DEBUFF`,
  // `BuffTypeInfo::RESTED` or `BuffTypeInfo::TERRAIN`.
  uint16_t slot;
  // `BuffTypeInfo::SAVED`, `BuffTypeInfo::HIDDEN` or none.
  uint16_t flags;
  // The `LocaTag` of its name, which a client shows.
  uint32_t name;
  // How long it lasts by default, in game nanoseconds.
  int64_t duration;
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
// registered with `HostApi::build_filter`.
typedef struct EnshwickBuildCheck {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The building player's character.
  EnshwickEntity player;
  // The item placed: a building block, a prop or another placeable.
  EnshwickItemId item;
  // Where, in 32.32 fixed-point metres.
  int64_t position[3];
} EnshwickBuildCheck;

// Which side of the game a mod or a binary is.
typedef uint8_t EnshwickSide;
// The dedicated server; as a mod's side, server features, which the
// client also loads to run while it hosts a game (solo play included).
#define EnshwickSide_SERVER 1
// The game client.
#define EnshwickSide_CLIENT 2
// Both binaries.
#define EnshwickSide_BOTH 3

// A game build's identity: the CodeView GUID, in file order, and age.
typedef struct EnshwickFingerprint {
  // The GUID in file order.
  uint8_t guid[16];
  // The linker's age counter.
  uint32_t age;
} EnshwickFingerprint;

// Where the loader's build table for the running build comes from.
typedef uint8_t EnshwickBuildTable;
// The table was made for this build: every entry it pins is there.
#define EnshwickBuildTable_EXACT 0
// The build has no table yet: the newest table of its side found what
// it could by code patterns, and the rest is missing until the table
// for this build arrives.
#define EnshwickBuildTable_PATTERNS 1

// The running game, as the loader identified it.
typedef struct EnshwickBuildInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // `Side::SERVER` or `Side::CLIENT`.
  EnshwickSide side;
  // Where the game's image is loaded.
  uint64_t base;
  // The build fingerprint.
  struct EnshwickFingerprint fingerprint;
  // Whether the loader's build table was made for this build.
  EnshwickBuildTable table;
} EnshwickBuildInfo;

// Whether a player may edit a box of the server's world, for
// `HostApi::build_rights`: the mod
// fills `player`, `min` and `max`, and the loader writes `refusal`.
typedef struct EnshwickBuildRights {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The player's character entity.
  EnshwickEntity player;
  // `BuildRights::ALLOWED` or why the server refuses, written by the
  // loader.
  uint32_t refusal;
  // The box's lowest corner, in metres.
  double min[3];
  // The box's highest corner, in metres, at least `min` on every axis.
  double max[3];
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
  size_t symbol_count;
  // Engine constants the mod pins for the build, such as offsets in the
  // caller frames it reads; the `rva` field holds the value.
  const struct EnshwickSymbol *constants;
  // How many constants `constants` points at.
  size_t constant_count;
} EnshwickBuildRow;

// The data of the `BuildingPlaced` and
// `BuildingRemoved` events.
typedef struct EnshwickBuildingData {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The placed entity; for a removal, the entity as it was.
  EnshwickEntity entity;
  // The acting player's character entity.
  EnshwickEntity player;
  // The item placed, which the entity keeps as its `UsedItem`; 0 for a
  // removal.
  EnshwickItemId item;
  // What happened: 0 placed, 1
  // picked up, 2 torn down.
  uint8_t kind;
  // 1 when undo or redo replayed the action.
  uint8_t replay;
  // The template GUID of the placed entity, in file order; zero for a
  // removal.
  uint8_t template_[16];
  // Where it was placed, in metres; zero for a removal.
  float position[3];
} EnshwickBuildingData;
// Placed by a player.
#define EnshwickBuildingData_PLACED 0
// Picked up, its item returned.
#define EnshwickBuildingData_PICKED_UP 1
// Torn down.
#define EnshwickBuildingData_TORN_DOWN 2

// One field of a service table or mod event, as its layout hash sees it.
//
// Frozen: written as an array.
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
// `HostApi::declare_change`, whose value it
// sets later; numbered by the loader, unique in the process.
typedef uint32_t EnshwickChangeId;

// How a `DataChange` changes its value.
typedef uint32_t EnshwickChangeOp;
// The value becomes `value`.
#define EnshwickChangeOp_SET 0
// The value is multiplied by `value`, a finite factor from 0 to 100.
#define EnshwickChangeOp_TIMES 1
// The mod's value is taken out, as if it never set one; `value` is
// ignored. Only `HostApi::set_change`
// takes it.
#define EnshwickChangeOp_RESET 2

// The data of the `ChunkLoaded` and
// `ChunkUnloaded` events.
typedef struct EnshwickChunkData {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The chunk's index in the world's grid: `z * width + x`.
  uint32_t index;
  // Its column, west to east from 0.
  uint32_t x;
  // Its row, from 0 along the other axis.
  uint32_t z;
  // Its side in metres: it spans `x * size` to `(x + 1) * size` on one
  // axis and `z * size` to `(z + 1) * size` on the other, at any height.
  uint32_t size;
} EnshwickChunkData;

// A comfort provider in a base registering its comfort for its category,
// as `HostApi::comfort_filter` hands it on.
typedef struct EnshwickComfort {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The comfort category; a base keeps the best provider per category.
  uint32_t category;
  // The comfort it registers.
  int32_t value;
  // Where the provider stands, in metres.
  float position[3];
} EnshwickComfort;

// A crafting recipe of the game by its recipe id, to grant, refuse or watch
// the crafting of one recipe.
typedef uint32_t EnshwickRecipeId;

// A player's craft request, handed to the filters registered with
// `HostApi::craft_filter` before the server crafts.
typedef struct EnshwickCraftCheck {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The crafting player's character.
  EnshwickEntity player;
  // The station, or 0 for crafting by hand.
  EnshwickEntity station;
  // The recipe id.
  EnshwickRecipeId recipe;
  // How many.
  uint32_t amount;
} EnshwickCraftCheck;

// Damage about to reach an entity's health, handed to the filters
// registered with `HostApi::damage_filter`, which may change it.
typedef struct EnshwickDamageCheck {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // `DamageCheck::HIT`, `DamageCheck::FALL`, `DamageCheck::SHROUD`,
  // `DamageCheck::SHROUD_DEATH`, `DamageCheck::DROWNING`,
  // `DamageCheck::DROWNING_DEATH`, `DamageCheck::FREEZING` or
  // `DamageCheck::ADMIN_KILL`.
  uint32_t cause;
  // The entity that takes the damage.
  EnshwickEntity target;
  // What dealt a hit: the attacker, or its projectile; 0 for other
  // causes.
  EnshwickEntity source;
  // Who dealt a hit, the root of the impact that hit (the archer for an
  // arrow), or `source` when the game names none; 0 for other causes.
  EnshwickEntity attacker;
  // A hit's damage by type (`DamageTypeNew`: cut 0, pierce 1, blunt 2,
  // fire 3, ice 4, shroud 5, lightning 6, healing 7, poison 8, wood 9,
  // stone 10, metal 11, explosion 12), after the target's resistances
  // and before its armour; healing is what heals. Zero for other causes.
  float amounts[13];
  // The health the damage takes, for causes other than a hit; for a hit,
  // the sum of its damage types but healing, which the loader does not
  // read back.
  float total;
} EnshwickDamageCheck;
// A hit from a weapon, a spell, a creature or an explosion.
#define EnshwickDamageCheck_HIT 0
// A fall.
#define EnshwickDamageCheck_FALL 1
// The shroud draining a character whose time in it ran out.
#define EnshwickDamageCheck_SHROUD 2
// The deadly shroud, which kills at once.
#define EnshwickDamageCheck_SHROUD_DEATH 3
// A character out of breath under water.
#define EnshwickDamageCheck_DROWNING 4
// A creature out of breath under water, which dies at once.
#define EnshwickDamageCheck_DROWNING_DEATH 5
// A character frozen through, out of body heat.
#define EnshwickDamageCheck_FREEZING 6
// An administrator's kill command, of chosen enemies or of whole
// factions or players: `total` is 1, and the kill goes ahead unless
// the filters leave 0.
#define EnshwickDamageCheck_ADMIN_KILL 7

// The vanilla value a `DataChange` changes; the field says what its
// `entry` is. A later minor may add fields, which an older loader refuses
// with `Status::BAD_ARGS`.
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

// Which vanilla items a `DataChange` reaches.
typedef uint32_t EnshwickDataSelect;
// The entry the change names.
#define EnshwickDataSelect_ENTRY 0
// Every item of the categories in `DataChange::categories`.
#define EnshwickDataSelect_CATEGORY 1
// Every item one slot holds more than one of.
#define EnshwickDataSelect_STACKABLE 2
// Every item that places a container.
#define EnshwickDataSelect_CONTAINERS 3
// Every recipe whose output is of the kind whose
// `RecipeKind` code is `DataChange::categories`.
#define EnshwickDataSelect_RECIPE_KIND 4
// Every recipe made at the workshop whose GUID is
// `DataChange::entry`.
#define EnshwickDataSelect_WORKSHOP 5
// Every backpack.
#define EnshwickDataSelect_BACKPACKS 6

// One change of a vanilla value, as
// `HostApi::change_data` takes it. The
// loader copies it; `source` is borrowed for the call.
typedef struct EnshwickDataChange {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The GUID of the vanilla entry, in memory order.
  uint8_t entry[16];
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
  uint8_t item[16];
  // Which vanilla entries the change reaches: `entry` alone, or every
  // item or recipe a selection picks, with `entry` all zero but for
  // `DataSelect::WORKSHOP`.
  EnshwickDataSelect select;
  // The item categories `DataSelect::CATEGORY` picks, a bit per the
  // game's `ItemCategory`, or the `RecipeKind` code
  // `DataSelect::RECIPE_KIND` picks; 0 for any other selection.
  uint32_t categories;
} EnshwickDataChange;

// The clock of the server's world, written by `HostApi::time_of_day`.
//
// Times are game nanoseconds of the day, 0 up to 24 hours; lengths are
// real nanoseconds.
typedef struct EnshwickDaytimeInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The time of day.
  int64_t time;
  // The time of day the day starts at.
  int64_t day_start;
  // The time of day the night starts at.
  int64_t day_end;
  // How long the day lasts in real time at speed 1.
  int64_t day_length;
  // How long the night lasts in real time at speed 1.
  int64_t night_length;
  // How fast the clock runs: 1 normally, faster while players sleep, and
  // times every mod's factor (`HostApi::set_time_speed`).
  float speed;
  // `DaytimeInfo::DAY` or none.
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
// This mod works better beside the other mod in the version range: it
// loads first when present, and the loader notes when it is missing or
// in another version.
#define EnshwickRelation_RECOMMENDS 3
// This mod takes the place of the other mod, such as one it folds in
// (any version; the range stays empty): while this one is present, the
// loader never starts the other and names its DLL to delete.
#define EnshwickRelation_REPLACES 4
// The other id is one this mod had before a rename: it replaces that
// mod as `REPLACES` does, a requirement or a point
// named by the old id finds this mod, and on the first start the loader
// copies the old id's settings and data to this one.
#define EnshwickRelation_WAS 5

// A relation to another mod, by id and SemVer range.
typedef struct EnshwickDependency {
  // The other mod's id.
  struct EnshwickStr id;
  // A Cargo-style version requirement, such as `>=0.1, <1.0`; empty for any.
  struct EnshwickStr range;
  // How the two relate.
  EnshwickRelation relation;
} EnshwickDependency;

// Flags of `HostApi::entity_data_open`.
typedef uint32_t EnshwickEntityDataFlags;
// The server keeps the values to itself.
#define EnshwickEntityDataFlags_NONE 0
// The clients that run Enshwick get every value.
#define EnshwickEntityDataFlags_SENT 1

// An entity placed in the world, from `HostApi::entities`.
//
// Like `PlayerInfo`, its layout is frozen: `HostApi::entities` writes
// an array of it with no entry size.
typedef struct EnshwickEntityPlace {
  // The entity.
  EnshwickEntity entity;
  // Zero.
  uint32_t reserved;
  // Where it is, in 32.32 fixed-point metres.
  int64_t position[3];
} EnshwickEntityPlace;

// Something the loader tells subscribed mods about, with
// `HostApi::subscribe`.
typedef uint32_t EnshwickEvent;
// The mod's settings changed: its file on disk, a setting set by
// the console or the Mods screen, or a client's join values; no data. The
// thread is not fixed: a loader thread about a second after a file
// edit, the thread that made any other change.
#define EnshwickEvent_SETTINGS_CHANGED 1
// The loader's settings changed, log levels among them; no data. Raised
// on a loader thread.
#define EnshwickEvent_LOADER_SETTINGS_CHANGED 2
// A simulation step ran; data is a `TickData`. Raised on the server's
// game thread after the step, about 20 times a second with nobody online
// and 60 with players, from before `Event::WORLD_LOADED`. Server only.
#define EnshwickEvent_TICKED 3
// The world finished loading and the server runs; no data. Raised on the
// game thread. Server only.
#define EnshwickEvent_WORLD_LOADED 4
// A save of the world finished writing to disk; no data. Raised on the
// game thread; the final save runs after `Event::SHUTDOWN_STARTED`. Server only.
#define EnshwickEvent_WORLD_SAVED 5
// A player's character entered the world; data is a `PlayerData`.
// Raised on the game thread. Server only.
#define EnshwickEvent_PLAYER_JOINED 6
// A player who joined is leaving, with their character still in the
// world; data is a `PlayerData`. Raised on the game thread. Server only.
#define EnshwickEvent_PLAYER_LEFT 7
// The server begins an orderly stop with the world and players still
// alive; no data. Raised on the game thread; a killed process raises
// nothing. Server only.
#define EnshwickEvent_SHUTDOWN_STARTED 8
// A player placed a building piece or prop; data is a `BuildingData`.
// Raised on the game thread after the step that placed it. Server only.
#define EnshwickEvent_BUILDING_PLACED 9
// A player picked up or tore down a placed piece; data is a
// `BuildingData` whose entity no longer exists. Raised on the game
// thread after the step. Server only.
#define EnshwickEvent_BUILDING_REMOVED 10
// The join check kept this client out of a server that runs join mods
// the client lacks or runs in another version; no data.
// `HostApi::join_missing` lists those mods with the versions the
// server runs and their pages. Raised on the game thread once the player
// closed the loader's dialog. Client only.
#define EnshwickEvent_JOIN_REFUSED 11
// The Flame Altars of the server's world changed: one was placed or
// removed, moved, raised a level or had its zone built anew; no data.
// `HostApi::altars` lists them as they are now. Raised on the game
// thread after the step that noticed it, while a mod has opened altars
// (`HostApi::altars_open`). Server only.
#define EnshwickEvent_ALTARS_CHANGED 12
// The day began in the server's world: the clock passed the hour day
// starts at, or a mod set it into the day; no data. Raised on the game
// thread after the step that noticed it. Server only.
#define EnshwickEvent_DAY_STARTED 13
// The night began in the server's world, as `Event::DAY_STARTED` the
// day; no data. Server only.
#define EnshwickEvent_NIGHT_STARTED 14
// A knowledge value of the server's world or of a connected player
// changed: a recipe learnt, a quest step, a place found; data is a
// `KnowledgeData`. Raised on the game thread after the step that
// applied it. Server only.
#define EnshwickEvent_KNOWLEDGE_CHANGED 15
// A player completed a quest and got its rewards; data is a
// `QuestData`. Raised on the game thread after the step. Server
// only.
#define EnshwickEvent_QUEST_COMPLETED 16
// A chunk of the server's world loaded: players came near, and its
// entities exist now; data is a `ChunkData`. Raised on the game
// thread after the step. Server only.
#define EnshwickEvent_CHUNK_LOADED 17
// A chunk of the server's world unloaded: no player is near, and its
// entities are gone; data is a `ChunkData`. Raised on the game thread
// after the step. Server only.
#define EnshwickEvent_CHUNK_UNLOADED 18
// The settings a mod reads were replaced whole, as a world loads in this
// process (its own settings from then on) or stops (the settings file
// again); no data. The settings that wait for a start apply too.
// Raised on the server's game thread.
#define EnshwickEvent_SETTINGS_REPLACED 19
// A player died, from a hit, a fall, the shroud or drowning; data is a
// `PlayerDeathData`. Raised on the game thread after the step.
// Deaths the game keeps no record of, such as freezing or an
// administrator's kill, raise none. Server only.
#define EnshwickEvent_PLAYER_DIED 20

// A mod event a mod defines, or the one it subscribes to.
typedef struct EnshwickEventSpec {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The event's name, `<defining mod id>:<what>`.
  struct EnshwickStr name;
  // The event data's fields; events grow by appending.
  struct EnshwickLayout layout;
} EnshwickEventSpec;

// One game event type of the running build, as `HostApi::event_types`
// writes it. `name` is the loader's, valid for the rest of the process.
//
// `HostApi::event_types`: crate::abi::HostApi::event_types
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
// `ExperiencePass`. Its layout is frozen: it is passed as an array.
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
// `HostApi::experience_filter` hands it on.
typedef struct EnshwickExperiencePass {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The characters, valid and writable for the call.
  struct EnshwickExperienceRow *rows;
  // How many rows there are.
  size_t row_count;
  // The highest level.
  uint32_t level_cap;
  // What level `level` (1 or more) needs; valid during the call with
  // `token`.
  uint32_t (*need_for)(void *token, uint32_t level);
  // The argument of `need_for`.
  void *token;
} EnshwickExperiencePass;

// The size, scale and version of the client's fog of war, which
// `HostApi::fog_read` writes.
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
} EnshwickFogInfo;

// A game event handed to a `GameEventFn`, from the ECS event streams
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

// Flags of a `HookSpec`.
typedef uint32_t EnshwickHookFlags;
// No flag.
#define EnshwickHookFlags_NONE 0
// No other mod may link on this target.
#define EnshwickHookFlags_EXCLUSIVE 1
// The link reads the game's call site through
// `HostApi::caller_frame`.
#define EnshwickHookFlags_CALLER_FRAME 2

// A request to link a detour into the chain of a game function.
//
// The loader points `next` at the following link or at the original
// function when it arms the chain; until then `next` holds 0 and the detour
// is never called.
typedef struct EnshwickHookSpec {
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
typedef uint32_t EnshwickStatus;
// Success.
#define EnshwickStatus_OK 0
// What was asked for does not exist on this game build.
#define EnshwickStatus_MISSING 1
// More than one candidate answers a lookup.
#define EnshwickStatus_AMBIGUOUS 2
// The request conflicts with another mod or with the loader's rules.
#define EnshwickStatus_REFUSED 3
// An argument is malformed.
#define EnshwickStatus_BAD_ARGS 4
// The call came in a phase where it is not allowed, such as a hook after
// arming.
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

// What `HostApi::service` found.
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

// A mod's handler for an `Event`: the event, its data (null for events
// without data, valid for the call, and no other thread writes it during
// the call) and the pointer given at subscription.
typedef void (*EnshwickEventFn)(EnshwickEvent event, const void *data, void *user);

// A game resource's key: its GUID in memory order, its type hash and part.
typedef struct EnshwickResourceKey {
  // The resource's GUID, in memory order (`bytes_le` of the textual form).
  uint8_t guid[16];
  // The reflected type hash of the resource's root type.
  uint32_t type_hash;
  // The part of a resource split in parts; 0 for most.
  uint32_t part;
} EnshwickResourceKey;

// Builds a served resource from its base: `base` is the base resource's bytes,
// valid for the call.
//
// On `Status::OK` the function writes a pointer and length of bytes that
// stay valid and writable for the life of the process: a rebuild
// (`HostApi::rebuild_resource`) writes
// over the last patch's bytes.
typedef EnshwickStatus (*EnshwickResourcePatchFn)(const uint8_t *base,
                                                  size_t base_len,
                                                  void *user,
                                                  const uint8_t **out,
                                                  size_t *out_len);

// Flags of a `Manifest`.
typedef uint32_t EnshwickManifestFlags;
// No flag.
#define EnshwickManifestFlags_NONE 0
// Server and clients must run the same version of this mod to play
// together; it counts in the join check.
#define EnshwickManifestFlags_JOIN 1
// The mod may be reloaded while the game runs, for development: it
// keeps no state it cannot hand over, stops its own threads in
// `shutdown` and uses only what a reload can unlink (hooks, events,
// lookups, settings, logging); see the modding book.
#define EnshwickManifestFlags_RELOAD 2

// A mod the loader loaded, from `HostApi::mods`; the loader itself comes
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
  // The sides the mod runs on.
  EnshwickSide side;
  // The manifest's flags; `ManifestFlags::JOIN` when it counts in the
  // join check.
  EnshwickManifestFlags flags;
  // 1 when the mod is off: refused at init or faulted since.
  uint8_t off;
  // The DLL's path relative to the game's folder, with `/` separators,
  // such as `mods/trading_post.dll`.
  struct EnshwickStr file;
  // The manifest's human metadata, as in `Manifest`; in
  // `HostApi::join_missing`, the name and homepage the server advertises
  // and empty strings for the rest.
  struct EnshwickStr name;
  // One sentence on what the mod does.
  struct EnshwickStr description;
  // The authors' names.
  const struct EnshwickStr *authors;
  // How many names `authors` points at.
  size_t author_count;
  // An SPDX licence expression.
  struct EnshwickStr license;
  // Where players read about the mod.
  struct EnshwickStr homepage;
  // In `HostApi::replaced`, the id of the mod that replaces this one;
  // empty elsewhere.
  struct EnshwickStr replaced_by;
} EnshwickModInfo;

// A player renaming an entity, handed to the filters registered with
// `HostApi::rename_filter` before the game applies it.
typedef struct EnshwickRenameCall {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The entity being renamed.
  EnshwickEntity entity;
  // 1 when `player` holds the renaming player, 0 when the loader could
  // not tell who sent the rename.
  uint8_t known;
  // The renaming player, when `known` is 1.
  struct EnshwickPlayerInfo player;
  // The new name as the client sent it; valid for the call.
  struct EnshwickStr text;
} EnshwickRenameCall;

// A mod's rename filter: the call and the pointer given at registration;
// returns 1 to let the rename through, 0 to refuse it.
typedef uint8_t (*EnshwickRenameCallFn)(const struct EnshwickRenameCall *call, void *user);

// The general-purpose registers and flags at a mid-function hook, as the
// interrupted game code had them.
//
// The loader saves them before it calls the handlers of a hook placed with
// `HostApi::hook_at` and loads them back afterwards, so a handler that
// writes a field changes what the game code sees next. `rsp` is read-only:
// the stack stays where it was.
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

// A handler of a mid-function hook: the saved registers, writable, and the
// pointer it was registered with. It must not unwind.
typedef void (*EnshwickMidHookFn)(struct EnshwickRegisters *registers, void *user);

// A request to run a handler at an instruction inside a game function.
//
// Every mod's handler on one address runs, by priority and then load order,
// each seeing the writes of the ones before; then the instructions the jump
// replaced run and the function goes on.
typedef struct EnshwickMidHookSpec {
  // The absolute address of the instruction to run the handler before.
  uint64_t address;
  // The handler.
  EnshwickMidHookFn handler;
  // Passed to the handler on every call.
  void *user;
  // Lower runs earlier; equal priorities run in load order.
  int32_t priority;
  // `HookFlags::EXCLUSIVE` or none.
  EnshwickHookFlags flags;
} EnshwickMidHookSpec;

// An item's static data, from `HostApi::item_info`.
typedef struct EnshwickItemData {
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
  // `ItemInfo` flags: `ItemData::HAS_PIDE` and the others on this type.
  uint32_t flags;
  // The GUID of the template a player places from the item, its
  // `equipment.placedEntity`, in file order; all zero for an item that
  // places nothing.
  uint8_t placed_entity[16];
} EnshwickItemData;
// Each item has an entity of its own, with durability or a level.
#define EnshwickItemData_HAS_PIDE (1 << 1)
// The item is a container.
#define EnshwickItemData_IS_CONTAINER (1 << 2)
// The item is a blueprint.
#define EnshwickItemData_IS_BLUEPRINT (1 << 3)
// Each item keeps a count the game shows as a bar on its slot, as the
// water pouch keeps its water (the game's fuel).
#define EnshwickItemData_USES_FUEL (1 << 13)

// One operation of an inventory transaction, in `InventoryRequest::ops`.
typedef struct EnshwickInventoryOp {
  // What to do: one of the constants on this type.
  uint32_t kind;
  // `InventoryOp::INCLUDE_SPECIAL` or none.
  uint32_t flags;
  // The inventory entity, or for slot operations the slot reference
  // (`sub-inventory | index << 32`).
  uint64_t target;
  // The destination inventory entity of a move; for a give with
  // `InventoryOp::FILLED`, the bits of the `f32` fill each new item's
  // meter starts with; 0 otherwise.
  uint64_t other;
  // The item id of a give or take.
  EnshwickItemId item;
  // How many to give or take; for a slot move, how many to move, 0 for the
  // whole stack.
  uint32_t count;
} EnshwickInventoryOp;
// Adds `count` new items `item` to the inventory `target`.
#define EnshwickInventoryOp_GIVE 1
// Removes `count` items `item` from the inventory `target`.
#define EnshwickInventoryOp_TAKE 2
// Destroys the stack in slot `target`.
#define EnshwickInventoryOp_DESTROY_SLOT 3
// Moves the stack in slot `target`, or `count` of it, into the inventory
// `other`.
#define EnshwickInventoryOp_MOVE_SLOT 4
// Moves every stack of the inventory `target` into the inventory `other`.
#define EnshwickInventoryOp_MOVE_ALL 5
// For `InventoryOp::MOVE_ALL`: also move the source's action bar,
// equipment and locked slots.
#define EnshwickInventoryOp_INCLUDE_SPECIAL 1
// Replaces the stack in slot `target` with `count` new items `item`,
// each with its own entity, given to the inventory in `other`'s low
// half and asked for the same slot; `other`'s high half holds the
// `f32` fill of their meter.
#define EnshwickInventoryOp_RENEW 6
// For `InventoryOp::GIVE`: `other` holds the fill of each new item's
// meter, the game's own per-item fuel fill.
#define EnshwickInventoryOp_FILLED 2

// Inventory operations to run in one transaction, for
// `HostApi::inventory_submit`.
typedef struct EnshwickInventoryRequest {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // `InventoryRequest::ALLOW_PARTIAL`, `InventoryRequest::NOTIFY`.
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

// The outcome of one operation in an `InventoryResult`.
typedef struct EnshwickInventoryOpResult {
  // The game's error code, 0 for none, or `InventoryOpResult::NOT_RUN`.
  uint32_t error;
  // How much of the count the operation could not do.
  uint32_t remaining;
} EnshwickInventoryOpResult;
// The operation did not run: one before it failed.
#define EnshwickInventoryOpResult_NOT_RUN 256

// What became of an `InventoryRequest`, passed to its
// `InventoryResultFn`.
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

// Receives the `InventoryResult` of a request, on the server's game thread
// after a simulation step; it must not unwind.
typedef void (*EnshwickInventoryResultFn)(const struct EnshwickInventoryResult *result, void *user);

// A mod's status source: the pointer given at registration and where to
// write the mod's lines for the loader's status summary. The text stays
// valid until the source runs again; empty for none.
typedef void (*EnshwickStatusFn)(void *user, struct EnshwickStr *out);

// A placement the game refused because the object's box does not lie in a
// single building zone, as `HostApi::zone_check_filter` hands it on.
// Coordinates are 32.32 fixed-point metres.
typedef struct EnshwickZoneCheck {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The building zones, half-open boxes, valid for the call.
  const struct EnshwickBounds *zones;
  // How many boxes `zones` holds.
  size_t zone_count;
  // The object's box, both corners included.
  struct EnshwickBounds object;
  // How far the game lets an object reach past a zone, on every side.
  int64_t margin;
} EnshwickZoneCheck;

// A mod's zone check filter: the check and the pointer given at
// registration; returns 1 to accept the placement after all.
typedef uint8_t (*EnshwickZoneCheckFn)(const struct EnshwickZoneCheck *check, void *user);

// A game system asking which entity is a survivor, as
// `HostApi::survivor_lookup` hands it to the mod that answers.
//
// The game keeps one entity per survivor; a mod that keeps several answers
// each asker with the one that suits it. Cells are
// whole 0.5 m cells.
typedef struct EnshwickSurvivorLookup {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The survivor's id.
  uint32_t survivor;
  // The game's own answer, 0 for none.
  EnshwickEntity vanilla;
  // Who asks; one of the constants on this type.
  uint32_t purpose;
  // The entity the asker works on, as its purpose says; 0 for none.
  EnshwickEntity entity;
  // The cell its purpose names.
  int64_t cell[3];
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

// The mod's survivor lookup: the call and the pointer given at
// registration; returns 1 with the answer in `out`, or 0 for the game's own.
typedef uint8_t (*EnshwickSurvivorLookupFn)(const struct EnshwickSurvivorLookup *call,
                                            void *user,
                                            EnshwickEntity *out);

// A filter of survivors' beds: returns 0 to make the game clear the bed.
typedef uint8_t (*EnshwickBedCheckFn)(const struct EnshwickBedCheck *check, void *user);

// A summoning staff's use about to spawn or move a survivor, as
// `HostApi::summon_filter` hands it on, and once it worked, as
// `HostApi::summoned` does.
typedef struct EnshwickSummon {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The item used.
  EnshwickItemId item;
  // The ammunition it used, 0 for none.
  EnshwickItemId ammo;
  // The template GUID it places, in file order.
  uint8_t template_[16];
  // The target cell.
  int64_t target[3];
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

// A summon filter: returns `Summon::TAKE` to make the game take the
// item in the same transaction, and only when the spawn or move works,
// `Summon::REFUSE` to refuse the use, or 0 to leave it.
typedef uint8_t (*EnshwickSummonFn)(const struct EnshwickSummon *summon, void *user);

// The client's survivor menu merging the statuses of every entity of one
// survivor.
//
// The survivor, `count` status bytes at `statuses`, valid for the call, and
// the pointer given at registration; returns `SURVIVOR_STATUS_SET` or-ed
// with the byte to show, or 0 to leave the game's.
typedef uint32_t (*EnshwickSurvivorStatusFn)(uint32_t survivor,
                                             const uint8_t *statuses,
                                             size_t count,
                                             void *user);

// A player about to interact with an entity, as
// `HostApi::interaction_filter` hands it
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
  EnshwickEntity player;
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
  double position[3];
} EnshwickInteraction;
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

// A mod's interaction filter: the call and the pointer given at
// registration; returns 0 to refuse, which the game handles as its own
// refusal, so nothing moves and nothing is lost.
typedef uint8_t (*EnshwickInteractionFn)(const struct EnshwickInteraction *call, void *user);

// A node of the game's skill tree by its id, as
// `Skill::node` names the nodes a character
// unlocked and a skill check names the node it asks about.
typedef uint32_t EnshwickSkillNode;

// The game asking whether a character unlocked a skill node, as
// `HostApi::skill_override` hands it on.
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
// A `SkillCheckFn` answer: the game's own.
#define EnshwickSkillCheck_VANILLA 0
// A `SkillCheckFn` answer: the skill counts as unlocked.
#define EnshwickSkillCheck_UNLOCKED 1
// A `SkillCheckFn` answer: the skill counts as locked.
#define EnshwickSkillCheck_LOCKED 2

// A skill override: the check and the pointer given at registration;
// returns `SkillCheck::VANILLA`, `SkillCheck::UNLOCKED` or
// `SkillCheck::LOCKED`.
typedef uint8_t (*EnshwickSkillCheckFn)(const struct EnshwickSkillCheck *check, void *user);

// A comfort filter: the registration and the pointer given at
// registration; returns the comfort to register.
typedef int32_t (*EnshwickComfortFn)(const struct EnshwickComfort *comfort, void *user);

// An experience filter: the pass and the pointer given at registration.
typedef void (*EnshwickExperiencePassFn)(struct EnshwickExperiencePass *pass, void *user);

// A mod's handler for game events: the event and the pointer given at
// subscription. It must not unwind.
typedef void (*EnshwickGameEventFn)(const struct EnshwickGameEventData *event, void *user);

// A request to move an entity, for `HostApi::teleport`.
typedef struct EnshwickTeleportRequest {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The entity to move.
  EnshwickEntity entity;
  // `TeleportRequest::BEST_SPAWN`, `TeleportRequest::ORIENTED` or
  // none.
  uint32_t flags;
  // Where to, in 32.32 fixed-point metres.
  int64_t position[3];
  // The orientation to arrive in, a quaternion `x, y, z, w`, with
  // `TeleportRequest::ORIENTED`; otherwise the entity keeps its own.
  float orientation[4];
} EnshwickTeleportRequest;
// The game looks for a free spot near `position` to stand on, as it
// does for respawns.
#define EnshwickTeleportRequest_BEST_SPAWN 1
// The entity arrives facing `orientation`.
#define EnshwickTeleportRequest_ORIENTED 2

// A request to spawn an entity from a template, for `HostApi::spawn`.
typedef struct EnshwickSpawnRequest {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The template GUID, in the order `HostApi::entity_template` writes.
  uint8_t template_[16];
  // Where, in 32.32 fixed-point metres.
  int64_t position[3];
  // The orientation, a quaternion `x, y, z, w`.
  float orientation[4];
  // The level the entity spawns at, as the game scales enemies; 0 for
  // the template's own.
  uint32_t level;
  // Zero.
  uint32_t reserved;
  // Game time in nanoseconds after which the game removes the entity; 0
  // to keep it.
  uint64_t lifetime_ns;
} EnshwickSpawnRequest;

// A mod's craft filter: the request and the pointer given at registration;
// returns 1 to let it through, 0 to refuse it.
typedef uint8_t (*EnshwickCraftCheckFn)(const struct EnshwickCraftCheck *call, void *user);

// A mod's build filter: the placement and the pointer given at
// registration; returns 1 to let it through, 0 to refuse it.
typedef uint8_t (*EnshwickBuildCheckFn)(const struct EnshwickBuildCheck *call, void *user);

// A message on a mod's channel, handed to the handler registered with
// `HostApi::net_channel`.
typedef struct EnshwickNetMessage {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The channel, as `HostApi::net_channel` wrote it.
  uint32_t channel;
  // Zero.
  uint32_t reserved;
  // Who sent it: a player's account key on the server,
  // `NetMessage::SERVER` on a client.
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

// A mod's handler for messages on its channel: the message and the pointer
// given at registration. It must not unwind.
typedef void (*EnshwickNetMessageFn)(const struct EnshwickNetMessage *message, void *user);

// A mod's damage filter: the damage, which it may change, and the pointer
// given at registration. It runs on the game's worker threads.
typedef void (*EnshwickDamageFilterFn)(struct EnshwickDamageCheck *check, void *user);

// A knowledge value of the game by its id: a recipe's, a quest's, or any
// other bool, counter or flags the world or a player keeps.
//
// A recipe's knowledge has the recipe's id, so a `RecipeId` converts.
typedef uint32_t EnshwickKnowledgeId;

// A knowledge value, written by `HostApi::knowledge`.
typedef struct EnshwickKnowledgeInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The knowledge id.
  EnshwickKnowledgeId id;
  // `KnowledgeInfo::BOOL`, `KnowledgeInfo::COUNTER` or
  // `KnowledgeInfo::FLAGS`.
  uint8_t kind;
  // `KnowledgeInfo::WORLD` or `KnowledgeInfo::PLAYER`.
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

// A front of weather in the server's world: the current phenomenon of one
// of its weather layers, written by `HostApi::weather_fronts`.
typedef struct EnshwickWeatherFront {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The weather layer it belongs to.
  uint32_t layer;
  // `WeatherFront::NONE`, `WeatherFront::RAIN` or
  // `WeatherFront::BLIZZARD`.
  uint32_t effect;
  // Where it is, in 32.32 fixed-point metres.
  int64_t position[3];
  // How far it reaches, in metres.
  float radius;
  // Zero.
  uint32_t reserved;
  // How long it has left, in game nanoseconds.
  int64_t remaining;
} EnshwickWeatherFront;
// Clouds without rain, or nothing.
#define EnshwickWeatherFront_NONE 0
// Rain, snow where it is cold.
#define EnshwickWeatherFront_RAIN 1
// A blizzard.
#define EnshwickWeatherFront_BLIZZARD 2

// A terrain material of the running game, written by
// `HostApi::terrain_materials`.
typedef struct EnshwickTerrainMaterialInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The material's voxel id, 1 to 127.
  uint8_t id;
  // `TerrainMaterialInfo::FLATTENS` or none.
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

// A terrain edit, for `HostApi::terrain_edit`.
typedef struct EnshwickTerrainEdit {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // `TerrainEdit::ADD`, `TerrainEdit::DIG` or
  // `TerrainEdit::FLATTEN`.
  uint32_t kind;
  // `TerrainEdit::ROUND` or `TerrainEdit::SQUARE`.
  uint32_t shape;
  // `TerrainEdit::DRY_RUN` or none.
  uint32_t flags;
  // The terrain material added or filled with, 1 to 127; unused by a
  // dig.
  uint32_t material;
  // Where, in metres; every axis 0 or more.
  float position[3];
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

// A frame of the client's interface, handed to the callbacks registered with
// `HostApi::ui_frame`; widgets drawn with `HostApi::ui_widget` name its
// `token`.
typedef struct EnshwickUiFrame {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // Names this frame in `HostApi::ui_widget`; valid until the callback
  // returns.
  uint64_t token;
  // Where the player is: `UiFrame::MENU`, `UiFrame::LOADING` or
  // `UiFrame::IN_GAME`.
  uint32_t screen;
  // Zero.
  uint32_t reserved;
} EnshwickUiFrame;
// The title screen and the menus before a game.
#define EnshwickUiFrame_MENU 0
// A game loading, or the intro and outro.
#define EnshwickUiFrame_LOADING 1
// In a game.
#define EnshwickUiFrame_IN_GAME 2

// A mod's callback for each frame of the client's interface: the frame and
// the pointer given at registration. It must not unwind.
typedef void (*EnshwickUiFrameFn)(const struct EnshwickUiFrame *frame, void *user);

// One widget, drawn with `HostApi::ui_widget`: `kind` says which, and
// which fields it reads; the loader writes `index`, `value` and `result`
// back.
typedef struct EnshwickUiWidget {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // What to draw: one of the kind constants, `UiWidget::WINDOW` to
  // `UiWidget::SOUND`.
  uint32_t kind;
  // `UiWidget::READ_ONLY`, `UiWidget::WRAP`, `UiWidget::CENTER`.
  uint32_t flags;
  // A label's style (`UiWidget::STYLE_BODY` and on) or a sound
  // (`UiWidget::SOUND_CLICK` and on).
  uint32_t style;
  // A slider's stops between `min` and `max`, both included; 0 for a
  // continuous slider.
  uint32_t steps;
  // The text: a label's, a button's, a row's label, a window's id.
  struct EnshwickStr text;
  // A window's title.
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
  // Out: 1 when a button was clicked or a value changed this frame.
  uint32_t result;
  // Where a HUD widget shows: `UiWidget::TOP_LEFT` to
  // `UiWidget::BOTTOM_RIGHT`; read in a HUD callback only.
  uint32_t anchor;
  // Zero.
  uint32_t reserved;
} EnshwickUiWidget;
// A window in the game's style, a panel over the screen that takes the
// input while it shows: `text` is its id, unique in the mod, `detail`
// its title. Widgets that follow go into it until `UiWidget::END`.
#define EnshwickUiWidget_WINDOW 1
// Closes the innermost open window.
#define EnshwickUiWidget_END 2
// A line of text in `style`; `UiWidget::WRAP` wraps it at the width
// of the window, `UiWidget::CENTER` centres it.
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
// Plays the interface sound `style`.
#define EnshwickUiWidget_SOUND 9
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

// A dialog in the game's own style, queued with `HostApi::ui_dialog`.
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
  // as `UiDialog::OK`; the buttons show in the order of their bits. At
  // least one.
  uint32_t buttons;
  // `UiDialog::WAITING` when the caller blocks until the answer.
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
// The answer when the dialog closed without one, as when the game
// quits.
#define EnshwickUiDialog_CLOSED UINT32_MAX
// The caller blocks until the answer: refused on the thread that
// draws the interface, which would never draw it.
#define EnshwickUiDialog_WAITING 1

// A mod's callback for the answer to its dialog: the answer, such as
// `UiDialog::OK` or `UiDialog::CLOSED`, and the pointer given with the
// dialog.
//
// Called once, on the thread that draws the interface, or on a thread of the
// loader's when the dialog shows as a system window. It must not unwind.
typedef void (*EnshwickUiAnswerFn)(uint32_t answer, void *user);

// A marker on the client's map, one of the set `HostApi::ui_markers`
// hands over.
typedef struct EnshwickUiMarker {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // Where, in 32.32 fixed-point metres, y up.
  int64_t position[3];
  // The label the map shows beside the icon.
  struct EnshwickStr label;
  // The colour, one of the game's custom marker colours,
  // `UiMarker::WHITE` to `UiMarker::BLACK`.
  uint32_t color;
  // Zero.
  uint32_t reserved;
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

// A mod's check of a new text of its settings file, before the loader writes
// it: `Status::OK` to accept it; else the reason, written to `why` and valid
// until the mod's next call, and `Status::REFUSED`.
//
// A reason that starts with a key of the mod's texts, followed by
// tab-separated `name=value` pairs (`"config.min\tmin=2"`), shows as that
// text in the reader's language with each `{name}` filled; any other
// reason shows as written. A name holds no `=`; neither holds a tab.
//
// It must not unwind.
typedef EnshwickStatus (*EnshwickConfigCheckFn)(struct EnshwickStr text,
                                                struct EnshwickStr *why,
                                                void *user);

// A stat kind's definition, for `HostApi::stat_open` and
// `HostApi::stat_info`.
typedef struct EnshwickStatDefData {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // `StatDefData::ADDITIVE`, `StatDefData::MULTIPLICATIVE` or
  // `StatDefData::FLAG`.
  uint8_t ty;
  // How the value reads: `StatDefData::NUMBER`,
  // `StatDefData::SIGNED` or `StatDefData::PERCENT`.
  uint8_t format;
  // Zero.
  uint16_t reserved;
  // The value without modifiers.
  float default_;
  // The least value.
  float min;
  // The greatest value.
  float max;
  // The text key that names the stat, in its mod's texts.
  struct EnshwickStr text;
} EnshwickStatDefData;
// A sum of points, such as extra health.
#define EnshwickStatDefData_ADDITIVE 0
// A factor, 1 without modifiers, such as a damage multiplier.
#define EnshwickStatDefData_MULTIPLICATIVE 1
// On or off: any value but 0 is on.
#define EnshwickStatDefData_FLAG 2
// A plain number.
#define EnshwickStatDefData_NUMBER 0
// A number with its sign.
#define EnshwickStatDefData_SIGNED 1
// A fraction shown as a percentage.
#define EnshwickStatDefData_PERCENT 2

// A stat kind as the loader numbers it: its index for calls and its key,
// the FNV-1a 32 hash of its full id, for what is stored.
typedef struct EnshwickStatRef {
  // The index in this process.
  uint32_t index;
  // The FNV-1a 32 hash of the full id, the same in every process.
  uint32_t key;
} EnshwickStatRef;

// One modifier of a stat, by index; written as arrays.
typedef struct EnshwickStatModifierData {
  // The stat's index.
  uint32_t stat;
  // `StatModifierData::ADD`, `StatModifierData::MULTIPLY`,
  // `StatModifierData::MAX` or `StatModifierData::OVERRIDE`.
  uint8_t op;
  // Zero.
  uint8_t reserved[3];
  // The modifier's value.
  float value;
} EnshwickStatModifierData;
// Adds the value.
#define EnshwickStatModifierData_ADD 0
// Multiplies by the value.
#define EnshwickStatModifierData_MULTIPLY 1
// Raises the result to at least the value.
#define EnshwickStatModifierData_MAX 2
// Sets the result.
#define EnshwickStatModifierData_OVERRIDE 3

// Sets or removes a mod's modifier on an entity, for
// `HostApi::stat_modify`.
typedef struct EnshwickStatModify {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The entity.
  EnshwickEntity entity;
  // The mod's own key; a modifier with the same mod, key and stat
  // replaces the one before.
  uint32_t key;
  // The modifier.
  struct EnshwickStatModifierData modifier;
  // `StatModify::REMOVE`, `StatModify::SAVED`.
  uint32_t flags;
  // How long it lasts in milliseconds; 0 until removed.
  uint64_t duration_ms;
} EnshwickStatModify;
// Removes the modifier instead.
#define EnshwickStatModify_REMOVE 1
// Kept with the world for the player's account, and given back when
// the player joins again; players only.
#define EnshwickStatModify_SAVED (1 << 1)

// A stat whose value changed on an entity, for the handlers registered
// with `HostApi::stats_changed`.
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

// A handler of stat changes: the change and the pointer given at
// registration. It runs on the game thread, after a step.
typedef void (*EnshwickStatChangeFn)(const struct EnshwickStatChange *change, void *user);

// One modifier that counts toward an entity's stat, and where it comes
// from, written by `HostApi::stat_sources` as an array.
typedef struct EnshwickStatSource {
  // `StatSource::ITEM`, `StatSource::BUFF` or
  // `StatSource::ENTITY`.
  uint8_t kind;
  // The operation, as `StatModifierData::op`.
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

// A mod's handler for a mod event: the event's data, valid for the call,
// and the pointer given at subscription.
typedef void (*EnshwickModEventFn)(const void *data, void *user);

// One line of what an item gives, written by `HostApi::item_lines` as an
// array.
typedef struct EnshwickItemLine {
  // The modifier.
  struct EnshwickStatModifierData modifier;
  // The index of the affix it comes from, plus one; 0 for the item's own.
  uint32_t affix;
} EnshwickItemLine;

// One step of a mod migration, as `HostApi::migrate` takes it.
//
// Frozen: written as an array.
typedef struct EnshwickMigrationStep {
  // What the step does: one of the constants below.
  uint32_t kind;
  // The setting or store the step reads, or renames from.
  struct EnshwickStr from;
  // The setting or store a rename writes to; empty otherwise.
  struct EnshwickStr to;
  // The GUID content had, for `MigrationStep::RENAME_CONTENT`.
  uint8_t from_guid[16];
  // The GUID content has now, for `MigrationStep::RENAME_CONTENT`.
  uint8_t to_guid[16];
} EnshwickMigrationStep;
// Renames the setting `from` to `to`, value and comments kept.
#define EnshwickMigrationStep_RENAME_SETTING 1
// Changes the value of the setting `from` through the mod's
// `MigrateFn`, which gets and returns a TOML value.
#define EnshwickMigrationStep_MAP_SETTING 2
// Removes the setting `from`.
#define EnshwickMigrationStep_REMOVE_SETTING 3
// Renames the world store `from` to `to`.
#define EnshwickMigrationStep_RENAME_STORE 4
// Changes the world store `from` through the mod's `MigrateFn`, which
// gets and returns the whole TOML file.
#define EnshwickMigrationStep_MAP_STORE 5
// Reads content saved under `from_guid` as the content now under
// `to_guid`.
#define EnshwickMigrationStep_RENAME_CONTENT 6
// Sets the setting `to` from the setting another mod keeps, `from`
// written `<mod id>:<key>`, in its settings file and in each world's
// copy; nothing where that mod has no such value.
#define EnshwickMigrationStep_TAKE_SETTING 7

// One mod migration: an id and its steps, in order.
//
// Frozen: written as an array.
typedef struct EnshwickMigration {
  // The migration's id, unique in the mod; the record keeps it.
  struct EnshwickStr id;
  // Its steps.
  const struct EnshwickMigrationStep *steps;
  // How many `steps` points at.
  uint32_t count;
} EnshwickMigration;

// Every migration of a mod, oldest first, for `HostApi::migrate`.
typedef struct EnshwickMigrationSet {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The migrations, oldest first.
  const struct EnshwickMigration *migrations;
  // How many `migrations` points at.
  uint32_t count;
  // Changes values for the steps that map them, a `MigrateFn`; the
  // loader refuses a set without it. Spelled out so C sees a nullable
  // function pointer.
  EnshwickStatus (*map)(void *user,
                        uint32_t migration,
                        uint32_t step,
                        struct EnshwickStr input,
                        struct EnshwickStr *out);
  // The pointer `map` takes.
  void *user;
} EnshwickMigrationSet;

// Knowledge a client asks the server for in its input, handed to the
// filters registered with `HostApi::knowledge_input_filter` with the
// verdict so far.
typedef struct EnshwickKnowledgeInput {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The asking player's character.
  EnshwickEntity player;
  // The knowledge id.
  EnshwickKnowledgeId id;
  // The kind the client names: `KnowledgeInfo::BOOL`,
  // `KnowledgeInfo::COUNTER` or `KnowledgeInfo::FLAGS`.
  uint8_t kind;
  // `KnowledgeInfo::WORLD` or `KnowledgeInfo::PLAYER`, as the id's
  // definition says; `KnowledgeInput::NO_SCOPE` for an id the game
  // does not know.
  uint8_t scope;
  // Zero.
  uint16_t reserved;
  // Ignored for a bool, added for a counter, the bit to set for flags.
  uint32_t value;
  // Why the loader's knowledge guard would refuse the request, in any
  // mode: `KnowledgeInput::ALLOWED` when it would not.
  uint32_t refusal;
  // 1 when the request goes through so far: after the guard, which
  // refuses only when enforcing, and the filters that ran before.
  uint32_t allowed;
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

// A mod's knowledge input filter: the request and the pointer given at
// registration; returns 1 to let it through, 0 to refuse it.
typedef uint8_t (*EnshwickKnowledgeInputFn)(const struct EnshwickKnowledgeInput *call, void *user);

// What a recipe rule of `HostApi::recipe_rule` names.
typedef uint32_t EnshwickRecipeRuleKind;
// One recipe: its debug name, content id or GUID.
#define EnshwickRecipeRuleKind_RECIPE 0
// Every recipe that makes an item: the item's debug name, content id
// or GUID.
#define EnshwickRecipeRuleKind_OUTPUT 1
// Every recipe made at a workshop: `hand`, the debug name of the item
// the workshop belongs to, the workshop's content id or GUID.
#define EnshwickRecipeRuleKind_WORKSHOP 2

// How the links of an extension point act on its answer, as the defining
// mod chooses.
typedef uint32_t EnshwickPointMode;
// Links see the answer and change nothing.
#define EnshwickPointMode_OBSERVE 0
// Links may change the answer; the defining mod may ignore the change.
#define EnshwickPointMode_ADVISE 1
// Links may change the answer, and the defining mod takes it.
#define EnshwickPointMode_OVERRIDE 2

// Flags of a `PointSpec`.
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

// What `HostApi::define_point` hands back.
typedef struct EnshwickPointHandle {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The id `HostApi::run_point` takes.
  uint32_t id;
  // How many links run, valid for the process: 0 until every mod is
  // initialised and whenever none is live, so the defining mod skips
  // `HostApi::run_point`.
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
// The link panicked; the loader turns it off.
#define EnshwickPointVerdict_PANICKED 3

// A mod's link on an extension point: the input, valid for the call, the
// answer so far, which the link may overwrite and then answers
// `PointVerdict::SET`, and the pointer given at attaching.
typedef EnshwickPointVerdict (*EnshwickPointFn)(const void *input, void *value, void *user);

// A mod's answer to the loader's questions about its player actions: the
// call, the pointer given at registration and where to write the reply,
// which stays valid until the callback runs again.
typedef void (*EnshwickActionFn)(const struct EnshwickActionCall *call,
                                 void *user,
                                 struct EnshwickStr *reply);

// What the item a recipe makes is, from
// `HostApi::recipe_kind`; a later minor may
// add kinds, which a mod reads as `RecipeKind::OTHER`.
typedef uint32_t EnshwickRecipeKind;
// None of the kinds below: equipment, props, materials and the rest.
#define EnshwickRecipeKind_OTHER 0
// A building block.
#define EnshwickRecipeKind_BUILDING_BLOCK 1
// A roof block.
#define EnshwickRecipeKind_ROOF 2
// An overgrowth material.
#define EnshwickRecipeKind_OVERGROWTH 3
// A terrain material.
#define EnshwickRecipeKind_TERRAIN 4

// A game rate of `HostApi::rate` and
// `HostApi::set_rate`, by the loader's stable
// number; the safe layer names them with `crate::rates::Rate`.
typedef uint32_t EnshwickRateId;
// Experience from combat.
#define EnshwickRateId_EXPERIENCE_COMBAT 1
// Experience from mining.
#define EnshwickRateId_EXPERIENCE_MINING 2
// Experience from exploration and quests.
#define EnshwickRateId_EXPERIENCE_EXPLORATION 3
// How fast workshops produce.
#define EnshwickRateId_WORKSHOP_SPEED 4
// How many resources a drop stacks.
#define EnshwickRateId_RESOURCE_DROPS 5
// The comfort furniture gives a base.
#define EnshwickRateId_COMFORT 6
// The skill points a character gains per level.
#define EnshwickRateId_SKILL_POINTS 7
// How many items each stack of random loot holds.
#define EnshwickRateId_LOOT 8

// A wait of the engine's event loop on its handles, about to happen, as
// `HostApi::loop_wait` hands it on.
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

// A handler of the event loop's waits: the wait, borrowed for the call,
// and the pointer given at registration. It must not unwind.
typedef void (*EnshwickLoopWaitFn)(const struct EnshwickLoopWaitData *wait, void *user);

// Which waits `HostApi::loop_wait` found and
// hooked, as bits.
typedef uint32_t EnshwickLoopWaitSites;
// None.
#define EnshwickLoopWaitSites_NONE 0
// The wait on handles, on the server and the client.
#define EnshwickLoopWaitSites_HANDLES 1
// The client's wait on handles and window messages.
#define EnshwickLoopWaitSites_MESSAGES 2

// A handler of the server loop's wait between two passes: the wait in
// nanoseconds, which it may change, and the pointer given at registration.
// It must not unwind.
typedef void (*EnshwickServerWaitFn)(uint64_t *wait_ns, void *user);

// An altar zone filter: the zone so far, writable, and the pointer given
// at registration. It must not unwind.
typedef void (*EnshwickAltarZoneFn)(struct EnshwickAltarZone *zone, void *user);

// An altar limit filter: the limit so far, writable, and the pointer given
// at registration. It must not unwind.
typedef void (*EnshwickAltarLimitFilterFn)(struct EnshwickAltarLimit *limit, void *user);

// A voxel of the server's world, written by
// `HostApi::voxel_read`. The loader decodes
// the game's voxel into it, so a mod never reads the game's own encoding.
//
// Its layout is frozen: `HostApi::voxel_read`
// writes an array of it with no entry size.
typedef struct EnshwickVoxel {
  // `Voxel::AIR`, `Voxel::TERRAIN` or `Voxel::BUILDING`; a reader
  // treats a kind it does not know as neither.
  uint8_t kind;
  // The game's material id: 0 for air, a terrain material's id as
  // `TerrainMaterialInfo::id` has it, or a building material's id.
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
// The most voxels one `HostApi::voxel_read`
// reads: a box of 64 voxels, 32 metres, a side.
#define EnshwickVoxel_READ_MAX ((64 * 64) * 64)
// The end of the coordinates a box covers on any axis, in voxels:
// every voxel of a box lies below it. 2³⁰, far past any world's edge.
#define EnshwickVoxel_END (1 << 30)

// One change of a transaction, handed to
// `HostApi::transaction_commit` in an
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
// A terrain add, dig or flatten: `data` points at a `TerrainEdit`
// without `TerrainEdit::DRY_RUN`.
#define EnshwickTransactionOp_TERRAIN 1
// Voxels written over a box: `data` points at a `VoxelWrite`.
#define EnshwickTransactionOp_VOXELS 2
// The building voxels of a box cleared to air: `data` points at a
// `VoxelRegion`.
#define EnshwickTransactionOp_ERASE 3
// A building block placed: `data` points at a `BlockPlace`.
#define EnshwickTransactionOp_BLOCK 4
// A prop placed: `data` points at a `PropPlace`.
#define EnshwickTransactionOp_PLACE_PROP 5
// A prop removed: `data` points at a `PropRemove`.
#define EnshwickTransactionOp_REMOVE_PROP 6

// Where a transaction stands, written by
// `HostApi::transaction_status`.
typedef struct EnshwickTransactionInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // One of the states below; a reader treats a state it does not know as
  // `TransactionInfo::PENDING`.
  uint32_t state;
  // Why it failed, or why its last undo was refused:
  // `Status::NOT_LOADED` for a part of the world not loaded,
  // `Status::OUTSIDE_WORLD` for a change that leaves the world,
  // `Status::REFUSED` for a region changed since, `Status::MISSING`
  // when the world ended or the build lacks a piece; `Status::OK`
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
  // reason, one of the `BuildRights` refusals; `BuildRights::ALLOWED`
  // otherwise.
  uint32_t refusal;
  // Zero.
  uint32_t reserved;
  // When a prop's state stopped it, or stopped its last undo, the kinds
  // of state the prop holds, a set of the `HOLDS_` bits; 0 otherwise.
  uint32_t holds;
  // The props it placed and still holds, which
  // `HostApi::transaction_entities`
  // lists.
  uint32_t created;
} EnshwickTransactionInfo;
// Queued, or written in part: the loader spreads a large one over the
// server's steps.
#define EnshwickTransactionInfo_PENDING 0
// Written whole; it can be undone.
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
// a placement, one whose `Level` differs from the level it first
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

// A mod's handler for a call it serves.
//
// It gets the input, valid for the call, the answer to write, as large as
// the spec's output, and the pointer given at serving, and answers
// `Status::OK` only once the whole answer is written.
// It runs on the caller's thread, any thread, possibly on several at once
// and inside another call, and must not unwind.
typedef EnshwickStatus (*EnshwickCallFn)(const void *input, void *output, void *user);

// A map marker of the scene the server runs, one of the list
// `HostApi::map_markers` writes.
typedef struct EnshwickMapMarkerInfo {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // Where it stands, in metres, y up.
  double position[3];
  // Its `markerType` reference, a GUID in memory order; zero for none.
  uint8_t marker_type[16];
  // Its label's text tag (`markerId`), a GUID in memory order; zero for
  // none.
  uint8_t label[16];
} EnshwickMapMarkerInfo;

// The table of services the loader offers a mod.
//
// Fields are only ever appended; a mod checks `Header::covers` before it
// calls a field added after the minor it was built against.
typedef struct EnshwickHostApi {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The loader's own version.
  struct EnshwickStr host_version;
  // Writes the running game's identity, as far as the `Header` `out`
  // starts with says the caller's struct reaches.
  void (*build)(struct EnshwickBuildInfo *out);
  // Resolves a symbolic id for the running build: the mod's own rows first,
  // then the loader's table.
  EnshwickStatus (*symbol)(EnshwickModHandle me, struct EnshwickStr name, uint64_t *out);
  // Resolves an ECS system's update function by its reflected name.
  EnshwickStatus (*system)(struct EnshwickStr name, uint64_t *out);
  // Resolves a field's offset in a reflected type.
  EnshwickStatus (*field_offset)(struct EnshwickStr type_name,
                                 struct EnshwickStr field,
                                 uint64_t *out);
  // Resolves a reflected type's size.
  EnshwickStatus (*type_size)(struct EnshwickStr type_name, uint32_t *out);
  // Links a detour; allowed during `enshwick_mod_init` only.
  EnshwickStatus (*hook)(EnshwickModHandle me, const struct EnshwickHookSpec *spec);
  // Writes the game caller's context at entry of the current chain, for
  // links with `HookFlags::CALLER_FRAME`; valid until the link calls
  // anything hooked.
  void (*caller_frame)(struct EnshwickCallerFrame *out);
  // Reports that the mod failed at run time; the loader unlinks every link
  // of the mod and turns it off.
  void (*fault)(EnshwickModHandle me, struct EnshwickStr reason);
  // Writes a line to the log under the mod's id.
  void (*log)(EnshwickModHandle me, EnshwickLevel level, struct EnshwickStr message);
  // The text of the mod's settings file, written from `defaults` first when
  // it is missing; the text stays valid for the life of the process, so
  // a thread may read it while another asks again.
  EnshwickStatus (*settings)(EnshwickModHandle me,
                             struct EnshwickStr defaults,
                             struct EnshwickStr *out);
  // Publishes the service `spec` describes, table and layout, for other
  // mods; `Status::REFUSED` when another mod publishes its name and
  // major. Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*publish)(EnshwickModHandle me, const struct EnshwickServiceSpec *spec);
  // Finds the service `wanted` describes, its table null, and checks
  // that the published layout starts with the wanted one; writes the
  // table and its publisher to `out`, and on `Status::MISSING` or
  // `Status::REFUSED` the reason.
  EnshwickStatus (*service)(EnshwickModHandle me,
                            const struct EnshwickServiceSpec *wanted,
                            struct EnshwickServiceFound *out);
  // Resolves an engine constant of the running build, such as a field
  // offset reflection does not describe, by its symbolic id.
  EnshwickStatus (*constant)(EnshwickModHandle me, struct EnshwickStr name, uint64_t *out);
  // The mod's own data folder, `mod-data/<id>`, created on first use; the
  // path stays valid for the process.
  EnshwickStatus (*data_dir)(EnshwickModHandle me, struct EnshwickStr *out);
  // The most detailed level kept for the mod's lines; the mod skips
  // formatting anything more detailed.
  EnshwickLevel (*log_level)(EnshwickModHandle me);
  // Calls `callback` with `user` on every `event` for the mod, on the
  // thread that raises it, until the mod is off; allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*subscribe)(EnshwickModHandle me,
                              EnshwickEvent event,
                              EnshwickEventFn callback,
                              void *user);
  // The ECS type hash of a reflected component type, by name.
  EnshwickStatus (*type_hash)(struct EnshwickStr type_name, uint32_t *out);
  // A read-only pointer to `entity`'s component of type `type_hash`, valid
  // until the game thread moves on; on the server's game thread only.
  // For the static half of an ECS type, such as `FuelSetup`, it is the
  // set-up data the entity's template holds.
  EnshwickStatus (*component)(EnshwickModHandle me,
                              EnshwickEntity entity,
                              uint32_t type_hash,
                              const void **out);
  // The template GUID of `entity`, in file order; on the server's game
  // thread only.
  EnshwickStatus (*entity_template)(EnshwickModHandle me, EnshwickEntity entity, uint8_t (*out)[16]);
  // The connected player whose character is `entity`; on the server's game
  // thread only.
  EnshwickStatus (*player)(EnshwickModHandle me,
                           EnshwickEntity entity,
                           struct EnshwickPlayerInfo *out);
  // Up to `capacity` connected players with characters in the world, and
  // how many there are; on the server's game thread only.
  // Every entry is as large as the header of the first one says.
  EnshwickStatus (*players)(EnshwickModHandle me,
                            struct EnshwickPlayerInfo *out,
                            uint32_t capacity,
                            uint32_t *count);
  // Serves the resource `key` as `patch` makes it from the resource
  // `base`, built the first time the game reads it: a new resource when
  // the keys differ, a changed vanilla one when they are equal. Allowed
  // during `enshwick_mod_init` only; one mod per key.
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
  // Up to `capacity` loaded mods, the loader first, and how many there
  // are. Entries are `entry_size` bytes apart, the caller's
  // `size_of::<ModInfo>()`; the loader fills what both sides know of each
  // and its own `Header`.
  EnshwickStatus (*mods)(EnshwickModHandle me,
                         struct EnshwickModInfo *out,
                         uint32_t entry_size,
                         uint32_t capacity,
                         uint32_t *count);
  // Reports the mod's join values as its settings file holds them,
  // `synced` (an inline TOML table), and writes the values to run with
  // over them: the server's while this client is joined to a server that
  // advertises values for the mod, else an empty string. A server
  // advertises the `synced` of each join mod. The text written stays valid
  // for the life of the process.
  EnshwickStatus (*sync)(EnshwickModHandle me, struct EnshwickStr synced, struct EnshwickStr *out);
  // Writes where the game's code section (`.text`) is loaded and how many
  // bytes it spans.
  EnshwickStatus (*code_range)(uint64_t *start, uint64_t *len);
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
                               size_t len);
  // The most recipes one workshop takes on the running game: 512, the
  // game's own list size, or `u32::MAX` once the loader has lifted that
  // limit at arming.
  uint32_t (*workshop_recipes)(void);
  // 1 while this process runs a game server: always on the dedicated
  // server, on the client while it hosts a game, solo play included.
  uint8_t (*hosting)(void);
  // The entity world of the game server this process runs, the value a
  // system context of that world holds at `ecs.context_entity_world`; 0
  // while none runs or before its first step.
  uint64_t (*server_world)(void);
  // Sends `text` to the chat of the player whose account key is `to`, or of
  // everyone for 0, from any thread; the server posts it after the next
  // step. Lines past 1024 bytes are cut.
  EnshwickStatus (*chat)(EnshwickModHandle me, EnshwickPlayerKey to, struct EnshwickStr text);
  // Gives `entity` the name `text`, as a player's rename does: shown on
  // hover, sent to every client and saved with the world; cut at 255
  // bytes, removed when empty. On the server's game thread only.
  EnshwickStatus (*set_label)(EnshwickModHandle me, EnshwickEntity entity, struct EnshwickStr text);
  // Calls `callback` with `user` before the server applies a player's
  // rename of an entity; any filter returning 0 refuses it. Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*rename_filter)(EnshwickModHandle me, EnshwickRenameCallFn callback, void *user);
  // Up to `capacity` of the server's join mods that the last refused join
  // found missing on this client or in another version, and how many
  // there are; laid out as `HostApi::mods` lays out its entries. Each
  // entry has the server's version, `ManifestFlags::JOIN`, `off` 1, the
  // client's DLL path when it has the mod (empty otherwise), and the name
  // and homepage the server advertises. The strings stay valid for the
  // process; once 64 KiB of advertised text is kept, an entry with new
  // text is left out. Empty before a refused join; see
  // `Event::JOIN_REFUSED`.
  EnshwickStatus (*join_missing)(EnshwickModHandle me,
                                 struct EnshwickModInfo *out,
                                 uint32_t entry_size,
                                 uint32_t capacity,
                                 uint32_t *count);
  // Runs a handler before the instruction at an address inside a game
  // function, with the registers there; allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*hook_at)(EnshwickModHandle me, const struct EnshwickMidHookSpec *spec);
  // Asks for inventory transactions and item data; allowed during
  // `enshwick_mod_init` only. `Status::MISSING` when this game build
  // lacks a piece of them. Requests run in the game server: the dedicated
  // server, or the client while it hosts.
  EnshwickStatus (*inventory_open)(EnshwickModHandle me);
  // Writes the static data of the item `item`, as far as the `Header`
  // `out` starts with says the caller's struct reaches;
  // `Status::MISSING` for an unknown item, or while no world runs.
  // Any thread.
  EnshwickStatus (*item_info)(EnshwickModHandle me,
                              EnshwickItemId item,
                              struct EnshwickItemData *out);
  // Queues `request`, from any thread, for the game's inventory system,
  // which runs it as one transaction: all or nothing. `done` gets the
  // result with `user` on the game thread after a later step, unless the
  // mod is off by then. On an error status nothing is queued and `done` is
  // never called.
  EnshwickStatus (*inventory_submit)(EnshwickModHandle me,
                                     const struct EnshwickInventoryRequest *request,
                                     EnshwickInventoryResultFn done,
                                     void *user);
  // Adds `callback` with `user` to the mod's status sources: the loader
  // calls it, on a thread of its own or the game thread, whenever it
  // writes its status summary (`logs/enshwick/status.log`, `/enshwick
  // status`) and shows its lines under the mod. Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*status)(EnshwickModHandle me, EnshwickStatusFn callback, void *user);
  // Runs the console line `line` as the server's console, an
  // administrator with no character, and writes its reply to `out`, valid
  // until the next call on this thread: `enshwick ...` for the loader's
  // own tools, or a mod's id and the text form of one of its
  // actions. On the server's game thread only;
  // `Status::MISSING` when nothing answers the line's first word.
  EnshwickStatus (*run_console)(EnshwickModHandle me,
                                struct EnshwickStr line,
                                struct EnshwickStr *out);
  // Asks the loader to follow the Flame Altars of the server's world, for
  // `HostApi::altars` and `Event::ALTARS_CHANGED`. Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*altars_open)(EnshwickModHandle me);
  // Up to `capacity` altars of the server's world and how many there are.
  // Entries are `entry_size` bytes apart, the caller's
  // `size_of::<AltarInfo>()`; the loader fills what both sides know of
  // each and its own `Header`. Any thread; empty while no world runs.
  // `Status::REFUSED` for a mod that did not open altars.
  EnshwickStatus (*altars)(EnshwickModHandle me,
                           struct EnshwickAltarInfo *out,
                           uint32_t entry_size,
                           uint32_t capacity,
                           uint32_t *count);
  // Calls `callback` with `user` when the game refuses a placement whose
  // box does not lie in one building zone; any filter returning 1 accepts
  // it after all. Runs on both sides, since a client predicts placement.
  // Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*zone_check_filter)(EnshwickModHandle me, EnshwickZoneCheckFn callback, void *user);
  // Makes `callback` with `user` the answer to the game's survivor
  // lookups that `SurvivorLookup` names; one mod at a time
  // (`Status::REFUSED` for another). Runs on the server's worker
  // threads, in the dedicated server or a hosting client, and for
  // `SurvivorLookup::CURSOR` on a client alone too. Allowed during
  // `enshwick_mod_init` only; `Status::MISSING` on a build that lacks a
  // piece.
  EnshwickStatus (*survivor_lookup)(EnshwickModHandle me,
                                    EnshwickSurvivorLookupFn callback,
                                    void *user);
  // Calls `callback` with `user` after a survivor's own bed check kept its
  // bed; any filter returning 0 clears it. Server's worker threads.
  // Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*survivor_bed_check)(EnshwickModHandle me, EnshwickBedCheckFn callback, void *user);
  // Calls `callback` with `user` before a summoning staff spawns or moves
  // a survivor; any filter returning 1 makes the use take the item.
  // Server's game thread. Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*summon_filter)(EnshwickModHandle me, EnshwickSummonFn callback, void *user);
  // Files item `item` as a summoning staff of the survivor `survivor`, so
  // the game treats it as that survivor's staff, on both sides. Allowed
  // during `enshwick_mod_init` only.
  EnshwickStatus (*summon_item)(EnshwickModHandle me, EnshwickItemId item, uint32_t survivor);
  // Makes `callback` with `user` merge the statuses the client's survivor
  // menu shows for a survivor with several entities. Client only; one mod
  // at a time. Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*survivor_status)(EnshwickModHandle me,
                                    EnshwickSurvivorStatusFn callback,
                                    void *user);
  // Calls `callback` with `user` before a player's interaction that
  // `Interaction` names; any filter returning 0 refuses it. Runs in the
  // server's world only, on the thread of the system that checks it.
  // Allowed during `enshwick_mod_init` only; `Status::MISSING` on a
  // build that lacks a piece.
  EnshwickStatus (*interaction_filter)(EnshwickModHandle me,
                                       EnshwickInteractionFn callback,
                                       void *user);
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
  EnshwickStatus (*comfort_filter)(EnshwickModHandle me, EnshwickComfortFn callback, void *user);
  // Calls `callback` with `user` before the server applies the
  // characters' gained experience, once per batch; filters run in load
  // order. Server's worker threads. Allowed during `enshwick_mod_init`
  // only.
  EnshwickStatus (*experience_filter)(EnshwickModHandle me,
                                      EnshwickExperiencePassFn callback,
                                      void *user);
  // Lets the game group 32 altar zones into its 16 bases instead of 16
  // zones, refusing an altar that would start a 17th base; the patches
  // count as the mod's own. `Status::MISSING` when this build's code is
  // not the one the loader knows: the game keeps 16. Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*raise_base_limit)(EnshwickModHandle me);
  // Writes how many altar zones and bases the game's last grouping found;
  // `Status::MISSING` before the first one or without the raised limit.
  // Any thread.
  EnshwickStatus (*base_zones)(EnshwickModHandle me, uint32_t *zones, uint32_t *bases);
  // Calls `callback` with `user` on every game event of the reflected
  // type whose hash is `type_hash`, or on every game event for 0, on
  // the server's game thread before the systems of the next step run.
  // Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*game_event)(EnshwickModHandle me,
                               uint32_t type_hash,
                               EnshwickGameEventFn callback,
                               void *user);
  // Up to `capacity` entities of the server's world that have a component
  // of the reflected type `type_hash` and a place in the world, and how
  // many there are. On the server's game thread only.
  EnshwickStatus (*entities)(EnshwickModHandle me,
                             uint32_t type_hash,
                             struct EnshwickEntityPlace *out,
                             uint32_t capacity,
                             uint32_t *count);
  // Asks the server to destroy `entity` after the systems of the next
  // step; `Status::REFUSED` for a player's character,
  // `Status::MISSING` for an entity not in the world. On the server's
  // game thread only.
  EnshwickStatus (*destroy)(EnshwickModHandle me, EnshwickEntity entity);
  // Asks the server to save the world now, as its timed saves do;
  // `Event::WORLD_SAVED` follows once the save is on disk. A save under
  // way counts as asked; `Status::REFUSED` while the world may not be
  // saved, as before it has loaded. On the server's game thread only.
  EnshwickStatus (*save_world)(EnshwickModHandle me);
  // Writes the key of the world the server runs: its save file's name and
  // a hash of its folder, `[a-z0-9_-]`, valid for the process.
  // `Status::MISSING` before the server has read or written the world.
  // Any thread.
  EnshwickStatus (*world_key)(EnshwickModHandle me, struct EnshwickStr *out);
  // A writable pointer to `entity`'s component of the reflected type
  // `type_hash`, valid until the game thread returns to the game; the
  // loader marks the component changed, as the game's own writes do, so
  // the game sends it to clients and saves it. `Status::REFUSED` for a
  // copy the game keeps for clients (`Network*`, `Client*`), which it
  // overwrites; `Status::STATIC_HALF` for the static half of an ECS
  // type, the template's set-up data;
  // `Status::MISSING` when the entity lacks the component. On the
  // server's game thread only.
  EnshwickStatus (*component_write)(EnshwickModHandle me,
                                    EnshwickEntity entity,
                                    uint32_t type_hash,
                                    void **out);
  // Raises the game event at `data`, `size` bytes of the reflected type
  // `type_hash`, its whole layout; the loader stamps it with the game
  // time and the systems of the next step see it. Only types the loader
  // allows, which the server's own systems raise: `Status::REFUSED`
  // for any other or when the step's stream is full,
  // `Status::BAD_ARGS` for a size that is not the type's. On the
  // server's game thread only.
  EnshwickStatus (*emit)(EnshwickModHandle me,
                         uint32_t type_hash,
                         const uint8_t *data,
                         uint32_t size);
  // Asks the server to move an entity as its admin teleport does, after
  // the next step; `Status::MISSING` for an entity not in the world,
  // `Status::REFUSED` for one the game's teleport does not move, as a
  // prop: it moves actors and characters.
  // On the server's game thread only.
  EnshwickStatus (*teleport)(EnshwickModHandle me, const struct EnshwickTeleportRequest *request);
  // Asks the server to spawn an entity from a template, as its admin
  // spawn does, and writes the new entity's id to `out`; the entity is in
  // the world after the next step. `Status::BAD_ARGS` for a zero
  // template; a template the game does not know spawns nothing. On the
  // server's game thread only.
  EnshwickStatus (*spawn)(EnshwickModHandle me,
                          const struct EnshwickSpawnRequest *request,
                          EnshwickEntity *out);
  // Calls `callback` with `user` on each player's craft request in the
  // server's world, on the game thread before the crafting system reads
  // it; any filter returning 0 refuses it, and nothing is made or taken.
  // Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*craft_filter)(EnshwickModHandle me, EnshwickCraftCheckFn callback, void *user);
  // Calls `callback` with `user` after the game's cursor check allows a
  // placement; any filter returning 0 refuses it as the game refuses a
  // placement without permission. Runs on both sides, on the cursor
  // systems' worker threads: the server's check decides, a client's
  // shows. Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*build_filter)(EnshwickModHandle me, EnshwickBuildCheckFn callback, void *user);
  // Opens the mod's channel `name`, 1 to 64 bytes, between its server
  // and client parts, and writes its id to `out`; `callback` gets each
  // message on it with `user`: on the server's game thread, on a client
  // on the thread that takes the server's chat. Join mods only
  // (`Status::REFUSED` for another), since both ends must run the mod.
  // Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*net_channel)(EnshwickModHandle me,
                                struct EnshwickStr name,
                                EnshwickNetMessageFn callback,
                                void *user,
                                uint32_t *out);
  // Sends `size` bytes at `data`, at most 4096, on the mod's `channel`:
  // to the server for `NetMessage::SERVER`, else from the server to the
  // client of the player with account key `peer`, or to every client
  // that runs Enshwick for 0. Any thread. `Status::MISSING` while the
  // peer cannot take messages, `Status::REFUSED` over the peer's rate,
  // `Status::BAD_ARGS` for too many bytes or another mod's channel.
  EnshwickStatus (*net_send)(EnshwickModHandle me,
                             uint32_t channel,
                             EnshwickPlayerKey peer,
                             const uint8_t *data,
                             uint32_t size);
  // `Status::OK` when `peer` takes messages: a player's client that
  // runs Enshwick, on the server; the server, for `NetMessage::SERVER`
  // on a client. `Status::MISSING` otherwise. Any thread.
  EnshwickStatus (*net_ready)(EnshwickModHandle me, EnshwickPlayerKey peer);
  // Shows `text`, cut at 255 bytes, on the screen of the player with
  // account key `key`, as the game shows its own "can't do that"
  // messages; `Status::MISSING` when the player's client lacks
  // Enshwick and cannot show it, and nothing is sent. On the server's
  // game thread only.
  EnshwickStatus (*notify)(EnshwickModHandle me, EnshwickPlayerKey key, struct EnshwickStr text);
  // Writes the clock of the server's world, as far as the `Header`
  // `out` starts with says the caller's struct reaches. On the server's
  // game thread only.
  EnshwickStatus (*time_of_day)(EnshwickModHandle me, struct EnshwickDaytimeInfo *out);
  // Sets the clock of the server's world to `time`, game nanoseconds of
  // the day, taken modulo 24 hours; clients follow after the next step.
  // On the server's game thread only.
  EnshwickStatus (*set_time_of_day)(EnshwickModHandle me, int64_t time);
  // Lets the mod change how fast the clock runs, with
  // `HostApi::set_time_speed`. Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*time_speed_control)(EnshwickModHandle me);
  // Sets the mod's factor on the clock's speed, from 0 (stopped) to 1000;
  // the clock runs at the game's own speed times every mod's factor.
  // `Status::REFUSED` for a mod without
  // `HostApi::time_speed_control`, `Status::BAD_ARGS` for a factor
  // out of range. On the server's game thread only.
  EnshwickStatus (*set_time_speed)(EnshwickModHandle me, float factor);
  // Copies the server's live game settings, `keen::ecs::GameSettings`,
  // into `out`, `size` bytes, which must be the type's reflected size
  // (`Status::BAD_ARGS` otherwise), and writes the preset the server
  // runs (`GameSettingsPreset`) to `preset`. On the server's game thread
  // only.
  EnshwickStatus (*game_settings)(EnshwickModHandle me,
                                  uint8_t *out,
                                  uint32_t size,
                                  uint32_t *preset);
  // Replaces the server's live game settings with `data`, the type's
  // whole reflected layout, as the game's admin command does: systems
  // see them from the next step and every client gets them; the preset
  // becomes custom. Until the server stops: nothing writes them to disk.
  // `Status::BAD_ARGS` for a wrong size or a day or night length of
  // zero or less. On the server's game thread only.
  EnshwickStatus (*set_game_settings)(EnshwickModHandle me, const uint8_t *data, uint32_t size);
  // Calls `callback` with `user` on damage about to reach an entity's
  // health in the server's world, before the game applies it: hits,
  // falls and the shroud; what the filters leave in the
  // `DamageCheck` is what the game applies. Runs on the game's worker
  // threads, in load order. Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*damage_filter)(EnshwickModHandle me, EnshwickDamageFilterFn callback, void *user);
  // Up to `capacity` buff and debuff types of the running game, and how
  // many there are. On the server's game thread only.
  // Every entry is as large as the header of the first one says.
  EnshwickStatus (*buff_types)(EnshwickModHandle me,
                               struct EnshwickBuffTypeInfo *out,
                               uint32_t capacity,
                               uint32_t *count);
  // Up to `capacity` buffs `entity` has, and how many there are;
  // `Status::MISSING` for an entity that holds no buffs. On the
  // server's game thread only.
  // Every entry is as large as the header of the first one says.
  EnshwickStatus (*buffs)(EnshwickModHandle me,
                          EnshwickEntity entity,
                          struct EnshwickActiveBuff *out,
                          uint32_t capacity,
                          uint32_t *count);
  // Gives an entity a buff, as the game restores a character's saved
  // buffs: at once, for the duration asked. One the entity has already
  // is renewed to that duration. `Status::MISSING` for an entity that
  // holds no buffs, `Status::BAD_ARGS` for an unknown type,
  // `Status::REFUSED` for a type removed in this step, which the game
  // clears only in the next. On the server's game thread only.
  EnshwickStatus (*apply_buff)(EnshwickModHandle me, const struct EnshwickBuffRequest *request);
  // Removes every buff of type `id` from `entity`, as the game cleanses
  // one: it goes in the next step. Writes 1 to `removed` when the entity
  // had it, else 0; `Status::MISSING` for an entity that holds no
  // buffs. On the server's game thread only.
  EnshwickStatus (*remove_buff)(EnshwickModHandle me,
                                EnshwickEntity entity,
                                uint32_t id,
                                uint8_t *removed);
  // Writes knowledge `id` of the player whose character is `entity`, or
  // of the world for 0 or for an id of the world's;
  // `Status::MISSING` for an id the game does not know,
  // `Status::REFUSED` for a player not in the game. On the server's
  // game thread only.
  EnshwickStatus (*knowledge)(EnshwickModHandle me,
                              EnshwickEntity entity,
                              EnshwickKnowledgeId id,
                              struct EnshwickKnowledgeInfo *out);
  // Grants knowledge `id`, of `kind`, to the player whose character is
  // `entity`, or to the world for 0, as the game grants it: the next
  // step applies it with what follows (experience, unlocks, clients,
  // the save). `value` is ignored for `KnowledgeInfo::BOOL`, added for
  // `KnowledgeInfo::COUNTER`, the bit to set for
  // `KnowledgeInfo::FLAGS`. `Status::BAD_ARGS` for an unknown id,
  // a kind that is not the id's or a bit the game does not expect,
  // `Status::REFUSED` for a player not in the
  // game or a full queue. On the server's game thread only.
  EnshwickStatus (*grant_knowledge)(EnshwickModHandle me,
                                    EnshwickEntity entity,
                                    EnshwickKnowledgeId id,
                                    uint32_t kind,
                                    uint32_t value);
  // Up to `capacity` quest ids of the running game, and how many there
  // are. On the server's game thread only.
  EnshwickStatus (*quests)(EnshwickModHandle me,
                           EnshwickKnowledgeId *out,
                           uint32_t capacity,
                           uint32_t *count);
  // Writes how clear, rainy, snowy and stormy it is at `position`, in
  // 32.32 fixed-point metres, as weights that add up to 1, as the game's
  // systems weigh it. On the server's game thread only.
  EnshwickStatus (*weather_at)(EnshwickModHandle me, const int64_t (*position)[3], float (*out)[4]);
  // Up to `capacity` current weather fronts of the server's world, one per
  // layer that has one, and how many there are. On the server's game
  // thread only.
  // Every entry is as large as the header of the first one says.
  EnshwickStatus (*weather_fronts)(EnshwickModHandle me,
                                   struct EnshwickWeatherFront *out,
                                   uint32_t capacity,
                                   uint32_t *count);
  // Starts new weather in every layer of the server's world: fronts of
  // `effect` (`WeatherFront::NONE` for clear skies) where a layer has
  // one, clear skies elsewhere, for `duration` game nanoseconds or the
  // game's own for 0; `u32::MAX` lets the game choose anew. Clients
  // follow. On the server's game thread only.
  EnshwickStatus (*set_weather)(EnshwickModHandle me, uint32_t effect, int64_t duration);
  // Up to `capacity` terrain materials of the running game, and how many
  // there are. On the server's game thread only.
  // Every entry is as large as the header of the first one says.
  EnshwickStatus (*terrain_materials)(EnshwickModHandle me,
                                      struct EnshwickTerrainMaterialInfo *out,
                                      uint32_t capacity,
                                      uint32_t *count);
  // Runs a terrain edit in the server's world as the game's terraforming
  // does, and writes the volume it changed in cubic metres (for a
  // flatten, 1 when anything changed) to `amount`; clients see it and
  // the save keeps it. Building voxels are left alone.
  // `Status::BAD_ARGS` for an edit out of range;
  // `Status::OUTSIDE_WORLD` for one that reaches past the world's edge
  // or above the height the loader takes, `Status::NOT_LOADED` unless
  // every column of the box it can write is loaded, writing nothing. On
  // the server's game thread only.
  EnshwickStatus (*terrain_edit)(EnshwickModHandle me,
                                 const struct EnshwickTerrainEdit *edit,
                                 float *amount);
  // Hands the loader the mod's settings schema, `schema`, the TOML text
  // of a `config::Schema`; a later call replaces
  // it. The loader answers `enshwick config` on the console from it and checks values against it
  // before it writes them. `Status::BAD_ARGS` for text that is not a
  // schema. Any time.
  EnshwickStatus (*config_schema)(EnshwickModHandle me, struct EnshwickStr schema);
  // Calls `callback` with `user` after a summoning staff's use spawned or
  // moved a survivor, with `Summon::consumed` set; its return value is
  // ignored. On the thread of the use, as `HostApi::summon_filter`.
  // Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*summoned)(EnshwickModHandle me, EnshwickSummonFn callback, void *user);
  // Opens the item data kind `id`, `name` for the mod's own
  // (`<mod>:<name>`) or `mod:name` for any mod's, each side 1 to 64 of
  // `[a-z0-9_-]`, and writes its index to `out`; every mod gets the same
  // index for one id. Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*item_data_open)(EnshwickModHandle me, struct EnshwickStr id, uint32_t *out);
  // Copies the value of kind `kind` on the item entity `pide` holding
  // item `item` into `out`, at most `capacity` bytes, and writes its
  // length to `len`; `Status::MISSING` when it has none or its entity
  // holds another item, `Status::BAD_ARGS` when `capacity` is short,
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
  // the clients that run Enshwick get the change. `Status::REFUSED`
  // on a client alone, `Status::MISSING` before the world has loaded.
  // Any thread.
  EnshwickStatus (*item_data_set)(EnshwickModHandle me,
                                  uint32_t kind,
                                  EnshwickEntity pide,
                                  EnshwickItemId item,
                                  const uint8_t *data,
                                  uint32_t len);
  // Calls `callback` with `user` once per frame of the client's
  // interface, on the thread that draws it, while the mod is on; the
  // callback draws with `HostApi::ui_widget`. Client only
  // (`Status::MISSING` on a server); when the interface could not be
  // linked, the callback never runs. Allowed during `enshwick_mod_init`
  // only.
  EnshwickStatus (*ui_frame)(EnshwickModHandle me, EnshwickUiFrameFn callback, void *user);
  // Draws `widget` in the frame `frame` names and writes back its
  // `index`, `value` and `result`. Only inside the mod's frame callback
  // (`Status::WRONG_THREAD` elsewhere); `Status::REFUSED` for a
  // widget outside a window or an `UiWidget::END` without one;
  // `Status::MISSING` for a widget this game build lacks.
  EnshwickStatus (*ui_widget)(EnshwickModHandle me, uint64_t frame, struct EnshwickUiWidget *widget);
  // Queues `dialog` and calls `callback` with the answer and `user` once
  // the player gives it; dialogs show one at a time, in order. Any
  // thread. Client only (`Status::MISSING` on a server); when the
  // interface could not be linked, the dialog shows as a system message
  // box. `Status::REFUSED` when too many wait; `Status::WRONG_THREAD`
  // for `UiDialog::WAITING` on the thread that draws the interface. On
  // an error status `callback` is never called.
  EnshwickStatus (*ui_dialog)(EnshwickModHandle me,
                              const struct EnshwickUiDialog *dialog,
                              EnshwickUiAnswerFn callback,
                              void *user);
  // Calls `callback` with `user` once per frame while the game draws its
  // HUD in game, hidden with it; the callback draws labels and bars with
  // `HostApi::ui_widget` at their `anchor`. Client only
  // (`Status::MISSING` on a server). Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*ui_hud)(EnshwickModHandle me, EnshwickUiFrameFn callback, void *user);
  // Replaces the mod's markers on the client's map with the `count` at
  // `markers`, copied; none clears them. Any thread. Client only
  // (`Status::MISSING` on a server); `Status::BAD_ARGS` past 256
  // markers.
  // Every entry is as large as the header of the first one says.
  EnshwickStatus (*ui_markers)(EnshwickModHandle me,
                               const struct EnshwickUiMarker *markers,
                               uint32_t count);
  // Makes `callback` with `user` the mod's check of a new settings text,
  // which the loader runs before it writes a change from the console or the
  // Mods screen; a refusal keeps the file. Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*config_check)(EnshwickModHandle me, EnshwickConfigCheckFn callback, void *user);
  // Hands the loader the mod's texts, a TOML table per language code as
  // `crate::i18n::Texts` reads them, for the loader's own screens: the
  // Mods screen labels the mod's settings with them. A later call
  // replaces them.
  EnshwickStatus (*texts)(EnshwickModHandle me, struct EnshwickStr toml);
  // Serves `len` bytes at `data` as game content, the pixels of a
  // texture the mod serves, and writes the content hash a resource names
  // them by to `out`; the same bytes give the same hash. The loader
  // keeps the bytes for the life of the process. Any thread.
  EnshwickStatus (*content_serve)(EnshwickModHandle me,
                                  const uint8_t *data,
                                  uint32_t len,
                                  uint32_t (*out)[4]);
  // Copies the game's own content `hash` from its archive into `out`, at
  // most `capacity` bytes, and writes its length to `len`;
  // `Status::MISSING` for a hash the archive lacks or keeps
  // compressed, `Status::BAD_ARGS` when `capacity` is short, with
  // `len` written. Reads the archive's files, never the game's memory.
  // Any thread.
  EnshwickStatus (*content_read)(EnshwickModHandle me,
                                 const uint32_t (*hash)[4],
                                 uint8_t *out,
                                 uint32_t capacity,
                                 uint32_t *len);
  // `Status::OK` when this client shows the UI images mods serve: the
  // content and UI registry links run and the registry knows the served
  // images; `Status::MISSING` otherwise, and always on a server. Any
  // thread.
  EnshwickStatus (*icons_ready)(EnshwickModHandle me);
  // Defines the stat kind `id` with `def`, or opens it with a null
  // `def`, and writes its index and key to `out`. A mod defines kinds in
  // its own namespace (`name` or `<mod>:name`); any mod opens any kind,
  // the loader's built-in ones (`enshwick:<name>`) among them. Every mod
  // gets the same index for one id. `Status::BAD_ARGS` for a malformed
  // id or definition, `Status::REFUSED` for a kind defined twice or in
  // another mod's namespace. Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*stat_open)(EnshwickModHandle me,
                              struct EnshwickStr id,
                              const struct EnshwickStatDefData *def,
                              struct EnshwickStatRef *out);
  // Writes the definition of stat `stat` to `def` and its full id to
  // `id`, a string the loader keeps for the process; `Status::MISSING`
  // for a kind opened but never defined. Any thread.
  EnshwickStatus (*stat_info)(EnshwickModHandle me,
                              uint32_t stat,
                              struct EnshwickStatDefData *def,
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
  // follows after the next step. `Status::REFUSED` on a client alone,
  // or for a saved modifier on an entity that is no player. Any thread.
  EnshwickStatus (*stat_modify)(EnshwickModHandle me, const struct EnshwickStatModify *request);
  // Gives the buff type `buff` the `count` modifiers at `modifiers`:
  // they count toward every watched entity that has the buff. Allowed
  // during `enshwick_mod_init` only.
  EnshwickStatus (*stat_buff)(EnshwickModHandle me,
                              uint32_t buff,
                              const struct EnshwickStatModifierData *modifiers,
                              uint32_t count);
  // Calls `callback` with `user` for every stat value that changed on a
  // watched entity, on the game thread after the step that changed it.
  // Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*stats_changed)(EnshwickModHandle me, EnshwickStatChangeFn callback, void *user);
  // Writes up to `capacity` modifiers that count toward stat `stat` on
  // `entity`, in the order they apply, and how many there are to
  // `count`; every stat for `u32::MAX`. Any thread.
  EnshwickStatus (*stat_sources)(EnshwickModHandle me,
                                 EnshwickEntity entity,
                                 uint32_t stat,
                                 struct EnshwickStatSource *out,
                                 uint32_t capacity,
                                 uint32_t *count);
  // Watches `entity` for the items it wears and the buffs it has, or
  // stops for `watch` 0: players are always watched, and so is every
  // entity a mod keeps a modifier on. Any thread.
  EnshwickStatus (*stat_watch)(EnshwickModHandle me, EnshwickEntity entity, uint8_t watch);
  // Defines the mod event `spec` describes, named `<id>:<name>` after
  // the defining mod, and writes the id `HostApi::send_event` takes;
  // `Status::REFUSED` for a name outside the mod's id or defined
  // twice. Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*define_event)(EnshwickModHandle me,
                                 const struct EnshwickEventSpec *spec,
                                 uint32_t *out);
  // Calls `callback` with `user` and the event's data on every mod event
  // `spec` names, once its definition, checked against `spec`'s layout
  // when every mod is initialised, matches. Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*subscribe_event)(EnshwickModHandle me,
                                    const struct EnshwickEventSpec *spec,
                                    EnshwickModEventFn callback,
                                    void *user);
  // Hands `data` to every subscriber of the mod's own event `event`, on
  // this thread, before it returns; reaches no one before every mod is
  // initialised. `Status::REFUSED` for an event another mod defined.
  // Any thread.
  EnshwickStatus (*send_event)(EnshwickModHandle me, uint32_t event, const void *data);
  // Defines the affix `id` in the mod's namespace with `def` and writes
  // its index and key to `out`. Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*affix_open)(EnshwickModHandle me,
                               struct EnshwickStr id,
                               const struct EnshwickAffixDefData *def,
                               struct EnshwickStatRef *out);
  // Rolls affixes onto an item from every mod's affixes that fit its
  // category, replacing those it had, and writes how many to `count`;
  // `Status::REFUSED` for a `AffixRoll::ONCE` roll of an item that
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
  // and `rarity`. Any thread.
  EnshwickStatus (*affix_info)(EnshwickModHandle me,
                               uint32_t affix,
                               struct EnshwickStr *text,
                               uint8_t *rarity);
  // Registers the mod's migrations, `set`, copied: the loader runs the
  // settings steps on the settings file now, and every step on each
  // world's files when the world loads, once per world, and records what
  // ran in `mod-data/enshwick/migrations.toml`. `Status::REFUSED` for
  // a second call or a migration that failed on the settings file, with
  // the reason logged. Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*migrate)(EnshwickModHandle me, const struct EnshwickMigrationSet *set);
  // Declares the content id `id`, `<mod>:<name>` in the mod's own
  // namespace, and writes its GUID to `out`; `Status::REFUSED` for a
  // malformed id, one outside the namespace, or one whose game id
  // another content id has, which turns the mod off with both ids and
  // both mods named. Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*declare_content)(EnshwickModHandle me, struct EnshwickStr id, uint8_t (*out)[16]);
  // Opens the entity data kind `id`, as `HostApi::item_data_open` opens
  // an item kind, and writes its index to `out`; with
  // `EntityDataFlags::SENT` the clients that run Enshwick get its
  // values. `HostApi::item_data_get` and `HostApi::item_data_set`
  // read and write it with `item` 0; a set runs on the server's game
  // thread. `Status::REFUSED` for an id opened as an item kind. Allowed
  // during `enshwick_mod_init` only.
  EnshwickStatus (*entity_data_open)(EnshwickModHandle me,
                                     struct EnshwickStr id,
                                     EnshwickEntityDataFlags flags,
                                     uint32_t *out);
  // Lists the mods in `mods/` that a present mod replaces (see
  // `Relation::REPLACES`), which the loader never started, as
  // `HostApi::mods` lists the loaded ones, with
  // `ModInfo::replaced_by` set. Any thread.
  EnshwickStatus (*replaced)(EnshwickModHandle me,
                             struct EnshwickModInfo *out,
                             uint32_t entry_size,
                             uint32_t capacity,
                             uint32_t *count);
  // Registers a filter on the knowledge clients ask the server for in
  // their input, which a vanilla game grants unchecked. Filters run in
  // load order after the loader's guard, each with the verdict so far,
  // and the last answer decides. It runs on the server's worker threads,
  // in the dedicated server or a hosting client. `Status::MISSING` on
  // a build without the input site. Allowed during `enshwick_mod_init`
  // only.
  EnshwickStatus (*knowledge_input_filter)(EnshwickModHandle me,
                                           EnshwickKnowledgeInputFn callback,
                                           void *user);
  // The game language of the reader, as the game numbers them (see
  // `crate::i18n::Language`). On a server, while the loader calls the
  // mod about one player (an action, a settings check, the player's mod
  // message) on that thread, the language that player's game named when
  // it joined, always one the game knows; else `0`, the fallback. On a
  // client, the language it last asked a text in, `0` before its first
  // text. Any thread; the answer holds for the calling thread.
  uint32_t (*language)(EnshwickModHandle me);
  // Replaces the live game settings with `data` as
  // `HostApi::set_game_settings` does, and keeps the fields with a
  // byte set in `mask` (as long as `data`) with the running world: the
  // loader applies them again each time the world loads, by the end of
  // its first step. `Status::MISSING` while no world runs.
  // On the server's game thread only.
  EnshwickStatus (*keep_game_settings)(EnshwickModHandle me,
                                       const uint8_t *data,
                                       const uint8_t *mask,
                                       uint32_t size);
  // Forgets the game settings kept for the running world and puts back
  // the values it loaded with. On the server's game thread only.
  EnshwickStatus (*forget_game_settings)(EnshwickModHandle me);
  // Takes knowledge `id`, of `kind`, back from the player whose character
  // is `entity`, or from the world for 0 or an id of the world's, and has
  // the clients sent it again: a `KnowledgeInfo::BOOL` cleared, `value`
  // taken from a `KnowledgeInfo::COUNTER` (down to 0), bit `value` of
  // `KnowledgeInfo::FLAGS` cleared. Saved with the next save; what
  // followed the grant, such as experience, stays.
  // `Status::BAD_ARGS` for an unknown id, a kind that is not the id's or
  // a bit past 31, `Status::REFUSED` for a player not in the game or
  // whose save has not loaded yet. On the server's game thread only.
  EnshwickStatus (*revoke_knowledge)(EnshwickModHandle me,
                                     EnshwickEntity entity,
                                     EnshwickKnowledgeId id,
                                     uint32_t kind,
                                     uint32_t value);
  // Grants knowledge `id` of a player's, as `HostApi::grant_knowledge`
  // does, or takes it back as `HostApi::revoke_knowledge` does when
  // `revoke` is 1, for the account whose `PlayerInfo::key` is `key`:
  // now when its character is in the game, else once its next character
  // has loaded in this world; the loader keeps waiting changes with the
  // world. `Status::BAD_ARGS` for an unknown id, an id of the world's
  // or a kind that is not the id's, `Status::MISSING` while no world
  // runs. On the server's game thread only.
  EnshwickStatus (*account_knowledge)(EnshwickModHandle me,
                                      EnshwickPlayerKey key,
                                      EnshwickKnowledgeId id,
                                      uint32_t kind,
                                      uint32_t value,
                                      uint32_t revoke);
  // Copies the game's own resource `key` from its archive into `out`, at
  // most `capacity` bytes, as the game reads it before it resolves its
  // references, and writes its length to `len`; `Status::MISSING` for
  // a key the archive lacks, `Status::BAD_ARGS` when `capacity` is
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
  // language reads its table. `Status::MISSING` for a tag the game
  // lacks, and always on a server, whose archive has no texts. Any
  // thread.
  EnshwickStatus (*game_text)(EnshwickModHandle me,
                              uint32_t id,
                              uint32_t language,
                              struct EnshwickStr *out);
  // During `enshwick_mod_init`, by a mod that serves `key`: keeps the
  // base of `key` when it is first built, so it can be built again.
  // Afterwards: builds `key` again from that base with the patches of
  // every mod still on, as values they read changed, and writes the
  // bytes that differ over the resource the game holds; `Status::OK`
  // with nothing done while `key` was not built yet, as its first build
  // takes the values in force. Any thread. `Status::MISSING` when its
  // base was not kept, `Status::REFUSED` when the new build differs in length, which the
  // game's copy cannot take, `Status::BAD_ARGS` for a key the mod does
  // not serve. Each rebuild keeps the patches' new bytes for the process.
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
                                uint32_t disable);
  // Writes 1 to `out` when the recipe with game id `recipe` is disabled
  // in this process now, else 0: on a client joined to a server, by the
  // server's rules. Any thread.
  EnshwickStatus (*recipe_disabled)(EnshwickModHandle me, EnshwickRecipeId recipe, uint32_t *out);
  // Defines the extension point `spec` describes, named
  // `<id>:<point>` after the defining mod, and writes its id and the
  // count of its live links to `out`; `Status::REFUSED` for a name
  // outside the mod's id or defined twice. Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*define_point)(EnshwickModHandle me,
                                 const struct EnshwickPointSpec *spec,
                                 struct EnshwickPointHandle *out);
  // Attaches `callback` with `user` to the extension point `link`
  // names, in the order it asks for. Matched against the point's
  // definition once every mod is initialised; a point no running mod
  // defines, or one whose layout or mode differs, leaves the link inert.
  // Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*attach_point)(EnshwickModHandle me,
                                 const struct EnshwickPointLink *link,
                                 EnshwickPointFn callback,
                                 void *user);
  // Runs the live links of the mod's own extension point `point` over
  // `value`, which holds the mod's own answer, on this thread, and writes
  // to `verdict` whether the answer was kept, changed or refused.
  // `Status::REFUSED` for a point another mod defined. Any thread.
  EnshwickStatus (*run_point)(EnshwickModHandle me,
                              uint32_t point,
                              const void *input,
                              void *value,
                              EnshwickPointVerdict *verdict);
  // Registers the mod's player actions: the loader asks `callback` with
  // `user`, on the server's game thread, for the actions a player may
  // use, to run one a player picked in the interface or the console
  // named, and for the hints a player has at hand. One registration per
  // mod. Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*actions)(EnshwickModHandle me, EnshwickActionFn callback, void *user);
  // Writes the first `capacity` of the running build's game event types
  // to `out`, null or room for `capacity` entries each as large as the
  // header of the first one says, and how many there are to `count`:
  // every reflected type that derives from the engine's event base, in no
  // set order. The names are the loader's, valid for the rest of the
  // process. A null `count` gives `Status::BAD_ARGS` after the entries
  // are written; a handle the loader never gave out gives
  // `Status::BAD_ARGS` and a mod that is off `Status::FAULTED`, with
  // nothing written. Any thread.
  EnshwickStatus (*event_types)(EnshwickModHandle me,
                                struct EnshwickEventTypeInfo *out,
                                uint32_t capacity,
                                uint32_t *count);
  // Writes to `out` the kind of the recipes that make the item `item`,
  // from the item's static data; `Status::MISSING` while the game has
  // not loaded the item, which on a dedicated server comes after its
  // recipe registry is first built, and while the loader serves no
  // resource. Any thread.
  EnshwickStatus (*recipe_kind)(EnshwickModHandle me, EnshwickItemId item, EnshwickRecipeKind *out);
  // Reports whether the mod's feature `name` runs on this game build:
  // off for the reason `off`, or on when `off` is empty. The loader
  // logs it and shows it in the status summary and on the Mods screen; a
  // later report of the same name replaces the earlier one.
  // `Status::BAD_ARGS` for an empty name. Both strings are borrowed for
  // the call. Any thread.
  EnshwickStatus (*feature)(EnshwickModHandle me, struct EnshwickStr name, struct EnshwickStr off);
  // Declares that the calling mod multiplies `rate`, so the loader arms
  // what the rate needs; its factor is 1 until
  // `HostApi::set_rate`. `Status::MISSING` on a build that lacks
  // what the rate needs, `Status::BAD_ARGS` for a rate this loader
  // does not know, `Status::REFUSED` for a rate clients count too in a
  // mod without `ManifestFlags::JOIN`. A second declaration of the same rate is
  // `Status::OK`. `Status::TOO_LATE` after `enshwick_mod_init`,
  // `Status::FAULTED` for a mod that is off. Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*rate)(EnshwickModHandle me, EnshwickRateId rate);
  // Puts `factor` in force as the calling mod's factor for `rate`: the
  // game uses the product of every declaring mod's factor, held to 0 to
  // 100, from the next time it reads the rate. `Status::BAD_ARGS` for
  // a factor that is not finite or lies outside 0 to 100, or a rate the
  // mod did not declare, `Status::FAULTED` for a mod that is off. Any
  // thread; neither allocates nor locks once every mod is initialised,
  // except for a recipe output rate, whose change builds the recipe
  // registry again.
  EnshwickStatus (*set_rate)(EnshwickModHandle me, EnshwickRateId rate, float factor);
  // Declares a change of a vanilla value. The loader merges the changes
  // of every mod once their `init` ran: a set in load order, the last
  // winning and the overridden one reported in both mods' logs and the
  // conflict report; then every factor, multiplied. It builds the changed
  // resource the first time the game reads it, on each side from its own
  // mods. `Status::BAD_ARGS` for a null `change`, an unknown field or
  // operation and a value outside the field's range; `Status::MISSING`
  // when the build does not reflect the field. Init only.
  EnshwickStatus (*change_data)(EnshwickModHandle me, const struct EnshwickDataChange *change);
  // Calls `callback` with `user` before every wait of the engine's event
  // loop on its handles, on the thread that waits, and writes to `found`
  // which waits it hooked (`LoopWaitSites`); a wait this build lacks,
  // whose code is not what its table expects, or that the mod hooked
  // already is left out. `Status::BAD_ARGS` for a null `found`, before
  // anything is hooked. Handlers run in load order. Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*loop_wait)(EnshwickModHandle me,
                              EnshwickLoopWaitFn callback,
                              void *user,
                              EnshwickLoopWaitSites *found);
  // Calls `callback` with `user` once a pass where the server's game loop
  // has picked its wait before the next pass, in nanoseconds, which the
  // handler may change; writes 1 to `found` when the loader hooked the
  // place, else 0. `Status::BAD_ARGS` for a null `found`, before
  // anything is hooked. Handlers run in load order, each on the last one's wait. On
  // the game thread of the server loop. Allowed during `enshwick_mod_init`
  // only.
  EnshwickStatus (*server_wait)(EnshwickModHandle me,
                                EnshwickServerWaitFn callback,
                                void *user,
                                uint32_t *found);
  // Caps the worker threads of the task system's default and background
  // groups (`foreground` and `background`) as it starts, 0 leaving a group as the game sizes it; with
  // several mods the lowest cap wins. Writes 1 to `found` when a cap is
  // hooked and the task system starts after the hooks arm, which only the
  // dedicated server does, else 0. `Status::BAD_ARGS` for a null
  // `found`, before anything is hooked. Allowed during
  // `enshwick_mod_init` only.
  EnshwickStatus (*task_workers)(EnshwickModHandle me,
                                 uint32_t foreground,
                                 uint32_t background,
                                 uint32_t *found);
  // Calls `callback` with `user` for every altar of the server's world
  // when the loader sizes the altar zones: after altars change and after
  // `HostApi::altar_zones_changed`. Each filter, in load order, sees the
  // size so far, the vanilla one first, and may change it: a `grow` below
  // 0 counts as 0, a `height` below one cell keeps the height before it,
  // and the altar and level stay. It opens altars too, and may call the
  // host, such as `HostApi::altars`. On the server's game thread.
  // Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*altar_zone_filter)(EnshwickModHandle me, EnshwickAltarZoneFn callback, void *user);
  // Asks the loader to size the altar zones again on the server's next
  // step, as when a mod's settings change what its filter answers. Any
  // thread, also during init. `Status::REFUSED` for a mod without an
  // altar zone filter.
  EnshwickStatus (*altar_zones_changed)(EnshwickModHandle me);
  // Calls `callback` with `user` when the game asks how many altars a
  // Flame level allows, on whichever thread asks: the placement cursor
  // while a player aims an altar and the server's placement. Each filter,
  // in load order, sees the game's own count and the count so far and may
  // change `allowed`; the level and the game's count stay. The loader keeps
  // the answer within the zones the game can group
  // (`HostApi::base_limit`). A filter runs often: keep it cheap; it may
  // call the host. `Status::TOO_LATE` after `enshwick_mod_init`,
  // `Status::MISSING` on a build without the game's lookup.
  EnshwickStatus (*altar_limit_filter)(EnshwickModHandle me,
                                       EnshwickAltarLimitFilterFn callback,
                                       void *user);
  // Writes to `out` the most altar zones and bases the game groups now:
  // its own, or the loader's maximum once `HostApi::raise_base_limit` is
  // in force, from `enshwick_mod_ready` on, with the zones its own
  // grouping takes. `Status::BAD_ARGS` for a null `out`. Any thread.
  EnshwickStatus (*base_limit)(EnshwickModHandle me, struct EnshwickBaseLimitInfo *out);
  // Reads the voxels of the server's world in the box of `size` voxels
  // from `min`, in voxels of half a metre from the world's corner, into
  // `out`: x fastest, then y, then z, `size[0] * size[1] * size[2]`
  // entries. `min` and `size` are borrowed for the call; `out` points at
  // `capacity` writable entries, untouched unless the call returns
  // `Status::OK`. `Status::BAD_ARGS` for a null pointer, an empty
  // box, one past `Voxel::END`, more than `Voxel::READ_MAX` voxels or
  // a `capacity` below them; `Status::OUTSIDE_WORLD` for a box past
  // the world's edge or above the height the loader takes;
  // `Status::NOT_LOADED` while part of the box is not loaded
  // yet, which starts loading it; `Status::MISSING` before a world
  // runs or on a build without the read. On the server's game thread at
  // a quiet moment only, as the world's writes: `Status::WRONG_THREAD`
  // inside a step.
  EnshwickStatus (*voxel_read)(EnshwickModHandle me,
                               const uint32_t (*min)[3],
                               const uint32_t (*size)[3],
                               struct EnshwickVoxel *out,
                               uint64_t capacity);
  // Declares a change of a vanilla value, as
  // `HostApi::change_data` does, whose value the mod sets later with
  // `HostApi::set_change`; until then the mod changes nothing. The
  // loader builds every resource the change reaches from the first read,
  // so a later set takes effect in place. Writes the change's number to
  // `out`. `change.op` and `change.value` are ignored.
  // `Status::BAD_ARGS` for a null `change` or `out`, an unknown field
  // or selection, and an ingredient, whose change alters a recipe's
  // length; `Status::MISSING` when the build does not reflect the
  // field; `Status::REFUSED` for a field clients read in a mod without
  // `ManifestFlags::JOIN`; `Status::TOO_LATE` after
  // `enshwick_mod_init`.
  EnshwickStatus (*declare_change)(EnshwickModHandle me,
                                   const struct EnshwickDataChange *change,
                                   EnshwickChangeId *out);
  // Puts `value` in force for the calling mod's change `id` by `op`:
  // `ChangeOp::SET`, `ChangeOp::TIMES`, or `ChangeOp::RESET` to
  // take the mod's value out. The loader merges every mod's changes
  // again and builds each resource it patches anew in place; the game
  // reads the new value from then on. During `enshwick_mod_init` the
  // value counts from the first read. `Status::BAD_ARGS` for an id
  // the mod did not declare, an unknown operation, or a value the field
  // does not take; `Status::FAULTED` for a mod that is off;
  // `Status::TOO_LATE` when the value takes a restart, and the log says
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
  // Fills `check.refusal` with whether the server lets the player whose
  // character is `check.player` edit the box from `check.min` to
  // `check.max`. The permissions follow the game's terrain tool:
  // `CanEditWorld` for every edit, `CanEditBase` where the box overlaps a
  // building zone. The loader adds two tests the building cursor runs for
  // items: both corners inside the world's building area, and no strict
  // no-build zone the world places. A player not in game has no
  // permissions. `check` is borrowed for the call. `Status::BAD_ARGS`
  // for a null `check`, an old header, a corner that is not finite or a
  // `max` below `min`; `Status::MISSING` while no world is loaded or on
  // a build without the checks. On the server's game thread at a quiet
  // moment only, outside a step or in a game event:
  // `Status::WRONG_THREAD` inside a step.
  EnshwickStatus (*build_rights)(EnshwickModHandle me, struct EnshwickBuildRights *check);
  // Queues the `count` changes at `ops` as one transaction of the
  // server's world and writes its id to `id`: the loader writes them in
  // order on the server's game thread at quiet moments, a budget of
  // bricks a step, keeping the earlier state of every brick a change can
  // touch so `HostApi::transaction_undo` can put it back. `ops` and the
  // records they point at are borrowed for the call. `Status::BAD_ARGS`
  // for a null pointer, no changes, a kind or record the loader refuses
  // (as `HostApi::terrain_edit` does, and a dry run), or more than its
  // cap of changes or bricks; `Status::OUTSIDE_WORLD` for a change that
  // leaves the world, once the loader knows its extent (a transaction
  // committed earlier fails with it before writing); `Status::REFUSED`
  // while the queue is full; `Status::MISSING` while no world runs or
  // on a build without the edits, the voxel read or the undo write. Any
  // thread.
  EnshwickStatus (*transaction_commit)(EnshwickModHandle me,
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
  // `TransactionInfo::UNDO_REFUSED` with what was put back kept. A
  // refused undo may be asked again. `Status::OK` when queued or
  // already undoing; `Status::REFUSED` for an id this mod did not get,
  // one the loader forgot, one undone, one whose undo data is gone, or
  // while the transactions in work already claim the loader's cap of
  // undo data. Any thread.
  EnshwickStatus (*transaction_undo)(EnshwickModHandle me, uint64_t id);
  // Writes where transaction `id` of this mod stands to `out`, a
  // `TransactionInfo` with its header. `Status::BAD_ARGS` for a null
  // `out`; `Status::MISSING` for an id this mod did not get or one the
  // loader forgot (it keeps the newest few thousand). Any thread.
  EnshwickStatus (*transaction_status)(EnshwickModHandle me,
                                       uint64_t id,
                                       struct EnshwickTransactionInfo *out);
  // As `HostApi::transaction_commit`, for the player whose character
  // is `actor`: before it writes anything the loader checks that the
  // server lets the player edit the box of every change, as
  // `HostApi::build_rights` does, and marks building events with the
  // player. A refusal fails it with `Status::REFUSED` and the reason in
  // `TransactionInfo::refusal`; nothing is written. The check runs at
  // its first step on the server's game thread, never at the commit.
  // `Status::BAD_ARGS` for an `actor` of 0. Any thread.
  EnshwickStatus (*transaction_commit_for)(EnshwickModHandle me,
                                           EnshwickEntity actor,
                                           const struct EnshwickTransactionOp *ops,
                                           uint32_t count,
                                           uint64_t *id);
  // `Status::OK` on the server's game thread at a quiet moment while a
  // world runs, where a mod may read live components that no system
  // writes meanwhile; `Status::WRONG_THREAD` on another thread or
  // inside a step, `Status::MISSING` while no world runs.
  EnshwickStatus (*world_quiet)(EnshwickModHandle me);
  // Asks the loader to keep a copy of the client's fog of war for this
  // mod. Client only (`Status::MISSING` on a dedicated server).
  // Allowed during `enshwick_mod_init` only.
  EnshwickStatus (*fog_open)(EnshwickModHandle me);
  // Writes the fog's `FogInfo` to `info`, then copies the box of
  // `width` × `height` pixels at (`x`, `y`) into `out`, row by row, at
  // most `capacity` bytes; a box of no pixels copies nothing. Any thread.
  // `Status::MISSING` while the client is in no world, when this build
  // lacks the fog, or after the loader stopped copying it; `Status::REFUSED` for a mod that did not
  // open it; `Status::OUTSIDE_WORLD` for a box past the image;
  // `Status::BAD_ARGS` for a null `info`, or a null or short `out`
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
  // Serves the call `spec` describes with `callback` and `user`, named
  // `<id>:<call>` after the serving mod or one of its former ids;
  // `Status::REFUSED` for another name, or a name and major served
  // twice, `Status::BAD_ARGS` for a spec that does not read. Allowed
  // during `enshwick_mod_init` only: `Status::TOO_LATE` once every mod
  // is initialised.
  EnshwickStatus (*serve_call)(EnshwickModHandle me,
                               const struct EnshwickCallSpec *spec,
                               EnshwickCallFn callback,
                               void *user);
  // Asks for the call `spec` describes and writes its id to `out`.
  // Matched against the served call by name and major once every mod is
  // initialised: the input and the answer must agree field by field and
  // in size, or the call stays unmatched with the reason in the log.
  // `Status::REFUSED` when the mod asked for the name and major already
  // with another spec, `Status::BAD_ARGS` for a spec that does not read
  // or a null `out`. Allowed during `enshwick_mod_init` only:
  // `Status::TOO_LATE` once every mod is initialised.
  EnshwickStatus (*find_call)(EnshwickModHandle me,
                              const struct EnshwickCallSpec *spec,
                              uint32_t *out);
  // Runs the call `id` from `HostApi::find_call` on this thread: the
  // server's handler reads `input` and writes the answer to `output`,
  // both as large as the spec says. `Status::MISSING` while the call
  // is unmatched or before every mod is initialised, `Status::REFUSED`
  // when the serving mod or its handler is off, `Status::BAD_ARGS` for
  // a null pointer or an id the mod was not given, or what the handler
  // answers. Any thread.
  EnshwickStatus (*call)(EnshwickModHandle me, uint32_t id, const void *input, void *output);
  // Writes the entities of the props transaction `id` of this mod placed
  // and still holds, oldest first, up to `capacity` of them at `out`,
  // and their number to `count`; a prop is in the world from the step
  // after the one that placed it. `out` may be null with `capacity` 0 to
  // learn the number. `Status::BAD_ARGS` for a null `count`;
  // `Status::MISSING` for an id this mod did not get or one the loader
  // forgot. Any thread.
  EnshwickStatus (*transaction_entities)(EnshwickModHandle me,
                                         uint64_t id,
                                         EnshwickEntity *out,
                                         uint32_t capacity,
                                         uint32_t *count);
  // Writes to `kept` 1 when a save keeps changes in the box from `min`
  // to `max`, in metres, and 0 when not: the game saves a world only
  // inside its bases' zones, so the box must lie wholly inside one, half
  // a metre in from its edges. `Status::BAD_ARGS` for a null pointer,
  // a corner that is not finite or a `max` below `min`;
  // `Status::MISSING` while no world is loaded or on a build without
  // the zone list. On the server's game thread at a quiet moment only:
  // `Status::WRONG_THREAD` elsewhere or inside a step.
  EnshwickStatus (*world_persists)(EnshwickModHandle me,
                                   const double (*min)[3],
                                   const double (*max)[3],
                                   uint8_t *kept);
  // Up to `capacity` map markers of the scene the server runs and how
  // many there are: what the scene's global chunk places, read from the
  // game's archive once per scene on a thread of the loader's;
  // `Status::NOT_LOADED` until that read is done. Entries are `entry_size` bytes apart,
  // the caller's `size_of::<MapMarkerInfo>()`, at least a `Header`
  // (`Status::BAD_ARGS` below it); a null `out` asks only for the
  // count, and a null `count` is not written. On the server's game
  // thread at a quiet moment (`Status::WRONG_THREAD` elsewhere or
  // inside a step); `Status::MISSING` while no world runs or when the
  // scene does not read on this build.
  EnshwickStatus (*map_markers)(EnshwickModHandle me,
                                struct EnshwickMapMarkerInfo *out,
                                uint32_t entry_size,
                                uint32_t capacity,
                                uint32_t *count);
  // Asks the game for the account action `action` on `account`, as its
  // administrators' account actions do: `AccountActionType::KICK`
  // removes a player online, `AccountActionType::BAN` puts the account
  // on the server's ban list and removes its player when online,
  // `AccountActionType::UN_BAN` takes it off the list. A ban of an
  // account not online goes straight onto the list, which the dedicated
  // server writes to `bannedAccounts` of `enshrouded_server.json`.
  // `Status::REFUSED` for a kick or a ban of an administrator online,
  // whom the game never removes, for a kick or a ban of a hosted game's
  // host, for a kick of an account not online, and for a ban of an account
  // not online that the game's list does not take; an account on the
  // list already is no error. `Status::BAD_ARGS` for account 0 or an
  // action this loader does not know; `Status::MISSING` while no server runs or on a build that
  // lacks the action. On the server's game thread between steps only:
  // `Status::WRONG_THREAD` elsewhere.
  EnshwickStatus (*account_action)(EnshwickModHandle me,
                                   EnshwickAccountActionType action,
                                   EnshwickPlayerKey account);
  // Puts `account` on the loader's allowlist when `listed` is 1, or takes
  // it off when 0: the accounts a dedicated server with the loader's
  // `allowlist` setting on lets join, beside its administrators. The
  // list is the file `mod-data/enshwick/allowlist.txt`, written at once
  // whether the setting is on or not; it applies from the next join.
  // `Status::OK` also when the list already says so;
  // `Status::BAD_ARGS` for account 0 or `listed` above 1;
  // `Status::REFUSED` when the file is not written;
  // `Status::MISSING` outside a dedicated server. Any thread, also
  // while another call or the console changes the list: changes take
  // turns and the file is replaced whole. A panic in the loader returns
  // `Status::REFUSED`. Since minor 1.
  EnshwickStatus (*allowlist_set)(EnshwickModHandle me, EnshwickPlayerKey account, uint8_t listed);
  // Writes 1 to `listed` when the loader's allowlist lists `account`, 0
  // when not or when the file never read, whether the `allowlist`
  // setting is on or not. `listed` is the caller's, writable for one
  // byte for the call, and written only on `Status::OK`.
  // `Status::BAD_ARGS` for account 0 or a null `listed`;
  // `Status::MISSING` outside a dedicated server. Any thread, also
  // while the list changes. A panic in the loader returns
  // `Status::REFUSED`. Since minor 1.
  EnshwickStatus (*allowlist_has)(EnshwickModHandle me, EnshwickPlayerKey account, uint8_t *listed);
} EnshwickHostApi;

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

// What a mod is, read by the loader before it runs any of the mod's code.
//
// It is static data returned by `enshwick_mod_query`.
typedef struct EnshwickManifest {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // `<author>.<name>`, see `is_valid_id`; names the settings file.
  struct EnshwickStr id;
  // The mod's SemVer version.
  struct EnshwickStr version;
  // The sides the mod runs on.
  EnshwickSide side;
  // Manifest flags.
  EnshwickManifestFlags flags;
  // Relations to other mods.
  const struct EnshwickDependency *dependencies;
  // How many entries `dependencies` points at.
  size_t dependency_count;
  // Symbols the mod pins itself, per build.
  const struct EnshwickBuildRow *builds;
  // How many rows `builds` points at.
  size_t build_count;
  // The name players read, such as `Survivors per Altar`; empty for the
  // id.
  struct EnshwickStr name;
  // One sentence on what the mod does.
  struct EnshwickStr description;
  // The authors' names.
  const struct EnshwickStr *authors;
  // How many names `authors` points at.
  size_t author_count;
  // An SPDX licence expression, such as `MIT OR Apache-2.0`.
  struct EnshwickStr license;
  // Where players read about the mod, an `https://` address.
  struct EnshwickStr homepage;
  // The schemas of the extension points the mod defines, as TOML that
  // `cargo enshwick point` reads; empty for none. The same text sits in
  // the DLL's `.enshpt` section, so a tool reads it without running the
  // mod.
  struct EnshwickStr points;
} EnshwickManifest;

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
  EnshwickEntity entity;
  // What killed the player: 0 a hit, 1
  // a fall, 2 the shroud,
  // 3 suffocation.
  uint32_t cause;
  // The account, the same number in every session.
  EnshwickPlayerKey key;
  // Where, in metres.
  float position[3];
  // For a hit, the template GUID of what dealt it, a creature's or an
  // impact's; zero for other causes.
  uint8_t killer[16];
  // 1 when the shroud was the deadly kind.
  uint8_t deadly_shroud;
  // 1 when the player drowned.
  uint8_t drowned;
  // Zero.
  uint16_t reserved;
  // For a hit, the attacker of the last hit the player took, from its
  // `HitHistory`, as `EntityDiedEvent` reads it; 0 for other
  // causes, or when the history names none.
  EnshwickEntity killer_entity;
  // Zero.
  uint32_t reserved_tail;
} EnshwickPlayerDeathData;
// A hit.
#define EnshwickPlayerDeathData_HIT 0
// A fall.
#define EnshwickPlayerDeathData_FALL 1
// The shroud.
#define EnshwickPlayerDeathData_SHROUD 2
// Out of breath, under water.
#define EnshwickPlayerDeathData_SUFFOCATED 3

// A prop a transaction places, as a player places one from an item, for
// `TransactionOp::PLACE_PROP`.
typedef struct EnshwickPropPlace {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The item whose placed template the prop is.
  EnshwickItemId item;
  // Zero.
  uint32_t reserved;
  // Where it stands, in metres.
  double position[3];
  // Which way it faces, a quaternion x, y, z, w.
  float orientation[4];
} EnshwickPropPlace;

// A prop a transaction removes, for `TransactionOp::REMOVE_PROP`.
typedef struct EnshwickPropRemove {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The prop's entity.
  EnshwickEntity entity;
  // `PropRemove::DISCARD_STATE` or none.
  uint32_t flags;
} EnshwickPropRemove;
// Removes the prop even when it holds state a player would lose (see
// `TransactionInfo::holds`); without it, such a prop is refused.
#define EnshwickPropRemove_DISCARD_STATE 1

// The data of the `QuestCompleted` event.
typedef struct EnshwickQuestData {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The player's character.
  EnshwickEntity entity;
  // The quest's id, a player knowledge id.
  EnshwickKnowledgeId quest;
} EnshwickQuestData;

// The data of the `Ticked` event.
typedef struct EnshwickTickData {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // Steps since the loader started counting, from 1.
  uint64_t step;
} EnshwickTickData;

// A box of voxels, for `TransactionOp::ERASE`.
typedef struct EnshwickVoxelRegion {
  // Size and version of this struct.
  struct EnshwickHeader header;
  // The box's lowest voxel, in voxels of half a metre.
  uint32_t min[3];
  // The box's voxels along each axis.
  uint32_t size[3];
} EnshwickVoxelRegion;

// Voxels a transaction writes over a box, for
// `TransactionOp::VOXELS`.
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
  uint64_t count;
} EnshwickVoxelWrite;

#if defined(_WIN32)
#define ENSHWICK_EXPORT __declspec(dllexport)
#else
#define ENSHWICK_EXPORT
#endif

// An `EnshwickStr` initializer for a string literal, without its final zero.
#define ENSHWICK_STR_INIT(literal) { (const uint8_t *)(literal), sizeof(literal) - 1 }

#ifdef __cplusplus
extern "C" {
#endif

// The ENSHWICK_ABI_MAJOR the mod was built for.
ENSHWICK_EXPORT uint32_t enshwick_mod_abi(void);
// The mod's manifest; static, no side effects.
ENSHWICK_EXPORT const EnshwickManifest *enshwick_mod_query(void);
// Registers hooks, services and settings.
ENSHWICK_EXPORT EnshwickStatus enshwick_mod_init(const EnshwickHostApi *host, EnshwickModHandle me);
// Runs after every chain is armed.
ENSHWICK_EXPORT void enshwick_mod_ready(const EnshwickHostApi *host, EnshwickModHandle me);
// Optional: stops what the mod started before a development reload unloads it.
ENSHWICK_EXPORT void enshwick_mod_shutdown(void);

#ifdef __cplusplus
}
#endif

#endif  /* ENSHWICK_H */
