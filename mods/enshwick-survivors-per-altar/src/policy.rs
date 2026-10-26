//! The copies' answers to the game's survivor lookups, bed checks and staff
//! uses. The original survivor, the one the game's record names, is never
//! touched.
//!
//! The game asks from many places every frame, so lookups share a read
//! lock: only the uniqueness pass, which files copies, writes. Altar zones
//! are read when the altars change, never in a lookup. Other mods change the
//! copy limit and the charges a new copy costs at the extension points
//! `survivors-per-altar:copy-limit` and `survivors-per-altar:charge-cost`.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, OnceLock, PoisonError, RwLock};
use std::time::{Duration, Instant};

use enshwick::abi::Side;
use enshwick::altars::Altars;
use enshwick::ecs::Entity;
use enshwick::events::{AltarsChanged, ShutdownStarted};
use enshwick::inventory::{Bar, Batch, Fill, Inventories, Op, SlotRef};
use enshwick::points::OwnedPoint;
use enshwick::points::survivors_per_altar::{ChargeCost, CopyLimit};
use enshwick::survivors::{
    Answer, BedCheck, Lookup, Purpose, Summon, SummonVerdict, WorkshopSurvivor,
};
use enshwick::{Load, Off};

use crate::Charges;
use crate::copies::{Copies, CopyStaff, Verdict, Zone};

/// How often the uniqueness pass lets go of copies it no longer sees.
const SWEEP: Duration = Duration::from_secs(2);
const FORGET_AFTER: Duration = Duration::from_secs(3);
/// How long a copy stands outside every known zone before it goes: the
/// altars arrive from the game's passes one batch at a time after a load,
/// and a copy whose altar is not listed yet must not be taken for one whose
/// altar is gone.
const OUTSIDE_GRACE: Duration = Duration::from_secs(10);

/// How the staffs behave, from the settings read as the game starts;
/// whether copy staffs work at all is [`crate::COPY_STAFFS`], and whether the
/// survivor's own staff places copies [`crate::VANILLA_PLACES`], which may
/// change while the game runs.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Staffs {
    /// The copy staffs' charges.
    pub(crate) charges: Charges,
}

/// A staff's use under way on this thread: its survivor and what the spawn
/// lookup answers, set by the summon filter and read by the lookup and the
/// handler after the use, all inside the game's spawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Plan {
    survivor: u32,
    answer: Answer,
    /// A copy staff placing a new copy, which spends `cost` charges.
    places: bool,
    /// The charges placing the copy spends.
    cost: u16,
}

thread_local! {
    static PLAN: Cell<Option<Plan>> = const { Cell::new(None) };
}

#[derive(Debug)]
struct Shared {
    copies: Copies,
    /// Each survivor's original, the entity the game's record names.
    originals: BTreeMap<u32, u32>,
    /// When the uniqueness pass last saw each copy.
    seen: BTreeMap<u32, Instant>,
    /// Since when each copy has stood outside every known zone.
    outside: BTreeMap<u32, Instant>,
    /// When the first altars of this game were listed: for
    /// [`OUTSIDE_GRACE`] after, the rest may still arrive.
    altars_since: Option<Instant>,
    swept: Instant,
    /// When the uniqueness pass last ran.
    last_pass: Instant,
}

impl Shared {
    fn new() -> Self {
        Self {
            copies: Copies::default(),
            originals: BTreeMap::new(),
            seen: BTreeMap::new(),
            outside: BTreeMap::new(),
            altars_since: None,
            swept: Instant::now(),
            last_pass: Instant::now(),
        }
    }
}

static SHARED: RwLock<Option<Shared>> = RwLock::new(None);

/// The extension points, once defined.
static COPY_LIMIT: OnceLock<OwnedPoint<CopyLimit>> = OnceLock::new();
static CHARGE_COST: OnceLock<OwnedPoint<ChargeCost>> = OnceLock::new();

/// The most entities `survivor` may have with `altars` altars standing and
/// `copies` entities now: one per altar, or what other mods' links answer.
fn copy_limit(survivor: u32, altars: usize, copies: usize) -> usize {
    let Some(point) = COPY_LIMIT.get() else {
        return altars;
    };
    let input = CopyLimit {
        survivor,
        altars: u32::try_from(altars).unwrap_or(u32::MAX),
        copies: u32::try_from(copies).unwrap_or(u32::MAX),
    };
    let own = input.altars;
    usize::try_from(point.decide(&input, own).value_or(own)).unwrap_or(altars)
}

/// The charges a new copy of `survivor` costs, 1 or what other mods' links
/// answer; `None` when a link refuses the staff's use.
fn charge_cost(survivor: u32, copies: usize, meter: Option<Bar>, full: u16) -> Option<u16> {
    let Some(point) = CHARGE_COST.get() else {
        return Some(1);
    };
    let input = ChargeCost {
        survivor,
        copies: u32::try_from(copies).unwrap_or(u32::MAX),
        charges: meter.map_or(full, |m| m.current),
        full,
    };
    let decision = point.decide(&input, 1);
    (!decision.is_refused()).then(|| decision.value_or(1))
}

pub(crate) fn link(load: &mut Load<'_>, staffs: Staffs) -> Result<(), Off> {
    let _first = COPY_LIMIT.set(load.define_point()?);
    let _first = CHARGE_COST.set(load.define_point()?);
    let altars = load.altars()?;
    forget();
    load.on::<AltarsChanged>(move |()| read_altars(altars))?;
    load.on_survivor_lookup(|lookup| {
        if !crate::PER_ALTAR.load(Ordering::Relaxed) {
            return Answer::Vanilla;
        }
        match lookup.purpose {
            Purpose::Uniqueness { .. } => write(|shared| answer(shared, lookup)),
            _ => read(|shared| answer_reading(shared, lookup)),
        }
        .unwrap_or(Answer::Vanilla)
    })?;
    load.on_survivor_bed_check(|check| {
        !crate::PER_ALTAR.load(Ordering::Relaxed)
            || read(|shared| keeps(shared, check)).unwrap_or(true)
    })?;
    load.on_summon(move |summon| {
        PLAN.with(|plan| plan.set(None));
        read(|shared| judge(shared, summon, staffs)).unwrap_or(SummonVerdict::Allow)
    })?;
    let charges = staffs.charges;
    let (world, scheduler) = (load.world()?, load.scheduler()?);
    let inventories = load.inventories().ok();
    load.on_summoned(move |summon| {
        let cost = PLAN
            .with(Cell::take)
            .filter(|plan| plan.places)
            .map_or(0, |plan| plan.cost);
        if cost == 0 || summon.consumed {
            return;
        }
        let (staff, slot, item) = (summon.pide, summon.slot, summon.item);
        if summon.meter.is_some() {
            if let Some(staff) = staff {
                owe(staff, cost);
                scheduler
                    .next_tick(move || settle_spends(|staff, cost| spend(&world, staff, cost)));
            }
        } else if let Some(inventories) = inventories {
            // A staff from before charges: the cost spent of a full one.
            scheduler.next_tick(move || renew_spent(&inventories, slot, item, charges, cost));
        }
    })?;
    let client = load.host().build().side == Side::CLIENT;
    load.on::<ShutdownStarted>(move |()| {
        // The final save runs after this: a charge owed since the last
        // step is spent before it.
        settle_spends(|staff, cost| spend(&world, staff, cost));
        if client {
            // Entities belong to one hosted game; the next starts clean.
            forget();
        }
    })?;
    Ok(())
}

/// Copy staffs that placed a copy and owe their charge, spent on the game
/// thread after the step of the use, or at the latest as the game stops.
static OWED: Mutex<Vec<(Entity, u16)>> = Mutex::new(Vec::new());

/// Notes that `staff` owes `cost` charges.
fn owe(staff: Entity, cost: u16) {
    OWED.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push((staff, cost));
}

/// Spends every charge owed, each once.
fn settle_spends(mut spend: impl FnMut(Entity, u16)) {
    let owed = core::mem::take(&mut *OWED.lock().unwrap_or_else(PoisonError::into_inner));
    for (staff, cost) in owed {
        spend(staff, cost);
    }
}

/// Lets the altars of the running game count as all listed.
#[cfg(test)]
pub(crate) fn settle() {
    write(|shared| {
        shared.altars_since = Instant::now().checked_sub(OUTSIDE_GRACE);
    });
}

fn forget() {
    *SHARED.write().unwrap_or_else(PoisonError::into_inner) = Some(Shared::new());
}

fn read<R>(f: impl FnOnce(&Shared) -> R) -> Option<R> {
    let slot = SHARED.read().unwrap_or_else(PoisonError::into_inner);
    slot.as_ref().map(f)
}

fn write<R>(f: impl FnOnce(&mut Shared) -> R) -> Option<R> {
    let mut slot = SHARED.write().unwrap_or_else(PoisonError::into_inner);
    slot.as_mut().map(f)
}

/// Takes the altars' zones, which move only when someone builds; an empty
/// list once zones were known means the last altar is gone.
fn read_altars(altars: Altars) {
    let list = altars.list();
    let now = Instant::now();
    write(|shared| {
        set_altars(
            shared,
            now,
            list.iter()
                .filter(|altar| !altar.zone.is_empty())
                .map(|altar| {
                    (
                        u64::from(altar.entity.0),
                        Zone::new(altar.zone.min, altar.zone.max),
                    )
                }),
        );
    });
}

/// Takes `altars` and notes when the first of this game arrived.
fn set_altars(shared: &mut Shared, now: Instant, altars: impl IntoIterator<Item = (u64, Zone)>) {
    shared.copies.set_altars(altars);
    if shared.copies.knows_altars() && shared.altars_since.is_none() {
        shared.altars_since = Some(now);
    }
}

/// Whether the altars may still be arriving: within [`OUTSIDE_GRACE`] of
/// the first ones. A copy whose home lies in the grown zone of a listed
/// altar, nearer its own altar not listed yet, then looks like a second
/// copy in that altar's slot.
fn settling(shared: &Shared, now: Instant) -> bool {
    shared
        .altars_since
        .is_none_or(|since| now.duration_since(since) < OUTSIDE_GRACE)
}

/// The uniqueness pass, which files copies.
fn answer(shared: &mut Shared, lookup: &Lookup) -> Answer {
    let npc = lookup.survivor.0;
    let found = lookup.vanilla.map_or(0, |e| e.0);
    match lookup.purpose {
        Purpose::Uniqueness { entity, home } => {
            let now = Instant::now();
            note_pass(shared, now);
            if now.duration_since(shared.swept) >= SWEEP {
                sweep(shared, now);
            }
            file_copy(shared, npc, found, entity.0, home, now)
        }
        _ => answer_reading(shared, lookup),
    }
}

/// Notes a uniqueness pass at `now`. After a stall nobody could be seen
/// meanwhile, and no altar could arrive.
fn note_pass(shared: &mut Shared, now: Instant) {
    let stall = now.duration_since(shared.last_pass);
    if stall >= FORGET_AFTER {
        for seen in shared.seen.values_mut().chain(shared.outside.values_mut()) {
            *seen = now;
        }
        shared.altars_since = shared.altars_since.map(|since| since + stall);
    }
    shared.last_pass = now;
}

/// Every other lookup, which only reads.
fn answer_reading(shared: &Shared, lookup: &Lookup) -> Answer {
    let npc = lookup.survivor.0;
    let found = lookup.vanilla.map_or(0, |e| e.0);
    match lookup.purpose {
        Purpose::Cursor => {
            let hosting = enshwick::host().is_some_and(|host| host.hosting());
            crate::cursor::answer(hosting, shared.copies.copy_count(npc))
        }
        Purpose::Uniqueness { .. } => Answer::Vanilla,
        Purpose::Spawn { target } => {
            if let Some(plan) = PLAN.with(Cell::get).filter(|p| p.survivor == npc) {
                return plan.answer;
            }
            if let Some(copy) = shared.copies.for_spawn(npc, target, found) {
                return Answer::Entity(Entity(copy));
            }
            enshwick::info!("survivor {npc:#010x}: a new copy at {target:?}");
            Answer::Nobody
        }
        _ if shared.copies.copy_count(npc) <= 1 => Answer::Vanilla,
        Purpose::BedOwner { bed_at } => match shared.copies.altar_at(bed_at) {
            None => Answer::Vanilla,
            Some(altar) => entity_or_nobody(shared.copies.slot(npc, altar)),
        },
        Purpose::Roaming { at, .. } => Answer::Entity(Entity(shared.copies.at(npc, at, found))),
        Purpose::AssignBed { bed, bed_at } => {
            let answer = shared
                .copies
                .altar_at(bed_at)
                .and_then(|a| shared.copies.slot(npc, a));
            enshwick::info!(
                "survivor {npc:#010x}: staff gives bed {} to {}",
                bed.0,
                answer.unwrap_or(0)
            );
            entity_or_nobody(answer)
        }
        Purpose::PreviousOccupant { bed, .. } => {
            // The copy that really sleeps in this bed.
            let sleeper = shared
                .copies
                .copies_of(npc)
                .map(|(e, _)| e)
                .find(|&e| lookup.bed_of(Entity(e)) == Some(bed));
            entity_or_nobody(sleeper)
        }
        _ => Answer::Nobody,
    }
}

fn entity_or_nobody(entity: Option<u32>) -> Answer {
    entity.map_or(Answer::Nobody, |e| Answer::Entity(Entity(e)))
}

fn file_copy(
    shared: &mut Shared,
    npc: u32,
    found: u32,
    me: u32,
    home: [i64; 3],
    now: Instant,
) -> Answer {
    shared.seen.insert(me, now);
    if found != 0 {
        shared.originals.insert(npc, found);
    }
    // Right after a load the zones are not known yet: keep every copy until
    // they are, or each would look like one outside the zones.
    if !shared.copies.knows_altars() {
        return keep(found, me);
    }
    let original = if found == 0 { me } else { found };
    let verdict = shared.copies.observe(npc, me, home, original);
    if verdict != Verdict::Outside {
        shared.outside.remove(&me);
    }
    match verdict {
        Verdict::Outside if !outside_long_enough(&mut shared.outside, me, now) => keep(found, me),
        Verdict::Outside => {
            shared.outside.remove(&me);
            shared.copies.forget(me);
            enshwick::info!("survivor {npc:#010x}: copy {me} outside every zone removed");
            Answer::Entity(Entity(original))
        }
        Verdict::Keep => keep(found, me),
        // Kept unfiled: the next pass files it once its altar is listed.
        Verdict::Remove if settling(shared, now) => keep(found, me),
        Verdict::Remove => {
            let holder = shared.copies.at(npc, home, original);
            enshwick::info!("survivor {npc:#010x}: copy {me} removed, {holder} keeps the slot");
            Answer::Entity(Entity(if holder == me { original } else { holder }))
        }
    }
}

/// The uniqueness answer that keeps `me`: nobody else when the game's record
/// names no entity yet.
fn keep(found: u32, me: u32) -> Answer {
    if found == 0 {
        Answer::Nobody
    } else {
        Answer::Entity(Entity(me))
    }
}

/// Whether the copy `me`, outside every known zone now, has stood outside
/// for [`OUTSIDE_GRACE`]; the first sight starts the count.
fn outside_long_enough(outside: &mut BTreeMap<u32, Instant>, me: u32, now: Instant) -> bool {
    let since = *outside.entry(me).or_insert(now);
    now.duration_since(since) >= OUTSIDE_GRACE
}

/// Whether a copy keeps a bed the game kept: not one at another altar than
/// its home's.
fn keeps(shared: &Shared, check: &BedCheck) -> bool {
    if !shared.copies.knows_altars() || shared.copies.copy_count(check.survivor.0) <= 1 {
        return true;
    }
    let (Some(mine), Some(beds)) = (
        shared.copies.altar_at(check.home),
        shared.copies.altar_at(check.bed_at),
    ) else {
        return true;
    };
    mine == beds
}

/// Spends `cost` charges of the copy staff `staff`, on the game thread.
fn spend(world: &enshwick::world::World, staff: Entity, cost: u16) {
    match world.change_item_meter(staff, |c| (c - i32::from(cost)).max(0)) {
        Ok(left) => enshwick::info!(
            "copy staff {}: {} of {} charges left",
            staff.0,
            left.current,
            left.max
        ),
        Err(why) => enshwick::warn!("copy staff {}: no charge spent: {why}", staff.0),
    }
}

/// Gives a staff from before charges, which just placed a copy, its meter:
/// a full staff's charges less the `cost` spent.
fn renew_spent(inventories: &Inventories, slot: SlotRef, item: u32, charges: Charges, cost: u16) {
    let Some(root) = inventories.read().root(slot) else {
        return;
    };
    let renew = Op::Renew {
        slot,
        root,
        item,
        count: 1,
        fill: Fill::of(charges.full.saturating_sub(cost), charges.full),
    };
    inventories.submit(&Batch::new().with(renew), |outcome| {
        enshwick::info!("copy staff renewed after a use: {:?}", outcome.done);
    });
}

/// Whether placing a copy that costs `cost` charges takes the whole staff:
/// its last charges. A staff from before charges, without a meter, counts as
/// full; a free copy never takes it.
pub(crate) fn last_charge(meter: Option<Bar>, full: u16, cost: u16) -> bool {
    cost > 0 && meter.map_or(full, |charges| charges.current) <= cost
}

/// What a staff's use does, and the plan the spawn lookup and the handler
/// after the use follow.
fn judge(shared: &Shared, summon: &Summon, staffs: Staffs) -> SummonVerdict {
    if summon.ammo != 0 {
        return SummonVerdict::Allow;
    }
    let survivor = summon.template.id();
    let original = shared.originals.get(&survivor).copied().unwrap_or(0);
    if let Some(staff) = crate::staffs::copy_staff(summon.item) {
        let charges = staffs.charges;
        // The staff's effect spawn carries the same item; only the survivor
        // itself is planned.
        if staff
            .survivor()
            .is_none_or(|s| summon.template != s.template())
        {
            return SummonVerdict::Allow;
        }
        if !crate::COPY_STAFFS.load(Ordering::Relaxed) || !crate::PER_ALTAR.load(Ordering::Relaxed)
        {
            enshwick::info!("copy staff of {}: refused, copy staffs are off", staff.key);
            return SummonVerdict::Refuse;
        }
        let limit = |altars, copies| copy_limit(survivor, altars, copies);
        let (answer, places) =
            match shared
                .copies
                .copy_staff(survivor, summon.target, original, limit)
            {
                CopyStaff::Refuse(why) => {
                    enshwick::info!(
                        "copy staff of {} at {:?}: refused, {why:?}",
                        staff.key,
                        summon.target
                    );
                    return SummonVerdict::Refuse;
                }
                CopyStaff::Move(copy) => (Answer::Entity(Entity(copy)), false),
                CopyStaff::Place => (Answer::Nobody, true),
            };
        let cost = if places {
            let copies = shared.copies.entity_count(survivor, original);
            let Some(cost) = charge_cost(survivor, copies, summon.meter, charges.full) else {
                enshwick::info!(
                    "copy staff of {} at {:?}: refused at survivors-per-altar:charge-cost",
                    staff.key,
                    summon.target
                );
                return SummonVerdict::Refuse;
            };
            cost
        } else {
            0
        };
        PLAN.with(|plan| {
            plan.set(Some(Plan {
                survivor,
                answer,
                places,
                cost,
            }));
        });
        let take = places && last_charge(summon.meter, charges.full, cost);
        enshwick::info!(
            "copy staff of {} at {:?} with {:?} charges: {}",
            staff.key,
            summon.target,
            summon.meter.map(|m| m.current),
            match (places, take) {
                (false, _) => "moves the copy there, free".to_owned(),
                (true, true) => format!("places a copy, its last charges ({cost})"),
                (true, false) => format!("places a copy, {cost} charges spent"),
            }
        );
        return if take {
            SummonVerdict::Take
        } else {
            SummonVerdict::Allow
        };
    }
    if !crate::PER_ALTAR.load(Ordering::Relaxed) {
        return SummonVerdict::Allow;
    }
    judge_own_staff(shared, summon, survivor, original)
}

/// What the survivor's own staff does: it moves the original, or with
/// [`crate::VANILLA_PLACES`] places and moves copies within the limit.
fn judge_own_staff(
    shared: &Shared,
    summon: &Summon,
    survivor: u32,
    original: u32,
) -> SummonVerdict {
    // The survivor's own staff moves the original, as in vanilla; a copy at
    // the target's altar then gives way to it.
    let vanilla =
        WorkshopSurvivor::by_staff(summon.item).is_some_and(|s| summon.template == s.template());
    if vanilla && !crate::VANILLA_PLACES.load(Ordering::Relaxed) {
        // Always the original, however many copies stand in the world.
        let answer = if original == 0 {
            Answer::Vanilla
        } else {
            Answer::Entity(Entity(original))
        };
        PLAN.with(|plan| {
            plan.set(Some(Plan {
                survivor,
                answer,
                places: false,
                cost: 0,
            }));
        });
    } else if vanilla && vanilla_staff_over_limit(&shared.copies, survivor, summon.target, original)
    {
        enshwick::info!(
            "survivor {survivor:#010x}: its own staff at {:?} refused, one entity per altar",
            summon.target
        );
        return SummonVerdict::Refuse;
    }
    SummonVerdict::Allow
}

/// Whether the survivor's own staff, placing copies, would place a new one
/// at `target` beyond the copy limit.
fn vanilla_staff_over_limit(
    copies: &Copies,
    survivor: u32,
    target: [i64; 3],
    original: u32,
) -> bool {
    copies.places_new(survivor, target)
        && copies.over_limit(survivor, original, |altars, count| {
            copy_limit(survivor, altars, count)
        })
}

/// Lets go of copies the uniqueness pass no longer sees.
fn sweep(shared: &mut Shared, now: Instant) {
    shared.swept = now;
    let gone: Vec<u32> = shared
        .seen
        .iter()
        .filter(|(_, at)| now.duration_since(**at) > FORGET_AFTER)
        .map(|(&e, _)| e)
        .collect();
    for entity in gone {
        shared.seen.remove(&entity);
        shared.outside.remove(&entity);
        shared.copies.forget(entity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_outside_every_zone_goes_only_after_the_grace() {
        let mut shared = Shared::new();
        shared.copies.set_altars([
            (1, Zone::new([0, 0, 0], [80, 80, 80])),
            (2, Zone::new([100, 0, 0], [180, 80, 80])),
        ]);
        let t0 = Instant::now();
        assert_eq!(
            file_copy(&mut shared, 7, 10, 10, [10, 0, 10], t0),
            Answer::Entity(Entity(10))
        );
        assert_eq!(
            file_copy(&mut shared, 7, 10, 11, [120, 0, 10], t0),
            Answer::Entity(Entity(11))
        );
        // Right after a load only one altar is listed yet: the copy at the
        // other one waits for its zone.
        shared
            .copies
            .set_altars([(1, Zone::new([0, 0, 0], [80, 80, 80]))]);
        let soon = t0 + Duration::from_secs(1);
        assert_eq!(
            file_copy(&mut shared, 7, 10, 11, [120, 0, 10], soon),
            Answer::Entity(Entity(11)),
            "kept while the altars arrive"
        );
        shared.copies.set_altars([
            (1, Zone::new([0, 0, 0], [80, 80, 80])),
            (2, Zone::new([100, 0, 0], [180, 80, 80])),
        ]);
        let later = t0 + OUTSIDE_GRACE + Duration::from_secs(1);
        assert_eq!(
            file_copy(&mut shared, 7, 10, 11, [120, 0, 10], later),
            Answer::Entity(Entity(11)),
            "filed at its altar once it is listed"
        );
        assert!(shared.outside.is_empty());
        // The altar is really gone: the copy goes once the grace ran out.
        shared
            .copies
            .set_altars([(1, Zone::new([0, 0, 0], [80, 80, 80]))]);
        assert_eq!(
            file_copy(&mut shared, 7, 10, 11, [120, 0, 10], later),
            Answer::Entity(Entity(11))
        );
        let gone = later + OUTSIDE_GRACE;
        assert_eq!(
            file_copy(&mut shared, 7, 10, 11, [120, 0, 10], gone),
            Answer::Entity(Entity(10)),
            "the original keeps the survivor"
        );
        assert_eq!(shared.copies.copy_count(7), 1);
        assert!(shared.outside.is_empty());
    }

    #[test]
    fn a_copy_in_a_listed_grown_zone_waits_for_its_own_altar_after_a_load() {
        let one = (1, Zone::new([0, 0, 0], [80, 80, 80]));
        // Altar 2's zone grew over altar 1's edge: the copy at x = 75 is
        // nearer altar 2.
        let two = (2, Zone::new([60, 0, 0], [140, 80, 80]));
        let mut shared = Shared::new();
        let t0 = Instant::now();
        set_altars(&mut shared, t0, [one]);
        assert_eq!(
            file_copy(&mut shared, 7, 10, 10, [10, 0, 10], t0),
            Answer::Entity(Entity(10))
        );
        let soon = t0 + Duration::from_secs(1);
        assert_eq!(
            file_copy(&mut shared, 7, 10, 11, [75, 0, 10], soon),
            Answer::Entity(Entity(11)),
            "kept while the altars arrive"
        );
        set_altars(&mut shared, soon, [one, two]);
        assert_eq!(
            file_copy(&mut shared, 7, 10, 11, [75, 0, 10], soon),
            Answer::Entity(Entity(11))
        );
        assert_eq!(shared.copies.slot(7, 2), Some(11), "filed at its own altar");
        assert_eq!(shared.copies.slot(7, 1), Some(10));
        // Once the altars are in, a second copy in a slot goes at once.
        let later = t0 + OUTSIDE_GRACE;
        assert_eq!(
            file_copy(&mut shared, 7, 10, 12, [100, 0, 10], later),
            Answer::Entity(Entity(11))
        );
    }

    #[test]
    fn a_stall_holds_the_wait_for_the_altars() {
        let mut shared = Shared::new();
        let t0 = Instant::now();
        set_altars(&mut shared, t0, [(1, Zone::new([0, 0, 0], [80, 80, 80]))]);
        shared.last_pass = t0;
        let stalled = t0 + OUTSIDE_GRACE + Duration::from_secs(1);
        note_pass(&mut shared, stalled);
        assert!(settling(&shared, stalled));
        assert!(!settling(&shared, stalled + OUTSIDE_GRACE));
    }

    #[test]
    fn only_the_last_charge_takes_the_staff() {
        let charges = |current| Some(Bar { current, max: 3 });
        assert!(!last_charge(charges(3), 3, 1));
        assert!(!last_charge(charges(2), 3, 1));
        assert!(last_charge(charges(1), 3, 1));
        assert!(
            last_charge(charges(0), 3, 1),
            "an empty staff goes on its use"
        );
        assert!(
            last_charge(charges(2), 3, 2),
            "a cost of two takes the last two"
        );
        assert!(!last_charge(charges(1), 3, 0), "a free copy takes nothing");
        assert!(
            !last_charge(None, 3, 1),
            "a staff from before charges counts as full"
        );
        assert!(
            last_charge(None, 1, 1),
            "one charge in all: its use takes it"
        );
    }

    #[test]
    fn an_owed_charge_is_spent_once_even_when_the_game_stops_first() {
        let mut spent = Vec::new();
        owe(Entity(4), 1);
        owe(Entity(5), 2);
        // The game stops before the step's tasks run: the stop spends them.
        settle_spends(|staff, cost| spent.push((staff.0, cost)));
        assert_eq!(spent, [(4, 1), (5, 2)]);
        // The task queued after the use finds nothing left to spend.
        settle_spends(|staff, cost| spent.push((staff.0, cost)));
        assert_eq!(spent, [(4, 1), (5, 2)]);
    }
}
