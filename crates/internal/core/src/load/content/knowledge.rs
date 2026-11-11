//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::content::knowledge::{ExperienceValue, NewExperience, NewKnowledge};
use crate::knowledge::KnowledgeId;
use crate::{Load, Off};
use enshwick_content::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(content_knowledge)]
impl Load<'_> {
    /// Adds a knowledge entry beside the game's own, on the server and the
    /// client alike, and returns its knowledge id. The game grants, sends
    /// and saves it like a vanilla entry; a player's entry gives its skill
    /// points and experience when granted.
    ///
    /// A save keeps the values of the mod's entries while the mod runs; a
    /// save made without the mod drops them, so they read 0 when the mod
    /// comes back (server 399, read on a server copy).
    ///
    /// # Errors
    ///
    /// [`Off`] for flags without bits or bits on another kind, skill points
    /// or experience on a world's entry, an id the game already knows, or
    /// an id the loader refuses (see [`Load::declare`]).
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::id;
    /// use enshwick::content::knowledge::NewKnowledge;
    /// use enshwick::knowledge::{KnowledgeType, Scope};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let kills = load.add_knowledge(NewKnowledge::new(id!("wolves-slain"), Scope::World, KnowledgeType::Counter))?;
    ///     enshwick::info!("wolves slain: knowledge {kills}");
    ///     Ok(())
    /// }
    /// ```
    #[script]
    pub fn add_knowledge(&mut self, new: NewKnowledge) -> Result<KnowledgeId, Off>;

    /// Adds an experience value beside the game's own and returns it, for
    /// a knowledge entry's [`NewKnowledge::experience`] or a quest's
    /// experience reward. The game counts the experience under its source,
    /// as it does a vanilla value's;
    /// [`experience_values`](crate::content::vanilla::experience_values) lists those.
    /// Adding the same id again replaces its source and gain. One patch of
    /// the balancing table serves all of a mod's values; it adds none when
    /// another mod's value there has the id of one of them.
    ///
    /// # Errors
    ///
    /// [`Off`] for a source the build lacks (also a number its source enum
    /// does not hold), an experience value the game already holds, an id
    /// the loader refuses (see [`Load::declare`]), and on the first call a
    /// balancing table that is not reflected, not one resource or does not
    /// read.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::id;
    /// use enshwick::content::knowledge::NewExperience;
    /// use enshwick::game_events::ExperienceSourceType;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let reward = load.add_experience(NewExperience::new(id!("trail-xp"), ExperienceSourceType::Quest, 1200))?;
    ///     enshwick::info!("trail reward: experience value {}", reward.get());
    ///     Ok(())
    /// }
    /// ```
    #[script]
    pub fn add_experience(&mut self, new: NewExperience) -> Result<ExperienceValue, Off>;
}
