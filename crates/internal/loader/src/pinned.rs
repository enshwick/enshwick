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
pub(crate) mod net;
pub(crate) mod npc;
pub(crate) mod pacing;
pub(crate) mod player;
pub(crate) mod resource;
pub(crate) mod save;
pub(crate) mod server;
pub(crate) mod session;
pub(crate) mod skill;
pub(crate) mod survivors;
pub(crate) mod terrain;
pub(crate) mod text;
pub(crate) mod textures;
pub(crate) mod ui;
pub(crate) mod user;
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
