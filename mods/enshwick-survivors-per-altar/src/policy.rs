//! The copies' answers to the game's survivor lookups, bed checks and staff
//! uses. The original survivor, the one the game's record names, is never
//! touched.
//!
//! The game asks from many places every frame, so lookups share a read
//! lock: only the uniqueness pass, which files copies, writes. Altar zones
//! are read when the altars change, never in a lookup.

use std::collections::BTreeMap;
use std::sync::{PoisonError, RwLock};
use std::time::{Duration, Instant};

use enshwick_loader_api::abi::Side;
use enshwick_loader_api::altars::Altars;
use enshwick_loader_api::ecs::Entity;
use enshwick_loader_api::events::{AltarsChanged, ShutdownStarted};
use enshwick_loader_api::inventory::Bar;
use enshwick_loader_api::survivors::{Answer, BedCheck, Lookup, Purpose, Summon};
use enshwick_loader_api::{Load, Off};

use crate::copies::{Copies, Verdict, Zone};

/// How often the uniqueness pass lets go of copies it no longer sees.
const SWEEP: Duration = Duration::from_secs(2);
const FORGET_AFTER: Duration = Duration::from_secs(3);

#[derive(Debug)]
struct Shared {
    copies: Copies,
    /// When the uniqueness pass last saw each copy.
    seen: BTreeMap<u32, Instant>,
    swept: Instant,
}

impl Shared {
    fn new() -> Self {
        Self {
            copies: Copies::default(),
            seen: BTreeMap::new(),
            swept: Instant::now(),
        }
    }
}

static SHARED: RwLock<Option<Shared>> = RwLock::new(None);

pub(crate) fn link(load: &mut Load<'_>, staffs: bool) -> Result<(), Off> {
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
    if staffs {
        load.on_summon(|summon| read(|shared| consumes(shared, summon)).unwrap_or(false))?;
        let (world, scheduler) = (load.world()?, load.scheduler()?);
        load.on_summoned(move |summon| {
            if !read(|shared| spends_charge(shared, summon)).unwrap_or(false) {
                return;
            }
            let staff = summon.pide;
            scheduler.next_tick(move || spend(&world, staff));
        })?;
    }
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

/// Takes the altars' zones, which move only when someone builds.
fn read_altars(altars: Altars) {
    let list = altars.list();
    if list.is_empty() {
        return;
    }
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

/// The copy staff a use at an altar spends: a copy staff placing its own
/// survivor, inside an altar zone. The staff's effect spawn, which carries
/// the same item, is left alone, and so is a use outside every altar zone,
/// where the copy would not stay.
fn staff_used(shared: &Shared, summon: &Summon) -> Option<&'static crate::staffs::Staff> {
    let staff = crate::staffs::copy_staff(summon.item)?;
    if summon.ammo != 0
        || staff
            .survivor()
            .is_none_or(|s| summon.template != s.template())
    {
        return None;
    }
    if shared.copies.altar_at(summon.target).is_none() {
        enshwick_loader_api::info!(
            "copy staff of {} used outside every altar zone: kept",
            staff.key
        );
        return None;
    }
    Some(staff)
}

/// Whether a use takes the whole staff: its last charge, or a staff from
/// before charges, which has no meter. Any other use keeps the staff and
/// spends a charge once it worked.
pub(crate) fn last_charge(meter: Option<Bar>) -> bool {
    meter.is_none_or(|charges| charges.current <= 1)
}

/// Marks a copy staff's use at an altar on its last charge as consuming, so
/// the game takes the staff in the same transaction as the spawn or move,
/// and only when that works.
fn consumes(shared: &Shared, summon: &Summon) -> bool {
    let Some(staff) = staff_used(shared, summon) else {
        return false;
    };
    let last = last_charge(summon.meter);
    enshwick_loader_api::info!(
        "copy staff of {} used at {:?} with {:?} charges: {}",
        staff.key,
        summon.target,
        summon.meter.map(|m| m.current),
        if last {
            "taken on success"
        } else {
            "a charge spent on success"
        }
    );
    last
}

/// Whether a use that worked spends a charge: one at an altar that kept the
/// staff.
fn spends_charge(shared: &Shared, summon: &Summon) -> bool {
    !summon.consumed && summon.meter.is_some() && staff_used(shared, summon).is_some()
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
        assert!(!last_charge(charges(3)));
        assert!(!last_charge(charges(2)));
        assert!(last_charge(charges(1)));
        assert!(last_charge(charges(0)), "an empty staff goes on its use");
        assert!(
            last_charge(None),
            "a staff from before charges is used once"
        );
    }
}
