//! The points of the Area loot mod (`area-loot`).

use crate::ecs::Entity;

/// Whether a loot container around an emptied one comes along,
/// `area-loot:eligible`: the answer is whether it does.
///
/// The mod answers true for a full enemy loot container within its radius
/// that no other player is at. Asked on the server's game thread when a
/// player empties a container, once per container around it.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::points::area_loot::Eligible;
/// use enshwick_loader_api::points::{Order, Verdict};
///
/// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
///     // Only the loot within five metres comes along.
///     load.extend_point(&Order::new(), |loot: &Eligible, _: &bool| {
///         if loot.distance > 5.0 { Verdict::Set(false) } else { Verdict::Pass }
///     })
/// }
/// ```
#[crate::point(name = "area-loot:eligible", mode = Override, output = bool)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Eligible {
    /// The player who emptied a container.
    pub player: Entity,
    /// The container the player emptied.
    pub emptied: Entity,
    /// The container around it.
    pub container: Entity,
    /// Metres between the two, across the ground.
    pub distance: f32,
}
