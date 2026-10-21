//! The copies' answers to the game's survivor lookups, bed checks and staff
//! uses. The original survivor, the one the game's record names, is never
//! touched.
//!
//! The game asks from many places every frame, so lookups share a read
//! lock: only the uniqueness pass, which files copies, writes. Altar zones
//! are read when the altars change, never in a lookup.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::atomic::Ordering;
use std::sync::{PoisonError, RwLock};
use std::time::{Duration, Instant};

use enshwick_loader_api::abi::Side;
use enshwick_loader_api::altars::Altars;
use enshwick_loader_api::ecs::Entity;
use enshwick_loader_api::events::{AltarsChanged, ShutdownStarted};
use enshwick_loader_api::inventory::{Bar, Batch, Fill, Inventories, Op, SlotRef};
use enshwick_loader_api::survivors::{
    Answer, BedCheck, Lookup, Purpose, Summon, SummonVerdict, WorkshopSurvivor,
};
use enshwick_loader_api::{Load, Off};

use crate::Charges;
use crate::copies::{Copies, CopyStaff, Verdict, Zone};

/// How often the uniqueness pass lets go of copies it no longer sees.
const SWEEP: Duration = Duration::from_secs(2);
const FORGET_AFTER: Duration = Duration::from_secs(3);

/// How the staffs behave, from the settings; whether copy staffs work at
/// all is [`crate::COPY_STAFFS`], which may change while the game runs.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Staffs {
    /// The copy staffs' charges.
    pub(crate) charges: Charges,
    /// Whether the survivor's own staff places and moves copies, as before
    /// copy staffs.
    pub(crate) vanilla_places: bool,
}

/// A staff's use under way on this thread: its survivor and what the spawn
/// lookup answers, set by the summon filter and read by the lookup and the
/// handler after the use, all inside the game's spawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Plan {
    survivor: u32,
    answer: Answer,
    /// A copy staff placing a new copy, which spends a charge.
    places: bool,
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
            swept: Instant::now(),
            last_pass: Instant::now(),
        }
    }
}

static SHARED: RwLock<Option<Shared>> = RwLock::new(None);

pub(crate) fn link(load: &mut Load<'_>, staffs: Staffs) -> Result<(), Off> {
    let altars = load.altars()?;
    forget();
    load.on::<AltarsChanged>(move |()| read_altars(altars))?;
    load.on_survivor_lookup(|lookup| {
        match lookup.purpose {
            Purpose::Uniqueness { .. } => write(|shared| answer(shared, lookup)),
            _ => read(|shared| answer_reading(shared, lookup)),
        }
        .unwrap_or(Answer::Vanilla)
    })?;
    load.on_survivor_bed_check(|check| read(|shared| keeps(shared, check)).unwrap_or(true))?;
    load.on_summon(move |summon| {
        PLAN.with(|plan| plan.set(None));
        read(|shared| judge(shared, summon, staffs)).unwrap_or(SummonVerdict::Allow)
    })?;
    let charges = staffs.charges;
    let (world, scheduler) = (load.world()?, load.scheduler()?);
    let inventories = load.inventories().ok();
    load.on_summoned(move |summon| {
        let placed = PLAN.with(Cell::take).is_some_and(|plan| plan.places);
        if !placed || summon.consumed {
            return;
        }
        let (staff, slot, item) = (summon.pide, summon.slot, summon.item);
        if summon.meter.is_some() {
            scheduler.next_tick(move || spend(&world, staff));
        } else if let Some(inventories) = inventories {
            // A staff from before charges: one spent of a full one.
            scheduler.next_tick(move || renew_spent(&inventories, slot, item, charges));
        }
    })?;
    if load.host().build().side == Side::CLIENT {
        // Entities belong to one hosted game; the next starts clean.
        load.on::<ShutdownStarted>(|()| forget())?;
    }
    Ok(())
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
    write(|shared| {
        shared.copies.set_altars(
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

/// The uniqueness pass, which files copies.
fn answer(shared: &mut Shared, lookup: &Lookup) -> Answer {
    let npc = lookup.survivor.0;
    let found = lookup.vanilla.map_or(0, |e| e.0);
    match lookup.purpose {
        Purpose::Uniqueness { entity, home } => {
            let now = Instant::now();
            if now.duration_since(shared.last_pass) >= FORGET_AFTER {
                // The server stalled: nobody could be seen meanwhile.
                for seen in shared.seen.values_mut() {
                    *seen = now;
                }
            }
            shared.last_pass = now;
            if now.duration_since(shared.swept) >= SWEEP {
                sweep(shared, now);
            }
            file_copy(shared, npc, found, entity.0, home, now)
        }
        _ => answer_reading(shared, lookup),
    }
}

/// Every other lookup, which only reads.
fn answer_reading(shared: &Shared, lookup: &Lookup) -> Answer {
    let npc = lookup.survivor.0;
    let found = lookup.vanilla.map_or(0, |e| e.0);
    match lookup.purpose {
        Purpose::Cursor => {
            let hosting = enshwick_loader_api::host().is_some_and(|host| host.hosting());
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
            enshwick_loader_api::info!("survivor {npc:#010x}: a new copy at {target:?}");
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
            enshwick_loader_api::info!(
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
        return if found == 0 {
            Answer::Nobody
        } else {
            Answer::Entity(Entity(me))
        };
    }
    let original = if found == 0 { me } else { found };
    match shared.copies.observe(npc, me, home, original) {
        Verdict::Keep if found == 0 => Answer::Nobody,
        Verdict::Keep => Answer::Entity(Entity(me)),
        Verdict::Remove => {
            let holder = shared.copies.at(npc, home, original);
            enshwick_loader_api::info!(
                "survivor {npc:#010x}: copy {me} removed, {holder} keeps the slot"
            );
            Answer::Entity(Entity(if holder == me { original } else { holder }))
        }
    }
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

/// Spends one charge of the copy staff `staff`, on the game thread.
fn spend(world: &enshwick_loader_api::world::World, staff: Entity) {
    match world.change_item_meter(staff, |c| c - 1) {
        Ok(left) => enshwick_loader_api::info!(
            "copy staff {}: {} of {} charges left",
            staff.0,
            left.current,
            left.max
        ),
        Err(why) => enshwick_loader_api::warn!("copy staff {}: no charge spent: {why}", staff.0),
    }
}

/// Gives a staff from before charges, which just placed a copy, its meter:
/// a full staff's charges less the one spent.
fn renew_spent(inventories: &Inventories, slot: SlotRef, item: u32, charges: Charges) {
    let Some(root) = inventories.read().root(slot) else {
        return;
    };
    let renew = Op::Renew {
        slot,
        root,
        item,
        count: 1,
        fill: Fill::of(charges.full.saturating_sub(1), charges.full),
    };
    inventories.submit(&Batch::new().with(renew), |outcome| {
        enshwick_loader_api::info!("copy staff renewed after a use: {:?}", outcome.done);
    });
}

/// Whether placing a copy takes the whole staff: its last charge. A staff
/// from before charges, without a meter, counts as full.
pub(crate) fn last_charge(meter: Option<Bar>, full: u16) -> bool {
    meter.map_or(full, |charges| charges.current) <= 1
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
        if !crate::COPY_STAFFS.load(Ordering::Relaxed) {
            enshwick_loader_api::info!("copy staff of {}: refused, copy staffs are off", staff.key);
            return SummonVerdict::Refuse;
        }
        let (answer, places) = match shared.copies.copy_staff(survivor, summon.target, original) {
            CopyStaff::Refuse(why) => {
                enshwick_loader_api::info!(
                    "copy staff of {} at {:?}: refused, {why:?}",
                    staff.key,
                    summon.target
                );
                return SummonVerdict::Refuse;
            }
            CopyStaff::Move(copy) => (Answer::Entity(Entity(copy)), false),
            CopyStaff::Place => (Answer::Nobody, true),
        };
        PLAN.with(|plan| {
            plan.set(Some(Plan {
                survivor,
                answer,
                places,
            }));
        });
        let take = places && last_charge(summon.meter, charges.full);
        enshwick_loader_api::info!(
            "copy staff of {} at {:?} with {:?} charges: {}",
            staff.key,
            summon.target,
            summon.meter.map(|m| m.current),
            match (places, take) {
                (false, _) => "moves the copy there, free",
                (true, true) => "places a copy, its last charge",
                (true, false) => "places a copy, a charge spent",
            }
        );
        return if take {
            SummonVerdict::Take
        } else {
            SummonVerdict::Allow
        };
    }
    // The survivor's own staff moves the original, as in vanilla; a copy at
    // the target's altar then gives way to it.
    let vanilla =
        WorkshopSurvivor::by_staff(summon.item).is_some_and(|s| summon.template == s.template());
    if vanilla && !staffs.vanilla_places {
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
            }));
        });
    }
    SummonVerdict::Allow
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
        shared.copies.forget(entity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_last_charge_takes_the_staff() {
        let charges = |current| Some(Bar { current, max: 3 });
        assert!(!last_charge(charges(3), 3));
        assert!(!last_charge(charges(2), 3));
        assert!(last_charge(charges(1), 3));
        assert!(last_charge(charges(0), 3), "an empty staff goes on its use");
        assert!(
            !last_charge(None, 3),
            "a staff from before charges counts as full"
        );
        assert!(last_charge(None, 1), "one charge in all: its use takes it");
    }
}
