//! The fake loader's calls: the calls the mod serves, which a test runs as
//! another mod would, and the calls the mod asks for, which a test serves
//! as their server would.

use crate::test::table::base::args;
use crate::test::table::entries;
use core::ffi::c_void;

use enshwick_core::abi::{CallFn, CallSpec, Status};
#[cfg(feature = "unstable")]
use enshwick_core::services::{Call, CallError};

use crate::test::Installed;
use crate::test::table::{Registered, put, with};

/// A call as declared: name, major, and the input's and answer's layouts
/// and sizes.
#[derive(Clone, PartialEq, Eq)]
struct Declared {
    name: String,
    major: u32,
    input: Vec<(String, u64)>,
    output: Vec<(String, u64)>,
    input_size: u32,
    output_size: u32,
}

#[cfg(feature = "unstable")]
impl Declared {
    fn of<C: Call>() -> Self {
        let fields = |layout: &[enshwick_core::abi::LayoutField]| {
            layout
                .iter()
                // SAFETY: the declaration's names are static strings.
                .filter_map(|f| unsafe { f.name.as_str() }.map(|n| (n.to_owned(), f.hash)))
                .collect()
        };
        Self {
            name: C::NAME.to_owned(),
            major: C::MAJOR,
            input: fields(C::LAYOUT),
            output: fields(C::OUTPUT),
            input_size: u32::try_from(size_of::<C>()).unwrap_or(u32::MAX),
            output_size: u32::try_from(size_of::<C::Output>()).unwrap_or(u32::MAX),
        }
    }
}

/// A served call: its declaration, and its handler with the pointer and
/// the handle that served it.
#[derive(Clone)]
struct Served {
    declared: Declared,
    handler: Registered<CallFn>,
}

/// The fake's calls.
#[derive(Default)]
pub(crate) struct FakeCalls {
    /// What the mod serves.
    served: Vec<Served>,
    /// What the mod asked for, by id.
    wanted: Vec<Declared>,
    /// What tests serve as other mods.
    others: Vec<Served>,
}

/// The declaration a spec carries; `None` for a bad one.
fn declared(spec: *const CallSpec) -> Option<Declared> {
    let spec = crate::test::table::get_headed(spec)?;
    // SAFETY: the safe layer passes the spec's name borrowed and unchanged
    // for the call.
    let name = unsafe { spec.name.as_str() }?.to_owned();
    // SAFETY: the safe layer passes the input layout, its fields and their
    // names borrowed and unchanged for the call.
    let input = unsafe { enshwick_core::services::read_layout(&spec.input) }?;
    // SAFETY: the safe layer passes the output layout, its fields and their
    // names borrowed and unchanged for the call.
    let output = unsafe { enshwick_core::services::read_layout(&spec.output) }?;
    Some(Declared {
        name,
        major: spec.major,
        input,
        output,
        input_size: spec.input.size,
        output_size: spec.output.size,
    })
}

entries! {
    pub(crate) fn serve_call(
        me as Mod,
        spec: *const CallSpec as args::Headed,
        callback: Option<CallFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        let Some(declared) = declared(spec) else {
            return Status::BAD_ARGS;
        };
        crate::test::table::with_open(|s| {
            let calls = &mut s.base.calls;
            if calls
                .served
                .iter()
                .any(|c| c.declared.name == declared.name && c.declared.major == declared.major)
            {
                return Status::REFUSED;
            }
            calls.served.push(Served {
                declared,
                handler: Registered::new(callback, user, me),
            });
            Status::OK
        })
    }

    pub(crate) fn find_call(
        me as Mod,
        spec: *const CallSpec as args::Headed,
        out: *mut u32 as args::Out,
    ) -> Status {
        let Some(declared) = declared(spec) else {
            return Status::BAD_ARGS;
        };
        if crate::test::table::late() {
            return Status::TOO_LATE;
        }
        let id = with(None, |s| {
            let required = s
                .base
                .manifest
                .as_ref()
                .is_some_and(|manifest| manifest.requires_owner_of(&declared.name));
            let unexported = s.base.manifest.as_ref().is_some_and(|manifest| {
                manifest.unexported(
                    enshwick_core::schema_section::Kind::Call,
                    &declared.name,
                    &s.host.base.exports,
                )
            });
            if unexported || (required && !s.base.calls.others.iter().any(|o| o.declared == declared)) {
                return None;
            }
            let wanted = &mut s.base.calls.wanted;
            let index = match wanted
                .iter()
                .position(|w| w.name == declared.name && w.major == declared.major)
            {
                Some(index) if wanted[index] == declared => index,
                Some(_) => return None,
                None => {
                    wanted.push(declared);
                    wanted.len() - 1
                }
            };
            u32::try_from(index).ok()
        });
        match id {
            Some(id) => put(out, id),
            None => Status::REFUSED,
        }
    }

    pub(crate) fn call(
        me as Mod,
        id: u32,
        input: *const c_void,
        output: *mut c_void,
    ) -> Status {
        if input.is_null() || output.is_null() {
            return Status::BAD_ARGS;
        }
        let server = with(Err(Status::MISSING), |s| {
            let wanted = s
                .base
                .calls
                .wanted
                .get(id as usize)
                .ok_or(Status::BAD_ARGS)?;
            s.base
                .calls
                .others
                .iter()
                .find(|o| o.declared == *wanted)
                .cloned()
                .ok_or(Status::MISSING)
        });
        match server {
            // SAFETY: the test served the handler for a declaration equal to the
            // mod's field by field and in size, and the safe layer passes input
            // and answer that large, live for the call; the state lock is not
            // held, so the handler may call the fake.
            Ok(server) => server
                .handler
                .run(|callback, user| unsafe { callback(input, output, user) }),
            Err(status) => status,
        }
    }
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// Serves the call `C` with `handler`, as another mod would, for the
    /// mod's [`Caller`](enshwick_core::services::Caller)s of `C`.
    ///
    /// Serve a call of a mod the mod requires before the mod's init: a
    /// required mod initialises first, so the fake, as the loader, refuses
    /// a caller of a call it does not serve by then.
    ///
    /// # Examples
    ///
    /// ```
    /// #[enshwick::call(of = trading_post, name = "price-of", output = u32)]
    /// #[derive(Clone, Copy, Debug)]
    /// pub struct PriceOf {
    ///     pub item: u32,
    /// }
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.serve_call(|input: &PriceOf| input.item * 10);
    /// ```
    #[cfg(feature = "unstable")]
    pub fn serve_call<C, F>(&self, handler: F)
    where
        C: Call,
        F: Fn(&C) -> C::Output + Send + Sync + 'static,
    {
        let (run, user) = enshwick_core::__fake::served_call(handler);
        with((), |s| {
            s.base.calls.others.push(Served {
                declared: Declared::of::<C>(),
                handler: Registered::unowned(run, user),
            });
        });
    }

    /// Calls the mod's own call `C` as another mod would.
    ///
    /// # Errors
    ///
    /// [`CallError::Unmatched`] when the mod serves no call that matches
    /// `C`, or what its handler answers.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::services::CallError;
    ///
    /// #[enshwick::call(name = "price-of", output = u32)]
    /// #[derive(Clone, Copy, Debug)]
    /// pub struct PriceOf {
    ///     pub item: u32,
    /// }
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.call(&PriceOf { item: 1 }), Err(CallError::Unmatched));
    /// ```
    #[cfg(feature = "unstable")]
    pub fn call<C: Call>(&self, input: &C) -> Result<C::Output, CallError> {
        let wanted = Declared::of::<C>();
        let server = with(None, |s| {
            s.base
                .calls
                .served
                .iter()
                .find(|c| c.declared == wanted)
                .cloned()
        });
        let Some(server) = server else {
            return Err(CallError::Unmatched);
        };
        let mut answer = core::mem::MaybeUninit::<C::Output>::uninit();
        // SAFETY: the mod served the handler for a declaration equal to `C`'s
        // field by field and in size; input and answer live for the call.
        let status = server.handler.run(|callback, user| unsafe {
            callback(
                core::ptr::from_ref(input).cast(),
                answer.as_mut_ptr().cast(),
                user,
            )
        });
        enshwick_core::__fake::outcome::<CallError>(status)?;
        // SAFETY: an OK status means the handler wrote an answer of the
        // matched type, `C::Output`.
        Ok(unsafe { answer.assume_init() })
    }

    /// The calls the mod serves, as `<name> v<major>`, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.served_calls().is_empty());
    /// ```
    #[must_use]
    pub fn served_calls(&self) -> Vec<String> {
        with(Vec::new(), |s| {
            s.base
                .calls
                .served
                .iter()
                .map(|c| format!("{} v{}", c.declared.name, c.declared.major))
                .collect()
        })
    }
}
