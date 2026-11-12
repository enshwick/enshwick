//! The script value model and the events a script subscribes to, which
//! `enshwick-core`'s `script` joins with how `Load` opens each part.

mod events;

#[doc(inline)]
pub use enshwick_values::script::*;
pub use events::{EventKind, GameEventKind, events, game_events};

/// A part of the API a script reaches as one namespace, such as `world`.
///
/// Each domain implements it beside the type it describes.
///
/// # Examples
///
/// See [`Function`].
pub trait Bound: Copy + Send + Sync + 'static {
    /// The namespace's name in a script.
    const NAME: &'static str;
    /// What the namespace is for, one sentence.
    const DOC: &'static str;
    /// The method lists of every [`#[script]`](macro@crate::script) block
    /// of the type.
    const PARTS: &'static [&'static [Function<Self>]];

    /// The value a script's calls go to, for the mod `load` initialises.
    ///
    /// # Errors
    ///
    /// [`Off`](crate::Off) when the game build lacks what the part needs;
    /// the script runs without that namespace.
    fn resolve(load: &mut impl crate::Loading) -> Result<Self, crate::Off>;

    /// Every method a script may call.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[cfg(any())] fn in_enshwick() { // names items above this crate: enshwick's doctests compile it
    /// use enshwick::script::Bound;
    ///
    /// assert!(enshwick::world::World::functions().any(|f| f.name() == "broadcast"));
    /// # }
    /// ```
    fn functions() -> impl Iterator<Item = &'static Function<Self>> {
        Self::PARTS.iter().flat_map(|part| part.iter())
    }
}
