//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::knowledge::{NewRule, RuleChange, RuleId, RuleInfo};
use crate::{Load, Off};
use enshwick_knowledge::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(content_rules)]
impl Load<'_> {
    /// Adds a knowledge rule to the game's own trigger list: while `when`
    /// holds, the game's server grants the rule's knowledge, with no mod
    /// code running then. A player's rule runs for each player, a world's
    /// for the world, every 32 steps of the world; a grant the knowledge
    /// holds already is skipped (server 26430253, read in the code). The
    /// list is the server's alone, so the rule needs the mod on the server
    /// only. The game grants only an entry its knowledge holds, the game's
    /// or a mod's.
    ///
    /// # Errors
    ///
    /// [`Off`] for a rule that grants nothing, adds to a counter or sets a
    /// flag past bit 31, a count query from 0, an id the loader refuses (see
    /// [`Load::declare`]), and a trigger list that is not reflected, not one
    /// resource or does not read, as on a client.
    ///
    /// # Examples
    ///
    /// See [`NewRule`].
    #[script]
    pub fn add_knowledge_rule(&mut self, new: NewRule) -> Result<(), Off>;

    /// The game's own knowledge rules, the world's and the players', as its
    /// trigger list holds them.
    ///
    /// # Errors
    ///
    /// [`Off`] for a trigger list that is not reflected, not one resource or
    /// does not read, as on a client.
    ///
    /// # Examples
    ///
    /// See [`RuleInfo`].
    #[script]
    pub fn knowledge_rules(&mut self) -> Result<Vec<RuleInfo>, Off>;

    /// Changes the vanilla rule `rule` in memory: when it grants and what.
    /// Mods that change one rule change it in load order.
    ///
    /// # Errors
    ///
    /// [`Off`] as [`Load::add_knowledge_rule`], and for a rule the list does
    /// not hold.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::knowledge::KnowledgeQuery;
    /// use enshwick::knowledge::{RuleChange, RuleId};
    ///
    /// fn init(load: &mut enshwick::Load<'_>, rule: RuleId, later: KnowledgeQuery) -> Result<(), enshwick::Off> {
    ///     let mut rule_change = RuleChange::default();
    ///     rule_change.when = Some(later);
    ///     load.change_knowledge_rule(rule, rule_change)
    /// }
    /// ```
    #[script]
    pub fn change_knowledge_rule(&mut self, rule: RuleId, change: RuleChange) -> Result<(), Off>;

    /// Turns the vanilla rule `rule` off in memory: it keeps its place with
    /// a query the game skips, so the places of the rules after it stay.
    ///
    /// # Errors
    ///
    /// [`Off`] for a rule the list does not hold, and a trigger list that
    /// is not reflected, not one resource or does not read.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>, rule: enshwick::knowledge::RuleId) -> Result<(), enshwick::Off> {
    ///     load.remove_knowledge_rule(rule)
    /// }
    /// ```
    #[script]
    pub fn remove_knowledge_rule(&mut self, rule: RuleId) -> Result<(), Off>;
}
