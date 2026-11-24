//! The one check of the calling mod every fake entry passes through, as the
//! loader's entry layer reads the handle before an entry's body runs.

use enshwick_core::abi::{ModHandle, Status};

use crate::test::table::State;

/// The handle the loader's own copy of the API calls with while it runs
/// data packs, as the loader's.
pub(crate) const LOADER_HANDLE: ModHandle = ModHandle(0);

/// Who may call an entry, as its twin in the loader names its caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A loaded mod that is on: [`Status::FAULTED`] for one that is off.
    Mod,
    /// A loaded mod, on or off: the entries a mod that is off still calls,
    /// such as its fault and the list of mods.
    AnyMod,
    /// A mod that is on, as [`Kind::Mod`], or the loader's own copy.
    Caller,
    /// A loaded mod, on or off, or the loader's own copy: the entries whose
    /// answer goes under the caller's id, such as its log and its lookups.
    Writer,
}

impl Kind {
    /// The loader's answer to `me` before the entry's body runs, `None`
    /// when the body runs: [`Status::BAD_ARGS`] for a handle the loader
    /// never gave out, [`Status::FAULTED`] for a mod that is off where the
    /// kind wants it on.
    pub(crate) fn refusal(self, s: &State, me: ModHandle) -> Option<Status> {
        if me == LOADER_HANDLE && matches!(self, Self::Caller | Self::Writer) {
            return None;
        }
        if !s.host.base.loads(me) {
            return Some(Status::BAD_ARGS);
        }
        let wants_on = matches!(self, Self::Mod | Self::Caller);
        (wants_on && s.base.off.contains(&me)).then_some(Status::FAULTED)
    }
}

/// The refusal [`Kind::refusal`] gives `me` on the installed fake;
/// [`Status::MISSING`] while none is installed, as every call after its
/// [`Installed`](crate::test::Installed) dropped answers.
pub(crate) fn refusal(kind: Kind, me: ModHandle) -> Option<Status> {
    crate::test::table::with(Some(Status::MISSING), |s| kind.refusal(s, me))
}

/// Writes the fake's `HostApi` entries, each answering its caller and its
/// parameters as the loader's `entries!` does before its body runs.
///
/// ```ignore
/// entries! {
///     pub(crate) fn symbol(me as Writer, name: Str as Text, out: *mut u64 as Out) -> Status {
///         found(with(None, |s| s.host.base.symbols.get(&text(name)).copied()), out)
///     }
/// }
/// ```
///
/// The first parameter is the caller's handle with its [`Kind`]; a later
/// one is `name: Type`, passed on unchecked; `name: Type as Kind`, refused
/// as the kind of [`args`](super::args) says; or `[a: A, b: B, ..] as
/// Kind`, two to four of the ABI's parameters checked together. Each kind
/// is the one the entry's twin in the loader's `host/entries` names for the
/// parameter, so both refuse alike, in the same order; the body gets every
/// parameter as the mod passed it.
macro_rules! entries {
    (
        $(
            $(#[$attr:meta])*
            $vis:vis fn $name:ident (
                $me:ident as $kind:ident $(, $($params:tt)*)?
            ) -> Status $body:block
        )*
    ) => {
        $(
            $crate::test::table::base::caller::entries!(
                @param [$(#[$attr])*] [$vis] $name $me $kind $body [] []
                $($($params)*)?
            );
        )*
    };
    (@param $attrs:tt $vis:tt $name:ident $me:ident $kind:ident $body:block
        [$($raw:tt)*] [$($check:tt)*]
        [$a:ident : $at:ty, $b:ident : $bt:ty $(,)?] as $k:ty $(, $($rest:tt)*)?
    ) => {
        $crate::test::table::base::caller::entries!(
            @param $attrs $vis $name $me $kind $body
            [$($raw)* $a: $at, $b: $bt,]
            [$($check)*
                if let Some(refused) =
                    <$k as $crate::test::table::base::args::Check2<$at, $bt>>::refusal(&$a, &$b)
                {
                    return refused;
                }
            ]
            $($($rest)*)?
        );
    };
    (@param $attrs:tt $vis:tt $name:ident $me:ident $kind:ident $body:block
        [$($raw:tt)*] [$($check:tt)*]
        [$a:ident : $at:ty, $b:ident : $bt:ty, $c:ident : $ct:ty $(,)?] as $k:ty
        $(, $($rest:tt)*)?
    ) => {
        $crate::test::table::base::caller::entries!(
            @param $attrs $vis $name $me $kind $body
            [$($raw)* $a: $at, $b: $bt, $c: $ct,]
            [$($check)*
                if let Some(refused) =
                    <$k as $crate::test::table::base::args::Check3<$at, $bt, $ct>>::refusal(
                        &$a, &$b, &$c,
                    )
                {
                    return refused;
                }
            ]
            $($($rest)*)?
        );
    };
    (@param $attrs:tt $vis:tt $name:ident $me:ident $kind:ident $body:block
        [$($raw:tt)*] [$($check:tt)*]
        [$a:ident : $at:ty, $b:ident : $bt:ty, $c:ident : $ct:ty, $d:ident : $dt:ty $(,)?]
        as $k:ty $(, $($rest:tt)*)?
    ) => {
        $crate::test::table::base::caller::entries!(
            @param $attrs $vis $name $me $kind $body
            [$($raw)* $a: $at, $b: $bt, $c: $ct, $d: $dt,]
            [$($check)*
                if let Some(refused) =
                    <$k as $crate::test::table::base::args::Check4<$at, $bt, $ct, $dt>>::refusal(
                        &$a, &$b, &$c, &$d,
                    )
                {
                    return refused;
                }
            ]
            $($($rest)*)?
        );
    };
    (@param $attrs:tt $vis:tt $name:ident $me:ident $kind:ident $body:block
        [$($raw:tt)*] [$($check:tt)*]
        $p:ident : $t:ty as $k:ty $(, $($rest:tt)*)?
    ) => {
        $crate::test::table::base::caller::entries!(
            @param $attrs $vis $name $me $kind $body
            [$($raw)* $p: $t,]
            [$($check)*
                if let Some(refused) =
                    <$k as $crate::test::table::base::args::Check<$t>>::refusal(&$p)
                {
                    return refused;
                }
            ]
            $($($rest)*)?
        );
    };
    (@param $attrs:tt $vis:tt $name:ident $me:ident $kind:ident $body:block
        [$($raw:tt)*] [$($check:tt)*]
        $p:ident : $t:ty $(, $($rest:tt)*)?
    ) => {
        $crate::test::table::base::caller::entries!(
            @param $attrs $vis $name $me $kind $body
            [$($raw)* $p: $t,] [$($check)*]
            $($($rest)*)?
        );
    };
    (@param [$(#[$attr:meta])*] [$vis:vis] $name:ident $me:ident $kind:ident $body:block
        [$($raw:tt)*] [$($check:tt)*]
    ) => {
        $(#[$attr])*
        $vis extern "system" fn $name(
            $me: ::enshwick_core::abi::ModHandle,
            $($raw)*
        ) -> ::enshwick_core::abi::Status {
            if let Some(refused) = $crate::test::table::base::caller::refusal(
                $crate::test::table::base::caller::Kind::$kind,
                $me,
            ) {
                return refused;
            }
            $($check)*
            $body
        }
    };
}

pub(crate) use entries;
