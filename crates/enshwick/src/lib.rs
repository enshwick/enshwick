//! The Enshwick modding API for Enshrouded: the one crate a Rust mod depends on.
//!
//! Enshwick is a modding API platform for Enshrouded. A mod maker gets:
//!
//! - a typed Rust API for settings, events, the world, items and recipes, the
//!   game's interface and messages between server and client;
//! - [Luau scripts](https://github.com/enshwick/enshwick/blob/main/book/src/luau.md)
//!   and [data packs](https://github.com/enshwick/enshwick/blob/main/book/src/content/packs.md)
//!   without code, for mods that need no Rust;
//! - tests without the game: the `test` feature adds a fake host, so a mod's
//!   unit tests run on any computer;
//! - translations: [`i18n`] reads a mod's texts in the 15 languages the game
//!   offers, and each player sees their own language.
//!
//! The Enshwick loader comes with the platform and runs every mod, so players
//! install one thing.
//!
//! A mod is a DLL in the game's `mods/` folder. It implements [`Mod`] and
//! exports itself with [`declare_mod!`]. In [`Mod::init`] it reads its
//! settings and adds its handlers through [`Load`]. Where the API has no
//! call, it links hooks declared with [`detour!`] or [`#[hook]`](macro@hook)
//! through [`Load::hook`]. The loader turns on every hook once all mods are
//! initialised. A mod that fails or panics is turned off, and the game runs
//! vanilla where the mod acted.
//!
//! Depend on this crate from crates.io; `cargo enshwick new` writes the
//! lines. The derive and attribute macros come from `enshwick-macros`, which
//! this crate re-exports:
//!
//! ```toml
//! [dependencies]
//! enshwick = "<version>"
//!
//! [dev-dependencies]
//! enshwick = { version = "<version>", features = ["test"] }
//! ```
//!
//! A mod builds for `x86_64-pc-windows-msvc`, the game's target. The crate
//! also builds on other hosts, so a mod's tests run against the fake host
//! anywhere.
//!
//! # Modules
//!
//! | Area | Modules |
//! | --- | --- |
//! | The mod and the loader | [`Load`], [`Host`], [`abi`], [`mods`], [`config`], [`i18n`], [`status`], [`state`], [`files`], [`migrations`] |
//! | Hooks and game memory | [`detour!`], [`hook`](macro@hook), [`code`], [`ecs`], [`raw`], [`pacing`] |
//! | Events and timing | [`events`], [`game_events`], [`schedule`], [`time`] |
//! | The server's world | [`altars`], [`buffs`], [`daytime`], [`effects`], [`game_settings`], [`terrain`], [`voxels`], [`weather`] |
//! | Players and their things | [`inventory`], [`interactions`], [`knowledge`], [`progress`] |
//! | Game rules | [`damage`], [`loot`], [`stats`], [`affixes`], [`survivors`], [`vetoes`] |
//! | Content | [`content`], [`packs`] |
//! | Between mods | [`services`], [`points`], [`net`], [`shape`] |
//! | Interface | [`ui`] |
#![cfg_attr(
    feature = "unstable",
    doc = "| Unstable, with the `unstable` feature | [`world`], [`map`], [`item_data`], [`entity_data`], [`actions`], [`console`], [`rates`], [`script`](mod@script) |"
)]
#![cfg_attr(
    not(feature = "unstable"),
    doc = "| Unstable, with the `unstable` feature | `world`, `map`, `item_data`, `entity_data`, `actions`, `console`, `rates`, `script` |"
)]
//! | Tests | `test`, with the `test` feature |
//!
//! Items marked unstable may change in any release. Everything else keeps
//! the compatibility rules of the version. The book's
//! [Stability](https://github.com/enshwick/enshwick/blob/main/book/src/stability.md)
//! chapter names what is unstable and why.
//!
//! # Features
//!
#![doc = document_features::document_features!()]
//!
//! # Examples
//!
//! The mod names itself once, in its `Cargo.toml`; its version, description,
//! authors, licence and homepage come from `[package]`:
//!
//! ```toml
//! [package.metadata.enshwick]
//! id = "you.hello"
//! side = "both"
//! join = true
//! ```
//!
//! ```rust,standalone_crate
//! /// Hello's settings.
//! #[derive(serde::Deserialize, serde::Serialize, enshwick::Config)]
//! struct Settings {
//!     /// Altars added at every Flame level.
//!     extra_altars: u8,
//! }
//!
//! impl Default for Settings {
//!     fn default() -> Self {
//!         Self { extra_altars: 2 }
//!     }
//! }
//!
//! struct Hello;
//!
//! impl enshwick::Mod for Hello {
//!     fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
//!         let settings: Settings = load.config()?;
//!         let extra = settings.extra_altars;
//!         load.on_altar_limit(move |limit| limit.allowed = limit.allowed.saturating_add(extra))?;
//!         enshwick::info!("{extra} more altars per Flame level");
//!         Ok(Self)
//!     }
//! }
//!
//! enshwick::declare_mod!(Hello);
//! ```
#![cfg_attr(docsrs, feature(doc_cfg))]

/// The version of this crate, which a mod's generated `Cargo.toml` pins.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// The macros name this crate by its path, as mods see it, and the points of
// Enshwick's own mods are declared with them.
extern crate self as enshwick;

pub mod abi;
#[cfg(feature = "unstable")]
pub mod actions;
pub mod affixes;
pub mod altars;
#[cfg(doctest)]
mod book;
pub mod buffs;
pub mod code;
pub mod config;
#[cfg(feature = "unstable")]
pub mod console;
pub mod content;
pub mod damage;
pub mod daytime;
mod declare;
pub mod ecs;
pub mod effects;
mod engine_enum;
mod engine_ids;
#[cfg(feature = "unstable")]
pub mod entity_data;
pub mod events;
pub mod files;
pub(crate) mod fresh;
pub mod game_events;
pub mod game_settings;
mod hook;
mod host;
pub mod i18n;
pub mod interactions;
pub mod inventory;
#[cfg(feature = "unstable")]
pub mod item_data;
// Without `unstable` item data stays inside, for the stats of single items.
#[cfg(not(feature = "unstable"))]
#[expect(
    unreachable_pub,
    dead_code,
    reason = "item data is public only with `unstable`"
)]
mod item_data;
pub mod knowledge;
mod load;
mod locks;
pub mod loot;
#[cfg(feature = "unstable")]
pub mod map;
pub mod migrations;
pub mod mods;
pub mod net;
pub mod pacing;
pub mod packs;
pub mod points;
pub mod progress;
#[cfg(feature = "unstable")]
pub mod rates;
// Without `unstable` the rates stay inside, for the recipe and comfort
// scaling the stable parts and the fake host share.
#[cfg(not(feature = "unstable"))]
#[expect(
    unreachable_pub,
    dead_code,
    reason = "the rates are public only with `unstable`"
)]
mod rates;
pub mod raw;
#[cfg(doctest)]
mod refused;
mod rules;
pub mod schedule;
#[cfg(feature = "unstable")]
pub mod script;
// Without `unstable` the binding stays inside, so the crate's own types keep
// their derives; only a script runtime, which turns the feature on, reads
// them.
#[cfg(not(feature = "unstable"))]
#[expect(
    unreachable_pub,
    dead_code,
    reason = "the script binding is public only with `unstable`"
)]
mod script;
pub mod services;
pub mod shape;
pub mod state;
pub mod stats;
pub mod status;
pub mod survivors;
pub mod terrain;
#[cfg(feature = "test")]
pub mod test;
pub mod time;
pub mod ui;
pub mod vetoes;
pub mod voxels;
pub mod weather;
#[cfg(feature = "unstable")]
pub mod world;
mod world_error;

pub use abi::{Entity, ItemId, PlayerKey, RecipeId};
pub use enshwick_macros::Config;
/// Describes a type for script languages through [`script::Typed`],
/// [`script::IntoScript`] and, when every field is shown,
/// [`script::FromScript`].
///
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// A struct with named fields is a record of its public fields; fields of
/// type `Header` and fields named with a leading `_` or `reserved` are left
/// out. A newtype is its inner
/// type. An enum of unit variants is a choice of the variants' names. Doc
/// comments become the script's documentation.
///
/// # Examples
///
/// ```
/// use enshwick::script::{FromScript, IntoScript, Value};
///
/// /// A purchase at a stall.
/// #[derive(Clone, Debug, PartialEq, enshwick::Script)]
/// pub struct Sale {
///     /// What it cost.
///     pub price: u32,
/// }
///
/// let value = Sale { price: 5 }.into_value();
/// assert_eq!(Sale::from_value(value)?, Sale { price: 5 });
/// # Ok::<(), enshwick::script::Error>(())
/// ```
#[cfg(feature = "unstable")]
pub use enshwick_macros::Script;
/// Derives [`trait@Shape`] for a type that crosses between mods inside a
/// [`#[service]`](macro@service), [`#[event]`](macro@event) or
/// [`#[point]`](macro@point), so a change inside it refuses the link.
///
/// A struct is `#[repr(C)]` or `#[repr(transparent)]`; an enum has an
/// integer `repr` and unit variants, each hashed with its discriminant.
/// `#[shape(values(A, B))]` on a newtype over an integer hashes its named
/// constants with their values. `#[shape(api = "<name>")]` marks a type
/// of this crate, which a point schema names `enshwick.<name>`.
///
/// # Examples
///
/// ```
/// use enshwick::Shape;
///
/// #[derive(Clone, Copy, Debug, Shape)]
/// #[repr(transparent)]
/// pub struct Coins(pub u32);
///
/// #[derive(Clone, Copy, Debug, Shape)]
/// #[repr(transparent)]
/// #[shape(values(BUY, SELL))]
/// pub struct Deal(pub u32);
///
/// impl Deal {
///     pub const BUY: Self = Self(0);
///     pub const SELL: Self = Self(1);
/// }
///
/// #[enshwick::event(name = "you.trading-post:sold")]
/// #[derive(Clone, Copy, Debug)]
/// pub struct Sold {
///     pub price: Coins,
///     pub deal: Deal,
/// }
/// ```
pub use enshwick_macros::Shape;
/// Declares a call, a described service: one function from the input
/// struct to an answer, which this mod serves and others call. The struct
/// becomes `repr(C)` and implements [`services::Call`] with a layout hash
/// per field and one for the answer, which the loader checks for every
/// caller.
///
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// `#[call(name = "<server id>:<call>", output = <type>, version = <major>)]`;
/// `version` is 1 when left out. A changed field or answer is a new major.
/// Every type in it has a [`trait@Shape`].
///
/// # Examples
///
/// ```
/// #[enshwick::call(name = "you.trading-post:price-of", output = u32)]
/// #[derive(Clone, Copy, Debug)]
/// pub struct PriceOf {
///     pub item: u32,
/// }
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.serve_call(|input: &PriceOf| input.item * 10)
/// }
/// ```
#[cfg(feature = "unstable")]
pub use enshwick_macros::call;
/// Exports a [`Mod`] from the DLL with its manifest, which it reads from the
/// crate's `Cargo.toml` at compile time.
///
/// The five functions of [`abi::exports`] each catch panics; the type gains
/// `instance()`, the mod's state once initialised. In a test build
/// (`cfg(test)`) the functions keep their Rust paths instead of their C
/// names. A mod built with `panic = "abort"` fails to compile: the loader
/// turns a panicking mod off only while the panic unwinds.
///
/// The mod's identity is the `[package.metadata.enshwick]` table:
///
/// | Key | What |
/// | --- | --- |
/// | `id` | `<author>.<name>`, each of `[a-z0-9-]` (see [`abi::is_valid_id`]); names the settings file, the data folder and every name the mod declares |
/// | `side` | `"server"`, `"client"` or `"both"` |
/// | `name` | what players read; the id when left out |
/// | `join` | `true`: the mod counts in the join check |
/// | `reload` | `true`: a development server may reload it ([`ManifestFlags::RELOAD`](abi::ManifestFlags::RELOAD)) |
/// | `developer` | `true`: a developer tool; every player reads a warning while it runs ([`ManifestFlags::DEVELOPER`](abi::ManifestFlags::DEVELOPER)) |
/// | `replaces` | ids of mods this one takes the place of; the loader never starts them beside it |
/// | `requires`, `after`, `recommends`, `breaks` | tables of other mods by alias, each a mod id or `{ id, version }` with a Cargo-style range; `after` takes no range; [`deps!`] names them in code |
/// | `builds` | rows of symbols and constants the mod pins for game builds |
///
/// The version, description, authors, licence and homepage (else the
/// repository) come from `[package]`. A changed `Cargo.toml` rebuilds the
/// mod; a key, side, id or alias the table does not take fails the build
/// with its name. The loader names the mod and its version in every message
/// about it.
///
/// After the type, `points: [..]`, `events: [..]` and `calls: [..]` list the
/// [extension points](crate::points) (each a [`#[point]`](macro@point)
/// input), the [mod events](crate::services) (each an
/// [`#[event]`](macro@event) type) and the calls it serves, each a `#[call]`
/// input; their schemas go into the manifest, where `cargo enshwick point`,
/// `event` and `call` read them.
///
/// A test that declares several mods in one binary gives each its table
/// inline with `test::declare_mod!`, behind the `test` feature.
///
/// # Examples
///
/// The `Cargo.toml` of this crate's examples holds:
///
/// ```toml
/// [package.metadata.enshwick]
/// id = "you.hello"
/// side = "both"
/// join = true
///
/// [package.metadata.enshwick.requires]
/// lockers = { id = "you.lockers", version = ">=0.1, <1.0" }
///
/// [package.metadata.enshwick.recommends]
/// lanterns = "you.lanterns"
///
/// [[package.metadata.enshwick.builds]]
/// side = "server"
/// guid = "1c129660cd6efc4f9f0b21652d5ff2bf"
/// age = 387
///
/// [package.metadata.enshwick.builds.symbols]
/// "survivors.spawn_return" = 0x4_05cb
/// ```
///
/// ```rust,standalone_crate
/// struct Hello;
///
/// impl enshwick::Mod for Hello {
///     fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
///         Ok(Self)
///     }
/// }
///
/// enshwick::declare_mod!(Hello);
///
/// assert!(Hello::instance().is_none());
/// // SAFETY: `enshwick_mod_query` returns the `'static` manifest of
/// // `declare_mod!`, which no thread writes.
/// let manifest = unsafe { &*enshwick_mod_query() };
/// // SAFETY: `declare_mod!` points the id at a string literal, alive for the process.
/// assert_eq!(unsafe { manifest.id.as_str() }, Some("you.hello"));
/// assert_eq!(manifest.dependency_count, 2);
/// assert_eq!(manifest.build_count, 1);
/// // SAFETY: `declare_mod!` points the licence at this crate's `[package]`
/// // licence, a literal alive for the process.
/// assert_eq!(unsafe { manifest.license.as_str() }, Some("MIT"));
/// ```
pub use enshwick_macros::declare_mod;
/// Names what other mods offer by the aliases the crate's `Cargo.toml`
/// gives them: `deps!()` writes `pub mod deps`, one module per alias in
/// `requires`, `after` and `recommends`.
///
/// Each module holds `ID`, the dependency's mod id, and `id("<name>")`, its
/// content id `<dependency>:<name>`. A required dependency's ids are
/// constants: the loader starts this mod only beside it. An optional one's,
/// from `after` or `recommends`, are `None` while it does not run; ask once,
/// at init, and keep the id. A malformed name panics, running or not; in a
/// constant it fails the build, with the error at the call that wrote it. A call,
/// point or event of a dependency is declared with its own name and
/// `of = <alias>`: `#[point(of = lockers, name = "price", ..)]` is
/// `you.lockers:price`. Without `of`, a local name is this mod's own. An
/// alias the table lacks fails the build with the aliases it has.
///
/// `deps::vanilla` names the game's own content by the debug names its data
/// gives it, the functions of [`content::vanilla`] such as
/// [`item`](content::vanilla::item); no dependency takes the alias
/// `vanilla`.
///
/// # Examples
///
/// The crate's `Cargo.toml`, as on [`declare_mod!`], requires `lockers` and
/// recommends `lanterns`:
///
/// ```rust
/// use enshwick::content::Id;
/// use enshwick::services::ModEvent;
///
/// enshwick::deps!();
///
/// const LOCKER: Id = deps::lockers::id("locker");
/// assert_eq!(LOCKER.to_string(), "you.lockers:locker");
/// assert_eq!(deps::lanterns::ID, "you.lanterns");
/// // Outside the game no other mod runs, and no game data reads.
/// assert_eq!(deps::lanterns::id("lantern"), None);
/// assert!(deps::vanilla::item("Prop_Production_T1_Forge").is_err());
///
/// #[enshwick::event(of = lockers, name = "opened")]
/// #[derive(Clone, Copy, Debug)]
/// pub struct Opened {
///     pub locker: u32,
/// }
///
/// assert_eq!(Opened::NAME, "you.lockers:opened");
/// ```
pub use enshwick_macros::deps;
/// Declares a mod event other mods subscribe to: the struct becomes
/// `repr(C)` and implements [`services::ModEvent`] with a layout hash per
/// field.
///
/// `#[event(name = "<defining mod id>:<what>")]`; the struct is `Copy`
/// plain data and grows by appending fields, as a
/// [`#[service]`](macro@service) table does.
///
/// # Examples
///
/// ```
/// #[enshwick::event(name = "you.trading-post:sold")]
/// #[derive(Clone, Copy, Debug)]
/// pub struct Sold {
///     pub item: u32,
///     pub price: u32,
/// }
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_mod_event(|sold: &Sold| enshwick::info!("sold {} for {}", sold.item, sold.price))
/// }
/// ```
pub use enshwick_macros::event;
/// Declares a hook from its body, as [`detour!`] does: the function stays,
/// and a static [`Hook`] named after it in upper case joins it.
///
/// The first argument is `next: Next<'_, F>`, where `F` is the target's
/// `extern "system" fn` type or an alias of it; the other arguments are the
/// target's, by plain names. Arguments and the return value are `Copy`. `#[hook(priority = <i32>, flags = <HookFlags>)]` sets the link's
/// order and flags. The static takes the function's visibility and doc
/// comment. A panic in the body runs the game's function at most once, as
/// with [`detour!`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::HookFlags;
/// use enshwick::{Next, hook};
///
/// /// Counts upgrades.
/// #[hook(priority = 5, flags = HookFlags::CALLER_FRAME)]
/// fn upgrade(next: Next<'_, unsafe extern "system" fn(*mut u64)>, ctx: *mut u64) {
///     // SAFETY: the original takes the same context.
///     unsafe { next.call(ctx) }
/// }
///
/// assert_eq!(UPGRADE.name(), "UPGRADE");
/// ```
pub use enshwick_macros::hook;
/// Declares an extension point other mods extend: the input struct becomes
/// `repr(C)` and implements [`points::Point`] with a layout hash per field
/// and one for the answer, which the loader checks for every link.
///
/// `#[point(name = "<owner id>:<point>", mode = Observe | Advise | Override,
/// output = <type>, refusable)]`; `refusable` lets a link refuse, which
/// stops the chain. The input grows by appending fields; a changed field or
/// answer type needs a new name. Every type in it has a [`trait@Shape`], so
/// a change deep inside a field refuses the link too.
///
/// # Examples
///
/// ```
/// use enshwick::points::{Order, Verdict};
///
/// #[enshwick::point(name = "you.trading-post:price", mode = Override, output = u32, refusable)]
/// #[derive(Clone, Copy, Debug)]
/// pub struct Price {
///     pub item: u32,
/// }
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.extend_point(&Order::new(), |price: &Price, _coins: &u32| {
///         if price.item == 0 { Verdict::Refuse } else { Verdict::Pass }
///     })
/// }
/// ```
pub use enshwick_macros::point;
/// Exports the methods of an `impl` block marked `#[script]` to script
/// languages: `#[script(<part>)]` adds the constant `SCRIPT_<PART>`, a list
/// of [`script::Function`] a [`script::Bound`] type names in its parts.
///
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// A method takes `self` or `&self` and has a doc comment. Parameters and
/// the return type implement [`script::FromScript`] and
/// [`script::IntoScript`]; `&T` reads a `T`; an `impl Fn(A) -> R` parameter
/// takes a script function, with `A: IntoScript` and `R: script::Answer`; a
/// `Result` returns its `Ok` value and raises its `Err`'s message.
///
/// # Examples
///
/// See [`script::Function`].
#[cfg(feature = "unstable")]
pub use enshwick_macros::script;
/// Declares a service table other mods use: the struct becomes `repr(C)`
/// and implements [`services::Service`] with a layout hash per field, which
/// the loader checks when a mod asks for the service.
///
/// `#[service(name = "<publisher id>:<what>", version = <major>)]`, the
/// version 1 when left out. Fields are `extern "system" fn` pointers or
/// plain data. Append fields to grow the table: users built against the
/// shorter table keep working. Change a published field only with a new
/// major. Every type in it has a [`trait@Shape`], so a change deep inside a
/// field refuses the user too.
///
/// # Examples
///
/// ```
/// #[enshwick::service(name = "you.trading-post:prices", version = 1)]
/// pub struct Prices {
///     pub price: extern "system" fn(item: u32) -> u32,
/// }
///
/// extern "system" fn price(_item: u32) -> u32 {
///     3
/// }
///
/// static PRICES: Prices = Prices { price };
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.publish(&PRICES)
/// }
/// ```
pub use enshwick_macros::service;
#[cfg(not(feature = "unstable"))]
pub(crate) use enshwick_macros::{Script, script};
pub use hook::{FnPtr, Hook, Next};
pub use host::{Host, host};
pub use load::{Load, Mod, Off};
pub use shape::Shape;
pub use world_error::WorldError;

#[doc(hidden)]
pub mod __private {
    pub use const_format::concatcp;

    pub use crate::hook::Frame;
    pub use crate::locks::Relock;
    pub use crate::script::convert::{argument, record_fields};

    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    use crate::abi::{HostApi, Level, ManifestFlags, ModHandle, Status};
    use crate::{Host, Load, Mod};

    pub use crate::content::recipes::derive::{RecipeEdit, modify_recipes_in};
    pub use crate::engine_ids::ALL as ENGINE_IDS;
    /// The ids the API asks the loader for engine facts by, for the loader and
    /// the catalogue to name them from one place.
    pub mod ids {
        pub use crate::engine_ids::*;
    }

    /// The rules the loader and the fake host refuse a mod's requests by.
    pub mod rules {
        pub use crate::rules::*;
    }

    /// Links an `InventorySetup` holds, for the inventory walks of the API
    /// and the loader (server 399, client 426).
    pub const INVENTORY_LINKS: usize = 16;
    /// Slots a sub-inventory holds, for the same walks.
    pub const INVENTORY_SLOTS: u8 = 8;
    /// The flag bit of an attribute set that marks it initialised, for the
    /// vitals reads of the API and the loader.
    pub const ATTRIBUTE_SET_INITIALISED: u8 = 1;

    /// Each experience source with its catalogue id, for the loader's
    /// experience rates.
    pub const EXPERIENCE_SOURCES: &[(crate::game_events::ExperienceSourceType, &str)] =
        &crate::game_events::ExperienceSourceType::IDS;

    /// The schema text of `declare_mod!`: empty when it names no point, so a
    /// mod without points carries nothing.
    #[must_use]
    pub const fn point_schemas(text: &'static str) -> &'static str {
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'[' {
                return text;
            }
            i += 1;
        }
        ""
    }

    /// The bytes of `text`, as a static in a section of its own holds them.
    ///
    /// # Panics
    ///
    /// When `N` is not the length of `text`, at compile time.
    #[must_use]
    pub const fn bytes<const N: usize>(text: &str) -> [u8; N] {
        let source = text.as_bytes();
        assert!(source.len() == N, "the section's size is the text's length");
        let mut out = [0u8; N];
        let mut i = 0;
        while i < N {
            out[i] = source[i];
            i += 1;
        }
        out
    }

    /// The first non-empty text of `texts`, or empty.
    #[must_use]
    pub const fn first_text(texts: &[&'static str]) -> &'static str {
        let mut i = 0;
        while i < texts.len() {
            if !texts[i].is_empty() {
                return texts[i];
            }
            i += 1;
        }
        ""
    }

    /// Cargo's `authors`, separated by `:`, trimmed and without empty names.
    const fn author_names<const N: usize>(cargo: &'static str) -> ([crate::abi::Str; N], usize) {
        let mut out = [crate::abi::Str::EMPTY; N];
        let mut count = 0;
        let mut rest = cargo.as_bytes();
        while !rest.is_empty() {
            let mut cut = 0;
            while cut < rest.len() && rest[cut] != b':' {
                cut += 1;
            }
            let (name, tail) = rest.split_at(cut);
            // Cut at an ASCII byte and trimmed of ASCII, so still UTF-8.
            if let Ok(name) = core::str::from_utf8(name.trim_ascii())
                && !name.is_empty()
            {
                if count < N {
                    out[count] = crate::abi::Str::new(name);
                }
                count += 1;
            }
            rest = match tail.split_first() {
                Some((_, after)) => after,
                None => tail,
            };
        }
        (out, count)
    }

    /// How many authors [`authors`] finds.
    #[must_use]
    pub const fn author_count(cargo: &'static str) -> usize {
        author_names::<0>(cargo).1
    }

    /// The authors' names for the manifest.
    #[must_use]
    pub const fn authors<const N: usize>(cargo: &'static str) -> [crate::abi::Str; N] {
        author_names::<N>(cargo).0
    }

    /// Compiles only for a `Copy` hook argument.
    pub const fn hook_argument<T: Copy>() {}

    /// Compiles only for a `Copy` hook return value.
    pub const fn hook_return<T: Copy>() {}

    #[must_use]
    pub const fn or_default<T: Copy>(given: &[T], default: T) -> T {
        match given {
            [value] => *value,
            _ => default,
        }
    }

    pub fn fault_in(hook: &str, payload: &(dyn core::any::Any + Send)) {
        crate::host::fault_with(&format!("panic in hook {hook}"), payload);
    }

    /// The most detailed level the loader keeps, read once at init.
    static MAX_LEVEL: AtomicU8 = AtomicU8::new(Level::INFO.0);

    #[inline]
    #[must_use]
    pub fn enabled(level: Level) -> bool {
        level.0 <= MAX_LEVEL.load(Ordering::Relaxed)
    }

    unsafe extern "system" fn refresh_level(
        _event: crate::abi::Event,
        _data: *const core::ffi::c_void,
        _user: *mut core::ffi::c_void,
    ) {
        if let Some(host) = crate::host() {
            MAX_LEVEL.store(host.log_level().0, Ordering::Relaxed);
        }
    }

    /// A call site that logs its first time in each mod life: holds the
    /// life it logged in.
    #[derive(Debug)]
    pub struct Once(AtomicU64);

    impl Once {
        #[must_use]
        pub const fn new() -> Self {
            Self(AtomicU64::new(0))
        }

        #[inline]
        #[must_use]
        pub fn first(&self) -> bool {
            let life = crate::fresh::life();
            self.0.load(Ordering::Relaxed) != life && self.0.swap(life, Ordering::Relaxed) != life
        }
    }

    impl Default for Once {
        fn default() -> Self {
            Self::new()
        }
    }

    /// A call site that logs at most once per period, counting what it skips.
    #[derive(Debug)]
    pub struct Every {
        /// The mod life the counts belong to.
        life: AtomicU64,
        next: AtomicU64,
        skipped: AtomicU64,
    }

    impl Every {
        #[must_use]
        pub const fn new() -> Self {
            Self {
                life: AtomicU64::new(0),
                next: AtomicU64::new(0),
                skipped: AtomicU64::new(0),
            }
        }

        /// The lines skipped since the last one when a line is due now.
        #[must_use]
        pub fn due(&self, period: Duration) -> Option<u64> {
            static EPOCH: OnceLock<Instant> = OnceLock::new();
            let now = u64::try_from(EPOCH.get_or_init(Instant::now).elapsed().as_millis())
                .unwrap_or(u64::MAX);
            let life = crate::fresh::life();
            if self.life.swap(life, Ordering::Relaxed) != life {
                self.next.store(0, Ordering::Relaxed);
                self.skipped.store(0, Ordering::Relaxed);
            }
            let next = self.next.load(Ordering::Relaxed);
            let period = u64::try_from(period.as_millis()).unwrap_or(u64::MAX);
            if now < next
                || self
                    .next
                    .compare_exchange(
                        next,
                        now.saturating_add(period),
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    )
                    .is_err()
            {
                self.skipped.fetch_add(1, Ordering::Relaxed);
                return None;
            }
            Some(self.skipped.swap(0, Ordering::Relaxed))
        }
    }

    impl Default for Every {
        fn default() -> Self {
            Self::new()
        }
    }

    pub fn log(level: Level, message: core::fmt::Arguments<'_>) {
        if !enabled(level) {
            return;
        }
        if let Some(host) = crate::host() {
            match message.as_str() {
                Some(text) => host.log(level, text),
                None => host.log(level, &message.to_string()),
            }
        }
    }

    /// Where `declare_mod!` keeps the mod's state once `init` made it, for
    /// the process in the game and for one install under the fake host.
    #[derive(Debug)]
    pub struct Slot<M>(crate::fresh::Fresh<M>);

    impl<M> Slot<M> {
        #[must_use]
        pub const fn new() -> Self {
            Self(crate::fresh::Fresh::new())
        }

        #[must_use]
        pub fn get(&'static self) -> Option<&'static M> {
            self.0.get()
        }
    }

    impl<M> Default for Slot<M> {
        fn default() -> Self {
            Self::new()
        }
    }

    /// # Safety
    ///
    /// - `api` is null, or points at an aligned [`HostApi`] readable for the size its
    ///   header states.
    /// - That table stays valid and unchanged for the rest of the process,
    ///   as the loader's process-wide table does.
    pub unsafe fn init<M: Mod>(
        api: *const HostApi,
        me: ModHandle,
        manifest: &'static crate::abi::Manifest,
        slot: &Slot<M>,
    ) -> Status {
        // SAFETY: `Host::new` needs `api` null or a table readable for its
        // header's size that outlives the process; this function's `# Safety`
        // bullets guarantee both.
        let Some(host) = (unsafe { Host::new(api, me) }) else {
            return Status::BAD_ARGS;
        };
        let host = host.install();
        crate::host::log_panics();
        MAX_LEVEL.store(host.log_level().0, Ordering::Relaxed);
        let _watched = host.subscribe(
            crate::abi::Event::LOADER_SETTINGS_CHANGED,
            refresh_level,
            core::ptr::null_mut(),
        );
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            #[cfg(feature = "test")]
            crate::test::table::note_manifest(api, manifest);
            M::init(&mut Load::of(
                host,
                manifest.flags.contains(ManifestFlags::JOIN),
            ))
        }));
        match outcome {
            Ok(Ok(state)) => match slot.0.set(state) {
                Ok(()) => Status::OK,
                Err(state) => {
                    let _dropped =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(state)));
                    Status::REFUSED
                }
            },
            Ok(Err(off)) => {
                host.fault(off.to_reason());
                Status::REFUSED
            }
            Err(payload) => {
                let message = crate::host::panic_text(payload.as_ref());
                host.fault(&format!("init panicked: {message}"));
                Status::FAULTED
            }
        }
    }

    /// Runs `init` with a [`Load`] for the data pack `me`, as the loader
    /// does for a pack folder; the process's host stays as it is.
    ///
    /// # Safety
    ///
    /// - `api` is null, or points at an aligned [`HostApi`] readable for the size its
    ///   header states.
    /// - That table stays valid and unchanged for the rest of the process,
    ///   as the loader's process-wide table does.
    pub unsafe fn init_pack(
        api: *const HostApi,
        me: ModHandle,
        init: impl FnOnce(&mut Load<'_>) -> Result<(), crate::Off>,
    ) -> Status {
        // SAFETY: `Host::new` needs `api` null or a table readable for its
        // header's size that outlives the process; this function's `# Safety`
        // bullets guarantee both.
        let Some(host) = (unsafe { Host::new(api, me) }) else {
            return Status::BAD_ARGS;
        };
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| init(&mut Load::new(host))));
        match outcome {
            Ok(Ok(())) => Status::OK,
            Ok(Err(off)) => {
                host.fault(off.to_reason());
                Status::REFUSED
            }
            Err(payload) => {
                let message = crate::host::panic_text(payload.as_ref());
                host.fault(&format!("init panicked: {message}"));
                Status::FAULTED
            }
        }
    }

    /// Makes the table `api` with the handle `me` this process's host, for
    /// the loader's own copy of the API, which runs data packs.
    ///
    /// # Safety
    ///
    /// - `api` is null, or points at an aligned [`HostApi`] readable for the size its
    ///   header states.
    /// - That table stays valid and unchanged for the rest of the process,
    ///   as the loader's process-wide table does.
    pub unsafe fn install_host(api: *const HostApi, me: ModHandle) {
        // SAFETY: `Host::new` needs `api` null or a table readable for its
        // header's size that outlives the process; this function's `# Safety`
        // bullets guarantee both.
        if let Some(host) = unsafe { Host::new(api, me) } {
            MAX_LEVEL.store(host.install().log_level().0, Ordering::Relaxed);
        }
    }

    pub fn shutdown<M: Mod>(slot: &'static Slot<M>) {
        let Some(state) = slot.get() else {
            return;
        };
        if let Err(payload) =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| state.shutdown()))
        {
            crate::host::fault_with("panic in shutdown", payload.as_ref());
        }
    }

    pub fn ready<M: Mod>(slot: &'static Slot<M>) {
        let (Some(state), Some(host)) = (slot.get(), crate::host()) else {
            return;
        };
        if let Err(payload) =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| state.ready(host)))
        {
            crate::host::fault_with("panic in ready", payload.as_ref());
        }
    }
}
