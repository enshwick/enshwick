//! The points of the survivors per altar mod (`survivors-per-altar`).

/// How many copies of a survivor its altars hold,
/// `survivors-per-altar:copy-limit`: the answer is the most entities of the
/// survivor, its original included, before a copy staff is refused.
///
/// The mod answers with the number of altars. Each altar still holds one
/// entity at most, so a higher answer adds one at most: a copy at every
/// altar while the original stands outside them. Asked on the server's
/// game thread when a staff would place a new copy: a copy staff, or the
/// survivor's own with `vanilla_staff_places_copies`.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::points::survivors_per_altar::CopyLimit;
/// use enshwick_loader_api::points::{Order, Verdict};
///
/// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
///     // At most three of each survivor, however many altars.
///     load.extend_point(&Order::new(), |_: &CopyLimit, limit: &u32| Verdict::Set((*limit).min(3)))
/// }
/// ```
#[crate::point(name = "survivors-per-altar:copy-limit", mode = Override, output = u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CopyLimit {
    /// The survivor, the number of its [`SurvivorId`](crate::survivors::SurvivorId).
    pub survivor: u32,
    /// How many altars stand.
    pub altars: u32,
    /// How many entities of the survivor stand now, its original included
    /// wherever it is.
    pub copies: u32,
}

/// How many charges placing a copy spends, `survivors-per-altar:charge-cost`:
/// the answer is the charges; a refusal refuses the staff's use, with
/// nothing spent.
///
/// The mod answers 1. A cost of 0 places the copy free; a cost at or above
/// the charges left takes the staff. Moving a copy is always free. Asked
/// on the server's game thread when a copy staff would place a new copy.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::points::survivors_per_altar::ChargeCost;
/// use enshwick_loader_api::points::{Order, Verdict};
///
/// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
///     // Every second copy of a survivor costs two charges.
///     load.extend_point(&Order::new(), |cost: &ChargeCost, charges: &u16| {
///         if cost.copies % 2 == 1 { Verdict::Set(charges * 2) } else { Verdict::Pass }
///     })
/// }
/// ```
#[crate::point(name = "survivors-per-altar:charge-cost", mode = Override, output = u16, refusable)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChargeCost {
    /// The survivor, the number of its [`SurvivorId`](crate::survivors::SurvivorId).
    pub survivor: u32,
    /// How many entities of the survivor stand now, its original included
    /// wherever it is.
    pub copies: u32,
    /// The charges the staff holds; a full staff's for one from before
    /// charges.
    pub charges: u16,
    /// The charges a full staff holds.
    pub full: u16,
}
