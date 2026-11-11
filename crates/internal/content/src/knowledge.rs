//! The kinds of knowledge value that content names, below the knowledge
//! domain, which re-exports them.

#[doc(no_inline)]
pub use crate::abi::KnowledgeId;

/// What a knowledge value is.
///
/// # Examples
///
/// See [`Known`](../knowledge/struct.Known.html).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum KnowledgeType {
    /// Set or not.
    Bool,
    /// A count.
    Counter,
    /// Bits.
    Flags,
    /// A kind this API does not name.
    Other(u8),
}

/// Whose a knowledge value is.
///
/// The name is enshwick's, not the game's.
///
/// # Examples
///
/// See [`Known`](../knowledge/struct.Known.html).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum Scope {
    /// The world's, shared by every player.
    World,
    /// Each player's own.
    Player,
}
