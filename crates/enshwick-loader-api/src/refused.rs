//! Code the macros refuse at build time, each case a `compile_fail`
//! doctest. Only `cargo test --doc` reads this module.

/// A field names `range` twice:
///
/// ```compile_fail
/// /// Settings.
/// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
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
/// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
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
/// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
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
/// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
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
/// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
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
/// #[enshwick_loader_api::hook]
/// fn moved(next: enshwick_loader_api::Next<extern "system" fn(Vec3)>, to: Vec3) {
///     next.get()(to)
/// }
/// ```
///
/// An `unsafe fn` hook body:
///
/// ```compile_fail
/// #[enshwick_loader_api::hook]
/// unsafe fn count(next: enshwick_loader_api::Next<extern "system" fn(u32) -> u32>, n: u32) -> u32 {
///     next.get()(n)
/// }
/// ```
///
/// Linking a hook outside `unsafe`, since the loader cannot check the
/// target's type:
///
/// ```compile_fail
/// #[enshwick_loader_api::hook]
/// fn count(next: enshwick_loader_api::Next<extern "system" fn(u32) -> u32>, n: u32) -> u32 {
///     next.get()(n)
/// }
///
/// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
///     load.hook(&COUNT, load.symbol("count")?)
/// }
/// ```
///
/// Keeping the game's code past `init`, when arming writes into it:
///
/// ```compile_fail
/// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<&'static [u8], enshwick_loader_api::Off> {
///     load.code()
/// }
/// ```
#[expect(dead_code, reason = "the doc comment carries the cases")]
struct Refused;
