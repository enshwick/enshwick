//! The namespaces a script reaches: the parts of the API a runtime opens for a mod.

use std::sync::Arc;

use super::{Bound, Error, Function, Signature, Value};

/// A part of the API as one namespace of a script, its type erased: what it
/// declares, and how a runtime opens it for a mod.
///
/// # Examples
///
/// ```rust,standalone_crate
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
    /// ```rust,standalone_crate
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
    /// ```rust,standalone_crate
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
    #[cfg(feature = "unstable")]
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
