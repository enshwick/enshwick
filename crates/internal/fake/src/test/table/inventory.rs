//! The fake's inventory entries: requests, item filters, damage filters,
//! survivors and effects (game events, teleports, spawns and drops).

use crate::test::table::base::args;
use crate::test::table::entries;
use core::ffi::c_void;

use enshwick_core::__fake::ids;

use crate::test::SubmittedRequest;
use crate::test::table::{Registered, with, with_open};

pub(crate) mod containers;

/// A submitted inventory request and where its result goes.
pub(crate) struct OpenRequest {
    pub(crate) request: SubmittedRequest,
    pub(crate) done: Registered<InventoryResultFn>,
}
use enshwick_core::abi::{
    BedCheckFn, DamageCheck, DamageFilterFn, InventoryRequest, InventoryResultFn, ItemFilterFn,
    Side, SpawnRequest, Status, SummonFilterFn, SurvivorLookupFn, SurvivorStatusFn,
    TeleportRequest,
};

/// What the installed fake's inventories answer and have recorded.
#[derive(Default)]
pub(crate) struct InventoryState {
    pub(crate) inventory_requests: Vec<OpenRequest>,
    pub(crate) damage_filters: Vec<Registered<DamageFilterFn>>,
    /// Each mod's survivor lookup, in load order.
    pub(crate) survivor_lookup: Vec<Registered<SurvivorLookupFn>>,
    pub(crate) bed_checks: Vec<Registered<BedCheckFn>>,
    pub(crate) summons: Vec<Registered<SummonFilterFn>>,
    pub(crate) summoned: Vec<Registered<SummonFilterFn>>,
    pub(crate) summon_items: Vec<(enshwick_core::ItemId, enshwick_core::survivors::NpcId)>,
    /// Stat kinds, modifiers and worn items.
    pub(crate) stats: crate::test::inventory::stats::FakeStats,
    /// Each mod's survivor menu merge, in load order.
    pub(crate) survivor_status: Vec<Registered<SurvivorStatusFn>>,
    pub(crate) item_filters: Vec<Registered<ItemFilterFn>>,
    /// The mods' container rules and the pickup messages they showed.
    pub(crate) containers: containers::Containers,
}

/// The fields an inventory request of every version holds, as the loader
/// requires.
const SUBMIT_REQUIRED: usize = core::mem::offset_of!(InventoryRequest, op_count) + 4;

entries! {
    pub(crate) fn item_filter(
        me as Mod,
        callback: Option<ItemFilterFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        with_open(|s| {
            if s.host.inventory.no_item_filters {
                return Status::UNAVAILABLE;
            }
            s.inventory
                .item_filters
                .push(Registered::new(callback, user, me));
            Status::OK
        })
    }

    pub(crate) fn drop_items(
        me as Caller,
        request: *const enshwick_core::abi::DropRequest as args::Headed,
        out: *mut enshwick_core::Entity as args::Out,
    ) -> Status {
        let Some(request) = crate::test::table::get_headed(request) else {
            return Status::BAD_ARGS;
        };
        if out.is_null() {
            return Status::BAD_ARGS;
        }
        let rarity = u8::try_from(request.rarity).ok();
        if !(1..=u32::from(u16::MAX)).contains(&request.count)
            || rarity
                .and_then(enshwick_core::affixes::ItemRarity::from_raw)
                .is_none()
        {
            return Status::BAD_ARGS;
        }
        let entity = with(None, |s| {
            if !s.host.content.items.contains_key(&request.item.get()) {
                return Some(Err(Status::MISSING));
            }
            // The loader checks the position in the spawn after the lookup.
            if !enshwick_core::abi::in_fixed_range(request.position) {
                return Some(Err(Status::BAD_ARGS));
            }
            // Above every spawn's id, so the two never share one.
            let entity = crate::test::table::world::FIRST_SPAWNED
                + 0x1_0000
                + u32::try_from(s.world.drops.len()).unwrap_or(0);
            s.world.drops.push((enshwick_core::Entity(entity), request));
            Some(Ok(entity))
        });
        match entity {
            Some(Ok(entity)) => crate::test::table::put(out, enshwick_core::Entity(entity)),
            Some(Err(status)) => status,
            None => Status::MISSING,
        }
    }

    pub(crate) fn emit(
        me as Caller,
        type_hash: u32,
        [data: *const u8, size: u32] as args::List,
    ) -> Status {
        if data.is_null() {
            return Status::BAD_ARGS;
        }
        if !crate::test::table::on_game_thread() {
            return Status::WRONG_THREAD;
        }
        // SAFETY: the safe layer passes `size` bytes at `data` for the call.
        let bytes = unsafe { core::slice::from_raw_parts(data, size as usize) }.to_vec();
        with(Status::MISSING, |s| {
            if crate::test::table::world::no_world(s) {
                return Status::UNAVAILABLE;
            }
            let step = s.base.ticks.step;
            let notify = s.host.base.type_hashes.get(ids::NOTIFY_EVENT) == Some(&type_hash);
            if notify && !s.world.notified.0.has_room(me, step) {
                return Status::REFUSED;
            }
            if notify {
                s.world.notified.0.count(me, step);
            }
            s.world.emits.push((type_hash, bytes));
            Status::OK
        })
    }

    pub(crate) fn teleport(
        me as Caller,
        request: *const TeleportRequest as args::Headed,
    ) -> Status {
        let Some(request) = crate::test::table::get_headed(request) else {
            return Status::BAD_ARGS;
        };
        with(Status::MISSING, |s| {
            if crate::test::table::world::entity_calls_off(s) {
                return Status::UNAVAILABLE;
            }
            if !enshwick_core::abi::in_fixed_range(request.position) {
                return Status::BAD_ARGS;
            }
            if !s.host.content.templates.contains_key(&request.entity.0)
                && !s
                    .host
                    .world
                    .placed
                    .iter()
                    .any(|(_, p)| p.entity == request.entity)
            {
                return Status::MISSING;
            }
            s.world.teleports.push(request);
            Status::OK
        })
    }

    pub(crate) fn spawn(
        me as Caller,
        request: *const SpawnRequest as args::Headed,
        out: *mut enshwick_core::Entity as args::Out,
    ) -> Status {
        let Some(request) = crate::test::table::get_headed(request) else {
            return Status::BAD_ARGS;
        };
        let entity = with(Err(Status::MISSING), |s| {
            if crate::test::table::world::entity_calls_off(s) {
                return Err(Status::UNAVAILABLE);
            }
            if request.template == enshwick_core::content::Guid::NIL
                || !enshwick_core::abi::in_fixed_range(request.position)
            {
                return Err(Status::BAD_ARGS);
            }
            let entity = crate::test::table::world::FIRST_SPAWNED
                + u32::try_from(s.world.spawns.len()).unwrap_or(0);
            s.world.spawns.push((entity, request));
            s.world.spawn_steps.push(s.base.ticks.step);
            Ok(entity)
        });
        match entity {
            Ok(entity) => crate::test::table::put(out, enshwick_core::Entity(entity)),
            Err(status) => status,
        }
    }

    pub(crate) fn damage_filter(
        me as Mod,
        callback: Option<DamageFilterFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        with_open(|s| {
            s.inventory
                .damage_filters
                .push(Registered::new(callback, user, me));
            Status::OK
        })
    }
}

/// The damage causes the API names, by their ABI number, in the loader's
/// groups: the causes whose sites arm all or none, so a build without one
/// site of a group loses all its causes. The hits come first.
pub(crate) const CAUSE_GROUPS: [&[u32]; 6] = [
    &[DamageCheck::IMPACT],
    &[DamageCheck::FALL_DAMAGE],
    &[DamageCheck::FOG, DamageCheck::DEADLY_FOG],
    &[DamageCheck::SUFFOCATED, DamageCheck::DROWNING_DEATH],
    &[DamageCheck::FREEZING],
    &[DamageCheck::ADMIN_KILL],
];

entries! {
    /// As the loader's: after init, the causes whose sites are not armed. A
    /// filter arms every group the build has whole; stats used in init arm the
    /// hits alone.
    pub(crate) fn damage_causes_off(
        me as AnyMod,
        [out: *mut u32, capacity: u32, count: *mut u32] as args::ListOut,
    ) -> Status {
        with(Status::MISSING, |s| {
            if !s.base.closed() {
                return Status::UNAVAILABLE;
            }
            let filtering = !s.inventory.damage_filters.is_empty();
            let stats = s.inventory.stats.wanted();
            let missing = &s.host.inventory.missing_damage_sites;
            let off: Vec<u32> = CAUSE_GROUPS
                .iter()
                .enumerate()
                .filter(|&(index, group)| {
                    let linked = filtering || (stats && index == 0);
                    !linked || group.iter().any(|cause| missing.contains(cause))
                })
                .flat_map(|(_, group)| group.iter().copied())
                .collect();
            crate::test::table::put_list(&off, out, capacity, count)
        })
    }

    pub(crate) fn survivor_lookup(
        me as Mod,
        callback: Option<SurvivorLookupFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        with_open(|s| {
            hold_each(
                &mut s.inventory.survivor_lookup,
                Registered::new(callback, user, me),
            );
            Status::OK
        })
    }
}

/// As the loader: each mod holds one registration, in load order, and a
/// second replaces its first.
fn hold_each<F: Copy>(held: &mut Vec<Registered<F>>, registered: Registered<F>) {
    match held.iter_mut().find(|r| r.owner() == registered.owner()) {
        Some(own) => *own = registered,
        None => held.push(registered),
    }
}

entries! {
    pub(crate) fn survivor_bed_check(
        me as Mod,
        callback: Option<BedCheckFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        with_open(|s| {
            s.inventory
                .bed_checks
                .push(Registered::new(callback, user, me));
            Status::OK
        })
    }

    pub(crate) fn summon_filter(
        me as Mod,
        callback: Option<SummonFilterFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        with_open(|s| {
            s.inventory
                .summons
                .push(Registered::new(callback, user, me));
            Status::OK
        })
    }

    pub(crate) fn summoned(
        me as Mod,
        callback: Option<SummonFilterFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        with_open(|s| {
            s.inventory
                .summoned
                .push(Registered::new(callback, user, me));
            Status::OK
        })
    }

    pub(crate) fn summon_item(
        me as Mod,
        item: enshwick_core::ItemId,
        survivor: enshwick_core::survivors::NpcId,
    ) -> Status {
        with_open(|s| {
            s.inventory.summon_items.push((item, survivor));
            Status::OK
        })
    }

    pub(crate) fn survivor_status(
        me as Mod,
        callback: Option<SurvivorStatusFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        with_open(|s| {
            if s.host.base.side != Side::CLIENT {
                return Status::UNAVAILABLE;
            }
            hold_each(
                &mut s.inventory.survivor_status,
                Registered::new(callback, user, me),
            );
            Status::OK
        })
    }

    pub(crate) fn request_inventory(me as Caller) -> Status {
        with_open(|s| {
            if s.host.inventory.no_inventories {
                Status::UNAVAILABLE
            } else {
                Status::OK
            }
        })
    }

    pub(crate) fn inventory_submit(
        me as Mod,
        request: *const InventoryRequest as args::HeadedSince<SUBMIT_REQUIRED>,
        done: Option<InventoryResultFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(done) = done else {
            return Status::BAD_ARGS;
        };
        // The loader requires only the fields through `op_count`.
        let required = core::mem::offset_of!(InventoryRequest, op_count) + size_of::<u32>();
        // SAFETY: the safe layer passes null or its version of the request,
        // readable for the size its header states, for the call.
        let Some(request) = (unsafe { enshwick_core::abi::read_headed(request, required) }) else {
            return Status::BAD_ARGS;
        };
        with(Status::MISSING, |s| {
            if s.host.inventory.no_inventories || !s.host.base.serving() {
                return Status::UNAVAILABLE;
            }
            // SAFETY: the request holds `op_count` operations, read during the
            // call.
            let Some(ops) = (unsafe {
                enshwick_core::__fake::copy::list_in(request.ops, request.op_count as usize)
            }) else {
                return Status::BAD_ARGS;
            };
            if enshwick_core::__fake::rules::inventory_checks::refused(ops) {
                return Status::BAD_ARGS;
            }
            let waiting = &mut s.inventory.inventory_requests;
            if waiting.len() >= enshwick_base::__internal::inventory_queue::MAX_QUEUED {
                return Status::BUSY;
            }
            waiting.push(OpenRequest {
                request: SubmittedRequest {
                    ops: ops.to_vec(),
                    flags: request.flags,
                    actor: request.actor.0,
                },
                done: Registered::new(done, user, me),
            });
            Status::OK
        })
    }
}

#[cfg(test)]
mod tests {
    use core::ffi::c_void;

    use enshwick_core::abi::{
        DamageCheck, Header, InventoryOp, InventoryRequest, InventoryResult, ModHandle, Side,
        StatSpec, Status, Str,
    };

    use crate::test::FakeHost;

    unsafe extern "system" fn ignored(result: *const InventoryResult, _: *mut c_void) {
        let _read = crate::test::table::base::io::get_headed(result);
    }

    fn give(count: u32) -> InventoryOp {
        InventoryOp {
            kind: InventoryOp::GIVE,

            target: enshwick_core::Entity(7),

            item: enshwick_core::ItemId::new(3),
            count,
            ..InventoryOp::default()
        }
    }

    fn submit(ops: &[InventoryOp]) -> Status {
        let request = InventoryRequest {
            header: Header::of::<InventoryRequest>(),
            flags: 0,
            actor: enshwick_core::Entity(0),
            ops: ops.as_ptr(),
            op_count: u32::try_from(ops.len()).unwrap_or(u32::MAX),
            reserved: 0,
        };
        super::inventory_submit(
            ModHandle(1),
            &raw const request,
            Some(ignored),
            core::ptr::null_mut(),
        )
    }

    #[test]
    fn the_fake_refuses_the_inventory_requests_the_loader_refuses() {
        let _loader = FakeHost::new().install();
        let too_many = vec![give(1); enshwick_core::inventory::MAX_OPS + 1];
        assert_eq!(
            submit(&too_many),
            Status::BAD_ARGS,
            "more ops than a request holds"
        );
        assert_eq!(submit(&[give(0)]), Status::BAD_ARGS, "a give of nothing");
        assert_eq!(submit(&[give(1)]), Status::OK);
    }

    #[test]
    fn the_fake_answers_busy_past_the_requests_the_loader_lets_wait() {
        use enshwick_base::__internal::inventory_queue::MAX_QUEUED;
        let _loader = FakeHost::new().install();
        for n in 0..MAX_QUEUED {
            assert_eq!(submit(&[give(1)]), Status::OK, "request {n}");
        }
        assert_eq!(submit(&[give(1)]), Status::BUSY);
        assert_eq!(submit(&[give(0)]), Status::BAD_ARGS, "the ops come first");
    }

    unsafe extern "system" fn keep(check: *mut DamageCheck, _: *mut c_void) {
        let _read = crate::test::table::base::io::get_headed(check.cast_const());
    }

    fn close_init() {
        crate::test::table::with((), |s| {
            s.base.registration = crate::test::table::Registration::Closed;
        });
    }

    fn causes_off() -> Vec<u32> {
        let mut out = [0u32; 16];
        let mut count = 0u32;
        let status = super::damage_causes_off(ModHandle(1), out.as_mut_ptr(), 16, &raw mut count);
        assert_eq!(status, Status::OK);
        out[..count as usize].to_vec()
    }

    /// Reads a stat kind that does not exist, which still loads the stats.
    fn use_stats() {
        let mut def = StatSpec {
            header: Header::of::<StatSpec>(),
            ..bytemuck::Zeroable::zeroed()
        };
        let mut id = Str::EMPTY;
        let status = crate::test::inventory::stats::stat_info(
            ModHandle(1),
            u32::MAX,
            &raw mut def,
            &raw mut id,
        );
        assert_eq!(status, Status::MISSING);
    }

    #[test]
    fn a_missing_site_takes_its_whole_group_off_as_on_the_loader() {
        let _loader = FakeHost::new().install();
        crate::test::table::with((), |s| {
            s.host
                .inventory
                .missing_damage_sites
                .push(DamageCheck::SUFFOCATED);
        });
        assert_eq!(
            super::damage_filter(ModHandle(1), Some(keep), core::ptr::null_mut()),
            Status::OK
        );
        close_init();
        assert_eq!(
            causes_off(),
            [DamageCheck::SUFFOCATED, DamageCheck::DROWNING_DEATH]
        );
    }

    #[test]
    fn stats_arm_the_hits_only_when_a_mod_used_them_during_init() {
        let _loader = FakeHost::new().install();
        close_init();
        use_stats();
        let every: Vec<u32> = super::CAUSE_GROUPS.concat();
        assert_eq!(causes_off(), every, "stats read after init arm nothing");
    }

    #[test]
    fn stats_used_during_init_arm_the_hits() {
        let _loader = FakeHost::new().install();
        use_stats();
        close_init();
        assert!(!causes_off().contains(&DamageCheck::IMPACT));
    }

    #[cfg(feature = "unstable")]
    #[test]
    fn the_groups_hold_every_cause_the_api_names_once() {
        let mut grouped = super::CAUSE_GROUPS.concat();
        grouped.sort_unstable();
        let mut named: Vec<u32> = enshwick_core::damage::DamageCause::NAMED
            .iter()
            .map(|cause| cause.to_abi())
            .collect();
        named.sort_unstable();
        assert_eq!(grouped, named);
    }

    #[test]
    fn the_fake_refuses_inventory_requests_on_a_client_that_hosts_nothing() {
        let _loader = FakeHost::new().side(Side::CLIENT).install();
        assert_eq!(submit(&[give(1)]), Status::UNAVAILABLE);
    }

    #[test]
    fn the_fake_checks_the_side_before_the_ops_as_the_loader_does() {
        let _loader = FakeHost::new().side(Side::CLIENT).install();
        let request = InventoryRequest {
            header: Header::of::<InventoryRequest>(),
            flags: 0,
            actor: enshwick_core::Entity(0),
            ops: core::ptr::null(),
            op_count: 3,
            reserved: 0,
        };
        let status = super::inventory_submit(
            ModHandle(1),
            &raw const request,
            Some(ignored),
            core::ptr::null_mut(),
        );
        assert_eq!(status, Status::UNAVAILABLE);
    }
}
