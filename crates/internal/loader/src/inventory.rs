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
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use enshwick::abi::{
    Header, HookFlags, InventoryOp, InventoryOpResult, InventoryRequest, InventoryResult,
    InventoryResultFn, ItemData, Registers, Side, Status,
};
use run::run;

use crate::host::Loader;
use crate::locks::Relock;
use crate::memory::read_ptr;
use crate::mid::MidLink;
use crate::mod_index::ModIndex;
use crate::pinned;
use crate::pinned::{Constant, Symbol};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::ecs::CONTEXT_ENTITY_WORLD.id(),
    pinned::ecs::ROW_ENTITY.id(),
    pinned::ecs::SLOT_ROOT.id(),
    pinned::inventory::ACTIONS_ROW.id(),
    pinned::inventory::ACTOR_FLAG.id(),
    pinned::inventory::COPY_100_DESC.id(),
    pinned::inventory::COPY_100_VIEW.id(),
    pinned::inventory::COPY_150_DESC.id(),
    pinned::inventory::COPY_150_VIEW.id(),
    pinned::inventory::COPY_198_DESC.id(),
    pinned::inventory::COPY_198_VIEW.id(),
    pinned::inventory::COPY_A8_DESC.id(),
    pinned::inventory::COPY_A8_VIEW.id(),
    pinned::inventory::COPY_DISCOVERED_DESC.id(),
    pinned::inventory::COPY_DISCOVERED_VIEW.id(),
    pinned::inventory::COPY_EMPTY_STATE_DESC.id(),
    pinned::inventory::COPY_EMPTY_STATE_VIEW.id(),
    pinned::inventory::COPY_EQUIPMENT_DESC.id(),
    pinned::inventory::COPY_EQUIPMENT_VIEW.id(),
    pinned::inventory::COPY_FULL_EVENTS_DESC.id(),
    pinned::inventory::COPY_FULL_EVENTS_VIEW.id(),
    pinned::inventory::COPY_ITEM_REGISTRY_DESC.id(),
    pinned::inventory::COPY_ITEM_REGISTRY_VIEW.id(),
    pinned::inventory::COPY_OWNERS_DESC.id(),
    pinned::inventory::COPY_OWNERS_VIEW.id(),
    pinned::inventory::COPY_PICKUP_EVENTS_DESC.id(),
    pinned::inventory::COPY_PICKUP_EVENTS_VIEW.id(),
    pinned::inventory::COPY_SETUP_DESC.id(),
    pinned::inventory::COPY_SETUP_VIEW.id(),
    pinned::inventory::COPY_SLOT_HINTS_DESC.id(),
    pinned::inventory::COPY_SLOT_HINTS_VIEW.id(),
    pinned::inventory::COPY_SLOTS_DESC.id(),
    pinned::inventory::COPY_SLOTS_VIEW.id(),
    pinned::inventory::COPY_TRANSFORM_DESC.id(),
    pinned::inventory::COPY_TRANSFORM_VIEW.id(),
    pinned::inventory::DESC_ACTOR.id(),
    pinned::inventory::DESC_CONTEXT.id(),
    pinned::inventory::DESC_OTHER.id(),
    pinned::inventory::DESC_SIZE.id(),
    pinned::inventory::DESC_ZERO.id(),
    pinned::inventory::DESTROY_SLOT.id(),
    pinned::inventory::GIVE.id(),
    pinned::inventory::MOVE_ALL.id(),
    pinned::inventory::MOVE_AMOUNT.id(),
    pinned::inventory::MOVE_SLOT.id(),
    pinned::inventory::RESEND.id(),
    pinned::inventory::TAKE.id(),
    pinned::inventory::TXN_COMMIT.id(),
    pinned::inventory::TXN_DROP.id(),
    pinned::inventory::TXN_INIT.id(),
    pinned::inventory::TXN_NEW.id(),
    pinned::inventory::TXN_ROLLBACK.id(),
    pinned::inventory::TXN_SIZE.id(),
    pinned::inventory::VIEW_ACTORS.id(),
    pinned::inventory::VIEW_FILL.id(),
    pinned::inventory::VIEW_FRAME.id(),
    pinned::inventory::VIEW_ITEM_REGISTRY.id(),
    pinned::inventory::VIEW_OWNERS.id(),
    pinned::inventory::VIEW_REPLICATION.id(),
    pinned::item::INFO.id(),
];

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
    /// Where `ItemInfo` holds the placed template's GUID, on builds that
    /// reflect it.
    item_placed_entity: Option<usize>,
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
    owner: ModIndex,
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
    owner: ModIndex,
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
        let mut requests = self.requests.relock();
        requests.push_back(request);
        self.pending.store(true, Ordering::Release);
    }

    /// Takes the oldest request; each is handed out exactly once, whichever
    /// thread asks.
    fn claim(&self) -> Option<Request> {
        if !self.pending.load(Ordering::Acquire) {
            return None;
        }
        let mut requests = self.requests.relock();
        let request = requests.pop_front();
        self.pending.store(!requests.is_empty(), Ordering::Release);
        request
    }

    /// Removes the requests for which `expired` holds.
    fn drain(&self, mut expired: impl FnMut(&Request) -> bool) -> Vec<Request> {
        if !self.pending.load(Ordering::Acquire) {
            return Vec::new();
        }
        let mut requests = self.requests.relock();
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
/// - `F` is a function pointer type.
/// - `address` is 0 or the address of a function with `F`'s exact signature
///   and calling convention, mapped for the process.
unsafe fn function<F: Copy>(address: u64) -> Option<F> {
    const { assert!(size_of::<F>() == size_of::<usize>()) };
    let address = usize::try_from(address).ok()?;
    // SAFETY: `F` is a function pointer the size of `usize` (asserted earlier),
    // and the caller's contract makes the non-zero `address` a function of
    // that type.
    (address != 0).then(|| unsafe { core::mem::transmute_copy::<usize, F>(&address) })
}

fn build(loader: &Loader) -> Option<Executor> {
    let symbol = |name: Symbol| loader.build.symbol(name.id()).map(|rva| loader.base + rva);
    let constant = |name: Constant| {
        loader
            .build
            .constant(name.id())
            .and_then(|v| usize::try_from(v).ok())
    };
    if loader.build.lacks_any("inventory.copy_") {
        return None;
    }
    let copies: Vec<(usize, usize)> = loader
        .build
        .constants()
        .iter()
        .filter_map(|(name, view)| {
            let field = name
                .strip_prefix("inventory.copy_")?
                .strip_suffix(".view")?;
            let desc = loader
                .build
                .constant(&format!("inventory.copy_{field}.desc"))?;
            Some((usize::try_from(desc).ok()?, usize::try_from(*view).ok()?))
        })
        .collect();
    let field = |name: &str| {
        let offset = loader
            .reflection
            .field_offset("keen::ItemInfo", name)
            .ok()?;
        usize::try_from(offset).ok()
    };
    let desc_size = constant(pinned::inventory::DESC_SIZE)?;
    let txn_size = constant(pinned::inventory::TXN_SIZE)?;
    if copies.is_empty() || desc_size > DESC_AREA {
        return None;
    }
    // SAFETY: the build table pins `inventory.desc_zero` to this signature.
    let desc_zero = unsafe { function(symbol(pinned::inventory::DESC_ZERO)?) }?;
    // SAFETY: the build table pins `inventory.txn_new` to this signature.
    let txn_new = unsafe { function(symbol(pinned::inventory::TXN_NEW)?) }?;
    // SAFETY: the build table pins `inventory.txn_init` to this signature.
    let txn_init = unsafe { function(symbol(pinned::inventory::TXN_INIT)?) }?;
    // SAFETY: the build table pins `inventory.txn_commit` to this signature.
    let commit = unsafe { function(symbol(pinned::inventory::TXN_COMMIT)?) }?;
    // SAFETY: the build table pins `inventory.txn_rollback` to this signature.
    let rollback = unsafe { function(symbol(pinned::inventory::TXN_ROLLBACK)?) }?;
    // SAFETY: the build table pins `inventory.txn_drop` to this signature.
    let txn_drop = unsafe { function(symbol(pinned::inventory::TXN_DROP)?) }?;
    // SAFETY: the build table pins `inventory.give` to this signature.
    let give = unsafe { function(symbol(pinned::inventory::GIVE)?) }?;
    // SAFETY: the build table pins `inventory.take` to this signature.
    let take = unsafe { function(symbol(pinned::inventory::TAKE)?) }?;
    // SAFETY: the build table pins `inventory.destroy_slot` to this signature.
    let destroy = unsafe { function(symbol(pinned::inventory::DESTROY_SLOT)?) }?;
    // SAFETY: the build table pins `inventory.move_slot` to this signature.
    let move_slot = unsafe { function(symbol(pinned::inventory::MOVE_SLOT)?) }?;
    // SAFETY: the build table pins `inventory.move_amount` to this signature.
    let move_amount = unsafe { function(symbol(pinned::inventory::MOVE_AMOUNT)?) }?;
    // SAFETY: the build table pins `inventory.move_all` to this signature.
    let move_all = unsafe { function(symbol(pinned::inventory::MOVE_ALL)?) }?;
    // SAFETY: the build table pins `inventory.resend` to this signature.
    let resend = unsafe { function(symbol(pinned::inventory::RESEND)?) }?;
    // SAFETY: the build table pins `ecs.slot_root` to this signature.
    let slot_root = unsafe { function(symbol(pinned::ecs::SLOT_ROOT)?) }?;
    // SAFETY: the build table pins `ecs.row_entity` to this signature.
    let row_entity = unsafe { function(symbol(pinned::ecs::ROW_ENTITY)?) }?;
    // SAFETY: the build table pins `inventory.actor_flag` to this signature.
    let actor_flag = unsafe { function(symbol(pinned::inventory::ACTOR_FLAG)?) }?;
    // SAFETY: the build table pins `item.info` to this signature.
    let item_lookup = unsafe { function(symbol(pinned::item::INFO)?) }?;
    let executor = Executor {
        site: symbol(pinned::inventory::ACTIONS_ROW)?,
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
        view_frame: constant(pinned::inventory::VIEW_FRAME)?,
        view_owners: constant(pinned::inventory::VIEW_OWNERS)?,
        view_actors: constant(pinned::inventory::VIEW_ACTORS)?,
        view_replication: constant(pinned::inventory::VIEW_REPLICATION)?,
        view_item_registry: constant(pinned::inventory::VIEW_ITEM_REGISTRY)?,
        desc_size,
        desc_context: constant(pinned::inventory::DESC_CONTEXT)?,
        desc_actor: constant(pinned::inventory::DESC_ACTOR)?,
        desc_other: constant(pinned::inventory::DESC_OTHER)?,
        copies,
        item_max_stack: field("maxStackSize")?,
        item_flags: field("flags")?,
        item_category: field("category")?,
        item_rarity: field("rarity")?,
        item_placed_entity: crate::catalogue::constant(
            enshwick_gamedata::catalogue::ITEM_PLACED_ENTITY,
            &loader.reflection,
        )
        .and_then(|offset| usize::try_from(offset).ok()),
        scratch: 0,
        check_world: loader.side != Side::SERVER,
        entity_world: constant(pinned::ecs::CONTEXT_ENTITY_WORLD)?,
    };
    checked(loader, executor, txn_size)
}

/// The symbols whose code the inventory hook relies on.
const CHECKED: [Symbol; 2] = [pinned::inventory::ACTIONS_ROW, pinned::inventory::VIEW_FILL];

/// `executor` with its scratch block, when the game code at the hook reads
/// as the build table says.
fn checked(loader: &Loader, executor: Executor, txn_size: usize) -> Option<Executor> {
    if !CHECKED.iter().all(|&symbol| loader.code_holds(symbol)) {
        crate::log!(
            WARN,
            "inventories off: the inventory system's code differs from the build table"
        );
        return None;
    }
    let layout = std::alloc::Layout::from_size_align(DESC_AREA + txn_size, 16).ok()?;
    // SAFETY: `layout` has a non-zero size, at least `DESC_AREA` bytes; the
    // block is never freed.
    let scratch = unsafe { std::alloc::alloc_zeroed(layout) };
    if scratch.is_null() {
        return None;
    }
    Some(Executor {
        scratch: scratch.expose_provenance(),
        ..executor
    })
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
        owner: ModIndex::LOADER,
        handler: row,
        user: 0,
        priority: i32::MIN,
        flags: HookFlags::NONE,
    };
    let status = loader.mids.relock().add(executor.site, link);
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
    FINISHED.relock().extend(results);
}

/// Queues a request of the mod `owner`.
pub(crate) fn submit(
    loader: &Loader,
    owner: ModIndex,
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
    // SAFETY: the mod's request carries a non-null `ops` (checked earlier) of
    // `op_count` operations, borrowed for this call and copied here.
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
    // SAFETY: the game's lookup at the table's `item.info`, a read of the
    // non-zero item registry (checked earlier) the inventory system last saw.
    let info = unsafe { (executor.item_lookup)(item, registry) };
    if info.is_null() {
        return Err(Status::MISSING);
    }
    Ok(ItemData {
        header: Header::of::<ItemData>(),
        id: item,
        // SAFETY: the lookup returned the item's non-null live `ItemInfo`
        // (checked earlier), with `maxStackSize` at the reflected offset.
        max_stack: unsafe { enshwick::raw::read_at(info, executor.item_max_stack) },
        // SAFETY: the lookup returned the item's non-null live `ItemInfo`
        // (checked earlier), with `category` at the reflected offset.
        category: unsafe { enshwick::raw::read_at(info, executor.item_category) },
        // SAFETY: the lookup returned the item's non-null live `ItemInfo`
        // (checked earlier), with `rarity` at the reflected offset.
        rarity: unsafe { enshwick::raw::read_at(info, executor.item_rarity) },
        // SAFETY: the lookup returned the item's non-null live `ItemInfo`
        // (checked earlier), with `flags` at the reflected offset.
        flags: unsafe { enshwick::raw::read_at(info, executor.item_flags) },
        placed_entity: executor.item_placed_entity.map_or([0; 16], |at| {
            // SAFETY: the lookup returned the item's non-null live
            // `ItemInfo` (checked earlier), whose `equipment.placedEntity`
            // at the reflected offset is a 16-byte GUID held inline (server
            // 399, client 426).
            unsafe { enshwick::raw::read_at(info, at) }
        }),
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
        let mut finished = FINISHED.relock();
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
        // SAFETY: the request's `InventoryResultFn` lives in the mod's DLL,
        // which stays loaded for the process, and takes the `user` word the
        // mod passed; `data` and its results outlive the call, and the mod's
        // safe layer catches its panics.
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
    // SAFETY: the loader's mid-function stub passes null or its own saved
    // registers, valid and unaliased for the call.
    let Some(registers) = (unsafe { registers.as_ref() }) else {
        return;
    };
    let Some(executor) = EXECUTOR.get().and_then(Option::as_ref) else {
        return;
    };
    let ctx = usize::try_from(registers.r13).unwrap_or(0);
    // SAFETY: at the table's `inventory.actions_row` (its code checked by
    // `Loader::code_holds`), `r13` holds the inventory system's live context, used
    // here on the system's thread while it runs.
    if executor.check_world && !unsafe { in_server_world(executor, ctx) } {
        return;
    }
    let view = usize::try_from(registers.rbp)
        .unwrap_or(0)
        .wrapping_sub(executor.view_frame);
    // SAFETY: at the table's `inventory.actions_row`, the system's view lies
    // `inventory.view_frame` below `rbp` in its live frame, filled before the
    // row loop (`inventory.view_fill` checked by `Loader::code_holds`); it holds the
    // item registry pointer at the table's `inventory.view_item_registry`.
    let registry = unsafe { read_ptr::<u8>(view, executor.view_item_registry) }.expose_provenance();
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
            // SAFETY: this runs in the row hook on the inventory system's
            // thread inside its row loop, with the system's live context in
            // `ctx` and its filled view at `view`, and `Turn` lets one
            // transaction run at a time.
            let finished = unsafe { run(executor, ctx, view, &request) };
            finish([finished]);
        }
    });
}

/// Whether `ctx` is a context of the hosted server's world.
///
/// # Safety
///
/// - `ctx` is 0 or the address of a live system context, used on its
///   system's thread while the system runs.
unsafe fn in_server_world(executor: &Executor, ctx: usize) -> bool {
    let server = crate::world::server_world();
    if server == 0 || ctx == 0 {
        return false;
    }
    // SAFETY: the caller's contract makes the non-zero `ctx` (checked earlier)
    // a live context, whose first word is its query descriptor pointer.
    let descriptor = unsafe { read_ptr::<u8>(ctx, 0) }.cast_const();
    if descriptor.is_null() {
        return false;
    }
    // SAFETY: `descriptor` is the context's non-null live query descriptor
    // (checked earlier), which keeps its entity world pointer at the table's
    // `ecs.context_entity_world`.
    let world = unsafe { enshwick::raw::read_ptr_at::<u8>(descriptor, executor.entity_world) };
    world.addr() as u64 == server
}

mod run;
#[cfg(test)]
mod tests;
