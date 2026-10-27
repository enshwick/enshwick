use enshwick::abi::{InventoryOp, InventoryOpResult, InventoryRequest, InventoryResult};

use crate::memory::{read, read_ptr, write};

use super::{
    ACTOR_FLAG, DESC_AREA, Executor, Finished, NOTIFY_FLAGS, Request, Res, commits, fill,
    renew_give,
};

/// Runs one request as one transaction.
///
/// # Safety
///
/// - The call runs on the inventory system's thread, inside its row loop,
///   while the system runs.
/// - `ctx` is the address of the system's live context and `view` of its
///   view, filled before the row loop.
/// - No other transaction runs on the scratch block during the call.
pub(super) unsafe fn run(
    executor: &Executor,
    ctx: usize,
    view: usize,
    request: &Request,
) -> Finished {
    // SAFETY: the caller's contract makes `view` the system's filled view,
    // which holds the replication resource pointer at the table's
    // `inventory.view_replication`.
    let replication =
        unsafe { read_ptr::<u8>(view, executor.view_replication) }.expose_provenance();
    // SAFETY: this function's own contract, passed on: the system's thread
    // inside its row loop, its live context and its filled view.
    let (mut touched, source) = unsafe { touched(executor, ctx, view, &request.ops) };
    let actor = if request.actor == 0 {
        let mut entity = 0u32;
        // SAFETY: the game's `ecs.row_entity` on the system's live context,
        // on its thread inside the row loop, with our own out word.
        let found = unsafe { (executor.row_entity)(ctx, &raw mut entity) };
        if found.is_null() {
            0
        } else {
            // SAFETY: `ecs.row_entity` returned a non-null (checked earlier)
            // pointer to the row's `u32` entity id, live for the row.
            unsafe { enshwick::raw::read_at(found.cast(), 0) }
        }
    } else {
        request.actor
    };
    let notify = request.flags & InventoryRequest::NOTIFY != 0;
    // SAFETY: this function's own contract, passed on: the system's thread
    // inside its row loop, its live context and its filled view, and the
    // scratch block free.
    unsafe { describe(executor, ctx, view, notify, actor, source) };
    let desc_ptr = core::ptr::with_exposed_provenance_mut::<u8>(executor.scratch);
    let txn_ptr = core::ptr::with_exposed_provenance_mut::<u8>(executor.scratch + DESC_AREA);
    // SAFETY: `txn_ptr` is the scratch block's 16-aligned transaction area of
    // the table's `inventory.txn_size` bytes, free by the caller's contract.
    unsafe { (executor.txn_new)(txn_ptr) };
    // SAFETY: `txn_ptr` holds the transaction `txn_new` just constructed and
    // `desc_ptr` the descriptor `describe` just filled.
    unsafe { (executor.txn_init)(txn_ptr, desc_ptr) };
    let allow_partial = request.flags & InventoryRequest::ALLOW_PARTIAL != 0;
    let mut results = Vec::with_capacity(request.ops.len());
    for op in &request.ops {
        // SAFETY: `txn_ptr` holds the transaction `txn_init` initialised, on
        // the inventory system's thread inside its row loop.
        let result = unsafe { apply(executor, txn_ptr, op) };
        results.push(result);
        if result.error != 0 || (!allow_partial && result.remaining != 0) {
            break;
        }
    }
    let committed = commits(&results, request.ops.len(), allow_partial);
    results.resize(
        request.ops.len(),
        InventoryOpResult {
            error: InventoryOpResult::NOT_RUN,
            remaining: 0,
        },
    );
    if committed {
        // SAFETY: `txn_ptr` holds the initialised transaction, on the
        // inventory system's thread inside its row loop; commit consumes the
        // op log.
        unsafe { (executor.commit)(txn_ptr) };
    } else {
        // SAFETY: `txn_ptr` holds the initialised transaction, on the
        // inventory system's thread inside its row loop; rollback restores
        // every slot the ops wrote.
        unsafe { (executor.rollback)(txn_ptr) };
    }
    // SAFETY: `txn_ptr` holds the transaction, committed or rolled back and
    // not used again before the next `txn_new`.
    unsafe { (executor.txn_drop)(txn_ptr) };
    touched.sort_unstable();
    touched.dedup();
    if replication != 0 {
        for &entity in touched.iter().filter(|&&e| e != 0) {
            // SAFETY: `replication` is the system's own non-zero (checked
            // earlier) replication resource from its view, used on its thread as
            // vanilla resends after a transfer.
            unsafe { (executor.resend)(replication, entity) };
        }
    }
    Finished {
        owner: request.owner,
        done: request.done,
        user: request.user,
        outcome: if committed {
            InventoryResult::COMMITTED
        } else {
            InventoryResult::ROLLED_BACK
        },
        results,
    }
}

/// The inventories the operations touch at the root, for the resend, and
/// the source of the first move, for the descriptor.
///
/// # Safety
///
/// - The call runs on the inventory system's thread, inside its row loop,
///   while the system runs.
/// - `ctx` is the address of the system's live context and `view` of its
///   view, filled before the row loop.
#[expect(clippy::cast_possible_truncation, reason = "checked entity ids")]
pub(super) unsafe fn touched(
    executor: &Executor,
    ctx: usize,
    view: usize,
    ops: &[InventoryOp],
) -> (Vec<u32>, u32) {
    // SAFETY: the caller's contract makes `view` the system's filled view,
    // which holds its `u64` owners handle at the table's
    // `inventory.view_owners`.
    let owners: u64 = unsafe { read(view, executor.view_owners) };
    let root = |slot: u64| {
        let mut out: Res = [0; 2];
        // SAFETY: the game's walk up `OwnerRelationship` at the table's
        // `ecs.slot_root`, with the view's owners handle and the system's live
        // context, on its thread inside the row loop, into our own record.
        unsafe { (executor.slot_root)(&raw mut out, slot, owners, ctx) };
        let (error, root) = (out[0] as u8, out[1] as u32);
        if error == 0 { root } else { 0 }
    };
    let mut touched: Vec<u32> = Vec::with_capacity(ops.len() * 2);
    let mut source = 0u32;
    for op in ops {
        let from = match op.kind {
            InventoryOp::GIVE | InventoryOp::TAKE => {
                touched.push(op.target as u32);
                continue;
            }
            InventoryOp::DESTROY_SLOT => {
                touched.push(root(op.target));
                continue;
            }
            InventoryOp::RENEW => {
                touched.extend([root(op.target), (op.other & 0xffff_ffff) as u32]);
                continue;
            }
            InventoryOp::MOVE_SLOT => root(op.target),
            _ => op.target as u32,
        };
        touched.extend([from, op.other as u32]);
        if source == 0 {
            source = from;
        }
    }
    (touched, source)
}

/// Fills the descriptor in the scratch block as the game's transfer handler
/// does: zeroed, the view's handles, the flags, context, actor and source.
///
/// # Safety
///
/// - The call runs on the inventory system's thread, inside its row loop,
///   while the system runs.
/// - `ctx` is the address of the system's live context and `view` of its
///   view, filled before the row loop.
/// - No other transaction runs on the scratch block during the call.
pub(super) unsafe fn describe(
    executor: &Executor,
    ctx: usize,
    view: usize,
    notify: bool,
    actor: u32,
    source: u32,
) {
    let desc = executor.scratch;
    // SAFETY: the scratch block, free by the caller's contract, holds
    // `DESC_AREA` bytes from `desc`, at least the table's
    // `inventory.desc_size` that `desc_zero` clears.
    unsafe { (executor.desc_zero)(core::ptr::with_exposed_provenance_mut(desc)) };
    let mut flags = if notify { NOTIFY_FLAGS } else { 0 };
    // SAFETY: the caller's contract makes `view` the system's filled view,
    // which holds its `u64` actors handle at the table's
    // `inventory.view_actors`.
    let actors: u64 = unsafe { read(view, executor.view_actors) };
    // SAFETY: the game's actor check at the table's `inventory.actor_flag`,
    // with the system's live context and the view's actors handle, called as
    // its transfer handler calls it, on the system's thread.
    if unsafe { (executor.actor_flag)(ctx, actors, actor) } != 0 {
        flags |= ACTOR_FLAG;
    }
    for &(at, from) in &executor.copies {
        if at + 8 <= executor.desc_size {
            // SAFETY: the caller's contract makes `view` the system's filled
            // view, which holds a `u64` handle at the table's
            // `inventory.copy_*.view` offset `from`.
            let value: u64 = unsafe { read(view, from) };
            // SAFETY: the eight bytes at `at` lie inside the descriptor's
            // `desc_size` bytes (checked earlier), in the free scratch block.
            unsafe { write(desc, at, value) };
        }
    }
    // SAFETY: the descriptor in the free scratch block starts with its
    // one-byte flags.
    unsafe { write(desc, 0, flags) };
    // SAFETY: the descriptor in the free scratch block keeps the context at
    // the table's `inventory.desc_context`, inside its `DESC_AREA` bytes.
    unsafe { write(desc, executor.desc_context, ctx) };
    // SAFETY: the descriptor in the free scratch block keeps the `u32` actor
    // at the table's `inventory.desc_actor`, inside its `DESC_AREA` bytes.
    unsafe { write(desc, executor.desc_actor, actor) };
    // SAFETY: the descriptor in the free scratch block keeps the `u32` source
    // at the table's `inventory.desc_other`, inside its `DESC_AREA` bytes.
    unsafe { write(desc, executor.desc_other, source) };
}

/// Runs one operation in the transaction.
///
/// # Safety
///
/// - `txn` holds a transaction `txn_init` initialised and not yet committed,
///   rolled back or dropped.
/// - The call runs on the inventory system's thread, inside its row loop,
///   while the system runs.
#[expect(clippy::cast_possible_truncation, reason = "checked by `valid`")]
pub(super) unsafe fn apply(
    executor: &Executor,
    txn: *mut u8,
    op: &InventoryOp,
) -> InventoryOpResult {
    let mut res: Res = [0; 2];
    let out = &raw mut res;
    let absent = [0u8; 16];
    match op.kind {
        InventoryOp::GIVE => {
            // `valid` refused a fill out of range.
            let fill = fill(op).unwrap_or(absent);
            // SAFETY: the game's add at the table's `inventory.give` on the
            // caller's live transaction, with our own result record and
            // options, no level or rarity; its last option is the new items'
            // fuel fill, which it puts on each new item's entity.
            unsafe {
                (executor.give)(
                    out,
                    txn,
                    op.target as u32,
                    0,
                    op.item.get(),
                    op.count.cast_signed(),
                    &raw const absent,
                    &raw const absent,
                    &raw const fill,
                )
            };
        }
        InventoryOp::TAKE => {
            // SAFETY: the game's consume at the table's `inventory.take`
            // across the sub-inventories, on the caller's live transaction,
            // with our own result record.
            unsafe { (executor.take)(out, txn, op.target as u32, op.item.get(), 0, op.count) };
        }
        InventoryOp::DESTROY_SLOT => {
            // SAFETY: the game's destroy of one stack at the table's
            // `inventory.destroy_slot`, on the caller's live transaction,
            // with our own result record.
            unsafe { (executor.destroy)(out, txn, op.target) };
        }
        InventoryOp::RENEW => {
            // SAFETY: the game's destroy of one stack at the table's
            // `inventory.destroy_slot`, on the caller's live transaction,
            // with our own result record.
            unsafe { (executor.destroy)(out, txn, op.target) };
            if res[0] as u8 == 0 {
                let give = renew_give(op);
                let fill = fill(&give).unwrap_or(absent);
                // SAFETY: the game's add at the table's `inventory.give` on
                // the caller's live transaction, asked for the slot just
                // emptied, with our own result record and options.
                unsafe {
                    (executor.give)(
                        out,
                        txn,
                        give.target as u32,
                        op.target,
                        give.item.get(),
                        give.count.cast_signed(),
                        &raw const absent,
                        &raw const absent,
                        &raw const fill,
                    )
                };
            }
        }
        InventoryOp::MOVE_SLOT if op.count == 0 => {
            // SAFETY: the game's move of the whole stack at the table's
            // `inventory.move_slot`, on the caller's live transaction, with
            // our own result record; a remainder goes back to the source.
            unsafe { (executor.move_slot)(out, txn, op.target, op.other as u32, 0, 1, 1, 1) };
        }
        InventoryOp::MOVE_SLOT => {
            // SAFETY: the game's move of `count` of the stack at the table's
            // `inventory.move_amount`, on the caller's live transaction, with
            // our own result record; a remainder goes back to the source.
            unsafe {
                (executor.move_amount)(
                    out,
                    txn,
                    op.target,
                    op.other as u32,
                    op.count as u16,
                    1,
                    1,
                    1,
                );
            };
        }
        _ => {
            let special = u8::from(op.flags & InventoryOp::INCLUDE_SPECIAL != 0);
            // SAFETY: the game's take-all at the table's
            // `inventory.move_all`, on the caller's live transaction, with our
            // own result record; remainders stay in the source.
            unsafe {
                (executor.move_all)(out, txn, op.target as u32, op.other as u32, 1, special);
            };
        }
    }
    InventoryOpResult {
        error: u32::from(res[0] as u8),
        remaining: res[1] as u32,
    }
}
