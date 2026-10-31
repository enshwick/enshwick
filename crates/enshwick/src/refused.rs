//! Code the macros refuse at build time, each case a `compile_fail`
//! doctest. Only `cargo test --doc` reads this module.

/// A field names `range` twice:
///
/// ```compile_fail
/// /// Settings.
/// #[enshwick::config]
/// #[derive(Default)]
/// struct Settings {
///     /// Slots.
///     #[config(range = 1..=4, range = 100..=200)]
///     slots: u32,
/// }
/// ```
///
/// A range without either end:
///
/// ```compile_fail
/// /// Settings.
/// #[enshwick::config]
/// #[derive(Default)]
/// struct Settings {
///     /// Slots.
///     #[config(range = ..)]
///     slots: u32,
/// }
/// ```
///
/// A range that starts above its end:
///
/// ```compile_fail
/// /// Settings.
/// #[enshwick::config]
/// #[derive(Default)]
/// struct Settings {
///     /// Slots.
///     #[config(range = 10..=1)]
///     slots: u32,
/// }
/// ```
///
/// An empty text key:
///
/// ```compile_fail
/// /// Settings.
/// #[enshwick::config]
/// #[derive(Default)]
/// #[config(text = "")]
/// struct Settings {
///     /// Slots.
///     slots: u32,
/// }
/// ```
///
/// A field serde reads through another type than its own:
///
/// ```compile_fail
/// mod seconds {
///     pub fn serialize<S: serde::Serializer>(d: &std::time::Duration, s: S) -> Result<S::Ok, S::Error> {
///         s.serialize_u64(d.as_secs())
///     }
///     pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<std::time::Duration, D::Error> {
///         <u64 as serde::Deserialize>::deserialize(d).map(std::time::Duration::from_secs)
///     }
/// }
///
/// /// Settings.
/// #[enshwick::config]
/// #[derive(Default)]
/// struct Settings {
///     /// How long.
///     #[serde(with = "seconds")]
///     timeout: std::time::Duration,
/// }
/// ```
///
/// A hook argument that is not `Copy`:
///
/// ```compile_fail
/// #[repr(C)]
/// pub struct Vec3 {
///     x: f32,
///     y: f32,
///     z: f32,
/// }
///
/// #[enshwick::hook]
/// fn moved(next: enshwick::Next<'_, extern "system" fn(Vec3)>, to: Vec3) {
///     next.call(to)
/// }
/// ```
///
/// An `unsafe fn` hook body:
///
/// ```compile_fail
/// #[enshwick::hook]
/// unsafe fn count(next: enshwick::Next<'_, extern "system" fn(u32) -> u32>, n: u32) -> u32 {
///     next.call(n)
/// }
/// ```
///
/// Linking a hook outside `unsafe`, since the loader cannot check the
/// target's type:
///
/// ```compile_fail
/// #[enshwick::hook]
/// fn count(next: enshwick::Next<'_, extern "system" fn(u32) -> u32>, n: u32) -> u32 {
///     next.call(n)
/// }
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.hook(&COUNT, load.symbol("count")?)
/// }
/// ```
///
/// Keeping the game's code past `init`, when arming writes into it:
///
/// ```compile_fail
/// fn init(load: &mut enshwick::Load<'_>) -> Result<&'static [u8], enshwick::Off> {
///     load.code()
/// }
/// ```
///
/// A point's input with padding, which a described link would read
/// uninitialised:
///
/// ```compile_fail
/// #[enshwick::point(name = "price", mode = Override, output = u32)]
/// #[derive(Clone, Copy, Debug)]
/// pub struct Price {
///     pub item: u32,
///     pub player: u64,
/// }
/// ```
///
/// A point's answer with padding:
///
/// ```compile_fail
/// /// An offer.
/// #[repr(C)]
/// #[derive(Clone, Copy, Debug, enshwick::Shape)]
/// pub struct Offer {
///     pub cheap: bool,
///     pub coins: u32,
/// }
///
/// #[enshwick::point(name = "best", mode = Override, output = Offer)]
/// #[derive(Clone, Copy, Debug)]
/// pub struct Best {
///     pub item: u32,
/// }
/// ```
///
/// A mod event's data with padding:
///
/// ```compile_fail
/// #[enshwick::event(name = "sold")]
/// #[derive(Clone, Copy, Debug)]
/// pub struct Sold {
///     pub item: u32,
///     pub player: u64,
/// }
/// ```
///
/// A call's input with padding:
///
/// ```compile_fail
/// #[enshwick::call(name = "rent", output = u64)]
/// #[derive(Clone, Copy, Debug)]
/// pub struct Rent {
///     pub item: u32,
///     pub player: u64,
/// }
/// ```
///
/// A content name with the mod's id, which `id!` puts in front itself:
///
/// ```compile_fail
/// const LOCKER: enshwick::content::Id = enshwick::content::id!("you.hello:locker");
/// ```
///
/// A content name in upper case:
///
/// ```compile_fail
/// const LOCKER: enshwick::content::Id = enshwick::content::id!("Locker");
/// ```
///
/// A derived part in upper case:
///
/// ```compile_fail
/// const NAME: enshwick::content::Id = enshwick::content::id!("locker").part("Name");
/// ```
///
/// An alias the crate's `Cargo.toml` does not name:
///
/// ```compile_fail
/// #[enshwick::event(of = stalls, name = "sold")]
/// #[derive(Clone, Copy, Debug)]
/// pub struct Sold {
///     pub item: u32,
/// }
/// ```
///
/// A full name with `of`, which puts the dependency's id in front itself:
///
/// ```compile_fail
/// #[enshwick::event(of = lockers, name = "you.lockers:opened")]
/// #[derive(Clone, Copy, Debug)]
/// pub struct Opened {
///     pub locker: u32,
/// }
/// ```
///
/// A point listed in `declare_mod!` that `exports` in the crate's
/// `Cargo.toml` leaves out:
///
/// ```compile_fail
/// #[enshwick::point(name = "hidden", mode = Override, output = u32)]
/// #[derive(Clone, Copy, Debug)]
/// pub struct Hidden {
///     pub item: u32,
/// }
///
/// struct Hello;
///
/// impl enshwick::Mod for Hello {
///     fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
///         Ok(Self)
///     }
/// }
///
/// enshwick::declare_mod!(Hello, points: [Hidden]);
/// ```
///
/// Another mod's point listed in `declare_mod!`, which lists only the mod's
/// own:
///
/// ```compile_fail
/// #[enshwick::point(of = lockers, name = "price", mode = Override, output = u32)]
/// #[derive(Clone, Copy, Debug)]
/// pub struct Price {
///     pub item: u32,
/// }
///
/// struct Hello;
///
/// impl enshwick::Mod for Hello {
///     fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
///         Ok(Self)
///     }
/// }
///
/// enshwick::declare_mod!(Hello, points: [Price]);
/// ```
///
/// A key with a Ukrainian text and no English one:
///
/// ```compile_fail
/// enshwick::texts!("tests/texts/no-english.toml");
/// ```
///
/// A table that is no language code (`ua` for `uk`):
///
/// ```compile_fail
/// enshwick::texts!("tests/texts/unknown-language.toml");
/// ```
///
/// A translation that fills in `{name}` where its English text fills in
/// `{owner}`:
///
/// ```compile_fail
/// enshwick::texts!("tests/texts/other-values.toml");
/// ```
///
/// A texts path for `declare_mod!`, which reads `src/texts.toml` only:
///
/// ```compile_fail
/// struct Hello;
///
/// impl enshwick::Mod for Hello {
///     fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
///         Ok(Self)
///     }
/// }
///
/// enshwick::declare_mod!(Hello, texts = "tests/texts/lockers.toml");
/// ```
#[expect(dead_code, reason = "the doc comment carries the cases")]
struct Refused;
