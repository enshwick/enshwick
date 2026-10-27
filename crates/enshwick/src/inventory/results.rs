use core::ffi::c_void;
use std::sync::{Arc, Mutex};

use crate::abi::{InventoryOpResult, InventoryResult};
use crate::engine_enum::Values;
use crate::engine_ids as ids;
use crate::locks::Relock;
use crate::script::{Choice, Error as ScriptError, FromScript, IntoScript, Ty, Typed, Value};

use super::ops::BatchError;

/// How a request ended.
///
/// # Examples
///
/// See [`Outcome`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Done {
    /// Every operation succeeded and the transaction was committed.
    Committed,
    /// An operation failed or fell short; nothing changed.
    RolledBack,
    /// This game build lacks a piece, or the loader's hook is not in place;
    /// nothing changed.
    Unavailable,
    /// The game did not run its inventory system in time, as with nobody
    /// online; nothing changed.
    TimedOut,
    /// The batch was refused before it reached the game.
    Refused(BatchError),
}

/// Why an operation failed; the loader reads the game's code for each
/// variant by its constant's name.
///
/// # Examples
///
/// ```
/// use enshwick::inventory::{OpError, Outcome};
///
/// fn split(outcome: &Outcome) -> bool {
///     outcome.ops.iter().any(|op| op.error == Some(OpError::OutOfMemory))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OpError {
    /// An operation before it failed, so it did not run; the loader's
    /// name, not the game's.
    NotRun,
    /// The item did not fit or does not exist.
    InvalidState,
    /// The op log is full or more than 16 inventories are touched; split the
    /// batch.
    OutOfMemory,
    /// A code this crate does not name, as the game gave it.
    Other(u32),
}

impl OpError {
    /// Each error with the id of the game's constant for it.
    pub(super) const IDS: [(Self, &'static str); 2] = [
        (Self::InvalidState, ids::INVENTORY_ERROR_INVALID_STATE),
        (Self::OutOfMemory, ids::INVENTORY_ERROR_OUT_OF_MEMORY),
    ];
}

/// The game's code of each [`OpError`] it names, for the running build.
pub(super) type OpErrors = Values<OpError, 2>;

/// The result of one operation.
///
/// # Examples
///
/// See [`Outcome`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, crate::Script)]
pub struct OpResult {
    /// Why it failed; none when it ran without an error.
    pub error: Option<OpError>,
    /// How much of the count the operation could not do.
    pub remaining: u32,
}

impl OpResult {
    /// Whether the operation ran without an error.
    ///
    /// # Examples
    ///
    /// See [`Outcome`].
    #[must_use]
    pub const fn ok(self) -> bool {
        self.error.is_none()
    }

    fn from_raw(raw: InventoryOpResult, errors: &OpErrors) -> Self {
        let error = match raw.error {
            0 => None,
            InventoryOpResult::NOT_RUN => Some(OpError::NotRun),
            code => Some(
                errors
                    .variant(u64::from(code))
                    .unwrap_or(OpError::Other(code)),
            ),
        };
        Self {
            error,
            remaining: raw.remaining,
        }
    }
}

/// What became of a request, with a result per operation once it ran.
///
/// # Examples
///
/// ```
/// use enshwick::inventory::{Done, Outcome};
///
/// fn report(outcome: &Outcome) {
///     match outcome.done {
///         Done::Committed => enshwick::info!("done"),
///         Done::RolledBack => {
///             let failed = outcome.ops.iter().position(|op| !op.ok() || op.remaining > 0);
///             enshwick::warn!("rolled back at operation {failed:?}");
///         }
///         other => enshwick::warn!("not run: {other:?}"),
///     }
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
pub struct Outcome {
    /// How it ended.
    pub done: Done,
    /// One per operation when the request ran, else empty.
    pub ops: Vec<OpResult>,
}

impl Outcome {
    /// Whether the change was made.
    ///
    /// # Examples
    ///
    /// See [`Ticket`].
    #[must_use]
    pub const fn is_committed(&self) -> bool {
        matches!(self.done, Done::Committed)
    }

    pub(super) const fn not_run(done: Done) -> Self {
        Self {
            done,
            ops: Vec::new(),
        }
    }

    /// The outcome the loader reported in `result`, its error codes read
    /// with `errors`.
    ///
    /// # Safety
    ///
    /// - `result.ops` is null or points at `op_count` initialised, aligned
    ///   op results, alive for the call.
    pub(super) unsafe fn from_raw(result: &InventoryResult, errors: &OpErrors) -> Self {
        let done = match result.outcome {
            InventoryResult::COMMITTED => Done::Committed,
            InventoryResult::ROLLED_BACK => Done::RolledBack,
            InventoryResult::TIMED_OUT => Done::TimedOut,
            _ => Done::Unavailable,
        };
        let ops = if result.ops.is_null() || result.op_count == 0 {
            &[][..]
        } else {
            // SAFETY: `ops` is not null in this branch, and the caller
            // guarantees `op_count` aligned op results there, alive for the
            // call.
            unsafe { core::slice::from_raw_parts(result.ops, result.op_count as usize) }
        };
        Self {
            done,
            ops: ops
                .iter()
                .map(|&raw| OpResult::from_raw(raw, errors))
                .collect(),
        }
    }
}

/// A submitted request; holds its [`Outcome`] once it is known. Clone it
/// freely.
///
/// # Examples
///
/// ```
/// fn finished(ticket: &enshwick::inventory::Ticket) -> bool {
///     ticket.outcome().is_some_and(|outcome| outcome.is_committed())
/// }
/// ```
#[derive(Clone, Debug, Default)]
pub struct Ticket(Arc<Mutex<Option<Outcome>>>);

impl Ticket {
    /// The outcome, once the request is done.
    ///
    /// # Examples
    ///
    /// See [`Ticket`].
    #[must_use]
    pub fn outcome(&self) -> Option<Outcome> {
        self.0.relock().clone()
    }

    /// Whether the request is done.
    ///
    /// # Examples
    ///
    /// ```
    /// assert!(!enshwick::inventory::Ticket::default().is_done());
    /// ```
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.0.relock().is_some()
    }
}

pub(super) type Callback = Box<dyn FnOnce(&Outcome) + Send>;

/// A request on its way: where its outcome goes.
pub(super) struct Pending {
    pub(super) ticket: Ticket,
    pub(super) done: Callback,
    pub(super) errors: OpErrors,
}

impl Pending {
    pub(super) fn complete(self, outcome: &Outcome) {
        *self.ticket.0.relock() = Some(outcome.clone());
        (self.done)(outcome);
    }
}

/// Completes the request whose `Pending` the loader hands back.
///
/// # Safety
///
/// - `user` is null or the `Box<Pending>` that [`Inventories::submit`] handed
///   over, passed back once.
/// - `result` is null or points at an [`InventoryResult`] readable for the
///   size its header states, with no other writer during the call.
/// - The result's `ops` is null or points at `op_count` aligned op results,
///   alive for the call.
pub(super) unsafe extern "system" fn deliver(result: *const InventoryResult, user: *mut c_void) {
    if user.is_null() {
        return;
    }
    // SAFETY: `user` is not null, as the early return ensures, and the caller
    // guarantees it is the `Box<Pending>` that `submit` leaked, passed back
    // once, so this is its only owner.
    let pending = unsafe { Box::from_raw(user.cast::<Pending>()) };
    // SAFETY: the caller guarantees `result` is null or an `InventoryResult`
    // readable for the size its header states with no concurrent writer,
    // which is the contract of `read_headed`.
    let outcome = unsafe { crate::abi::read_headed(result, size_of::<InventoryResult>()) }
        .map_or_else(
            || Outcome::not_run(Done::Unavailable),
            // SAFETY: the caller guarantees the result's `ops` is null or
            // `op_count` aligned op results alive for the call.
            |r| unsafe { Outcome::from_raw(&r, &pending.errors) },
        );
    crate::host::guard(
        "panic in an inventory callback",
        || (),
        || {
            pending.complete(&outcome);
        },
    );
}

/// A script reads how a batch ended as the variant's name; `Refused` drops
/// its reason.
impl Typed for Done {
    const TY: &'static Ty = &Ty::Choice(&Choice::new(
        "Done",
        "How a request ended.",
        &[
            (
                "Committed",
                "Every operation succeeded and the transaction was committed.",
            ),
            (
                "RolledBack",
                "An operation failed or fell short; nothing changed.",
            ),
            (
                "Unavailable",
                "This game build lacks a piece, or the loader's hook is not in place; nothing changed.",
            ),
            (
                "TimedOut",
                "The game did not run its inventory system in time; nothing changed.",
            ),
            (
                "Refused",
                "The batch was refused before it reached the game.",
            ),
        ],
    ));
}

impl IntoScript for Done {
    fn into_value(self) -> Value {
        Value::String(
            match self {
                Self::Committed => "Committed",
                Self::RolledBack => "RolledBack",
                Self::Unavailable => "Unavailable",
                Self::TimedOut => "TimedOut",
                Self::Refused(_) => "Refused",
            }
            .to_owned(),
        )
    }
}

impl FromScript for Done {
    fn from_value(value: Value) -> Result<Self, ScriptError> {
        let name = String::from_value(value)?;
        Ok(match name.as_str() {
            "Committed" => Self::Committed,
            "RolledBack" => Self::RolledBack,
            "Unavailable" => Self::Unavailable,
            "TimedOut" => Self::TimedOut,
            _ => {
                return Err(ScriptError::new(format!(
                    "{name:?} is not one of \"Committed\", \"RolledBack\", \"Unavailable\", \"TimedOut\""
                )));
            }
        })
    }
}

/// A script reads an op's error as the variant's name; `Other` drops the
/// game's code.
impl Typed for OpError {
    const TY: &'static Ty = &Ty::Choice(&Choice::new(
        "OpError",
        "Why an operation failed.",
        &[
            (
                "NotRun",
                "An operation before it failed, so it did not run.",
            ),
            ("InvalidState", "The item did not fit or does not exist."),
            (
                "OutOfMemory",
                "The op log is full or more than 16 inventories are touched.",
            ),
            ("Other", "A code the API does not name."),
        ],
    ));
}

impl IntoScript for OpError {
    fn into_value(self) -> Value {
        Value::String(
            match self {
                Self::NotRun => "NotRun",
                Self::InvalidState => "InvalidState",
                Self::OutOfMemory => "OutOfMemory",
                Self::Other(_) => "Other",
            }
            .to_owned(),
        )
    }
}

impl FromScript for OpError {
    fn from_value(value: Value) -> Result<Self, ScriptError> {
        let name = String::from_value(value)?;
        Ok(match name.as_str() {
            "NotRun" => Self::NotRun,
            "InvalidState" => Self::InvalidState,
            "OutOfMemory" => Self::OutOfMemory,
            _ => {
                return Err(ScriptError::new(format!(
                    "{name:?} is not one of \"NotRun\", \"InvalidState\", \"OutOfMemory\""
                )));
            }
        })
    }
}

/// A script waits for the done callback; the ticket reaches it as nothing.
impl Typed for Ticket {
    const TY: &'static Ty = &Ty::Nil;
}

impl IntoScript for Ticket {
    fn into_value(self) -> Value {
        Value::Nil
    }
}
