//! The points of the Flame altar count mod (`flame-altar-count`).

/// How many Flame Altars a Flame level allows, `flame-altar-count:limit`:
/// the answer is the altars.
///
/// The mod answers from its table and keeps the result between 1 and
/// `cap`. Asked whenever the game checks the limit, on the thread that
/// checks it.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::points::flame_altar_count::AltarLimit;
/// use enshwick_loader_api::points::{Order, Verdict};
///
/// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
///     // Two more altars at every level.
///     load.extend_point(&Order::new(), |_: &AltarLimit, altars: &u8| {
///         Verdict::Set(altars.saturating_add(2))
///     })
/// }
/// ```
#[crate::point(name = "flame-altar-count:limit", mode = Override, output = u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AltarLimit {
    /// The Flame level, from 0.
    pub flame_level: u32,
    /// The most altars the game's grouping takes on this build.
    pub cap: u8,
}
