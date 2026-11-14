//! The events a script subscribes to: the loader's, and the game's typed ones.

use core::ffi::c_void;

use super::{IntoScript, Record, Ty, Value};

/// A loader event a script subscribes to, from the tables of
/// [`events`](crate::events).
///
/// # Examples
///
/// ```rust,standalone_crate
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

/// The function behind [`GameEventKind::subscribe`].
type SubscribeGame = fn(&mut crate::BaseLoad, Box<GameHandler>) -> Result<(), crate::Off>;

/// A handler of a typed game event: it gets the event's data as a function
/// that converts it, so a handler with nothing to do converts nothing.
type GameHandler = dyn Fn(&dyn Fn() -> Value) + Send + Sync;

/// A typed game event a script subscribes to, from
/// [`game_events`](crate::game_events): its data and how to subscribe.
///
/// # Examples
///
/// ```rust,standalone_crate
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
        E: crate::events::Kind<Data = E> + IntoScript + Clone + Send + Sync,
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
    /// [`Load::on`](../struct.Load.html#method.on) does for the Rust type.
    ///
    /// # Errors
    ///
    /// [`Off`](crate::Off) when this build lacks a field of the event, or
    /// after `init`.
    ///
    /// # Examples
    ///
    /// ```rust,standalone_crate
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let died = enshwick::script::game_events()
    ///         .find(|e| e.name() == "EntityDiedEvent")
    ///         .ok_or(enshwick::Off::new("no such event"))?;
    ///     died.subscribe(load, |data| enshwick::debug!("{:?}", data()))
    /// }
    /// ```
    pub fn subscribe(
        &self,
        load: &mut impl crate::Loading,
        handler: impl Fn(&dyn Fn() -> Value) + Send + Sync + 'static,
    ) -> Result<(), crate::Off> {
        (self.subscribe)(load.base_mut(), Box::new(handler))
    }
}

fn subscribe_game<E>(
    load: &mut crate::BaseLoad,
    handler: Box<GameHandler>,
) -> Result<(), crate::Off>
where
    E: crate::events::Kind<Data = E> + IntoScript + Clone + Send + Sync,
{
    load.on::<E>(move |event| handler(&|| event.clone().into_value()))
}

/// Every typed game event a script may subscribe to.
///
/// # Examples
///
/// See [`GameEventKind`].
pub fn game_events() -> impl Iterator<Item = &'static GameEventKind> {
    crate::game_events::SCRIPT_KINDS.iter()
}
