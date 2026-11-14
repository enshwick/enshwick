//! Rust handles a script holds as objects and calls their methods on, such
//! as the `Rate` that `load.rate(...)` returns.

use core::any::Any;
use std::sync::Arc;

use super::{Declared, Error, FromScript, Function, IntoScript, Ty, Typed, Value, declare};

/// A Rust handle a script holds as an object with the handle's
/// [`#[script]`](macro@crate::script) methods, called as `handle:method()`.
///
/// # Examples
///
/// ```rust,standalone_crate
/// use enshwick::script::{Class, Function, IntoScript, Object, Value};
///
/// #[derive(Clone, Copy, Debug)]
/// pub struct Counter(u32);
///
/// #[enshwick::script(counts)]
/// impl Counter {
///     /// The count.
///     #[script]
///     pub fn get(&self) -> u32 {
///         self.0
///     }
/// }
///
/// impl Object for Counter {
///     const CLASS: &'static Class = &Class::of::<Self>("Counter", "A count.");
///     const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_COUNTS];
/// }
///
/// let Value::Object(handle) = Counter(3).into_value() else { unreachable!() };
/// let get = handle.class().methods().into_iter().find(|m| m.name() == "get");
/// let answer = handle.call(&get.ok_or(enshwick::script::Error::new("no get"))?, &mut [])?;
/// assert!(matches!(answer, Value::Number(n) if n == 3.0));
/// # Ok::<(), enshwick::script::Error>(())
/// ```
pub trait Object: Clone + Send + Sync + 'static {
    /// Its class: the Rust type's name, its doc and its methods.
    const CLASS: &'static Class;
    /// The method lists of every [`#[script]`](macro@crate::script) block
    /// of the type.
    const PARTS: &'static [&'static [Function<Self>]];
}

/// The type of an [`Object`], its Rust type erased.
///
/// # Examples
///
/// See [`Object`].
#[derive(Debug)]
pub struct Class {
    name: &'static str,
    doc: &'static str,
    describe: fn() -> Vec<Declared>,
    call: fn(&(dyn Any + Send + Sync), usize, &mut [Value]) -> Result<Value, Error>,
    lent: bool,
}

impl Class {
    /// The class of `T`, named `name`, the Rust type's name, with `doc`, one
    /// sentence.
    ///
    /// # Examples
    ///
    /// See [`Object`].
    #[must_use]
    pub const fn of<T: Object>(name: &'static str, doc: &'static str) -> Self {
        Self {
            name,
            doc,
            describe: || declare(T::PARTS.iter().flat_map(|part| part.iter())),
            call: call::<T>,
            lent: false,
        }
    }

    /// The class of a `T` a hook lends a script's function for one call,
    /// such as the damage a filter changes: a runtime may hand the function
    /// the same script object on every call, holding the call's handle.
    ///
    /// # Examples
    ///
    /// ```rust,standalone_crate
    /// use enshwick::script::{Class, Function, Object};
    ///
    /// #[derive(Clone, Copy, Debug)]
    /// pub struct Hit(f32);
    ///
    /// #[enshwick::script(hits)]
    /// impl Hit {
    ///     /// The damage.
    ///     #[script]
    ///     pub fn amount(&self) -> f32 {
    ///         self.0
    ///     }
    /// }
    ///
    /// impl Object for Hit {
    ///     const CLASS: &'static Class = &Class::lent::<Self>("Hit", "A hit.");
    ///     const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_HITS];
    /// }
    ///
    /// assert!(Hit::CLASS.is_lent());
    /// ```
    #[must_use]
    pub const fn lent<T: Object>(name: &'static str, doc: &'static str) -> Self {
        Self {
            lent: true,
            ..Self::of::<T>(name, doc)
        }
    }

    /// Whether its objects are lent for one call, as [`Class::lent`] makes
    /// them.
    ///
    /// # Examples
    ///
    /// See [`Class::lent`].
    #[must_use]
    pub const fn is_lent(&self) -> bool {
        self.lent
    }

    /// The Rust type's name, the object's type in a script.
    ///
    /// # Examples
    ///
    /// See [`Object`].
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// What the object is for, one sentence.
    ///
    /// # Examples
    ///
    /// See [`Object`].
    #[must_use]
    pub const fn doc(&self) -> &'static str {
        self.doc
    }

    /// Its methods, in the order [`Handle::call`] indexes them.
    ///
    /// # Examples
    ///
    /// See [`Object`].
    #[must_use]
    pub fn methods(&self) -> Vec<Declared> {
        (self.describe)()
    }
}

fn call<T: Object>(
    this: &(dyn Any + Send + Sync),
    index: usize,
    args: &mut [Value],
) -> Result<Value, Error> {
    let this = this
        .downcast_ref::<T>()
        .ok_or_else(|| Error::new("an object of another class"))?;
    T::PARTS
        .iter()
        .flat_map(|part| part.iter())
        .nth(index)
        .ok_or_else(|| Error::new(format!("{} has no such method", T::CLASS.name)))?
        .call(this, args)
}

/// An [`Object`] on its way to or from a script, its type erased.
///
/// # Examples
///
/// See [`Object`].
#[derive(Clone)]
pub struct Handle {
    class: &'static Class,
    this: Arc<dyn Any + Send + Sync>,
}

impl core::fmt::Debug for Handle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Handle").field(&self.class.name).finish()
    }
}

impl Handle {
    /// Its class.
    ///
    /// # Examples
    ///
    /// See [`Object`].
    #[must_use]
    pub const fn class(&self) -> &'static Class {
        self.class
    }

    /// Calls `method`, one of [`Class::methods`] of its class, with `args`.
    ///
    /// # Errors
    ///
    /// As [`Function::call`], and for a method past the end of its class.
    ///
    /// # Examples
    ///
    /// See [`Object`].
    pub fn call(&self, method: &Declared, args: &mut [Value]) -> Result<Value, Error> {
        (self.class.call)(&*self.this, method.index(), args)
    }
}

/// An object a Rust caller already shares crosses as that same allocation:
/// a hook that keeps one `Arc` and hands a script a clone on every call
/// allocates nothing.
///
/// # Examples
///
/// ```rust,standalone_crate
/// use std::sync::Arc;
///
/// use enshwick::script::{Class, FromScript, Function, IntoScript, Object};
///
/// #[derive(Clone, Copy, Debug)]
/// pub struct Hit(f32);
///
/// #[enshwick::script(hits)]
/// impl Hit {
///     /// The damage.
///     #[script]
///     pub fn amount(&self) -> f32 {
///         self.0
///     }
/// }
///
/// impl Object for Hit {
///     const CLASS: &'static Class = &Class::lent::<Self>("Hit", "A hit.");
///     const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_HITS];
/// }
///
/// let kept = Arc::new(Hit(3.0));
/// let back = Arc::<Hit>::from_value(Arc::clone(&kept).into_value())?;
/// assert!(Arc::ptr_eq(&kept, &back));
/// # Ok::<(), enshwick::script::Error>(())
/// ```
impl<T: Object> Typed for Arc<T> {
    const TY: &'static Ty = &Ty::Object(T::CLASS);
}

impl<T: Object> IntoScript for Arc<T> {
    fn into_value(self) -> Value {
        Value::Object(Handle {
            class: T::CLASS,
            this: self,
        })
    }
}

impl<T: Object> FromScript for Arc<T> {
    fn from_value(value: Value) -> Result<Self, Error> {
        match value {
            Value::Object(handle) => handle
                .this
                .downcast::<T>()
                .map_err(|_| Error::new(format!("expected a {}", T::CLASS.name))),
            _ => Err(Error::new(format!("expected a {}", T::CLASS.name))),
        }
    }
}

impl<T: Object> Typed for T {
    const TY: &'static Ty = &Ty::Object(T::CLASS);
}

impl<T: Object> IntoScript for T {
    fn into_value(self) -> Value {
        Value::Object(Handle {
            class: T::CLASS,
            this: Arc::new(self),
        })
    }
}

impl<T: Object> FromScript for T {
    fn from_value(value: Value) -> Result<Self, Error> {
        match value {
            Value::Object(handle) => handle
                .this
                .downcast_ref::<T>()
                .cloned()
                .ok_or_else(|| Error::new(format!("expected a {}", T::CLASS.name))),
            _ => Err(Error::new(format!("expected a {}", T::CLASS.name))),
        }
    }
}
