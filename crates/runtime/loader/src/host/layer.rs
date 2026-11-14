//! The one layer every `HostApi` entry passes through, between a mod's call
//! and the loader's domain code.
//!
//! [`entries!`] writes each entry from a typed signature: the panic guard and
//! the entry's own fuse it turns off, the caller's check, the reads of what the mod
//! passed and the writes of what it gets back, and the answer for an error.
//! An entry's body holds only its call into the domain. Domain code returns
//! its own errors, each a [`Refusal`] that says its [`Status`] and whether
//! the loader logs it or turns the calling mod off; only this layer turns
//! them into a [`Status`].

use core::cell::Cell;
use core::fmt;

use enshwick_core::abi::{Level, Status};

use crate::engine::fuse::{Fuse, Off};
use crate::engine::loader::{Loader, ModRecord};
use crate::engine::mod_index::ModIndex;

mod args;
pub(crate) mod guards;
mod refusals;

pub(crate) use args::{
    AnyMod, Arg, Arg2, Arg3, Arg4, Asker, Buffer, BytesOut, BytesSlot, Caller, Headed, HeadedInOut,
    HeadedList, HeadedListOut, HeadedOut, HeadedSince, In, List, ListOut, MaybeBuffer, MaybeHeaded,
    Mod, Out, SizedListOut, Text, UnalignedList, Values,
};
pub(crate) use refusals::{Rule, RuleOff};

/// How the loader answers a domain error besides its status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Policy {
    /// The status says all: a world not loaded, an entry the build lacks.
    Quiet,
    /// The calling mod asked for what the loader's rules refuse: logged as
    /// that mod's warning.
    Warn,
    /// The calling mod broke a rule it cannot recover from, such as a
    /// content id two mods declare: the mod turns off.
    TurnOff,
}

/// A domain error an entry answers a mod with.
pub(crate) trait Refusal: fmt::Display {
    /// The status the mod gets.
    fn status(&self) -> Status;

    /// What the loader does besides; quiet unless the error says otherwise.
    fn policy(&self) -> Policy {
        Policy::Quiet
    }
}

/// Why an entry refused a call: the status the mod gets and what the
/// loader does besides, built from a [`Refusal`] or a bare [`Status`].
#[derive(Debug)]
pub(crate) struct Refused {
    status: Status,
    then: Then,
}

#[derive(Debug)]
enum Then {
    Nothing,
    Warn(String),
    TurnOff(String),
}

impl Refused {
    /// The status the mod gets.
    #[must_use]
    pub(crate) const fn status(&self) -> Status {
        self.status
    }

    /// A refusal for what `status` already says, for a check the layer or an
    /// entry makes itself.
    #[must_use]
    pub(crate) const fn with(status: Status) -> Self {
        Self {
            status,
            then: Then::Nothing,
        }
    }
}

impl From<Status> for Refused {
    fn from(status: Status) -> Self {
        Self::with(status)
    }
}

impl<E: Refusal> From<E> for Refused {
    #[cold]
    fn from(error: E) -> Self {
        let then = match error.policy() {
            Policy::Quiet => Then::Nothing,
            Policy::Warn => Then::Warn(error.to_string()),
            Policy::TurnOff => Then::TurnOff(error.to_string()),
        };
        Self {
            status: error.status(),
            then,
        }
    }
}

/// [`Ok`] for [`Status::OK`], else the status refused.
///
/// # Errors
///
/// The status itself when it is not [`Status::OK`].
pub(crate) fn checked(status: Status) -> Result<(), Refused> {
    if status == Status::OK {
        Ok(())
    } else {
        Err(Refused::with(status))
    }
}

/// One entry's call: its name and the calling mod, once its handle read.
pub(crate) struct Call {
    name: &'static str,
    owner: Cell<Option<(&'static Loader, ModIndex, &'static ModRecord)>>,
}

impl Call {
    /// The call of the entry `name`, its caller not read yet.
    const fn new(name: &'static str) -> Self {
        Self {
            name,
            owner: Cell::new(None),
        }
    }

    /// Notes the calling mod, for the log line or the turn-off of a refusal.
    fn owned_by(&self, loader: &'static Loader, index: ModIndex, record: &'static ModRecord) {
        self.owner.set(Some((loader, index, record)));
    }

    /// Does what `refused` asks besides its status, as the calling mod or,
    /// without one, as the loader.
    #[cold]
    fn settle(&self, refused: Refused) -> Status {
        match (refused.then, self.owner.get()) {
            (Then::Nothing, _) => {}
            (Then::TurnOff(reason), Some((loader, index, _))) => loader.turn_off(index, &reason),
            (Then::Warn(reason), Some((_, _, record))) => {
                crate::engine::log::write(Level::WARN, &record.id, &reason);
            }
            (Then::Warn(reason) | Then::TurnOff(reason), None) => {
                crate::log!(WARN, "{}: {reason}", self.name);
            }
        }
        refused.status
    }
}

/// What an entry returns to the mod: its answer for a refusal and for a
/// panic in the loader's own code.
pub(crate) trait Reply: Sized {
    /// The answer after a panic.
    const FAULT: Self;

    /// The answer for a refusal of `status`.
    fn refused(status: Status) -> Self;
}

impl Reply for Status {
    const FAULT: Self = Self::REFUSED;

    fn refused(status: Status) -> Self {
        status
    }
}

/// [`Reply`] for an entry whose return value carries no status: a refusal
/// answers what a panic answers.
macro_rules! reply_without_status {
    ($($ty:ty = $fault:expr),* $(,)?) => {$(
        impl Reply for $ty {
            const FAULT: Self = $fault;

            fn refused(_: Status) -> Self {
                Self::FAULT
            }
        }
    )*};
}

reply_without_status!(() = (), u8 = 0, u32 = 0, u64 = 0, Level = Level::INFO);

/// What an entry's body gives on success, turned into the entry's return
/// value: the value itself, or [`Status::OK`] for `()`.
pub(crate) trait Answer<R> {
    fn answer(self) -> R;
}

impl<R: Reply> Answer<R> for R {
    fn answer(self) -> R {
        self
    }
}

impl Answer<Status> for () {
    fn answer(self) -> Status {
        Status::OK
    }
}

/// Runs the body of the entry `feature` names, as [`entries!`] writes each
/// entry: settles a refusal and catches a panic, so nothing unwinds into
/// the mod.
///
/// A panic inside a mod's callback that the body ran turns that mod off;
/// any other turns `feature`, the entry's own, off, and [`live`] refuses the
/// later calls.
#[inline]
pub(crate) fn run<R: Reply, A: Answer<R>>(
    feature: &'static Fuse,
    body: impl FnOnce(&Call) -> Result<A, Refused>,
) -> R {
    let call = Call::new(feature.feature());
    let outer = crate::engine::profile::marker();
    let answered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match body(&call) {
        Ok(answer) => answer.answer(),
        Err(refused) => R::refused(call.settle(refused)),
    }));
    answered.unwrap_or_else(|payload| {
        // Turning off what panicked, or dropping the payload, may panic too.
        let again = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            panicked(&call, feature, outer, payload);
        }));
        if let Err(payload) = again {
            leak(payload);
        }
        R::FAULT
    })
}

/// Refuses a call to an entry a panic turned off, as the panic itself was
/// answered; [`entries!`] asks it once the caller and arguments are read, so
/// a mod that is off still gets [`Status::FAULTED`].
///
/// # Errors
///
/// [`Status::REFUSED`] while `entry` is off.
#[inline]
pub(crate) fn live(entry: &Fuse) -> Result<(), Refused> {
    if entry.is_off() {
        Err(Refused::with(Status::REFUSED))
    } else {
        Ok(())
    }
}

/// Leaks a panic payload whose drop already panicked once, so it never
/// drops again.
#[expect(
    clippy::mem_forget,
    reason = "dropping it again could unwind into the mod"
)]
fn leak(payload: Box<dyn core::any::Any + Send>) {
    core::mem::forget(payload);
}

/// Turns off what panicked and drops the payload, inside the second guard
/// of [`run`].
fn panicked(call: &Call, feature: &Fuse, outer: u32, payload: Box<dyn core::any::Any + Send>) {
    let text = crate::engine::early::panic_text(payload.as_ref()).to_string();
    drop(payload);
    let Some(owner) = crate::engine::profile::unwound(outer) else {
        feature.turn_off(&Off::fault(
            feature.feature(),
            &format!("the loader panicked in {}: {text}", call.name),
        ));
        return;
    };
    let reason = format!("panic in its callback inside {}: {text}", call.name);
    if let Some(loader) = crate::engine::loader::loader() {
        loader.turn_off(owner, &reason);
    } else {
        crate::log!(ERROR, "mod {owner}: {reason}");
    }
}

/// The running loader, for an entry that takes no mod handle.
///
/// # Errors
///
/// [`Status::BAD_ARGS`] before the loader runs, as no mod can call then.
pub(crate) fn loaded() -> Result<&'static Loader, Refused> {
    crate::engine::loader::loader().ok_or(Refused::with(Status::BAD_ARGS))
}

/// Writes `HostApi` entries from typed signatures, each with its own
/// [`Fuse`], named after the entry, that a panic in it turns off.
///
/// ```ignore
/// entries! {
///     fn item_info(loader as Caller, item: ItemId, out as HeadedOut<ItemData>) -> Status {
///         out.put(crate::inventory::transactions::item_info(loader, item.get())?)
///     }
/// }
/// ```
///
/// A parameter is `name: Type`, passed on as the mod gave it; `name as
/// Kind`, read through [`Arg`]; or `[a, b, ..] as Kind`, two to four of the
/// ABI's parameters read through [`Arg2`] to [`Arg4`] and bound to the first
/// name. Each kind's `# Safety` is what the ABI promises for a parameter of
/// that kind, so an entry names the kind its `HostApi` field documents. The
/// body gives a `Result` whose error converts into [`Refused`]; a missing
/// return type is `()`.
macro_rules! entries {
    (
        $(
            $(#[$attr:meta])*
            fn $name:ident ( $($params:tt)* ) $(-> $ret:ty)? $body:block
        )*
    ) => {
        $(
            $crate::host::layer::entries!(
                @param call [$(#[$attr])*] $name [$($ret)?] $body [] []
                $($params)*
            );
        )*
    };
    (@param $call:ident $attrs:tt $name:ident $ret:tt $body:block
        [$($raw:tt)*] [$($read:tt)*]
        $param:ident as $kind:ty $(, $($rest:tt)*)?
    ) => {
        $crate::host::layer::entries!(
            @param $call $attrs $name $ret $body
            [$($raw)* $param: <$kind as $crate::host::layer::Arg>::Raw,]
            [$($read)*
                // SAFETY: the entry's signature names the kind its `HostApi`
                // field documents for this parameter, and `Arg::from_abi`
                // needs no more than that promise.
                let $param = unsafe { <$kind as $crate::host::layer::Arg>::from_abi($param, $call) }?;
            ]
            $($($rest)*)?
        );
    };
    (@param $call:ident $attrs:tt $name:ident $ret:tt $body:block
        [$($raw:tt)*] [$($read:tt)*]
        [$a:ident, $b:ident] as $kind:ty $(, $($rest:tt)*)?
    ) => {
        $crate::host::layer::entries!(
            @param $call $attrs $name $ret $body
            [$($raw)*
                $a: <$kind as $crate::host::layer::Arg2>::First,
                $b: <$kind as $crate::host::layer::Arg2>::Second,
            ]
            [$($read)*
                // SAFETY: the entry's signature names the kind its `HostApi`
                // field documents for these parameters, and `Arg2::from_abi`
                // needs no more than that promise.
                let $a = unsafe { <$kind as $crate::host::layer::Arg2>::from_abi($a, $b, $call) }?;
            ]
            $($($rest)*)?
        );
    };
    (@param $call:ident $attrs:tt $name:ident $ret:tt $body:block
        [$($raw:tt)*] [$($read:tt)*]
        [$a:ident, $b:ident, $c:ident] as $kind:ty $(, $($rest:tt)*)?
    ) => {
        $crate::host::layer::entries!(
            @param $call $attrs $name $ret $body
            [$($raw)*
                $a: <$kind as $crate::host::layer::Arg3>::First,
                $b: <$kind as $crate::host::layer::Arg3>::Second,
                $c: <$kind as $crate::host::layer::Arg3>::Third,
            ]
            [$($read)*
                // SAFETY: the entry's signature names the kind its `HostApi`
                // field documents for these parameters, and `Arg3::from_abi`
                // needs no more than that promise.
                let $a = unsafe {
                    <$kind as $crate::host::layer::Arg3>::from_abi($a, $b, $c, $call)
                }?;
            ]
            $($($rest)*)?
        );
    };
    (@param $call:ident $attrs:tt $name:ident $ret:tt $body:block
        [$($raw:tt)*] [$($read:tt)*]
        [$a:ident, $b:ident, $c:ident, $d:ident] as $kind:ty $(, $($rest:tt)*)?
    ) => {
        $crate::host::layer::entries!(
            @param $call $attrs $name $ret $body
            [$($raw)*
                $a: <$kind as $crate::host::layer::Arg4>::First,
                $b: <$kind as $crate::host::layer::Arg4>::Second,
                $c: <$kind as $crate::host::layer::Arg4>::Third,
                $d: <$kind as $crate::host::layer::Arg4>::Fourth,
            ]
            [$($read)*
                // SAFETY: the entry's signature names the kind its `HostApi`
                // field documents for these parameters, and `Arg4::from_abi`
                // needs no more than that promise.
                let $a = unsafe {
                    <$kind as $crate::host::layer::Arg4>::from_abi($a, $b, $c, $d, $call)
                }?;
            ]
            $($($rest)*)?
        );
    };
    (@param $call:ident $attrs:tt $name:ident $ret:tt $body:block
        [$($raw:tt)*] [$($read:tt)*]
        $param:ident : $ty:ty $(, $($rest:tt)*)?
    ) => {
        $crate::host::layer::entries!(
            @param $call $attrs $name $ret $body
            [$($raw)* $param: $ty,] [$($read)*]
            $($($rest)*)?
        );
    };
    (@param $call:ident [$(#[$attr:meta])*] $name:ident [] $body:block
        $raw:tt $read:tt
    ) => {
        $crate::host::layer::entries!(
            @param $call [$(#[$attr])*] $name [()] $body $raw $read
        );
    };
    (@param $call:ident [$(#[$attr:meta])*] $name:ident [$ret:ty] $body:block
        [$($raw:tt)*] [$($read:tt)*]
    ) => {
        $(#[$attr])*
        pub(crate) extern "system" fn $name($($raw)*) -> $ret {
            static ENTRY: $crate::engine::fuse::Fuse =
                $crate::engine::fuse::Fuse::new(stringify!($name));
            $crate::host::layer::run(&ENTRY, |$call| {
                let _: &$crate::host::layer::Call = $call;
                $($read)*
                $crate::host::layer::live(&ENTRY)?;
                $body
            })
        }
    };
}

pub(crate) use entries;

#[cfg(test)]
mod tests;
