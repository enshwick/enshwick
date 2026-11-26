//! What the loot filters see of a roll: the roll and where an item may come from.

use super::{Kinds, LootContainerId};
use crate::Entity;

/// One roll of a container's loot, as the filters see it.
///
/// # Examples
///
/// See [`Source`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct Roll {
    /// What the container is.
    pub kind: Kinds,
    /// The container; `None` for a single-item roll.
    pub entity: Option<Entity>,
    /// The world node a chest is placed at; `None` for a corpse.
    pub node: Option<LootContainerId>,
    /// The loot labels of the enemy or chest type; `None` without them.
    pub labels: Option<LootContainerId>,
    /// Whether the corpse is a boss's.
    pub boss: bool,
    /// Whether the chest needs a key or a lockpick to open.
    pub locked: bool,
}

impl Roll {
    /// A `Roll` of its parts; `entity`, `node` and `labels` start as `None`.
    ///
    /// # Examples
    ///
    /// See [`Roll`].
    #[must_use]
    pub const fn new(kind: Kinds, boss: bool, locked: bool) -> Self {
        Self {
            kind,
            entity: None,
            node: None,
            labels: None,
            boss,
            locked,
        }
    }
}

/// Where a [`Drop`] may come from.
///
/// # Examples
///
/// ```
/// use enshwick::Entity;
/// use enshwick::loot::{LootContainerId, Source, Kinds, Roll};
///
/// let mut boss = Roll::new(Kinds::CORPSES, true, false);
/// boss.entity = Some(Entity::from_engine(7));
/// boss.labels = Some(LootContainerId::new(9));
/// assert!(Source::bosses().matches(&boss));
/// assert!(!Source::locked_chests().matches(&boss));
/// assert!(Source::kinds(Kinds::CORPSES).with_labels(&[LootContainerId::new(9)]).matches(&boss));
/// assert!(!Source::kinds(Kinds::CONTAINERS).with_nodes(&[LootContainerId::new(3)]).matches(&boss));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, crate::Script)]
#[script(name = "LootSource")]
#[non_exhaustive]
pub struct Source {
    /// The kinds of container.
    pub kinds: Kinds,
    /// Only bosses' corpses.
    #[script(default)]
    pub bosses_only: bool,
    /// Only chests that need a key or a lockpick.
    #[script(default)]
    pub locked_only: bool,
    /// Only these loot label ids ([`Roll::labels`]); empty for any.
    #[script(default)]
    pub labels: Vec<LootContainerId>,
    /// Only these world nodes ([`Roll::node`]); empty for any.
    #[script(default)]
    pub nodes: Vec<LootContainerId>,
}

impl Source {
    /// Every container of `kinds`.
    ///
    /// # Examples
    ///
    /// See [`Source`].
    #[must_use]
    pub const fn kinds(kinds: Kinds) -> Self {
        Self {
            kinds,
            bosses_only: false,
            locked_only: false,
            labels: Vec::new(),
            nodes: Vec::new(),
        }
    }

    /// Every boss's corpse.
    ///
    /// # Examples
    ///
    /// See [`Source`].
    #[must_use]
    pub const fn bosses() -> Self {
        let mut source = Self::kinds(Kinds::CORPSES);
        source.bosses_only = true;
        source
    }

    /// Every chest that needs a key or a lockpick.
    ///
    /// # Examples
    ///
    /// See [`Source`].
    #[must_use]
    pub const fn locked_chests() -> Self {
        let mut source = Self::kinds(Kinds::CHESTS);
        source.locked_only = true;
        source
    }

    /// The same, limited to the loot label ids `labels`.
    ///
    /// # Examples
    ///
    /// See [`Source`].
    #[must_use]
    pub fn with_labels(mut self, labels: &[LootContainerId]) -> Self {
        self.labels.extend_from_slice(labels);
        self
    }

    /// The same, limited to the world nodes `nodes`.
    ///
    /// # Examples
    ///
    /// See [`Source`].
    #[must_use]
    pub fn with_nodes(mut self, nodes: &[LootContainerId]) -> Self {
        self.nodes.extend_from_slice(nodes);
        self
    }

    /// Whether `roll` comes from here.
    ///
    /// # Examples
    ///
    /// See [`Source`].
    #[must_use]
    pub fn matches(&self, roll: &Roll) -> bool {
        self.kinds.contains(roll.kind)
            && (!self.bosses_only || roll.boss)
            && (!self.locked_only || roll.locked)
            && (self.labels.is_empty() || roll.labels.is_some_and(|l| self.labels.contains(&l)))
            && (self.nodes.is_empty() || roll.node.is_some_and(|n| self.nodes.contains(&n)))
    }
}
