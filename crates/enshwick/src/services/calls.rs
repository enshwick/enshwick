//! Described services, calls: one function from an input struct to an
//! answer, which one mod serves and others call, from Rust or a script.
//!
//! A call is declared once with [`#[call]`](macro@crate::call). Its schema
//! goes into the server's manifest beside its points' when the manifest
//! lists it (`declare_mod!(.., calls: [..])`), and `cargo enshwick deps` writes the
//! declaration for another mod from it. The loader matches a caller with
//! the server by name and major once every mod is initialised: the input
//! and the answer must agree field by field and in size. A changed call is
//! a new major, which a server may serve beside the old one.

use core::ffi::c_void;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::abi::{CallSpec, Header, Layout, LayoutField, Status, Str};
use crate::load::{hand_over, ok};
use crate::points::DescribedField;
use crate::{Host, Load, Off};

/// A call: the input struct one mod hands another, which answers with
/// [`Call::Output`].
///
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// # Safety
///
/// - Implemented by [`#[call]`](macro@crate::call) only.
/// - The type is `repr(C)`.
/// - [`LAYOUT`] lists every field of the type in order, with its offset,
///   size and alignment in the hash, and [`OUTPUT`] the answer's type.
///
/// [`LAYOUT`]: Call::LAYOUT
/// [`OUTPUT`]: Call::OUTPUT
///
/// # Examples
///
/// ```
/// use enshwick::services::Call;
///
/// #[enshwick::call(name = "price-of", output = u32)]
/// #[derive(Clone, Copy, Debug)]
/// pub struct PriceOf {
///     pub item: u32,
/// }
///
/// assert_eq!(PriceOf::NAME, "you.hello:price-of");
/// assert_eq!(PriceOf::MAJOR, 1);
/// ```
pub unsafe trait Call: Copy + 'static {
    /// The call's name, `<server id>:<call>`.
    const NAME: &'static str;
    /// The incompatible version: calls of two majors share nothing.
    const MAJOR: u32;
    /// The input's fields, in order.
    const LAYOUT: &'static [LayoutField];
    /// The answer's type, as one field.
    const OUTPUT: &'static [LayoutField];
    /// The call's schema as `cargo enshwick deps` reads it from the server's
    /// manifest: its `[[call]]` table with the layout hashes, then the
    /// `[[type]]` tables of its input and answer.
    const SCHEMA: &'static str;
    /// The answer.
    type Output: Copy + 'static;
}

/// Why a call answered nothing.
///
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// # Examples
///
/// ```
/// use enshwick::services::CallError;
///
/// assert_eq!(CallError::Unmatched.to_string(), "no running mod serves the call as declared here");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
#[non_exhaustive]
pub enum CallError {
    /// No running mod serves it as declared here, or not every mod is
    /// initialised yet; the log says why once every mod is.
    #[error("no running mod serves the call as declared here")]
    Unmatched,
    /// The serving mod is off, or its handler, which a panic turns off
    /// until the next start.
    #[error("the serving mod or its handler is off")]
    Off,
    /// The server's handler panicked or refused this call.
    #[error("the server's handler failed")]
    Failed,
}

/// The error a call's status stands for, or `None` for success.
fn error_of(status: Status) -> Option<CallError> {
    match status {
        Status::OK => None,
        Status::MISSING => Some(CallError::Unmatched),
        Status::REFUSED => Some(CallError::Off),
        _ => Some(CallError::Failed),
    }
}

/// The ABI spec of `C`, borrowing its static layouts.
fn spec<C: Call>() -> CallSpec {
    CallSpec {
        header: Header::of::<CallSpec>(),
        name: Str::new(C::NAME),
        major: C::MAJOR,
        input: crate::services::layout(C::LAYOUT, size_of::<C>()),
        output: crate::services::layout(C::OUTPUT, size_of::<C::Output>()),
    }
}

/// Another mod's call `C`, from [`Load::caller`]: runs the server's handler
/// on this thread.
///
/// Unstable: needs the `unstable` feature and may change in any release.
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
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let price_of = load.caller::<PriceOf>()?;
///     load.on::<enshwick::events::WorldLoaded>(move |_| {
///         if let Ok(coins) = price_of.call(&PriceOf { item: 7 }) {
///             enshwick::info!("item 7 costs {coins}");
///         }
///     })
/// }
/// ```
#[derive(Debug)]
pub struct Caller<C> {
    host: Host,
    id: u32,
    _call: PhantomData<fn(C)>,
}

impl<C> Clone for Caller<C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C> Copy for Caller<C> {}

impl<C: Call> Caller<C> {
    /// Runs the call with `input` and returns the server's answer.
    ///
    /// # Errors
    ///
    /// [`CallError`] when no running mod serves the call as declared here,
    /// the server is off, or its handler failed.
    ///
    /// # Examples
    ///
    /// See [`Caller`].
    pub fn call(&self, input: &C) -> Result<C::Output, CallError> {
        let mut answer = core::mem::MaybeUninit::<C::Output>::uninit();
        let status = (self.host.api().call)(
            self.host.handle(),
            self.id,
            core::ptr::from_ref(input).cast(),
            answer.as_mut_ptr().cast(),
        );
        match error_of(status) {
            // SAFETY: an OK status means the server's handler wrote an
            // answer of the matched type, the same as `C::Output` field by
            // field and in size.
            None => Ok(unsafe { answer.assume_init() }),
            Some(error) => Err(error),
        }
    }
}

/// A call described at run time, such as by the schema a script carries.
///
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// # Examples
///
/// ```
/// use enshwick::points::DescribedField;
/// use enshwick::services::DescribedCall;
///
/// let price_of = DescribedCall::new(
///     "you.trading-post:price-of",
///     1,
///     (vec![DescribedField::new("item", 1)], 4),
///     (vec![DescribedField::new("output", 2)], 4),
/// );
/// assert_eq!(price_of.input_size, 4);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DescribedCall {
    /// The call's name, `<server id>:<call>`.
    pub name: String,
    /// Its major.
    pub major: u32,
    /// The input's fields, each with its layout hash.
    pub input: Vec<DescribedField>,
    /// The input's size in bytes.
    pub input_size: u32,
    /// The answer, one field, with its layout hash.
    pub output: Vec<DescribedField>,
    /// The answer's size in bytes.
    pub output_size: u32,
}

impl DescribedCall {
    /// The call `name` of `major`, with the input's and the answer's
    /// fields and sizes.
    ///
    /// # Examples
    ///
    /// See [`DescribedCall`].
    #[must_use]
    pub fn new(
        name: &str,
        major: u32,
        input: (Vec<DescribedField>, u32),
        output: (Vec<DescribedField>, u32),
    ) -> Self {
        Self {
            name: name.to_owned(),
            major,
            input: input.0,
            input_size: input.1,
            output: output.0,
            output_size: output.1,
        }
    }

    /// Calls `with` with the call's ABI spec, which borrows from `self`.
    fn with_spec<R>(&self, with: impl FnOnce(&CallSpec) -> R) -> R {
        let raw = |fields: &[DescribedField]| -> Vec<LayoutField> {
            fields
                .iter()
                .map(|f| LayoutField {
                    name: Str::new(&f.name),
                    hash: f.hash,
                })
                .collect()
        };
        let (input, output) = (raw(&self.input), raw(&self.output));
        let layout = |fields: &[LayoutField], size: u32| Layout {
            fields: fields.as_ptr(),
            count: u32::try_from(fields.len()).unwrap_or(u32::MAX),
            size,
        };
        let spec = CallSpec {
            header: Header::of::<CallSpec>(),
            name: Str::new(&self.name),
            major: self.major,
            input: layout(&input, self.input_size),
            output: layout(&output, self.output_size),
        };
        with(&spec)
    }
}

/// Another mod's described call, from [`Load::described_caller`].
///
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// # Examples
///
/// ```
/// use enshwick::points::DescribedField;
/// use enshwick::services::DescribedCall;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let price_of = DescribedCall::new(
///         "you.trading-post:price-of",
///         1,
///         (vec![DescribedField::new("item", 1)], 4),
///         (vec![DescribedField::new("output", 2)], 4),
///     );
///     // SAFETY: the input is one `u32`, for which any bytes are valid.
///     let price_of = unsafe { load.described_caller(&price_of) }?;
///     let mut coins = [0u8; 4];
///     let _answer = price_of.call(&7u32.to_le_bytes(), &mut coins);
///     Ok(())
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct DescribedCaller {
    host: Host,
    id: u32,
    input_size: usize,
    output_size: usize,
}

impl DescribedCaller {
    /// Runs the call with `input`, the input's bytes, and writes the answer's
    /// bytes to `output`.
    ///
    /// # Errors
    ///
    /// [`CallError::Failed`] when `input` or `output` is not the size the
    /// description gives, else as [`Caller::call`].
    ///
    /// # Examples
    ///
    /// See [`DescribedCaller`].
    pub fn call(&self, input: &[u8], output: &mut [u8]) -> Result<(), CallError> {
        if input.len() != self.input_size || output.len() != self.output_size {
            return Err(CallError::Failed);
        }
        // An empty slice may dangle; the loader takes a pointer to one byte.
        let mut none = 0u8;
        let input_ptr = if input.is_empty() {
            (&raw const none).cast()
        } else {
            input.as_ptr().cast()
        };
        let output_ptr = if output.is_empty() {
            (&raw mut none).cast()
        } else {
            output.as_mut_ptr().cast()
        };
        let status = (self.host.api().call)(self.host.handle(), self.id, input_ptr, output_ptr);
        error_of(status).map_or(Ok(()), Err)
    }
}

/// A described server's handler and its sizes.
struct Described<F> {
    input_size: usize,
    output_size: usize,
    run: F,
}

impl Load<'_> {
    /// Serves the call `C` with `handler` for other mods to call, from the
    /// caller's thread. A panic in `handler` fails that call alone.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// `C` comes from [`#[call]`](macro@crate::call), named after this mod:
    /// `<id>:<call>`. List it in the manifest's `calls` so its schema reaches
    /// other mods' tools.
    ///
    /// # Errors
    ///
    /// [`Off`] when the name is outside this mod's id, the call and major
    /// are served already, or when called after `init`.
    ///
    /// # Examples
    ///
    /// ```
    /// #[enshwick::call(name = "price-of", output = u32)]
    /// #[derive(Clone, Copy, Debug)]
    /// pub struct PriceOf {
    ///     pub item: u32,
    /// }
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.serve_call(|input: &PriceOf| input.item * 10)
    /// }
    /// ```
    pub fn serve_call<C, F>(&mut self, handler: F) -> Result<(), Off>
    where
        C: Call,
        F: Fn(&C) -> C::Output + Send + Sync + 'static,
    {
        let host = self.host();
        let spec = spec::<C>();
        hand_over(
            Served::new(C::NAME, handler),
            |user| (host.api().serve_call)(host.handle(), &raw const spec, run_typed::<C, F>, user),
            &format!(
                "call {} v{}: named after another mod or served twice",
                C::NAME,
                C::MAJOR
            ),
        )
    }

    /// Asks for another mod's call `C`, and returns the handle that runs it.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// The loader matches it once every mod is initialised: with no running
    /// mod serving `C` as declared here, it logs why, and every call answers
    /// [`CallError::Unmatched`]. Declare the server in the manifest with
    /// `recommends`, or `requires` when this mod is pointless without it: a
    /// required server initialises first, so the loader matches the call
    /// at once.
    ///
    /// # Errors
    ///
    /// [`Off`] when called after `init`, when this mod asked for `C`
    /// already with another declaration, or when this mod requires the
    /// server and it does not serve `C` as declared here; the log says why.
    ///
    /// # Examples
    ///
    /// See [`Caller`].
    pub fn caller<C: Call>(&mut self) -> Result<Caller<C>, Off> {
        let spec = spec::<C>();
        let mut id = 0;
        let status =
            (self.host().api().find_call)(self.host().handle(), &raw const spec, &raw mut id);
        ok(
            status,
            Caller {
                host: self.host(),
                id,
                _call: PhantomData,
            },
            || format!("call {} v{}", C::NAME, C::MAJOR),
        )
    }

    /// Serves the described call `call` with `handler`, which gets the
    /// input's bytes and writes the answer's, as [`Load::serve_call`] does for a
    /// type; it returns `false` to fail the call.
    ///
    /// # Errors
    ///
    /// As [`Load::serve_call`].
    ///
    /// # Safety
    ///
    /// - Every answer `handler` writes and returns `true` for is a valid
    ///   value of the type the answer's hash names, since a caller reads it
    ///   as that type: a `bool` is 0 or 1, an enum one of its variants.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::points::DescribedField;
    /// use enshwick::services::DescribedCall;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let price_of = DescribedCall::new(
    ///         "you.trading-post:price-of",
    ///         1,
    ///         (vec![DescribedField::new("item", 1)], 4),
    ///         (vec![DescribedField::new("output", 2)], 4),
    ///     );
    ///     // SAFETY: the answer is one `u32`, for which any bytes are valid.
    ///     unsafe {
    ///         load.serve_described_call(&price_of, |input, output| {
    ///             output.copy_from_slice(input);
    ///             true
    ///         })
    ///     }
    /// }
    /// ```
    pub unsafe fn serve_described_call<F>(
        &mut self,
        call: &DescribedCall,
        handler: F,
    ) -> Result<(), Off>
    where
        F: Fn(&[u8], &mut [u8]) -> bool + Send + Sync + 'static,
    {
        let host = self.host();
        let server = Served::new(
            &call.name,
            Described {
                input_size: usize::try_from(call.input_size).unwrap_or(0),
                output_size: usize::try_from(call.output_size).unwrap_or(0),
                run: handler,
            },
        );
        call.with_spec(|spec| {
            hand_over(
                server,
                |user| (host.api().serve_call)(host.handle(), spec, run_described::<F>, user),
                &format!(
                    "call {} v{}: named after another mod or served twice",
                    call.name, call.major
                ),
            )
        })
    }

    /// Asks for another mod's described call `call`, as [`Load::caller`]
    /// does for a type.
    ///
    /// # Errors
    ///
    /// As [`Load::caller`].
    ///
    /// # Safety
    ///
    /// - Every input passed to the returned caller is a valid value of the
    ///   type each input field's hash names, since the server reads it as
    ///   that type: a `bool` is 0 or 1, an enum one of its variants.
    ///
    /// # Examples
    ///
    /// See [`DescribedCaller`].
    pub unsafe fn described_caller(
        &mut self,
        call: &DescribedCall,
    ) -> Result<DescribedCaller, Off> {
        let host = self.host();
        let mut id = 0;
        let status =
            call.with_spec(|spec| (host.api().find_call)(host.handle(), spec, &raw mut id));
        ok(
            status,
            DescribedCaller {
                host,
                id,
                input_size: usize::try_from(call.input_size).unwrap_or(0),
                output_size: usize::try_from(call.output_size).unwrap_or(0),
            },
            || format!("call {} v{}", call.name, call.major),
        )
    }
}

/// A server's handler, and whether a panic turned it off.
pub(crate) struct Served<F> {
    name: String,
    off: AtomicBool,
    run: F,
}

impl<F> Served<F> {
    /// `run`, serving the call `name`.
    pub(crate) fn new(name: &str, run: F) -> Self {
        Self {
            name: name.to_owned(),
            off: AtomicBool::new(false),
            run,
        }
    }

    /// Turns the handler off after its first panic, saying so once.
    fn panicked(&self) -> Status {
        if !self.off.swap(true, Ordering::AcqRel) {
            crate::warn!(
                "call {}: the handler panicked and is off until the next start; calls answer that the server is off",
                self.name
            );
        }
        Status::FAULTED
    }
}

/// Runs a typed server; `user` is the [`Served`] boxed by
/// [`Load::serve_call`], alive for the process. The first panic turns the
/// handler off and leaves the mod on.
pub(crate) unsafe extern "system" fn run_typed<C: Call, F: Fn(&C) -> C::Output>(
    input: *const c_void,
    output: *mut c_void,
    user: *mut c_void,
) -> Status {
    // SAFETY: `user` is the leaked `Box<Served<F>>` this function was served
    // with.
    let served = unsafe { &*user.cast::<Served<F>>() };
    if served.off.load(Ordering::Acquire) {
        return Status::REFUSED;
    }
    if input.is_null() || output.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: the loader matched the caller's input to `C` field by field
    // and in size, `#[call]` refuses padding so every byte is initialised,
    // the caller's bytes are a valid `C` (a typed caller's own, or a
    // described caller's under its safety contract), live for the call, and
    // may be unaligned.
    // The calling mod's input, not game memory.
    // ast-grep-ignore: game-memory-through-raw
    let input = unsafe { input.cast::<C>().read_unaligned() };
    let output = output.cast::<C::Output>();
    let Ok(answer) =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (served.run)(&input)))
    else {
        return served.panicked();
    };
    // SAFETY: the loader matched the caller's answer to `C::Output` field by
    // field and in size; the caller's buffer lives for the call, written by
    // nothing else meanwhile, and may be unaligned. `#[call]` refuses
    // padding, so a described caller reads only initialised bytes.
    unsafe { output.write_unaligned(answer) };
    Status::OK
}

/// Runs a described server; `user` is the [`Served`] boxed by
/// [`Load::serve_described_call`], alive for the process, its sizes beside
/// the handler.
unsafe extern "system" fn run_described<F: Fn(&[u8], &mut [u8]) -> bool>(
    input: *const c_void,
    output: *mut c_void,
    user: *mut c_void,
) -> Status {
    // SAFETY: `user` is the leaked `Box<Served<Described<F>>>` this function was
    // served with.
    let served = unsafe { &*user.cast::<Served<Described<F>>>() };
    if served.off.load(Ordering::Acquire) {
        return Status::REFUSED;
    }
    if input.is_null() || output.is_null() {
        return Status::BAD_ARGS;
    }
    let sized = &served.run;
    // SAFETY: the loader matched the caller's input to the description in
    // size, so `input_size` bytes are readable for the call, and initialised:
    // `#[call]` refuses padding in a typed caller's input.
    let input = unsafe { core::slice::from_raw_parts(input.cast::<u8>(), sized.input_size) };
    // SAFETY: the loader matched the caller's answer to the description in
    // size, so `output_size` bytes are writable for the call, written by
    // nothing else meanwhile.
    unsafe { core::ptr::write_bytes(output.cast::<u8>(), 0, sized.output_size) };
    // SAFETY: the same bytes, zeroed just before, so initialised.
    let answer = unsafe { core::slice::from_raw_parts_mut(output.cast::<u8>(), sized.output_size) };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (sized.run)(input, answer))) {
        Ok(true) => Status::OK,
        Ok(false) => Status::FAULTED,
        Err(_) => served.panicked(),
    }
}
