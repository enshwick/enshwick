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
//! | The mod and the loader | [`Load`], [`Host`], [`abi`], [`mods`], [`config`](mod@config), [`i18n`], [`status`], [`state`], [`files`], [`migrations`], [`Relock`] |
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
//! #[enshwick::config]
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
/// #[enshwick::event(name = "sold")]
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
/// `#[call(name = "<call>", output = <type>, version = <major>)]`, with
/// `of = <alias>` for a dependency's call ([`deps!`]); `version` is 1 when
/// left out. A changed field or answer is a new major. Every type in it has
/// a [`trait@Shape`].
///
/// # Examples
///
/// ```
/// #[enshwick::call(name = "price-of", output = u32)]
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
/// Declares a mod's settings struct: serde's `Deserialize` and `Serialize`,
/// unknown keys refused and [`trait@config::Config`], so the mod needs no
/// serde of its own.
///
/// `#[enshwick::config(default)]` fills the keys a file leaves out from
/// `Default`. `validate = path` and `text = "key"` are the struct
/// attributes of [`trait@config::Config`]; field attributes stay
/// `#[config(..)]` and `#[serde(..)]`.
///
/// # Examples
///
/// ```
/// use enshwick::config::Config;
///
/// /// Locker settings.
/// #[enshwick::config(default)]
/// #[derive(Default)]
/// struct Settings {
///     /// How many lockers one player may own.
///     #[config(startup, range = 1..=16)]
///     limit: u32,
/// }
///
/// assert!(Settings::template().contains("limit = 0"));
/// ```
///
/// A misspelt key is refused with its name, not read past:
///
/// ```
/// /// Locker settings.
/// #[enshwick::config]
/// #[derive(Debug, Default)]
/// struct Settings {
///     /// How many lockers one player may own.
///     limit: u32,
/// }
///
/// let error = toml::from_str::<Settings>("limits = 4").map_err(|e| e.to_string());
/// assert!(error.is_err_and(|e| e.contains("unknown field `limits`")));
/// ```
pub use enshwick_macros::config;
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
/// The mod's own [extension points](crate::points), [mod
/// events](crate::services) and calls, each a [`#[point]`](macro@point),
/// [`#[event]`](macro@event) or `#[call]` declared without `of`, whose
/// local name `exports` lists, put their schemas into the DLL's `.enshpt`
/// section; the others stay the mod's own. The manifest collects the
/// schemas from there at the loader's first query, and `cargo enshwick
/// deps` reads them from the DLL file.
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
/// trading_post = "you.trading-post"
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
/// // SAFETY: `enshwick_mod_query` returns the manifest of
/// // `declare_mod!`, valid until its shutdown.
/// let manifest = unsafe { &*enshwick_mod_query() };
/// // SAFETY: `declare_mod!` points the id at a string literal, alive for the process.
/// assert_eq!(unsafe { manifest.id.as_str() }, Some("you.hello"));
/// assert_eq!(manifest.dependency_count, 3);
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
/// content id `<dependency>:<name>`, once the loader lets this mod name it:
/// the dependency's `exports` list `<name>` or the name it is a part of. A
/// required dependency's `id` returns [`Off`] for a name it does not export.
/// An optional one's, from `after` or `recommends`, is `None` while it does
/// not run or does not export the name. Ask once, at init, and keep the id.
/// A malformed name panics, running or not. A call,
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
/// recommends `lanterns` and `trading_post`:
///
/// ```rust
/// use enshwick::content::Id;
/// use enshwick::services::ModEvent;
///
/// enshwick::deps!();
///
/// let locker: Id = deps::lockers::id("locker")?;
/// assert_eq!(locker.to_string(), "you.lockers:locker");
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
/// # Ok::<(), enshwick::Off>(())
/// ```
pub use enshwick_macros::deps;
/// Declares a mod event other mods subscribe to: the struct becomes
/// `repr(C)` and implements [`services::ModEvent`] with a layout hash per
/// field.
///
/// `#[event(name = "<what>")]` for this mod's event, `#[event(of = <alias>,
/// name = "<what>")]` for a dependency's ([`deps!`]); the struct is `Copy`
/// plain data and grows by appending fields, as a
/// [`#[service]`](macro@service) table does.
///
/// # Examples
///
/// The crate's `Cargo.toml` recommends `trading_post`, which sends `sold`:
///
/// ```
/// #[enshwick::event(of = trading_post, name = "sold")]
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
/// `#[point(name = "<point>", mode = Observe | Advise | Override, output =
/// <type>, refusable)]`, with `of = <alias>` for a dependency's point
/// ([`deps!`]); `refusable` lets a link refuse, which stops the chain. The
/// input grows by appending fields; a changed field or answer type needs a
/// new name. Every type in it has a [`trait@Shape`], so
/// a change deep inside a field refuses the link too.
///
/// # Examples
///
/// The crate's `Cargo.toml` recommends `trading_post`, which owns `price`:
///
/// ```
/// use enshwick::points::{Order, Verdict};
///
/// #[enshwick::point(of = trading_post, name = "price", mode = Override, output = u32, refusable)]
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
/// `#[service(name = "<what>", version = <major>)]`, with `of = <alias>`
/// for a dependency's service ([`deps!`]); the version is 1 when left out.
/// Fields are `extern "system" fn` pointers or plain data. Append fields to grow the table: users built against the
/// shorter table keep working. Change a published field only with a new
/// major. Every type in it has a [`trait@Shape`], so a change deep inside a
/// field refuses the user too.
///
/// # Examples
///
/// ```
/// #[enshwick::service(name = "prices", version = 1)]
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
/// Names the keys of the mod's texts: `texts!()` reads the crate's
/// `src/texts.toml` when it builds and writes `pub mod texts`, a constant
/// per key.
///
/// A key's constant is the key in upper case with `.` and `-` as `_`, such
/// as `texts::SAFE_NAME` for `"safe.name"`; its doc shows its text in each
/// language and the values it fills in. `texts::value` holds a constant per
/// value name, such as `value::OWNER` for `{owner}`; `texts::ALL` lists
/// every key; `texts::TOML` is the file. The build fails when a text is
/// empty; when a key lacks a text in a language of
/// [`Language::REQUIRED`](i18n::Language::REQUIRED), and names those
/// languages; when a table is no language code of [`i18n::Language`]; or
/// when two texts of a key fill in different values.
/// [`declare_mod!`] embeds the same file, which [`Load::texts`] serves.
///
/// `texts!("<path>")` reads another file from the crate's root, for tests.
///
/// # Examples
///
/// The file has `"locker.name" = "Locker"` and
/// `"locker.label" = "Locker · {owner}"`, in English and Ukrainian:
///
/// ```rust
/// use enshwick::i18n::{Language, Texts};
///
/// // In a mod: enshwick::texts!();
/// enshwick::texts!("tests/texts/lockers.toml");
///
/// let all = Texts::parse(texts::TOML)?;
/// assert_eq!(all.get(texts::LOCKER_NAME, Language::UK_UA), Some("Шафка"));
/// let label = all.fill(texts::LOCKER_LABEL, Language::FALLBACK, &[(texts::value::OWNER, "Ada")]);
/// assert_eq!(label, "Locker · Ada");
/// assert_eq!(texts::ALL, ["locker.label", "locker.name"]);
/// # Ok::<(), enshwick::Off>(())
/// ```
pub use enshwick_macros::texts;
#[cfg(not(feature = "unstable"))]
pub(crate) use enshwick_macros::{Script, script};
pub use hook::{FnPtr, Hook, Next};
pub use host::{Host, host};
pub use load::{Load, Mod, Off};
pub use locks::{Relock, RelockRw, RelockWait};
pub use shape::Shape;
pub use world_error::WorldError;

#[doc(hidden)]
pub mod __private;
#[doc(hidden)]
pub mod schema_section;
