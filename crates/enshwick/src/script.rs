//! What the API offers a script language, described beside the Rust items.
//!
//! [`#[derive(Script)]`](macro@crate::Script) describes types,
//! [`#[script]`](macro@crate::script) methods, and the tables of
//! [`events`](crate::events) events; runtimes and definitions read them.
//!
//! # Examples
//!
//! ```
//! use enshwick::script::{IntoScript, Ty, Typed, Value};
//!
//! #[derive(Clone, Debug, enshwick::Script)]
//! /// A coin purse.
//! pub struct Purse {
//!     /// Coins in it.
//!     pub coins: u32,
//! }
//!
//! let Ty::Record(record) = Purse::TY else { unreachable!() };
//! assert_eq!(record.fields()[0].name(), "coins");
//! assert!(matches!(Purse { coins: 3 }.into_value(), Value::Record(_)));
//! ```

use core::ffi::c_void;
use std::sync::Arc;

pub(crate) mod convert;
#[cfg(test)]
mod tests;

/// A value's type as a script language sees it.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum Ty {
    /// No value.
    Nil,
    /// `true` or `false`.
    Bool,
    /// A number; integers keep their Rust range, checked on the way in.
    Number,
    /// Text; a 64-bit key is its decimal digits, since a script number
    /// holds 53 bits.
    String,
    /// Three numbers, such as a position in metres.
    Vector,
    /// The inner type or nothing.
    Optional(&'static Ty),
    /// A list of the inner type.
    List(&'static Ty),
    /// A named type with fields.
    Record(&'static Record),
    /// One of a set of names.
    Choice(&'static Choice),
    /// A function the script passes in.
    Function(&'static Signature),
}

/// A named type with named fields, from [`#[derive(Script)]`](macro@crate::Script).
///
/// # Examples
///
/// See the [module](self).
#[derive(Debug)]
pub struct Record {
    name: &'static str,
    doc: &'static str,
    fields: &'static [Field],
}

impl Record {
    #[doc(hidden)]
    #[must_use]
    pub const fn new(name: &'static str, doc: &'static str, fields: &'static [Field]) -> Self {
        Self { name, doc, fields }
    }

    /// The Rust type's name.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// The type's doc comment up to its first section.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn doc(&self) -> &'static str {
        self.doc
    }

    /// Its fields in order; a [`Value::Record`] holds their values in the
    /// same order.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn fields(&self) -> &'static [Field] {
        self.fields
    }
}

/// A field of a [`Record`].
///
/// # Examples
///
/// See the [module](self).
#[derive(Debug)]
pub struct Field {
    name: &'static str,
    doc: &'static str,
    ty: &'static Ty,
}

impl Field {
    #[doc(hidden)]
    #[must_use]
    pub const fn new(name: &'static str, doc: &'static str, ty: &'static Ty) -> Self {
        Self { name, doc, ty }
    }

    /// The Rust field's name.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// The field's doc comment.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn doc(&self) -> &'static str {
        self.doc
    }

    /// Its type.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn ty(&self) -> &'static Ty {
        self.ty
    }
}

/// A set of names, from [`#[derive(Script)]`](macro@crate::Script) on an enum
/// of unit variants; the value is the variant's name.
///
/// # Examples
///
/// ```
/// use enshwick::script::{Ty, Typed};
///
/// /// Where a notice went.
/// #[derive(Clone, Copy, Debug, enshwick::Script)]
/// pub enum Sent {
///     /// On screen.
///     Shown,
///     /// To chat.
///     Chat,
/// }
///
/// let Ty::Choice(choice) = Sent::TY else { unreachable!() };
/// assert_eq!(choice.names().collect::<Vec<_>>(), ["Shown", "Chat"]);
/// ```
#[derive(Debug)]
pub struct Choice {
    name: &'static str,
    doc: &'static str,
    values: &'static [(&'static str, &'static str)],
}

impl Choice {
    #[doc(hidden)]
    #[must_use]
    pub const fn new(
        name: &'static str,
        doc: &'static str,
        values: &'static [(&'static str, &'static str)],
    ) -> Self {
        Self { name, doc, values }
    }

    /// The Rust type's name.
    ///
    /// # Examples
    ///
    /// See [`Choice`].
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// The type's doc comment up to its first section.
    ///
    /// # Examples
    ///
    /// See [`Choice`].
    #[must_use]
    pub const fn doc(&self) -> &'static str {
        self.doc
    }

    /// The variants' names in order.
    ///
    /// # Examples
    ///
    /// See [`Choice`].
    pub fn names(&self) -> impl Iterator<Item = &'static str> {
        self.values.iter().map(|(name, _)| *name)
    }

    /// Each variant's name with its doc comment.
    ///
    /// # Examples
    ///
    /// See [`Choice`].
    #[must_use]
    pub const fn values(&self) -> &'static [(&'static str, &'static str)] {
        self.values
    }
}

/// The parameters and answer of a function.
///
/// # Examples
///
/// See [`Function`].
#[derive(Debug)]
pub struct Signature {
    params: &'static [Param],
    ret: &'static Ty,
}

impl Signature {
    #[doc(hidden)]
    #[must_use]
    pub const fn new(params: &'static [Param], ret: &'static Ty) -> Self {
        Self { params, ret }
    }

    /// The parameters in order.
    ///
    /// # Examples
    ///
    /// See [`Function`].
    #[must_use]
    pub const fn params(&self) -> &'static [Param] {
        self.params
    }

    /// What it answers; [`Ty::Nil`] for nothing.
    ///
    /// # Examples
    ///
    /// See [`Function`].
    #[must_use]
    pub const fn ret(&self) -> &'static Ty {
        self.ret
    }
}

/// A parameter of a [`Signature`].
///
/// # Examples
///
/// See [`Function`].
#[derive(Debug)]
pub struct Param {
    name: &'static str,
    ty: &'static Ty,
}

impl Param {
    #[doc(hidden)]
    #[must_use]
    pub const fn new(name: &'static str, ty: &'static Ty) -> Self {
        Self { name, ty }
    }

    /// The Rust parameter's name; empty for a parameter of a function the
    /// script passes in.
    ///
    /// # Examples
    ///
    /// See [`Function`].
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Its type.
    ///
    /// # Examples
    ///
    /// See [`Function`].
    #[must_use]
    pub const fn ty(&self) -> &'static Ty {
        self.ty
    }
}

/// The call shim a [`Function`] runs: the receiver and the arguments, each
/// already of its parameter's type.
pub type Call<T> = fn(&T, Vec<Value>) -> Result<Value, Error>;

/// A method of `T` a script may call, from [`#[script]`](macro@crate::script).
///
/// # Examples
///
/// ```
/// use enshwick::script::{Bound, Value};
///
/// #[derive(Clone, Copy, Debug)]
/// pub struct Dice;
///
/// #[enshwick::script(rolls)]
/// impl Dice {
///     /// The sum of `count` dice of six sides, all showing four.
///     #[script]
///     pub fn roll(self, count: u32) -> u32 {
///         count * 4
///     }
/// }
///
/// impl Bound for Dice {
///     const NAME: &'static str = "dice";
///     const DOC: &'static str = "Dice.";
///     const PARTS: &'static [&'static [enshwick::script::Function<Self>]] = &[Self::SCRIPT_ROLLS];
///
///     fn resolve(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
///         Ok(Self)
///     }
/// }
///
/// let roll = &Dice::SCRIPT_ROLLS[0];
/// assert_eq!(roll.name(), "roll");
/// assert_eq!(roll.signature().params()[0].name(), "count");
/// let Ok(Value::Number(sum)) = roll.call(&Dice, vec![Value::Number(2.0)]) else { unreachable!() };
/// assert_eq!(sum, 8.0);
/// ```
pub struct Function<T: 'static> {
    name: &'static str,
    doc: &'static str,
    signature: Signature,
    call: Call<T>,
}

impl<T> core::fmt::Debug for Function<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Function")
            .field("name", &self.name)
            .field("signature", &self.signature)
            .finish_non_exhaustive()
    }
}

impl<T> Function<T> {
    #[doc(hidden)]
    #[must_use]
    pub const fn new(
        name: &'static str,
        doc: &'static str,
        signature: Signature,
        call: Call<T>,
    ) -> Self {
        Self {
            name,
            doc,
            signature,
            call,
        }
    }

    /// The method's name, the same in Rust and in a script.
    ///
    /// # Examples
    ///
    /// See [`Function`].
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// The method's doc comment up to its first section.
    ///
    /// # Examples
    ///
    /// See [`Function`].
    #[must_use]
    pub const fn doc(&self) -> &'static str {
        self.doc
    }

    /// Its parameters and answer.
    ///
    /// # Examples
    ///
    /// See [`Function`].
    #[must_use]
    pub const fn signature(&self) -> &Signature {
        &self.signature
    }

    /// Calls the method on `this` with `args`, one per parameter, each of
    /// the parameter's type.
    ///
    /// # Errors
    ///
    /// [`Error`] for a missing argument or one of another type, and for the
    /// method's own error when it returns a `Result`.
    ///
    /// # Examples
    ///
    /// See [`Function`].
    pub fn call(&self, this: &T, args: Vec<Value>) -> Result<Value, Error> {
        if args.len() > self.signature.params.len() {
            return Err(Error::new(format!(
                "{} takes {} arguments, not {}",
                self.name,
                self.signature.params.len(),
                args.len()
            )));
        }
        (self.call)(this, args)
    }
}

/// A part of the API a script reaches as one namespace, such as `world`.
///
/// # Examples
///
/// See [`Function`].
pub trait Bound: Copy + Send + Sync + 'static {
    /// The namespace's name in a script.
    const NAME: &'static str;
    /// What the namespace is for, one sentence.
    const DOC: &'static str;
    /// The method lists of every [`#[script]`](macro@crate::script) block
    /// of the type.
    const PARTS: &'static [&'static [Function<Self>]];

    /// The value a script's calls go to, for the mod `load` initialises.
    ///
    /// # Errors
    ///
    /// [`Off`](crate::Off) when the game build lacks what the part needs;
    /// the script runs without that namespace.
    fn resolve(load: &mut crate::Load<'_>) -> Result<Self, crate::Off>;

    /// Every method a script may call.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::script::Bound;
    ///
    /// assert!(enshwick::world::World::functions().any(|f| f.name() == "broadcast"));
    /// ```
    fn functions() -> impl Iterator<Item = &'static Function<Self>> {
        Self::PARTS.iter().flat_map(|part| part.iter())
    }
}

/// A value on its way between Rust and a script, shaped by its [`Ty`].
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Value {
    /// [`Ty::Nil`], and an absent [`Ty::Optional`].
    Nil,
    /// [`Ty::Bool`].
    Bool(bool),
    /// [`Ty::Number`].
    Number(f64),
    /// [`Ty::String`] and [`Ty::Choice`].
    String(String),
    /// [`Ty::Vector`].
    Vector([f64; 3]),
    /// [`Ty::List`].
    List(Vec<Value>),
    /// [`Ty::Record`]: the fields' values in the record's order.
    Record(Vec<Value>),
    /// [`Ty::Function`].
    Function(Callback),
}

/// The function behind a [`Callback`].
type CallbackFn = dyn Fn(Vec<Value>) -> Result<Value, Error> + Send + Sync;

/// A function a script passed in, callable from Rust on any thread the
/// runtime allows; the runtime reports the script's own errors.
///
/// # Examples
///
/// ```
/// use enshwick::script::{Callback, Value};
///
/// let double = Callback::new(|args| match args.first() {
///     Some(Value::Number(n)) => Ok(Value::Number(n * 2.0)),
///     _ => Ok(Value::Nil),
/// });
/// assert!(matches!(double.call(vec![Value::Number(2.0)]), Ok(Value::Number(4.0))));
/// ```
#[derive(Clone)]
pub struct Callback(Arc<CallbackFn>);

impl core::fmt::Debug for Callback {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Callback")
    }
}

impl Callback {
    /// Wraps `function`.
    ///
    /// # Examples
    ///
    /// See [`Callback`].
    pub fn new(
        function: impl Fn(Vec<Value>) -> Result<Value, Error> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(function))
    }

    /// Calls the function with `args`.
    ///
    /// # Errors
    ///
    /// The function's own error, such as a script's.
    ///
    /// # Examples
    ///
    /// See [`Callback`].
    pub fn call(&self, args: Vec<Value>) -> Result<Value, Error> {
        (self.0)(args)
    }
}

/// Why a call between Rust and a script failed: an argument of the wrong
/// type, the API's own error, or the script's.
///
/// # Examples
///
/// ```
/// let error = enshwick::script::Error::new("count: expected a number");
/// assert_eq!(error.to_string(), "count: expected a number");
/// ```
#[derive(Clone, Debug, thiserror::Error)]
#[error("{0}")]
pub struct Error(String);

impl Error {
    /// An error saying `message`.
    ///
    /// # Examples
    ///
    /// See [`Error`].
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    /// The same error under `context`, such as a parameter's name.
    #[must_use]
    pub(crate) fn within(self, context: &str) -> Self {
        Self(format!("{context}: {}", self.0))
    }
}

impl From<Error> for crate::Off {
    fn from(error: Error) -> Self {
        Self::new(error.0)
    }
}

/// A type with a script [`Ty`].
///
/// # Examples
///
/// ```
/// use enshwick::script::{Ty, Typed};
///
/// assert!(matches!(<Option<u32> as Typed>::TY, Ty::Optional(Ty::Number)));
/// ```
pub trait Typed {
    /// Its type in a script.
    const TY: &'static Ty;
}

/// A Rust value a script receives.
///
/// # Examples
///
/// See the [module](self).
pub trait IntoScript: Typed {
    /// The value as [`Typed::TY`] shapes it.
    fn into_value(self) -> Value;
}

/// A Rust value made from what a script passed.
///
/// # Examples
///
/// ```
/// use enshwick::script::{FromScript, Value};
///
/// assert_eq!(u8::from_value(Value::Number(7.0))?, 7);
/// assert!(u8::from_value(Value::Number(300.0)).is_err());
/// # Ok::<(), enshwick::script::Error>(())
/// ```
pub trait FromScript: Typed + Sized {
    /// The Rust value of `value`.
    ///
    /// # Errors
    ///
    /// [`Error`] when `value` is not of [`Typed::TY`] or out of the Rust
    /// type's range.
    fn from_value(value: Value) -> Result<Self, Error>;
}

/// What a Rust closure returns from a script's answer, when the API calls a
/// [`Callback`] in its place.
///
/// # Examples
///
/// ```
/// use std::ops::ControlFlow;
///
/// use enshwick::script::{Answer, Value};
///
/// assert_eq!(ControlFlow::<()>::from_answer(Ok(Value::Bool(false))), ControlFlow::Break(()));
/// assert_eq!(ControlFlow::<()>::from_answer(Ok(Value::Nil)), ControlFlow::Continue(()));
/// ```
pub trait Answer {
    /// What the script's function returns.
    const TY: &'static Ty;

    /// The Rust answer for the script's; an error, which the runtime has
    /// reported, answers as the API would without the script.
    fn from_answer(answer: Result<Value, Error>) -> Self;
}

/// A loader event a script subscribes to, from the tables of
/// [`events`](crate::events).
///
/// # Examples
///
/// ```
/// let joined = enshwick::script::events().find(|e| e.name() == "PlayerJoined");
/// assert!(joined.is_some_and(|e| e.event() == enshwick::abi::Event::PLAYER_JOINED));
/// ```
#[derive(Debug)]
pub struct EventKind {
    name: &'static str,
    doc: &'static str,
    event: crate::abi::Event,
    data: &'static Ty,
    read: unsafe fn(*const c_void) -> Option<Value>,
}

impl EventKind {
    pub(crate) const fn new(
        name: &'static str,
        doc: &'static str,
        event: crate::abi::Event,
        data: &'static Ty,
        read: unsafe fn(*const c_void) -> Option<Value>,
    ) -> Self {
        Self {
            name,
            doc,
            event,
            data,
            read,
        }
    }

    /// The event's Rust name, such as `PlayerJoined`.
    ///
    /// # Examples
    ///
    /// See [`EventKind`].
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// The event's doc comment.
    ///
    /// # Examples
    ///
    /// See [`EventKind`].
    #[must_use]
    pub const fn doc(&self) -> &'static str {
        self.doc
    }

    /// The raw event.
    ///
    /// # Examples
    ///
    /// See [`EventKind`].
    #[must_use]
    pub const fn event(&self) -> crate::abi::Event {
        self.event
    }

    /// What a handler gets; [`Ty::Nil`] for events without data.
    ///
    /// # Examples
    ///
    /// See [`EventKind`].
    #[must_use]
    pub const fn data(&self) -> &'static Ty {
        self.data
    }

    /// The event's data from the raw pointer the loader passes with it.
    ///
    /// # Safety
    ///
    /// - `raw` is null or the pointer the loader passed with
    ///   [`EventKind::event`]: the event's data, readable for the size its
    ///   header states; no alignment is needed.
    /// - No other thread writes that data during the call.
    #[must_use]
    pub unsafe fn read(&self, raw: *const c_void) -> Option<Value> {
        // SAFETY: the caller passes the loader's pointer for this event with
        // no other thread writing its data, by this function's `# Safety`,
        // which is what the reader asks.
        unsafe { (self.read)(raw) }
    }
}

/// Every loader event a script may subscribe to.
///
/// # Examples
///
/// See [`EventKind`].
pub fn events() -> impl Iterator<Item = &'static EventKind> {
    crate::events::SCRIPT_EVENTS
        .iter()
        .flat_map(|part| part.iter())
}

/// A part of the API as one namespace of a script, its type erased: what it
/// declares, and how a runtime opens it for a mod.
///
/// # Examples
///
/// ```
/// let names: Vec<&str> = enshwick::script::NAMESPACES.iter().map(|n| n.name()).collect();
/// assert!(names.contains(&"world"));
/// ```
#[derive(Debug)]
pub struct Namespace {
    name: &'static str,
    doc: &'static str,
    describe: fn() -> Vec<Declared>,
    open: fn(&mut crate::Load<'_>) -> Result<Opened, crate::Off>,
}

/// A function of a [`Namespace`] as definitions declare it.
///
/// # Examples
///
/// See [`Namespace::functions`].
#[derive(Clone, Copy, Debug)]
pub struct Declared {
    /// Its place in its namespace's [`Bound::functions`].
    index: usize,
    name: &'static str,
    doc: &'static str,
    signature: &'static Signature,
}

impl Declared {
    /// The function's name.
    ///
    /// # Examples
    ///
    /// See [`Namespace::functions`].
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Its doc comment up to the first section.
    ///
    /// # Examples
    ///
    /// See [`Namespace::functions`].
    #[must_use]
    pub const fn doc(&self) -> &'static str {
        self.doc
    }

    /// Its parameters and answer.
    ///
    /// # Examples
    ///
    /// See [`Namespace::functions`].
    #[must_use]
    pub const fn signature(&self) -> &'static Signature {
        self.signature
    }
}

/// The function behind an [`Opened`] namespace: the function's place in its
/// namespace, from [`Declared`], and its arguments.
type OpenedFn = dyn Fn(usize, Vec<Value>) -> Result<Value, Error> + Send + Sync;

/// A [`Namespace`] opened for one mod: calls its functions by index.
///
/// # Examples
///
/// See [`Namespace::open`].
#[derive(Clone)]
pub struct Opened(Arc<OpenedFn>);

impl core::fmt::Debug for Opened {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Opened")
    }
}

impl Opened {
    /// Calls `function`, one of [`Namespace::functions`] of the namespace
    /// this was opened from, with `args`.
    ///
    /// # Errors
    ///
    /// As [`Function::call`], and for a function past the end of this
    /// namespace; a function of another namespace is not detected.
    ///
    /// # Examples
    ///
    /// See [`Namespace::open`].
    pub fn call(&self, function: &Declared, args: Vec<Value>) -> Result<Value, Error> {
        (self.0)(function.index, args)
    }
}

impl Namespace {
    const fn of<T: Bound>() -> Self {
        Self {
            name: T::NAME,
            doc: T::DOC,
            describe: describe::<T>,
            open: open::<T>,
        }
    }

    /// The namespace's name in a script.
    ///
    /// # Examples
    ///
    /// See [`Namespace`].
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// What it is for, one sentence.
    ///
    /// # Examples
    ///
    /// See [`Namespace`].
    #[must_use]
    pub const fn doc(&self) -> &'static str {
        self.doc
    }

    /// Its functions, in the order [`Opened::call`] indexes them.
    ///
    /// # Examples
    ///
    /// ```
    /// let world = enshwick::script::NAMESPACES.iter().find(|n| n.name() == "world");
    /// assert!(world.is_some_and(|w| w.functions().iter().any(|f| f.name() == "players")));
    /// ```
    #[must_use]
    pub fn functions(&self) -> Vec<Declared> {
        (self.describe)()
    }

    /// Opens the namespace for the mod `load` initialises.
    ///
    /// # Errors
    ///
    /// [`Off`](crate::Off) when the game build lacks what it needs.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::script::{NAMESPACES, Value};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let log = NAMESPACES.iter().find(|n| n.name() == "log").ok_or(enshwick::Off::new("no log"))?;
    ///     let functions = log.functions();
    ///     let info = functions.iter().find(|f| f.name() == "info").ok_or(enshwick::Off::new("no info"))?;
    ///     log.open(load)?.call(info, vec![Value::String("hello".into())])?;
    ///     Ok(())
    /// }
    /// ```
    pub fn open(&self, load: &mut crate::Load<'_>) -> Result<Opened, crate::Off> {
        (self.open)(load)
    }
}

fn describe<T: Bound>() -> Vec<Declared> {
    T::functions()
        .enumerate()
        .map(|(index, f)| Declared {
            index,
            name: f.name,
            doc: f.doc,
            signature: &f.signature,
        })
        .collect()
}

fn open<T: Bound>(load: &mut crate::Load<'_>) -> Result<Opened, crate::Off> {
    let this = T::resolve(load)?;
    Ok(Opened(Arc::new(move |index, args| {
        T::functions()
            .nth(index)
            .ok_or_else(|| Error::new(format!("{} has no such function", T::NAME)))?
            .call(&this, args)
    })))
}

/// Every namespace a script reaches, in the order a runtime opens them.
///
/// # Examples
///
/// See [`Namespace`].
pub const NAMESPACES: &[Namespace] = &[
    Namespace::of::<Log>(),
    Namespace::of::<crate::world::World>(),
    Namespace::of::<crate::schedule::Scheduler>(),
    Namespace::of::<crate::inventory::Inventories>(),
];

/// The mod's log as a script writes to it: one line per call, under the
/// mod's id.
///
/// # Examples
///
/// See [`Namespace::open`].
#[derive(Clone, Copy, Debug)]
pub struct Log(crate::Host);

#[crate::script(lines)]
impl Log {
    /// Writes `text` to the mod's log as information.
    #[script]
    pub fn info(&self, text: &str) {
        self.0.log(crate::abi::Level::INFO, text);
    }

    /// Writes `text` to the mod's log as a warning.
    #[script]
    pub fn warn(&self, text: &str) {
        self.0.log(crate::abi::Level::WARN, text);
    }

    /// Writes `text` to the mod's log as an error; the mod keeps running.
    #[script]
    pub fn error(&self, text: &str) {
        self.0.log(crate::abi::Level::ERROR, text);
    }
}

impl Bound for Log {
    const NAME: &'static str = "log";
    const DOC: &'static str = "The mod's log file, one line per call.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_LINES];

    fn resolve(load: &mut crate::Load<'_>) -> Result<Self, crate::Off> {
        Ok(Self(load.host()))
    }
}

/// The function behind [`GameEventKind::subscribe`].
type SubscribeGame = fn(&mut crate::Load<'_>, Box<GameHandler>) -> Result<(), crate::Off>;

/// A handler of a typed game event: it gets the event's data as a function
/// that converts it, so a handler with nothing to do converts nothing.
type GameHandler = dyn Fn(&dyn Fn() -> Value) + Send + Sync;

/// A typed game event a script subscribes to, from
/// [`game_events`](crate::game_events): its data and how to subscribe.
///
/// # Examples
///
/// ```
/// let died = enshwick::script::game_events().find(|e| e.name() == "EntityDiedEvent");
/// assert!(died.is_some());
/// ```
#[derive(Debug)]
pub struct GameEventKind {
    record: &'static Record,
    data: &'static Ty,
    subscribe: SubscribeGame,
}

impl GameEventKind {
    pub(crate) const fn of<E>() -> Self
    where
        E: crate::game_events::Kind + IntoScript + Clone + Send + Sync,
    {
        let record = match E::TY {
            Ty::Record(record) => *record,
            #[expect(
                clippy::panic,
                reason = "evaluated at compile time: a typed game event that is no record does not build"
            )]
            _ => panic!("a typed game event derives Script as a record"),
        };
        Self {
            record,
            data: E::TY,
            subscribe: subscribe_game::<E>,
        }
    }

    /// The event's Rust name, the game's type name without its namespace,
    /// such as `EntityDiedEvent`.
    ///
    /// # Examples
    ///
    /// See [`GameEventKind`].
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.record.name()
    }

    /// The event's doc comment up to its first section.
    ///
    /// # Examples
    ///
    /// See [`GameEventKind`].
    #[must_use]
    pub const fn doc(&self) -> &'static str {
        self.record.doc()
    }

    /// What a handler gets.
    ///
    /// # Examples
    ///
    /// See [`GameEventKind`].
    #[must_use]
    pub const fn data(&self) -> &'static Ty {
        self.data
    }

    /// Calls `handler` with each event's data, as
    /// [`Load::on_game`](crate::Load::on_game) does for the Rust type.
    ///
    /// # Errors
    ///
    /// [`Off`](crate::Off) when this build lacks a field of the event, or
    /// after `init`.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let died = enshwick::script::game_events()
    ///         .find(|e| e.name() == "EntityDiedEvent")
    ///         .ok_or(enshwick::Off::new("no such event"))?;
    ///     died.subscribe(load, |data| enshwick::debug!("{:?}", data()))
    /// }
    /// ```
    pub fn subscribe(
        &self,
        load: &mut crate::Load<'_>,
        handler: impl Fn(&dyn Fn() -> Value) + Send + Sync + 'static,
    ) -> Result<(), crate::Off> {
        (self.subscribe)(load, Box::new(handler))
    }
}

fn subscribe_game<E>(
    load: &mut crate::Load<'_>,
    handler: Box<GameHandler>,
) -> Result<(), crate::Off>
where
    E: crate::game_events::Kind + IntoScript + Clone + Send + Sync,
{
    load.on_game::<E>(move |event| handler(&|| event.clone().into_value()))
}

/// Every typed game event a script may subscribe to.
///
/// # Examples
///
/// See [`GameEventKind`].
pub fn game_events() -> impl Iterator<Item = &'static GameEventKind> {
    crate::game_events::SCRIPT_KINDS.iter()
}
