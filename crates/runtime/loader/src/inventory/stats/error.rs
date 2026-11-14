//! Why a stat or affix call was refused.

/// Why a stat or affix call was refused.
#[derive(Debug, thiserror::Error)]
pub(crate) enum StatsError {
    /// A definition after init.
    #[error("stats and affixes are defined during init only")]
    TooLate,
    /// A mod index the loader does not hold.
    #[error("no such mod")]
    NoMod,
    /// An id that is not `<mod>:<name>`.
    #[error("the id is not `<mod>:<name>`")]
    BadId,
    /// A definition outside the mod's own namespace.
    #[error("the id is outside the mod's namespace")]
    NotOwn,
    /// An id whose key another holds.
    #[error(transparent)]
    Taken(#[from] crate::base::ids::DeclareIdError),
    /// A kind or affix defined already.
    #[error("defined already")]
    Defined,
    /// More kinds or affixes than an index holds.
    #[error("too many stats or affixes")]
    Full,
    /// A modifier op or affix definition that does not read.
    #[error("the definition is malformed")]
    BadDef,
    /// Stats do not run: init has not ended, or no mod uses them.
    #[error("stats do not run")]
    NotRunning,
    /// A stat index past the kinds.
    #[error("no such stat")]
    BadStat,
    /// A kind opened but not defined.
    #[error("the stat is not defined")]
    Undefined,
    /// An affix index past the affixes.
    #[error("no such affix")]
    NoAffix,
    /// Entity 0, an unknown op or a duration past the clock.
    #[error("the modifier is malformed")]
    BadModifier,
    /// Before stats run, or entity 0, there is nothing to watch.
    #[error("nothing to watch")]
    NothingToWatch,
    /// This process does not run the server's world.
    #[error("not the server")]
    NotServer,
    /// A saved modifier on an entity that is no player.
    #[error("only a player's modifiers are saved")]
    NotPlayer,
    /// The server's world, unreadable now.
    #[error(transparent)]
    World(#[from] crate::engine::world::WorldError),
}
