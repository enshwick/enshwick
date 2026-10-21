//! Inventory transactions for mods, run inside the game's inventory system.
//!
//! The game changes inventories through a transaction object that needs a
//! live system context and the component handles of a system's view, so a
//! mod's request cannot run between steps. Requests are queued from any
//! thread; a loader mid-function hook at the per-row loop head of
//! `inventory_actions`, where `r13` holds the system context and
//! `rbp - 0x60` the filled view, claims them one at a time and runs each as
//! the game's own transfer handler does: descriptor from the view, ops,
//! commit only when every op succeeded, otherwise rollback, then a resend of
//! every inventory touched. Results go back on the game thread after the
//! step.
//!
//! The rows are players, so with nobody online nothing runs; requests that
//! wait too long are answered as timed out.
//!
//! On a client that hosts a game the same system runs in the server's world;
//! the hook skips contexts of any other world, and requests are refused
//! while the client hosts nothing.

use core::ffi::c_void;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use enshwick_loader_api::abi::{
    Header, HookFlags, InventoryOp, InventoryOpResult, InventoryRequest, InventoryResult,
    InventoryResultFn, ItemData, Registers, Side, Status,
};

use crate::chains::LOADER;
use crate::host::Loader;
use crate::mid::MidLink;

/// The most operations one request may hold; the game's op log takes 0x200
/// records and an operation writes one per slot it changes.
pub(crate) const MAX_OPS: usize = 64;
/// How long a request may wait for the inventory system.
const TIMEOUT: Duration = Duration::from_secs(30);
/// Bytes kept for the descriptor before the transaction, which is 16-aligned.
const DESC_AREA: usize = 0x100;
/// Descriptor flags: emit `PickupEvent` and `InventoryFull`.
const NOTIFY_FLAGS: u8 = 1 | 4;
/// Descriptor flag the game sets for some actors.
const ACTOR_FLAG: u8 = 0x20;

type DescZero = unsafe extern "system" fn(desc: *mut u8);
type TxnFn = unsafe extern "system" fn(txn: *mut u8);
type TxnInit = unsafe extern "system" fn(txn: *mut u8, desc: *const u8);
type Res = [u64; 2];
type Give = unsafe extern "system" fn(
    res: *mut Res,
    txn: *mut u8,
    inventory: u32,
    hint: u64,
    item: u32,
    count: i32,
    level: *const [u8; 16],
    rarity: *const [u8; 16],
    extra: *const [u8; 16],
) -> *mut Res;
type Take = unsafe extern "system" fn(
    res: *mut Res,
    txn: *mut u8,
    inventory: u32,
    item: u32,
    hint: u64,
    count: u32,
) -> *mut Res;
type Destroy = unsafe extern "system" fn(res: *mut Res, txn: *mut u8, slot: u64) -> *mut Res;
type MoveSlot = unsafe extern "system" fn(
    res: *mut Res,
    txn: *mut u8,
    slot: u64,
    to: u32,
    mode: u32,
    remainder: i32,
    a: u8,
    b: u8,
) -> *mut Res;
type MoveAmount = unsafe extern "system" fn(
    res: *mut Res,
    txn: *mut u8,
    slot: u64,
    to: u32,
    amount: u16,
    remainder: i32,
    a: u8,
    b: u8,
) -> *mut Res;
type MoveAll = unsafe extern "system" fn(
    res: *mut Res,
    txn: *mut u8,
    from: u32,
    to: u32,
    remainder: i32,
    special: u8,
) -> *mut Res;
type Resend = unsafe extern "system" fn(replication: usize, entity: u32);
type SlotRoot =
    unsafe extern "system" fn(out: *mut Res, slot: u64, owners: u64, ctx: usize) -> *mut Res;
type RowEntity = unsafe extern "system" fn(ctx: usize, out: *mut u32) -> *const u32;
type ActorFlag = unsafe extern "system" fn(ctx: usize, actors: u64, actor: u32) -> u8;
type ItemLookup = unsafe extern "system" fn(item: u32, registry: usize) -> *const u8;

/// The game functions and offsets the executor uses, from the build table.
#[derive(Debug)]
struct Executor {
    site: u64,
    desc_zero: DescZero,
    txn_new: TxnFn,
    txn_init: TxnInit,
    commit: TxnFn,
    rollback: TxnFn,
    txn_drop: TxnFn,
    give: Give,
    take: Take,
    destroy: Destroy,
    move_slot: MoveSlot,
    move_amount: MoveAmount,
    move_all: MoveAll,
    resend: Resend,
    slot_root: SlotRoot,
    row_entity: RowEntity,
    actor_flag: ActorFlag,
    item_lookup: ItemLookup,
    view_frame: usize,
    view_owners: usize,
    view_actors: usize,
    view_replication: usize,
    view_item_registry: usize,
    desc_size: usize,
    desc_context: usize,
    desc_actor: usize,
    desc_other: usize,
    /// `(desc, view)` offsets of the fields copied from the view.
    copies: Vec<(usize, usize)>,
    item_max_stack: usize,
    item_flags: usize,
    item_category: usize,
    item_rarity: usize,
    /// The descriptor and the transaction, allocated once; one transaction
    /// runs at a time.
    scratch: usize,
    /// On a client the system also runs outside the hosted server's world:
    /// check each context's world.
    check_world: bool,
    /// Where a context's descriptor holds its entity world.
    entity_world: usize,
}

/// A queued request.
#[derive(Debug)]
struct Request {
    owner: usize,
    flags: u32,
    actor: u32,
    ops: Vec<InventoryOp>,
    done: InventoryResultFn,
    user: usize,
    queued: Instant,
}

/// A request's result, waiting for the game thread.
#[derive(Debug)]
struct Finished {
    owner: usize,
    done: InventoryResultFn,
    user: usize,
    outcome: u32,
    results: Vec<InventoryOpResult>,
}

impl Finished {
    fn without_run(request: &Request, outcome: u32) -> Self {
        Self {
            owner: request.owner,
            done: request.done,
            user: request.user,
            outcome,
            results: Vec::new(),
        }
    }
}

/// Requests in order; `pending` is set while it holds any, so a row with
/// nothing to do takes one atomic load.
#[derive(Debug, Default)]
struct Queue {
    pending: AtomicBool,
    requests: Mutex<VecDeque<Request>>,
}

impl Queue {
    const fn new() -> Self {
        Self {
            pending: AtomicBool::new(false),
            requests: Mutex::new(VecDeque::new()),
        }
    }

    fn push(&self, request: Request) {
        let mut requests = self.requests.lock().unwrap_or_else(PoisonError::into_inner);
        requests.push_back(request);
        self.pending.store(true, Ordering::Release);
    }

    /// Takes the oldest request; each is handed out exactly once, whichever
    /// thread asks.
    fn claim(&self) -> Option<Request> {
        if !self.pending.load(Ordering::Acquire) {
            return None;
        }
        let mut requests = self.requests.lock().unwrap_or_else(PoisonError::into_inner);
        let request = requests.pop_front();
        self.pending.store(!requests.is_empty(), Ordering::Release);
        request
    }

    /// Removes the requests for which `expired` holds.
    fn drain(&self, mut expired: impl FnMut(&Request) -> bool) -> Vec<Request> {
        if !self.pending.load(Ordering::Acquire) {
            return Vec::new();
        }
        let mut requests = self.requests.lock().unwrap_or_else(PoisonError::into_inner);
        let mut gone = Vec::new();
        requests.retain_mut(|r| {
            let late = expired(r);
            if late {
                gone.push(Request {
                    ops: std::mem::take(&mut r.ops),
                    ..*r
                });
            }
            !late
        });
        self.pending.store(!requests.is_empty(), Ordering::Release);
        gone
    }
}

/// Holds the right to run a transaction; one at a time.
struct Turn<'a>(&'a AtomicBool);

impl<'a> Turn<'a> {
    fn take(busy: &'a AtomicBool) -> Option<Self> {
        busy.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| Self(busy))
    }
}

impl Drop for Turn<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

static EXECUTOR: OnceLock<Option<Executor>> = OnceLock::new();
static QUEUE: Queue = Queue::new();
static BUSY: AtomicBool = AtomicBool::new(false);
static FINISHED: Mutex<Vec<Finished>> = Mutex::new(Vec::new());
/// A mod asked for inventories during init.
static WANTED: AtomicBool = AtomicBool::new(false);
/// The row hook is in place.
static ARMED: AtomicBool = AtomicBool::new(false);
/// The arming is over, armed or not.
static SETTLED: AtomicBool = AtomicBool::new(false);
/// The item registry the inventory system last saw.
static REGISTRY: AtomicUsize = AtomicUsize::new(0);

fn executor(loader: &Loader) -> Option<&'static Executor> {
    EXECUTOR.get_or_init(|| build(loader)).as_ref()
}

/// Turns a resolved address into a function pointer of type `F`.
///
/// # Safety
///
/// `address` is a function of type `F`.
unsafe fn function<F: Copy>(address: u64) -> Option<F> {
    let address = usize::try_from(address).ok()?;
    // SAFETY: the caller guarantees the type; `F` is a function pointer the
    // size of `usize`.
    (address != 0).then(|| unsafe { core::mem::transmute_copy::<usize, F>(&address) })
}

fn build(loader: &Loader) -> Option<Executor> {
    let symbol = |name: &str| loader.build.symbol(name).map(|rva| loader.base + rva);
    let constant = |name: &str| {
        loader
            .build
            .constant(name)
            .and_then(|v| usize::try_from(v).ok())
    };
    if loader.build.lacks_any("inventory.copy_") {
        return None;
    }
    let copies: Vec<(usize, usize)> = loader
        .build
        .constants()
        .iter()
        .filter(|(name, _)| name.starts_with("inventory.copy_"))
        .filter_map(|&(_, packed)| {
            Some((
                usize::try_from(packed >> 32).ok()?,
                usize::try_from(packed & 0xffff_ffff).ok()?,
            ))
        })
        .collect();
    let field = |name: &str| {
        let offset = loader
            .reflection
            .field_offset("keen::ItemInfo", name)
            .ok()?;
        usize::try_from(offset).ok()
    };
    let desc_size = constant("inventory.desc_size")?;
    let txn_size = constant("inventory.txn_size")?;
    if copies.is_empty() || desc_size > DESC_AREA {
        return None;
    }
    // SAFETY: the build table pins `inventory.desc_zero` to this signature.
    let desc_zero = unsafe { function(symbol("inventory.desc_zero")?) }?;
    // SAFETY: the build table pins `inventory.txn_new` to this signature.
    let txn_new = unsafe { function(symbol("inventory.txn_new")?) }?;
    // SAFETY: the build table pins `inventory.txn_init` to this signature.
    let txn_init = unsafe { function(symbol("inventory.txn_init")?) }?;
    // SAFETY: the build table pins `inventory.txn_commit` to this signature.
    let commit = unsafe { function(symbol("inventory.txn_commit")?) }?;
    // SAFETY: the build table pins `inventory.txn_rollback` to this signature.
    let rollback = unsafe { function(symbol("inventory.txn_rollback")?) }?;
    // SAFETY: the build table pins `inventory.txn_drop` to this signature.
    let txn_drop = unsafe { function(symbol("inventory.txn_drop")?) }?;
    // SAFETY: the build table pins `inventory.give` to this signature.
    let give = unsafe { function(symbol("inventory.give")?) }?;
    // SAFETY: the build table pins `inventory.take` to this signature.
    let take = unsafe { function(symbol("inventory.take")?) }?;
    // SAFETY: the build table pins `inventory.destroy_slot` to this signature.
    let destroy = unsafe { function(symbol("inventory.destroy_slot")?) }?;
    // SAFETY: the build table pins `inventory.move_slot` to this signature.
    let move_slot = unsafe { function(symbol("inventory.move_slot")?) }?;
    // SAFETY: the build table pins `inventory.move_amount` to this signature.
    let move_amount = unsafe { function(symbol("inventory.move_amount")?) }?;
    // SAFETY: the build table pins `inventory.move_all` to this signature.
    let move_all = unsafe { function(symbol("inventory.move_all")?) }?;
    // SAFETY: the build table pins `inventory.resend` to this signature.
    let resend = unsafe { function(symbol("inventory.resend")?) }?;
    // SAFETY: the build table pins `ecs.slot_root` to this signature.
    let slot_root = unsafe { function(symbol("ecs.slot_root")?) }?;
    // SAFETY: the build table pins `ecs.row_entity` to this signature.
    let row_entity = unsafe { function(symbol("ecs.row_entity")?) }?;
    // SAFETY: the build table pins `inventory.actor_flag` to this signature.
    let actor_flag = unsafe { function(symbol("inventory.actor_flag")?) }?;
    // SAFETY: the build table pins `item.info` to this signature.
    let item_lookup = unsafe { function(symbol("item.info")?) }?;
    let executor = Executor {
        site: symbol("inventory.actions_row")?,
        desc_zero,
        txn_new,
        txn_init,
        commit,
        rollback,
        txn_drop,
        give,
        take,
        destroy,
        move_slot,
        move_amount,
        move_all,
        resend,
        slot_root,
        row_entity,
        actor_flag,
        item_lookup,
        view_frame: constant("inventory.view_frame")?,
        view_owners: constant("inventory.view_owners")?,
        view_actors: constant("inventory.view_actors")?,
        view_replication: constant("inventory.view_replication")?,
        view_item_registry: constant("inventory.view_item_registry")?,
        desc_size,
        desc_context: constant("inventory.desc_context")?,
        desc_actor: constant("inventory.desc_actor")?,
        desc_other: constant("inventory.desc_other")?,
        copies,
        item_max_stack: field("maxStackSize")?,
        item_flags: field("flags")?,
        item_category: field("category")?,
        item_rarity: field("rarity")?,
        scratch: 0,
        check_world: loader.side != Side::SERVER,
        entity_world: constant("ecs.context_entity_world")?,
    };
    checked(loader, executor, txn_size)
}

/// `executor` with its scratch block, when the game code at the hook reads
/// as the build table says.
fn checked(loader: &Loader, executor: Executor, txn_size: usize) -> Option<Executor> {
    if !code_matches(loader, "inventory.actions_row", "inventory.row_bytes")
        || !code_matches(loader, "inventory.view_fill", "inventory.view_fill_bytes")
    {
        crate::log!(
            WARN,
            "inventories off: the inventory system's code differs from the build table"
        );
        return None;
    }
    let layout = std::alloc::Layout::from_size_align(DESC_AREA + txn_size, 16).ok()?;
    // SAFETY: a non-zero size; the block is never freed.
    let scratch = unsafe { std::alloc::alloc_zeroed(layout) };
    if scratch.is_null() {
        return None;
    }
    Some(Executor {
        scratch: scratch.expose_provenance(),
        ..executor
    })
}

/// Whether the 8 code bytes at the symbol `at` are the constant `bytes`.
fn code_matches(loader: &Loader, at: &str, bytes: &str) -> bool {
    let (Some(rva), Some(expected)) = (loader.build.symbol(at), loader.build.constant(bytes))
    else {
        return false;
    };
    let Ok(address) = usize::try_from(loader.base + rva) else {
        return false;
    };
    // SAFETY: an address inside the game's image, which stays mapped.
    let found = unsafe { read::<u64>(address) };
    found == expected
}

/// Reads a `T` at `address`.
///
/// # Safety
///
/// The address holds a readable `T`.
unsafe fn read<T: Copy>(address: usize) -> T {
    // SAFETY: forwarded from the caller.
    unsafe { core::ptr::with_exposed_provenance::<T>(address).read_unaligned() }
}

/// Writes `value` at `address`.
///
/// # Safety
///
/// The address holds a writable `T`.
unsafe fn write<T: Copy>(address: usize, value: T) {
    // SAFETY: forwarded from the caller.
    unsafe { core::ptr::with_exposed_provenance_mut::<T>(address).write_unaligned(value) };
}

/// A mod asks for inventories: allowed during init, on a build that has them.
pub(crate) fn open(loader: &Loader) -> Status {
    if loader.delivery.get().is_some() {
        return Status::TOO_LATE;
    }
    if executor(loader).is_none() {
        return Status::MISSING;
    }
    WANTED.store(true, Ordering::Release);
    Status::OK
}

/// Links the row hook when a mod asked for inventories.
pub(crate) fn link(loader: &Loader) {
    if !WANTED.load(Ordering::Acquire) {
        return;
    }
    let Some(executor) = executor(loader) else {
        return;
    };
    let link = MidLink {
        owner: LOADER,
        handler: row,
        user: 0,
        priority: i32::MIN,
        flags: HookFlags::NONE,
    };
    let status = loader
        .mids
        .lock()
        .map_or(Status::REFUSED, |mut mids| mids.add(executor.site, link));
    if !status.is_ok() {
        crate::log!(
            WARN,
            "inventories off: the row hook was refused ({})",
            status.0
        );
    }
}

/// Records whether the row hook got armed; `failed` are the mid-function
/// addresses that did not.
pub(crate) fn settle(loader: &Loader, failed: impl IntoIterator<Item = u64>) {
    let site = executor(loader).map(|e| e.site);
    let armed = WANTED.load(Ordering::Acquire)
        && site.is_some_and(|site| failed.into_iter().all(|f| f != site));
    ARMED.store(armed, Ordering::Release);
    SETTLED.store(true, Ordering::Release);
    if WANTED.load(Ordering::Acquire) && !armed {
        crate::log!(WARN, "inventories off: the row hook is not armed");
        let gone = QUEUE.drain(|_| true);
        finish(
            gone.iter()
                .map(|r| Finished::without_run(r, InventoryResult::UNAVAILABLE)),
        );
    }
}

/// A hosted game ended: its item registry is gone, and the requests still
/// queued for its world are answered as unavailable.
pub(crate) fn forget() {
    REGISTRY.store(0, Ordering::Release);
    let gone = QUEUE.drain(|_| true);
    finish(
        gone.iter()
            .map(|r| Finished::without_run(r, InventoryResult::UNAVAILABLE)),
    );
    deliver();
}

fn finish(results: impl IntoIterator<Item = Finished>) {
    FINISHED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .extend(results);
}

/// Queues a request of the mod `owner`.
pub(crate) fn submit(
    loader: &Loader,
    owner: usize,
    request: &InventoryRequest,
    done: InventoryResultFn,
    user: *mut c_void,
) -> Status {
    if executor(loader).is_none() || !WANTED.load(Ordering::Acquire) {
        return Status::MISSING;
    }
    if SETTLED.load(Ordering::Acquire) && !ARMED.load(Ordering::Acquire) {
        return Status::MISSING;
    }
    if loader.side != Side::SERVER && !crate::world::hosting() {
        return Status::MISSING;
    }
    let count = request.op_count as usize;
    if request.ops.is_null() || count == 0 || count > MAX_OPS {
        return Status::BAD_ARGS;
    }
    // SAFETY: the mod passes `op_count` operations, read during the call.
    let ops = unsafe { core::slice::from_raw_parts(request.ops, count) }.to_vec();
    if ops.iter().any(|op| !valid(op)) {
        return Status::BAD_ARGS;
    }
    QUEUE.push(Request {
        owner,
        flags: request.flags,
        actor: request.actor,
        ops,
        done,
        user: user.expose_provenance(),
        queued: Instant::now(),
    });
    Status::OK
}

/// Whether an operation names what its kind needs.
fn valid(op: &InventoryOp) -> bool {
    let entity = |v: u64| u32::try_from(v).is_ok_and(|v| v != 0);
    match op.kind {
        InventoryOp::GIVE | InventoryOp::TAKE => {
            entity(op.target)
                && op.item != 0
                && op.count != 0
                && i32::try_from(op.count).is_ok()
                && (op.kind == InventoryOp::TAKE || fill(op).is_some())
        }
        InventoryOp::DESTROY_SLOT => op.target & 0xffff_ffff != 0,
        InventoryOp::RENEW => {
            op.target & 0xffff_ffff != 0
                && op.other & 0xffff_ffff != 0
                && op.item != 0
                && op.count != 0
                && i32::try_from(op.count).is_ok()
                && fill(&renew_give(op)).is_some()
        }
        InventoryOp::MOVE_SLOT => {
            op.target & 0xffff_ffff != 0 && entity(op.other) && u16::try_from(op.count).is_ok()
        }
        InventoryOp::MOVE_ALL => entity(op.target) && entity(op.other) && op.target != op.other,
        _ => false,
    }
}

/// The give a renewal makes: its inventory and fill from `other`.
fn renew_give(op: &InventoryOp) -> InventoryOp {
    InventoryOp {
        kind: InventoryOp::GIVE,
        flags: InventoryOp::FILLED,
        target: op.other & 0xffff_ffff,
        other: op.other >> 32,
        item: op.item,
        count: op.count,
    }
}

/// The game's optional fill of a give: the fill as `f32`, then a byte that
/// says it is there; all zero for none. `None` for a fill outside 0 to 1.
fn fill(op: &InventoryOp) -> Option<[u8; 16]> {
    let mut option = [0u8; 16];
    if op.flags & InventoryOp::FILLED == 0 {
        return Some(option);
    }
    let fill = f32::from_bits(u32::try_from(op.other).ok()?);
    if !(0.0..=1.0).contains(&fill) {
        return None;
    }
    option[..4].copy_from_slice(&fill.to_le_bytes());
    option[4] = 1;
    Some(option)
}

/// The static data of `item`, once the inventory system has run.
pub(crate) fn item_info(loader: &Loader, item: u32) -> Result<ItemData, Status> {
    let executor = executor(loader).ok_or(Status::MISSING)?;
    let registry = REGISTRY.load(Ordering::Acquire);
    if registry == 0 || item == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: the game's lookup in its item registry, a hash map read.
    let info = unsafe { (executor.item_lookup)(item, registry) };
    if info.is_null() {
        return Err(Status::MISSING);
    }
    let info = info.addr();
    Ok(ItemData {
        header: Header::of::<ItemData>(),
        id: item,
        // SAFETY: a live ItemInfo holds these fields.
        max_stack: unsafe { read(info + executor.item_max_stack) },
        // SAFETY: as above.
        category: unsafe { read(info + executor.item_category) },
        // SAFETY: as above.
        rarity: unsafe { read(info + executor.item_rarity) },
        // SAFETY: as above.
        flags: unsafe { read(info + executor.item_flags) },
    })
}

/// Answers the requests that waited too long and hands out the results; on
/// the game thread after each step.
pub(crate) fn deliver() {
    let now = Instant::now();
    let late = QUEUE.drain(|r| now.duration_since(r.queued) > TIMEOUT);
    if !late.is_empty() {
        finish(
            late.iter()
                .map(|r| Finished::without_run(r, InventoryResult::TIMED_OUT)),
        );
    }
    let finished = {
        let mut finished = FINISHED.lock().unwrap_or_else(PoisonError::into_inner);
        if finished.is_empty() {
            return;
        }
        std::mem::take(&mut *finished)
    };
    for result in finished {
        if crate::host::is_off(result.owner) {
            continue;
        }
        let data = InventoryResult {
            header: Header::of::<InventoryResult>(),
            outcome: result.outcome,
            op_count: u32::try_from(result.results.len()).unwrap_or(0),
            ops: result.results.as_ptr(),
        };
        // SAFETY: the mod's callback with its pointer; the safe layer catches
        // its panics. The result lives for the call.
        crate::profile::call(
            result.owner,
            crate::profile::Feature::Inventory,
            || unsafe {
                (result.done)(
                    &raw const data,
                    core::ptr::with_exposed_provenance_mut(result.user),
                );
            },
        );
    }
}

/// Whether a transaction with these results is committed.
fn commits(results: &[InventoryOpResult], count: usize, allow_partial: bool) -> bool {
    results.len() == count
        && results
            .iter()
            .all(|r| r.error == 0 && (allow_partial || r.remaining == 0))
}

/// The switch of the inventory requests.
static INVENTORY_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("inventory requests");

/// The row hook: runs the queued requests inside the inventory system.
unsafe extern "system" fn row(registers: *mut Registers, _user: *mut c_void) {
    // SAFETY: the stub passes its saved registers, valid for the call.
    let Some(registers) = (unsafe { registers.as_ref() }) else {
        return;
    };
    let Some(executor) = EXECUTOR.get().and_then(Option::as_ref) else {
        return;
    };
    let ctx = usize::try_from(registers.r13).unwrap_or(0);
    // SAFETY: the system's live context, on its thread.
    if executor.check_world && !unsafe { in_server_world(executor, ctx) } {
        return;
    }
    let view = usize::try_from(registers.rbp)
        .unwrap_or(0)
        .wrapping_sub(executor.view_frame);
    // SAFETY: the view is filled before the loop and lives on the frame.
    let registry: usize = unsafe { read(view + executor.view_item_registry) };
    REGISTRY.store(registry, Ordering::Release);
    if !QUEUE.pending.load(Ordering::Acquire) {
        return;
    }
    let Some(_turn) = Turn::take(&BUSY) else {
        return;
    };
    let _outcome = INVENTORY_FUSE.run("the row", || {
        while let Some(request) = QUEUE.claim() {
            if crate::host::is_off(request.owner) {
                continue;
            }
            // SAFETY: inside the inventory system, with its context and view.
            let finished = unsafe { run(executor, ctx, view, &request) };
            finish([finished]);
        }
    });
}

/// Whether `ctx` is a context of the hosted server's world.
///
/// # Safety
///
/// `ctx` is a live system context, used on its thread.
unsafe fn in_server_world(executor: &Executor, ctx: usize) -> bool {
    let server = crate::world::server_world();
    if server == 0 || ctx == 0 {
        return false;
    }
    // SAFETY: a context starts with its query descriptor.
    let descriptor: usize = unsafe { read(ctx) };
    if descriptor == 0 {
        return false;
    }
    // SAFETY: the descriptor holds its entity world.
    let world: usize = unsafe { read(descriptor + executor.entity_world) };
    world as u64 == server
}

/// Runs one request as one transaction.
///
/// # Safety
///
/// On the inventory system's thread, inside its row loop: `ctx` is its
/// context and `view` its filled view.
unsafe fn run(executor: &Executor, ctx: usize, view: usize, request: &Request) -> Finished {
    // SAFETY: the view's handles, filled before the loop.
    let replication: usize = unsafe { read(view + executor.view_replication) };
    // SAFETY: forwarded from the caller.
    let (mut touched, source) = unsafe { touched(executor, ctx, view, &request.ops) };
    let actor = if request.actor == 0 {
        let mut entity = 0u32;
        // SAFETY: the current row's entity, from the system context.
        let found = unsafe { (executor.row_entity)(ctx, &raw mut entity) };
        if found.is_null() {
            0
        } else {
            // SAFETY: the game returns a pointer to the entity id.
            unsafe { found.read() }
        }
    } else {
        request.actor
    };
    let notify = request.flags & InventoryRequest::NOTIFY != 0;
    // SAFETY: forwarded from the caller.
    unsafe { describe(executor, ctx, view, notify, actor, source) };
    let desc_ptr = core::ptr::with_exposed_provenance_mut::<u8>(executor.scratch);
    let txn_ptr = core::ptr::with_exposed_provenance_mut::<u8>(executor.scratch + DESC_AREA);
    // SAFETY: the transaction's 16-aligned storage, sized from the table.
    unsafe { (executor.txn_new)(txn_ptr) };
    // SAFETY: a constructed transaction and a filled descriptor.
    unsafe { (executor.txn_init)(txn_ptr, desc_ptr) };
    let allow_partial = request.flags & InventoryRequest::ALLOW_PARTIAL != 0;
    let mut results = Vec::with_capacity(request.ops.len());
    for op in &request.ops {
        // SAFETY: an initialised transaction inside the system.
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
        // SAFETY: as above; commit consumes the op log.
        unsafe { (executor.commit)(txn_ptr) };
    } else {
        // SAFETY: as above; rollback restores every slot the ops wrote.
        unsafe { (executor.rollback)(txn_ptr) };
    }
    // SAFETY: the transaction, done with.
    unsafe { (executor.txn_drop)(txn_ptr) };
    touched.sort_unstable();
    touched.dedup();
    if replication != 0 {
        for &entity in touched.iter().filter(|&&e| e != 0) {
            // SAFETY: the system's own replication resource, as vanilla
            // resends after a transfer.
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
/// As for [`run`].
#[expect(clippy::cast_possible_truncation, reason = "checked entity ids")]
unsafe fn touched(
    executor: &Executor,
    ctx: usize,
    view: usize,
    ops: &[InventoryOp],
) -> (Vec<u32>, u32) {
    // SAFETY: the view's handles, filled before the loop.
    let owners: u64 = unsafe { read(view + executor.view_owners) };
    let root = |slot: u64| {
        let mut out: Res = [0; 2];
        // SAFETY: the game's walk up `OwnerRelationship`, inside the system.
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
/// As for [`run`].
unsafe fn describe(
    executor: &Executor,
    ctx: usize,
    view: usize,
    notify: bool,
    actor: u32,
    source: u32,
) {
    let desc = executor.scratch;
    // SAFETY: the scratch block holds `desc_size` bytes from `desc`.
    unsafe { (executor.desc_zero)(core::ptr::with_exposed_provenance_mut(desc)) };
    let mut flags = if notify { NOTIFY_FLAGS } else { 0 };
    // SAFETY: the view's handles, filled before the loop.
    let actors: u64 = unsafe { read(view + executor.view_actors) };
    // SAFETY: the game's actor check, as its transfer handler calls it.
    if unsafe { (executor.actor_flag)(ctx, actors, actor) } != 0 {
        flags |= ACTOR_FLAG;
    }
    for &(at, from) in &executor.copies {
        if at + 8 <= executor.desc_size {
            // SAFETY: a handle of the view.
            let value: u64 = unsafe { read(view + from) };
            // SAFETY: inside the descriptor.
            unsafe { write(desc + at, value) };
        }
    }
    // SAFETY: the descriptor's fields, inside it.
    unsafe { write(desc, flags) };
    // SAFETY: as above.
    unsafe { write(desc + executor.desc_context, ctx) };
    // SAFETY: as above.
    unsafe { write(desc + executor.desc_actor, actor) };
    // SAFETY: as above.
    unsafe { write(desc + executor.desc_other, source) };
}

/// Runs one operation in the transaction.
///
/// # Safety
///
/// `txn` is initialised, inside the inventory system.
#[expect(clippy::cast_possible_truncation, reason = "checked by `valid`")]
unsafe fn apply(executor: &Executor, txn: *mut u8, op: &InventoryOp) -> InventoryOpResult {
    let mut res: Res = [0; 2];
    let out = &raw mut res;
    let absent = [0u8; 16];
    match op.kind {
        InventoryOp::GIVE => {
            // `valid` refused a fill out of range.
            let fill = fill(op).unwrap_or(absent);
            // SAFETY: the game's add with no level or rarity; its last
            // option is the new items' fuel fill, which it puts on each new
            // item's entity.
            unsafe {
                (executor.give)(
                    out,
                    txn,
                    op.target as u32,
                    0,
                    op.item,
                    op.count.cast_signed(),
                    &raw const absent,
                    &raw const absent,
                    &raw const fill,
                )
            };
        }
        InventoryOp::TAKE => {
            // SAFETY: the game's consume across the sub-inventories.
            unsafe { (executor.take)(out, txn, op.target as u32, op.item, 0, op.count) };
        }
        InventoryOp::DESTROY_SLOT => {
            // SAFETY: the game's destroy of one stack.
            unsafe { (executor.destroy)(out, txn, op.target) };
        }
        InventoryOp::RENEW => {
            // SAFETY: the game's destroy of one stack.
            unsafe { (executor.destroy)(out, txn, op.target) };
            if res[0] as u8 == 0 {
                let give = renew_give(op);
                let fill = fill(&give).unwrap_or(absent);
                // SAFETY: the game's add, asked for the slot just emptied,
                // with the new items' fuel fill.
                unsafe {
                    (executor.give)(
                        out,
                        txn,
                        give.target as u32,
                        op.target,
                        give.item,
                        give.count.cast_signed(),
                        &raw const absent,
                        &raw const absent,
                        &raw const fill,
                    )
                };
            }
        }
        InventoryOp::MOVE_SLOT if op.count == 0 => {
            // SAFETY: the whole stack, a remainder back to the source.
            unsafe { (executor.move_slot)(out, txn, op.target, op.other as u32, 0, 1, 1, 1) };
        }
        InventoryOp::MOVE_SLOT => {
            // SAFETY: `count` of the stack, a remainder back to the source.
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
            // SAFETY: the game's take-all, remainders left in the source.
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicU32;

    use super::*;

    unsafe extern "system" fn nothing(_result: *const InventoryResult, _user: *mut c_void) {}

    fn request(user: usize) -> Request {
        Request {
            owner: 0,
            flags: 0,
            actor: 0,
            ops: Vec::new(),
            done: nothing,
            user,
            queued: Instant::now(),
        }
    }

    fn ok(remaining: u32) -> InventoryOpResult {
        InventoryOpResult {
            error: 0,
            remaining,
        }
    }

    #[test]
    fn a_transaction_commits_only_when_every_op_ran_and_finished() {
        assert!(commits(&[ok(0), ok(0)], 2, false));
        assert!(!commits(&[ok(0), ok(3)], 2, false));
        assert!(commits(&[ok(0), ok(3)], 2, true));
        let failed = InventoryOpResult {
            error: 0x15,
            remaining: 0,
        };
        assert!(!commits(&[ok(0), failed], 2, true));
        assert!(!commits(&[ok(0)], 2, true));
    }

    #[test]
    fn each_request_is_claimed_exactly_once_across_threads() {
        let queue = Arc::new(Queue::new());
        for user in 1..=1000 {
            queue.push(request(user));
        }
        let seen: Arc<Vec<AtomicU32>> = Arc::new((0..=1000).map(|_| AtomicU32::new(0)).collect());
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let (queue, seen) = (Arc::clone(&queue), Arc::clone(&seen));
                std::thread::spawn(move || {
                    while let Some(request) = queue.claim() {
                        seen[request.user].fetch_add(1, Ordering::Relaxed);
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        assert!(seen[1..].iter().all(|n| n.load(Ordering::Relaxed) == 1));
        assert!(!queue.pending.load(Ordering::Relaxed));
        assert!(queue.claim().is_none());
    }

    #[test]
    fn the_server_row_has_every_piece_the_executor_reads() {
        let server = crate::builds::embedded(Side::SERVER);
        let symbols = [
            "inventory.actions_row",
            "inventory.view_fill",
            "inventory.skip_entity",
            "inventory.desc_zero",
            "inventory.actor_flag",
            "inventory.txn_new",
            "inventory.txn_init",
            "inventory.txn_commit",
            "inventory.txn_rollback",
            "inventory.txn_drop",
            "inventory.give",
            "inventory.take",
            "inventory.destroy_slot",
            "inventory.move_slot",
            "inventory.move_amount",
            "inventory.move_all",
            "inventory.resend",
            "item.info",
            "ecs.slot_root",
            "ecs.row_entity",
        ];
        for name in symbols {
            assert!(server.symbol(name).is_some(), "{name}");
        }
        let constants = [
            "inventory.row_bytes",
            "inventory.view_fill_bytes",
            "inventory.view_frame",
            "inventory.view_owners",
            "inventory.view_actors",
            "inventory.view_replication",
            "inventory.view_item_registry",
            "inventory.desc_size",
            "inventory.desc_context",
            "inventory.desc_actor",
            "inventory.desc_other",
            "inventory.txn_size",
        ];
        for name in constants {
            assert!(server.constant(name).is_some(), "{name}");
        }
        let copies = server
            .constants()
            .iter()
            .filter(|(name, _)| name.starts_with("inventory.copy_"))
            .count();
        assert_eq!(copies, 15);
        assert!(server.constant("inventory.desc_size").unwrap() <= DESC_AREA as u64);
    }

    #[test]
    fn one_turn_at_a_time() {
        let busy = AtomicBool::new(false);
        let turn = Turn::take(&busy).unwrap();
        assert!(Turn::take(&busy).is_none());
        drop(turn);
        assert!(Turn::take(&busy).is_some());
    }

    #[test]
    fn late_requests_drain_and_the_rest_stay_in_order() {
        let queue = Queue::new();
        for user in 1..=4 {
            queue.push(request(user));
        }
        let gone = queue.drain(|r| r.user % 2 == 0);
        assert_eq!(gone.iter().map(|r| r.user).collect::<Vec<_>>(), [2, 4]);
        assert_eq!(queue.claim().unwrap().user, 1);
        assert_eq!(queue.claim().unwrap().user, 3);
        assert!(queue.claim().is_none());
    }

    #[test]
    fn operations_must_name_what_they_need() {
        let give = InventoryOp {
            kind: InventoryOp::GIVE,
            target: 2,
            item: 7,
            count: 1,
            ..InventoryOp::default()
        };
        assert!(valid(&give));
        assert!(!valid(&InventoryOp { count: 0, ..give }));
        assert!(!valid(&InventoryOp {
            count: u32::MAX,
            ..give
        }));
        assert!(!valid(&InventoryOp { kind: 99, ..give }));
        let filled = InventoryOp {
            flags: InventoryOp::FILLED,
            other: u64::from(0.5f32.to_bits()),
            ..give
        };
        assert!(valid(&filled));
        assert_eq!(
            fill(&filled).map(|f| (f[..4].to_vec(), f[4])),
            Some((0.5f32.to_le_bytes().to_vec(), 1))
        );
        assert!(!valid(&InventoryOp {
            other: u64::from(1.5f32.to_bits()),
            ..filled
        }));
        assert!(!valid(&InventoryOp {
            other: u64::from(f32::NAN.to_bits()),
            ..filled
        }));
        let all = InventoryOp {
            kind: InventoryOp::MOVE_ALL,
            target: 40,
            other: 2,
            ..InventoryOp::default()
        };
        assert!(valid(&all));
        assert!(!valid(&InventoryOp { other: 40, ..all }));
        let slot = InventoryOp {
            kind: InventoryOp::MOVE_SLOT,
            target: 0x29 | 3 << 32,
            other: 2,
            count: 70_000,
            ..InventoryOp::default()
        };
        assert!(!valid(&slot));
        assert!(valid(&InventoryOp { count: 5, ..slot }));
    }
}
