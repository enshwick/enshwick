//! What the API offers a script language, described beside the Rust items.
//!
//! Unstable: needs the `unstable` feature and may change in any release.
//!
//! [`#[derive(Script)]`](macro@crate::Script) describes types,
//! [`#[script]`](macro@crate::script) methods, and the tables of
//! [`events`](crate::events) events; runtimes and definitions read them.
//!
//! # Examples
//!
//! ```rust,standalone_crate
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

use std::sync::Arc;

pub(crate) mod convert;
mod events;
mod namespaces;
mod objects;
#[cfg(test)]
#[cfg(feature = "unstable")]
mod tests;
mod unions;

#[cfg_attr(
    not(feature = "unstable"),
    expect(
        unused_imports,
        reason = "the script binding is public only with `unstable`"
    )
)]
pub use events::{EventKind, GameEventKind, events, game_events};
#[cfg_attr(
    not(feature = "unstable"),
    expect(
        unused_imports,
        reason = "the script binding is public only with `unstable`"
    )
)]
pub use namespaces::{Declared, Log, NAMESPACES, Namespace, Opened};
pub use objects::{Class, Handle, Object};
pub use unions::{Union, Variant};

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
    /// Game data of any shape: nil, a boolean, a number, text, or a table
    /// of these, keyed by name or listed; a resource value takes this form.
    Data,
    /// Raw bytes: a Luau `buffer`.
    Bytes,
    /// A Rust handle with methods, held as an [`Object`].
    Object(&'static Class),
    /// One of an enum's variants, some carrying a value.
    Union(&'static Union),
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
    defaulted: bool,
}

impl Field {
    #[doc(hidden)]
    #[must_use]
    pub const fn new(name: &'static str, doc: &'static str, ty: &'static Ty) -> Self {
        Self {
            name,
            doc,
            ty,
            defaulted: false,
        }
    }

    #[doc(hidden)]
    #[must_use]
    pub const fn defaulted(name: &'static str, doc: &'static str, ty: &'static Ty) -> Self {
        Self {
            name,
            doc,
            ty,
            defaulted: true,
        }
    }

    /// Whether a script may leave the field out, for its Rust default, as
    /// `#[script(default)]` marks it.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn has_default(&self) -> bool {
        self.defaulted
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
/// ```rust,standalone_crate
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
///
/// # Examples
///
/// See [`Function`], whose `#[script]` methods get one each.
pub type Call<T> = fn(&T, Vec<Value>) -> Result<Value, Error>;

/// A method of `T` a script may call, from [`#[script]`](macro@crate::script).
///
/// # Examples
///
/// ```rust,standalone_crate
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

/// The call shim an [`InitFunction`] runs: the mod's [`Load`](crate::Load)
/// and the arguments, each already of its parameter's type.
///
/// # Examples
///
/// See [`InitFunction`].
pub type InitCall = fn(&mut crate::Load<'_>, Vec<Value>) -> Result<Value, Error>;

/// A method of [`Load`](crate::Load) a script may call while the mod
/// starts, as a Rust mod calls it in `init`, from
/// [`#[script]`](macro@crate::script) on an `impl Load` block.
///
/// # Examples
///
/// ```rust,standalone_crate
/// use enshwick::script::{InitFunction, Value};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let log_dir: &InitFunction = enshwick::script::NAMESPACES
///         .iter()
///         .flat_map(|n| n.init_functions())
///         .find(|f| f.name() == "mod_dir")
///         .ok_or(enshwick::Off::new("no mod_dir"))?;
///     let _dir: Value = log_dir.call(load, Vec::new())?;
///     Ok(())
/// }
/// ```
pub struct InitFunction {
    name: &'static str,
    doc: &'static str,
    signature: Signature,
    call: InitCall,
}

impl core::fmt::Debug for InitFunction {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InitFunction")
            .field("name", &self.name)
            .field("signature", &self.signature)
            .finish_non_exhaustive()
    }
}

impl InitFunction {
    #[doc(hidden)]
    #[must_use]
    pub const fn new(
        name: &'static str,
        doc: &'static str,
        signature: Signature,
        call: InitCall,
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
    /// See [`InitFunction`].
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// The method's doc comment up to its first section.
    ///
    /// # Examples
    ///
    /// See [`InitFunction`].
    #[must_use]
    pub const fn doc(&self) -> &'static str {
        self.doc
    }

    /// Its parameters and answer.
    ///
    /// # Examples
    ///
    /// See [`InitFunction`].
    #[must_use]
    pub const fn signature(&self) -> &Signature {
        &self.signature
    }

    /// Calls the method on `load` with `args`, one per parameter, each of
    /// the parameter's type.
    ///
    /// # Errors
    ///
    /// As [`Function::call`].
    ///
    /// # Examples
    ///
    /// See [`InitFunction`].
    pub fn call(&self, load: &mut crate::Load<'_>, args: Vec<Value>) -> Result<Value, Error> {
        if args.len() > self.signature.params.len() {
            return Err(Error::new(format!(
                "{} takes {} arguments, not {}",
                self.name,
                self.signature.params.len(),
                args.len()
            )));
        }
        (self.call)(load, args)
    }
}

/// What a hook hands a script's handler in place of its `&mut` argument:
/// a view of it as a value, and the change the handler returns, applied
/// back; a handler that returns nil changes nothing.
///
/// # Examples
///
/// ```rust,standalone_crate
/// use enshwick::script::{Context, Error, Ty, Typed, Value};
///
/// struct Price(u32);
///
/// impl Context for Price {
///     const VIEW: &'static Ty = <u32 as Typed>::TY;
///     const CHANGE: &'static Ty = <u32 as Typed>::TY;
///
///     fn view(&self) -> Value {
///         Value::Number(self.0.into())
///     }
///
///     fn apply(&mut self, change: Value) -> Result<(), Error> {
///         self.0 = enshwick::script::FromScript::from_value(change)?;
///         Ok(())
///     }
/// }
///
/// let mut price = Price(3);
/// price.apply(Value::Number(5.0))?;
/// assert_eq!(price.0, 5);
/// # Ok::<(), Error>(())
/// ```
pub trait Context {
    /// What the handler gets.
    const VIEW: &'static Ty;
    /// What the handler may return besides nil.
    const CHANGE: &'static Ty;

    /// The handler's view of the context.
    fn view(&self) -> Value;

    /// Applies what the handler returned, of [`Context::CHANGE`].
    ///
    /// # Errors
    ///
    /// [`Error`] for a change the context refuses; the runtime turns the
    /// script's mod off with it.
    fn apply(&mut self, change: Value) -> Result<(), Error>;
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
    /// ```rust,standalone_crate
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
    /// A table keyed by name, in [`Ty::Data`]: the fields of a struct.
    Map(Vec<(String, Value)>),
    /// [`Ty::Bytes`], and raw bytes inside [`Ty::Data`].
    Bytes(Vec<u8>),
    /// [`Ty::Object`].
    Object(Handle),
}

/// The function behind a [`Callback`].
type CallbackFn = dyn Fn(Vec<Value>) -> Result<Value, Error> + Send + Sync;

/// A function a script passed in, callable from Rust on any thread the
/// runtime allows; the runtime reports the script's own errors.
///
/// # Examples
///
/// ```rust,standalone_crate
/// use enshwick::script::{Callback, Value};
///
/// let double = Callback::new(|args| match args.first() {
///     Some(Value::Number(n)) => Ok(Value::Number(n * 2.0)),
///     _ => Ok(Value::Nil),
/// });
/// assert!(matches!(double.call(vec![Value::Number(2.0)]), Ok(Value::Number(4.0))));
/// ```
#[derive(Clone)]
pub struct Callback {
    function: Arc<CallbackFn>,
    fault: Option<Arc<FaultFn>>,
}

/// What a [`Callback`]'s runtime does with an answer the API cannot use.
type FaultFn = dyn Fn(&Error) + Send + Sync;

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
        Self {
            function: Arc::new(function),
            fault: None,
        }
    }

    /// The same function, with `fault` told of an answer the API cannot
    /// use, such as a change a [`Context`] refuses.
    ///
    /// # Examples
    ///
    /// ```rust,standalone_crate
    /// use enshwick::script::{Callback, Error, Value};
    ///
    /// let told = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    /// let seen = told.clone();
    /// let callback = Callback::new(|_| Ok(Value::Nil))
    ///     .with_fault(move |_| seen.store(true, std::sync::atomic::Ordering::Relaxed));
    /// callback.fault(&Error::new("no"));
    /// assert!(told.load(std::sync::atomic::Ordering::Relaxed));
    /// ```
    #[must_use]
    pub fn with_fault(self, fault: impl Fn(&Error) + Send + Sync + 'static) -> Self {
        Self {
            fault: Some(Arc::new(fault)),
            ..self
        }
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
        (self.function)(args)
    }

    /// Tells the runtime that the function's answer did not fit, with
    /// `error`; without a runtime to tell, nothing happens.
    ///
    /// # Examples
    ///
    /// See [`Callback::with_fault`].
    pub fn fault(&self, error: &Error) {
        if let Some(fault) = &self.fault {
            fault(error);
        }
    }
}

/// Why a call between Rust and a script failed: an argument of the wrong
/// type, the API's own error, or the script's.
///
/// # Examples
///
/// ```rust,standalone_crate
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
/// ```rust,standalone_crate
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
/// ```rust,standalone_crate
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
/// ```rust,standalone_crate
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

    /// The Rust answer for the script's; an error, a script error or an
    /// answer of another type, which the runtime has reported and turned
    /// the mod off for, answers as the API would without the script.
    fn from_answer(answer: Result<Value, Error>) -> Self;
}
