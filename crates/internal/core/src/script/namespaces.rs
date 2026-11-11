//! The namespaces a script reaches: the parts of the API a runtime opens for a mod.

use std::sync::Arc;

use super::{
    Bound, Declared, Error, Function, InitFunction, IntoScript as _, Object, Signature, Ty, Value,
    declare,
};

/// A part of the API as one namespace of a script, its type erased: what it
/// declares, and how a runtime opens it for a mod.
///
/// Parts that share a name are one namespace in a script, such as a
/// module's handle and its [`Load`](crate::Load) methods; the empty name is
/// the `enshwick` table itself.
///
/// # Examples
///
/// ```rust,standalone_crate
/// # extern crate enshwick_core as enshwick;
/// let names: Vec<&str> = enshwick::script::NAMESPACES.iter().map(|n| n.name()).collect();
/// assert!(names.contains(&"world"));
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Namespace {
    name: &'static str,
    doc: &'static str,
    describe: fn() -> Vec<Declared>,
    open: fn(&mut crate::Load<'_>) -> Result<Opened, crate::Off>,
    init: &'static [&'static [InitFunction]],
    request: Option<Request>,
}

/// The start-time call that opens a namespace a script asks for, as a Rust
/// mod asks [`Load`](crate::Load) for the handle in `init`.
#[derive(Clone, Copy, Debug)]
struct Request {
    name: &'static str,
    signature: &'static Signature,
    open: fn(&mut crate::Load<'_>) -> Result<(Opened, Value), crate::Off>,
}

/// The signature of `T`'s getter: no parameters, the handle as an object.
struct Getter<T>(core::marker::PhantomData<T>);

impl<T: Object> Getter<T> {
    const SIGNATURE: &'static Signature = &Signature::new(&[], &Ty::Object(T::CLASS));
}

/// The function behind an [`Opened`] namespace: the function's place in its
/// namespace, from [`Declared`], and its arguments.
type OpenedFn = dyn Fn(usize, &mut [Value]) -> Result<Value, Error> + Send + Sync;

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
    pub fn call(&self, function: &Declared, args: &mut [Value]) -> Result<Value, Error> {
        (self.0)(function.index(), args)
    }
}

impl Namespace {
    const fn of<T: Bound>() -> Self {
        Self {
            name: T::NAME,
            doc: T::DOC,
            describe: describe::<T>,
            open: open::<T>,
            init: &[],
            request: None,
        }
    }

    /// [`Namespace::of`] for a handle whose opening switches a loader
    /// feature on: a script opens it with `getter` at start, as a Rust mod
    /// calls the `Load` method of that name in `init`, so a mod that never
    /// asks pays nothing.
    const fn on_request<T: Bound + Object>(getter: &'static str) -> Self {
        Self {
            request: Some(Request {
                name: getter,
                signature: Getter::<T>::SIGNATURE,
                open: request::<T>,
            }),
            ..Self::of::<T>()
        }
    }

    /// The start-time function that opens this namespace, for a namespace a
    /// script asks for; `None` for one [`Namespace::open`] opens for every
    /// mod.
    ///
    /// # Examples
    ///
    /// ```rust,standalone_crate
    /// # extern crate enshwick_core as enshwick;
    /// let getters: Vec<&str> = enshwick::script::NAMESPACES
    ///     .iter()
    ///     .filter_map(|n| Some(n.request_function()?.name()))
    ///     .collect();
    /// assert!(getters.contains(&"altars"));
    /// ```
    #[must_use]
    pub fn request_function(&self) -> Option<Declared> {
        self.request
            .as_ref()
            .map(|request| Declared::new(0, request.name, self.doc, request.signature))
    }

    /// Opens a namespace a script asks for with
    /// [`Namespace::request_function`]: the namespace and its handle as an
    /// object; `None` for a namespace [`Namespace::open`] opens.
    ///
    /// # Errors
    ///
    /// [`Off`](crate::Off) when the game build lacks what it needs, or
    /// after `init`.
    ///
    /// # Examples
    ///
    /// See [`Namespace::request_function`].
    pub fn request(
        &self,
        load: &mut crate::Load<'_>,
    ) -> Option<Result<(Opened, Value), crate::Off>> {
        self.request.as_ref().map(|request| (request.open)(load))
    }

    /// The [`Load`](crate::Load) methods of `init` as the namespace `name`,
    /// callable while the mod starts.
    const fn starting(
        name: &'static str,
        doc: &'static str,
        init: &'static [&'static [InitFunction]],
    ) -> Self {
        Self {
            name,
            doc,
            describe: Vec::new,
            open: |_| Ok(Opened(Arc::new(|_, _| Err(Error::new("no such function"))))),
            init,
            request: None,
        }
    }

    /// Its functions a script calls only while the mod starts, as a Rust
    /// mod calls them in `init`.
    ///
    /// # Examples
    ///
    /// See [`InitFunction`].
    pub fn init_functions(&self) -> impl Iterator<Item = &'static InitFunction> {
        self.init.iter().flat_map(|part| part.iter())
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
    /// # extern crate enshwick_core as enshwick;
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
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::script::{NAMESPACES, Value};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let log = NAMESPACES.iter().find(|n| n.name() == "log").ok_or(enshwick::Off::new("no log"))?;
    ///     let functions = log.functions();
    ///     let info = functions.iter().find(|f| f.name() == "info").ok_or(enshwick::Off::new("no info"))?;
    ///     log.open(load)?.call(info, &mut [Value::String("hello".into())])?;
    ///     Ok(())
    /// }
    /// ```
    pub fn open(&self, load: &mut crate::Load<'_>) -> Result<Opened, crate::Off> {
        (self.open)(load)
    }
}

fn describe<T: Bound>() -> Vec<Declared> {
    declare(T::functions())
}

fn open<T: Bound>(load: &mut crate::Load<'_>) -> Result<Opened, crate::Off> {
    T::resolve(load).map(opened)
}

fn request<T: Bound + Object>(load: &mut crate::Load<'_>) -> Result<(Opened, Value), crate::Off> {
    let this = T::resolve(load)?;
    Ok((opened(this), this.into_value()))
}

fn opened<T: Bound>(this: T) -> Opened {
    Opened(Arc::new(move |index, args| {
        T::functions()
            .nth(index)
            .ok_or_else(|| Error::new(format!("{} has no such function", T::NAME)))?
            .call(&this, args)
    }))
}

mod actions;
mod base;
mod content;
mod inventory;
mod knowledge;
mod ui;
mod world;

/// Every namespace a script reaches, in the order a runtime opens them:
/// each domain's list, lowest domain first.
///
/// # Examples
///
/// See [`Namespace`].
pub const NAMESPACES: &[Namespace] = &joined::<{ count(DOMAINS) }>(DOMAINS);

/// Each domain's namespaces, in the domain order of `domains.toml`.
const DOMAINS: &[&[Namespace]] = &[
    base::NAMESPACES,
    content::NAMESPACES,
    world::NAMESPACES,
    inventory::NAMESPACES,
    knowledge::NAMESPACES,
    actions::NAMESPACES,
    ui::NAMESPACES,
];

/// How many namespaces `lists` hold together.
const fn count(lists: &[&[Namespace]]) -> usize {
    let (mut total, mut at) = (0, 0);
    while at < lists.len() {
        total += lists[at].len();
        at += 1;
    }
    total
}

/// `lists` one after another, as one array of `N`, their count.
const fn joined<const N: usize>(lists: &[&[Namespace]]) -> [Namespace; N] {
    let mut out = [Namespace::of::<Log>(); N];
    let (mut list, mut at) = (0, 0);
    while list < lists.len() {
        let mut item = 0;
        while item < lists[list].len() {
            out[at] = lists[list][item];
            at += 1;
            item += 1;
        }
        list += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{DOMAINS, NAMESPACES};

    #[test]
    fn the_namespaces_are_every_domain_list_in_turn() {
        let expected: Vec<&str> = DOMAINS
            .iter()
            .flat_map(|list| list.iter().map(|n| n.name()))
            .collect();
        let joined: Vec<&str> = NAMESPACES.iter().map(|n| n.name()).collect();
        assert_eq!(joined, expected);
        assert_eq!(joined.first(), Some(&"log"));
        assert_eq!(joined.last(), Some(&"sound"));
    }
}

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

    /// The most detailed level the mod's log keeps, as
    /// [`Host::log_level`](crate::Host::log_level) answers it.
    #[script]
    #[must_use]
    pub fn level(&self) -> crate::abi::Level {
        self.0.log_level()
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
