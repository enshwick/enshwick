//! The points of the Safes mod (`safes`).

use crate::ecs::Entity;

/// What a player tries to do with a Safe.
///
/// # Examples
///
/// ```
/// use enshwick::points::safes::SafeAction;
///
/// assert_ne!(SafeAction::USE, SafeAction::REMOVE);
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Shape)]
#[shape(values(USE, REMOVE, MANAGE))]
pub struct SafeAction(pub u32);

impl SafeAction {
    /// Open it, take, put or sort, craft and build from it.
    pub const USE: Self = Self(0);
    /// Pick it up or dismantle it.
    pub const REMOVE: Self = Self(1);
    /// Rename it or change who may use it.
    pub const MANAGE: Self = Self(2);
}

/// Who may open, use, remove or manage a Safe, `safes:access`: the answer
/// is whether the player may, and a refusal is final.
///
/// The Safes mod answers first: the owner may do all, the players they
/// allowed only use it, administrators all when its settings allow. A
/// player the server has not registered yet is refused before the point is
/// asked. Asked on the server's worker threads, inside the game's systems:
/// a link reads lock-free state only.
///
/// # Examples
///
/// ```
/// use enshwick::points::safes::{Access, SafeAction};
/// use enshwick::points::{Order, Verdict};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     // Nobody but the owner removes a Safe, administrators included.
///     load.extend_point(&Order::new(), |access: &Access, _may: &bool| {
///         if access.action == SafeAction::REMOVE && access.player != access.owner {
///             Verdict::Refuse
///         } else {
///             Verdict::Pass
///         }
///     })
/// }
/// ```
#[crate::point(name = "safes:access", mode = Override, output = bool, refusable)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Access {
    /// The player's account key.
    pub player: u64,
    /// The owner's account key.
    pub owner: u64,
    /// The player's character.
    pub character: Entity,
    /// The Safe; [`Entity`] of the chest itself.
    pub safe: Entity,
    /// What the player tries.
    pub action: SafeAction,
    /// Whether the server makes the player an administrator.
    pub admin: bool,
    /// Whether the owner allowed the player at this Safe.
    pub allowed: bool,
}
