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
//! # extern crate enshwick_core as enshwick;
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

mod namespaces;
#[cfg(test)]
#[cfg(feature = "unstable")]
mod tests;

#[doc(inline)]
pub use enshwick_base::script::*;
#[cfg_attr(
    not(feature = "unstable"),
    expect(
        unused_imports,
        reason = "the script binding is public only with `unstable`"
    )
)]
pub use namespaces::{Log, NAMESPACES, Namespace, Opened};

/// The call shim an [`InitFunction`] runs: the mod's [`Load`](crate::Load)
/// and the arguments, each already of its parameter's type.
///
/// # Examples
///
/// See [`InitFunction`].
pub type InitCall = fn(&mut crate::Load<'_>, &mut [Value]) -> Result<Value, Error>;

/// A method of [`Load`](crate::Load) a script may call while the mod
/// starts, as a Rust mod calls it in `init`, from
/// [`#[script]`](macro@crate::script) on an `impl Load` block.
///
/// # Examples
///
/// ```rust,standalone_crate
/// # extern crate enshwick_core as enshwick;
/// use enshwick::script::{InitFunction, Value};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let log_dir: &InitFunction = enshwick::script::NAMESPACES
///         .iter()
///         .flat_map(|n| n.init_functions())
///         .find(|f| f.name() == "mod_dir")
///         .ok_or(enshwick::Off::new("no mod_dir"))?;
///     let _dir: Value = log_dir.call(load, &mut [])?;
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
    pub fn call(&self, load: &mut crate::Load<'_>, args: &mut [Value]) -> Result<Value, Error> {
        if args.len() > self.signature.params().len() {
            return Err(Error::new(format!(
                "{} takes {} arguments, not {}",
                self.name,
                self.signature.params().len(),
                args.len()
            )));
        }
        (self.call)(load, args)
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
    /// ```rust,standalone_crate
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::script::Bound;
    ///
    /// assert!(enshwick::world::World::functions().any(|f| f.name() == "broadcast"));
    /// ```
    fn functions() -> impl Iterator<Item = &'static Function<Self>> {
        Self::PARTS.iter().flat_map(|part| part.iter())
    }
}
