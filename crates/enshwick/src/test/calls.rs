//! The fake loader's calls: the calls the mod serves, which a test runs as
//! another mod would, and the calls the mod asks for, which a test serves
//! as their server would.

use core::ffi::c_void;

use crate::abi::{CallFn, CallSpec, ModHandle, Status};
#[cfg(feature = "unstable")]
use crate::services::{Call, CallError};

use super::Installed;
use super::table::{put, with};

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
        let fields = |layout: &[crate::abi::LayoutField]| {
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

/// A served call: its declaration, handler and pointer.
#[derive(Clone)]
struct Served {
    declared: Declared,
    callback: CallFn,
    user: usize,
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
    let spec = super::table::get_headed(spec)?;
    // SAFETY: the safe layer passes the spec's name borrowed and unchanged
    // for the call.
    let name = unsafe { spec.name.as_str() }?.to_owned();
    // SAFETY: the safe layer passes the input layout, its fields and their
    // names borrowed and unchanged for the call.
    let input = unsafe { crate::services::read_layout(&spec.input) }?;
    // SAFETY: the safe layer passes the output layout, its fields and their
    // names borrowed and unchanged for the call.
    let output = unsafe { crate::services::read_layout(&spec.output) }?;
    Some(Declared {
        name,
        major: spec.major,
        input,
        output,
        input_size: spec.input.size,
        output_size: spec.output.size,
    })
}

pub(crate) extern "system" fn serve_call(
    _me: ModHandle,
    spec: *const CallSpec,
    callback: CallFn,
    user: *mut c_void,
) -> Status {
    let Some(declared) = declared(spec) else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        let calls = &mut s.calls;
        if calls
            .served
            .iter()
            .any(|c| c.declared.name == declared.name && c.declared.major == declared.major)
        {
            return Status::REFUSED;
        }
        calls.served.push(Served {
            declared,
            callback,
            user: user.expose_provenance(),
        });
        Status::OK
    })
}

pub(crate) extern "system" fn find_call(
    _me: ModHandle,
    spec: *const CallSpec,
    out: *mut u32,
) -> Status {
    let Some(declared) = declared(spec) else {
        return Status::BAD_ARGS;
    };
    let id = with(None, |s| {
        let required = s
            .manifest
            .as_ref()
            .is_some_and(|manifest| manifest.requires_owner_of(&declared.name));
        let unexported = s
            .manifest
            .as_ref()
            .is_some_and(|manifest| manifest.unexported(&declared.name, &s.host.exports));
        if unexported || (required && !s.calls.others.iter().any(|o| o.declared == declared)) {
            return None;
        }
        let wanted = &mut s.calls.wanted;
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

pub(crate) extern "system" fn call(
    _me: ModHandle,
    id: u32,
    input: *const c_void,
    output: *mut c_void,
) -> Status {
    if input.is_null() || output.is_null() {
        return Status::BAD_ARGS;
    }
    let server = with(Err(Status::MISSING), |s| {
        let wanted = s.calls.wanted.get(id as usize).ok_or(Status::BAD_ARGS)?;
        s.calls
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
        Ok(server) => unsafe {
            (server.callback)(
                input,
                output,
                core::ptr::with_exposed_provenance_mut(server.user),
            )
        },
        Err(status) => status,
    }
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// Serves the call `C` with `handler`, as another mod would, for the
    /// mod's [`Caller`](crate::services::Caller)s of `C`.
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
        let user = Box::into_raw(Box::new(crate::services::Served::new(C::NAME, handler)));
        with((), |s| {
            s.calls.others.push(Served {
                declared: Declared::of::<C>(),
                callback: crate::services::run_typed::<C, F>,
                user: user.expose_provenance(),
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
            s.calls
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
        let status = unsafe {
            (server.callback)(
                core::ptr::from_ref(input).cast(),
                answer.as_mut_ptr().cast(),
                core::ptr::with_exposed_provenance_mut(server.user),
            )
        };
        match status {
            // SAFETY: an OK status means the handler wrote an answer of the
            // matched type, `C::Output`.
            Status::OK => Ok(unsafe { answer.assume_init() }),
            Status::MISSING => Err(CallError::Unmatched),
            Status::REFUSED => Err(CallError::Off),
            _ => Err(CallError::Failed),
        }
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
            s.calls
                .served
                .iter()
                .map(|c| format!("{} v{}", c.declared.name, c.declared.major))
                .collect()
        })
    }
}
