//! The copies' answers to the game's survivor lookups, bed checks and staff
//! uses. The original survivor, the one the game's record names, is never
//! touched.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use enshwick_loader_api::abi::Side;
use enshwick_loader_api::altars::Altars;
use enshwick_loader_api::ecs::Entity;
use enshwick_loader_api::events::Shutdown;
use enshwick_loader_api::survivors::{Answer, BedCheck, Lookup, Purpose, Summon};
use enshwick_loader_api::{Load, Off};

use crate::copies::{Copies, Verdict, Zone};

const REFRESH: Duration = Duration::from_secs(2);
const FORGET_AFTER: Duration = Duration::from_secs(3);

#[derive(Debug, Default)]
struct Shared {
    copies: Copies,
    /// When the uniqueness pass last saw each copy.
    seen: BTreeMap<u32, Instant>,
    refreshed: Option<Instant>,
}

static SHARED: Mutex<Option<Shared>> = Mutex::new(None);

pub(crate) fn link(load: &mut Load<'_>, staffs: bool) -> Result<(), Off> {
    let altars = load.altars()?;
    if let Ok(mut shared) = SHARED.lock() {
        *shared = Some(Shared::default());
    }
    load.on_survivor_lookup(move |lookup| {
        with_shared(altars, |shared| answer(shared, lookup)).unwrap_or(Answer::Vanilla)
    })?;
    load.on_survivor_bed_check(move |check| {
        with_shared(altars, |shared| keeps(shared, check)).unwrap_or(true)
    })?;
    if staffs {
        load.on_summon(move |summon| {
            with_shared(altars, |shared| consumes(shared, summon)).unwrap_or(false)
        })?;
    }
    if load.host().build().side == Side::CLIENT {
        // Entities belong to one hosted game; the next starts clean.
        load.on::<Shutdown>(|()| forget())?;
    }
    Ok(())
}

fn forget() {
    if let Ok(mut shared) = SHARED.lock() {
        *shared = Some(Shared::default());
    }
}

fn with_shared<R>(altars: Altars, f: impl FnOnce(&mut Shared) -> R) -> Option<R> {
    let mut slot = SHARED.lock().ok()?;
    let shared = slot.as_mut()?;
    refresh(altars, shared);
    Some(f(shared))
}

fn answer(shared: &mut Shared, lookup: &Lookup) -> Answer {
    let npc = lookup.survivor.0;
    let found = lookup.vanilla.map_or(0, |e| e.0);
    let hosting = enshwick_loader_api::host().is_some_and(|host| host.hosting());
    match lookup.purpose {
        Purpose::Cursor => crate::cursor::answer(hosting, shared.copies.copy_count(npc)),
        Purpose::Uniqueness { me, home } => file_copy(shared, npc, found, me.0, home),
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

fn file_copy(shared: &mut Shared, npc: u32, found: u32, me: u32, home: [i64; 3]) -> Answer {
    shared.seen.insert(me, Instant::now());
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
fn keeps(shared: &mut Shared, check: &BedCheck) -> bool {
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

/// Marks a copy staff's use at an altar as consuming, so the game takes one
/// staff in the same transaction as the spawn or move, and only when that
/// works. The staff's effect spawn, which carries the same item, is left
/// alone, and so is a use outside every altar zone, where the copy would not
/// stay.
fn consumes(shared: &mut Shared, summon: &Summon) -> bool {
    let Some(staff) = crate::staffs::copy_staff(summon.item) else {
        return false;
    };
    if summon.ammo != 0 || summon.template != staff.template {
        return false;
    }
    if shared.copies.altar_at(summon.target).is_none() {
        enshwick_loader_api::info!(
            "copy staff of {} used outside every altar zone: kept",
            staff.key
        );
        return false;
    }
    enshwick_loader_api::info!(
        "copy staff of {} used at {:?}: taken on success",
        staff.key,
        summon.target
    );
    true
}

/// Altars move only when someone builds: read their zones now and then, and
/// let go of copies the uniqueness pass no longer sees.
fn refresh(altars: Altars, shared: &mut Shared) {
    if shared.refreshed.is_some_and(|at| at.elapsed() < REFRESH) {
        return;
    }
    shared.refreshed = Some(Instant::now());
    let list = altars.list();
    if !list.is_empty() {
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
    }
    let gone: Vec<u32> = shared
        .seen
        .iter()
        .filter(|(_, at)| at.elapsed() > FORGET_AFTER)
        .map(|(&e, _)| e)
        .collect();
    for entity in gone {
        shared.seen.remove(&entity);
        shared.copies.forget(entity);
    }
}
