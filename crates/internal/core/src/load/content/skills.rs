//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::content::Guid;
use crate::content::skills::*;
use crate::resources::{Registry, Value};
use crate::typed::ecs::UnlockedSkillNodes;
use crate::typed::{
    Named as _, PerkCollectionResource, Reflected as _, SkillNodeType, SkillTreeResource,
};
use crate::{Load, Off};
use enshwick_content::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(content_skills)]
impl Load<'_> {
    /// The vanilla nodes of the skill tree, to link a mod's nodes from and
    /// place them beside.
    ///
    /// # Errors
    ///
    /// [`Off`] when the tree does not read on this build.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[script]
    pub fn skill_nodes(&self) -> Result<Vec<SkillNode>, Off>;

    /// Adds a node to the skill tree, on the server and the client alike: a
    /// copy of the vanilla node [`NewSkillNode::like`], its effect and
    /// icons among it, at its own place, linked from the nodes it names. A
    /// player unlocks it with skill points as the game's own nodes; the
    /// tree screen draws it from the data.
    ///
    /// A player's save keeps a fixed count of unlocked nodes (256 on server
    /// 399, beside the game's 243), and the game's own tree takes most of
    /// them: every mod's nodes share the rest, in load order. Each node
    /// shows as the mod's feature `skill node <id>` in the status summary
    /// and on the Mods screen, with the reason when it is refused.
    ///
    /// # Errors
    ///
    /// [`Off`] when the tree holds as many nodes as a save keeps, `like` is
    /// not in the game's tree, the node would have levels, the place is not
    /// finite or lies within half the vanilla tree's smallest gap of
    /// another node, a node is linked from twice, or the loader refuses the
    /// id (see [`Load::declare`]). A node beyond the room after other mods'
    /// nodes, or linked from a node no mod adds, is refused when the game
    /// first reads the tree, and left out.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[script]
    pub fn add_skill_node(&mut self, new: NewSkillNode) -> Result<SkillNodeId, Off>;

    /// Adds a perk, listed beside the vanilla ones: a copy of the vanilla
    /// perk [`NewPerk::like`] under the mod's content id, with its own name
    /// and description, on the server and the client alike.
    ///
    /// # Errors
    ///
    /// [`Off`] when `like` is no vanilla perk or the loader refuses the id
    /// (see [`Load::declare`]).
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::{Guid, id};
    /// use enshwick::content::skills::NewPerk;
    ///
    /// fn init(load: &mut enshwick::Load<'_>, leech: Guid) -> Result<(), enshwick::Off> {
    ///     let perk = load.add_perk(NewPerk {
    ///         id: id!("thirst"),
    ///         like: leech,
    ///         name: id!("thirst.name").guid(),
    ///         description: id!("thirst.text").guid(),
    ///     })?;
    ///     enshwick::info!("perk {perk}");
    ///     Ok(())
    /// }
    /// ```
    #[script]
    pub fn add_perk(&mut self, new: NewPerk) -> Result<Guid, Off>;
}
