//! The build table ids the loader names, typed by what they are.
//!
//! A [`Symbol`] is an engine function or address and a [`Constant`] an
//! offset, size, hash or count. Each id is declared once here, under the part of
//! the table it belongs to, and code names it by path, such as
//! `pinned::world::TIME`. Ids put together at run time, such as
//! `survivors.{name}`, go through the loader's `_by_name` lookups and are
//! listed by hand in their part's `TABLE_IDS`.

pub(crate) mod access;
pub(crate) mod altar;
pub(crate) mod attribute;
pub(crate) mod balancing;
pub(crate) mod bases;
pub(crate) mod buffs;
pub(crate) mod building;
pub(crate) mod chat;
pub(crate) mod chunks;
pub(crate) mod comfort;
pub(crate) mod damage;
pub(crate) mod daytime;
pub(crate) mod ecs;
pub(crate) mod experience;
pub(crate) mod game;
pub(crate) mod inventory;
pub(crate) mod item;
pub(crate) mod knowledge;
pub(crate) mod label;
pub(crate) mod loot;
pub(crate) mod map;
pub(crate) mod net;
pub(crate) mod npc;
pub(crate) mod pacing;
pub(crate) mod player;
pub(crate) mod resource;
pub(crate) mod save;
pub(crate) mod server;
pub(crate) mod session;
pub(crate) mod settings;
pub(crate) mod skill;
pub(crate) mod survivors;
pub(crate) mod terrain;
pub(crate) mod text;
pub(crate) mod textures;
pub(crate) mod ui;
pub(crate) mod user;
pub(crate) mod voxel;
pub(crate) mod weather;
pub(crate) mod world;

/// An engine function or address the build table pins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Symbol(&'static str);

/// An offset, size, hash or count the build table pins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Constant(&'static str);

macro_rules! id_type {
    ($($name:ident),*) => {$(
        impl $name {
            /// The id `id`, as the build table spells it.
            pub(crate) const fn new(id: &'static str) -> Self {
                Self(id)
            }

            /// The id as the build table spells it.
            #[must_use]
            pub(crate) const fn id(self) -> &'static str {
                self.0
            }
        }

        impl core::fmt::Display for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str(self.0)
            }
        }
    )*};
}

id_type!(Symbol, Constant);

/// Every symbol the loader links a detour on, which a test checks offers a
/// check of its code in every table that pins it.
#[cfg(test)]
pub(crate) const HOOKED: &[Symbol] = &[
    access::BASE_STOCK,
    access::DISMANTLE_METHOD,
    access::OFFER_TEST,
    access::SORT,
    access::TRANSFER,
    bases::BASE_AT,
    bases::BASE_IDS,
    bases::BED_CLEANUP,
    bases::CLIENT_CREATE,
    bases::CLIENT_DESTROY,
    bases::COMFORT_CLEANUP,
    bases::CURSOR,
    bases::GROUPING,
    bases::HOLDER_AT,
    bases::IN_BASE,
    bases::NEW_BASE,
    bases::NPC_TOTALS,
    bases::PROPS_CLEANUP,
    bases::REBUILD,
    bases::SERVER_CREATE,
    bases::SERVER_DESTROY,
    bases::SERVER_UPDATE,
    bases::SORT,
    bases::SORT_ALL,
    bases::STOCK_CACHE,
    bases::ZONES_AT,
    building::CURSOR_CHECK,
    building::MAX_ALTARS,
    building::PICKUP,
    building::SEAM_CHECK,
    building::SPAWN_PROP,
    chat::CLIENT_SEND,
    chat::POST,
    chunks::STEP,
    comfort::BASE_REGISTER,
    daytime::SET_SPEED,
    ecs::DESTROY_REQUEST,
    ecs::EVENT_MERGE,
    experience::KILL,
    experience::SOURCE_FACTOR,
    label::REPLACE,
    loot::STACK_ROLL,
    map::FOG_DESTROY,
    map::FOG_REVEAL,
    npc::BED_CHECK,
    npc::LOOKUP,
    npc::LOOKUP_CURSOR,
    npc::MOVE,
    npc::UPDATE,
    player::DEATH_SINK,
    resource::KEY_OF,
    resource::OPEN,
    resource::RESOLVE,
    save::PATH,
    server::PLAYER_REMOVE,
    server::POST_STEP,
    server::SAVE_STEP,
    server::SET_STATE,
    settings::FACTORY_SPEED,
    settings::RESOURCE_DROPS,
    skill::NODE_UNLOCKED,
    skill::POINTS_PER_LEVEL,
    survivors::REGISTRY_LOAD,
    survivors::SPAWN,
    text::LOOKUP,
    text::LOOKUP_ENGLISH,
    textures::KEYS,
    textures::REGISTRY_FILL,
    textures::REQUEST,
    ui::HUD,
    ui::POPUP,
    world::STEP,
];

/// Every symbol whose code the loader replaces or calls as a whole, which a
/// test checks carries a body in every table that pins it, so a
/// change to the game's logic there turns its feature off.
#[cfg(test)]
pub(crate) const BODIES: &[Symbol] = &[
    bases::BASE_AT,
    bases::BASE_IDS,
    bases::BED_CLEANUP,
    bases::CLIENT_CREATE,
    bases::CLIENT_DESTROY,
    bases::COMFORT_CLEANUP,
    bases::CTOR,
    bases::CURSOR,
    bases::ENTITY_QUERY,
    bases::GROUPING,
    bases::HOLDER_AT,
    bases::IN_BASE,
    bases::NEW_BASE,
    bases::NPC_TOTALS,
    bases::PROPS_CLEANUP,
    bases::REBUILD,
    bases::SERVER_CREATE,
    bases::SERVER_DESTROY,
    bases::SERVER_UPDATE,
    bases::SET_INSERT,
    bases::SORT,
    bases::SORT_ALL,
    bases::STOCK_ADVANCE,
    bases::STOCK_CACHE,
    bases::TEARDOWN,
    bases::ZONES_AT,
];
