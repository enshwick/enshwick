use core::ffi::c_void;
use core::time::Duration;

use crate::abi::{Event, TickData, TickHandle};
use crate::events::{LoaderEvent, TickPeriod, Ticked};
use crate::host::Host;
use crate::services::{Emitter, ModEvent};

use super::{Load, Off, event_spec, hand_over, ok};

impl Load<'_> {
    /// Calls `handler` on every event `E` with its typed data: a loader event
    /// of [`events`](crate::events) on the thread that raises it, or a typed
    /// game event of [`game_events`](crate::game_events) on the server's game
    /// thread. A panic in `handler` turns the mod off.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader does not know the event, after `init` for a
    /// game event, or when this build lacks a field of a game event.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::events::{PlayerJoined, WorldSaved};
    /// use enshwick::game_events::EntityDiedEvent;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on::<PlayerJoined>(|player| enshwick::info!("entity {} joined", player.entity))?;
    ///     load.on::<WorldSaved>(|()| enshwick::info!("saved"))?;
    ///     load.on::<EntityDiedEvent>(|died| enshwick::debug!("{:?} died", died.dead_entity_id))
    /// }
    /// ```
    pub fn on<E: crate::events::Kind>(
        &mut self,
        handler: impl Fn(&E::Data) + Send + Sync + 'static,
    ) -> Result<(), Off> {
        E::subscribe(self, handler)
    }

    /// Calls `handler` on every `event` for this mod with the event's data,
    /// null for events without, on the thread that raises it. A panic in
    /// `handler` turns the mod off.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader does not know `event`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Event;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_raw_event(Event::SETTINGS_CHANGED, |_| enshwick::info!("settings edited"))
    /// }
    /// ```
    pub fn on_raw_event<F>(&mut self, event: Event, handler: F) -> Result<(), Off>
    where
        F: Fn(*const c_void) + Send + Sync + 'static,
    {
        let host = self.host;
        hand_over(
            handler,
            |user| host.subscribe(event, deliver::<F>, user),
            &format!("event {}", event.0),
        )
    }

    /// Calls `handler` on the server's game thread about every `every` of
    /// wall time, after the step that reaches it, and on the first step after
    /// `init`; [`Duration::ZERO`] calls it every step. Steps in between cost
    /// the mod nothing. The [`TickData`] says how many steps and how much time
    /// passed since the handler's last call; the [`TickPeriod`] changes the
    /// period later. A panic in `handler` turns the mod off.
    ///
    /// # Errors
    ///
    /// [`Off`] after `init`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_tick(Duration::from_secs(5), |tick| {
    ///         enshwick::debug!("{} steps in {:?}", tick.elapsed_steps, tick.elapsed());
    ///     })?;
    ///     Ok(())
    /// }
    /// ```
    pub fn on_tick(
        &mut self,
        every: Duration,
        handler: impl Fn(&TickData) + Send + Sync + 'static,
    ) -> Result<TickPeriod, Off> {
        let host = self.host;
        let handle = subscribe_tick(host, every, typed_tick(handler))?;
        Ok(TickPeriod::new(host, handle))
    }

    /// Calls `handler` on the server's game thread after every server step,
    /// for work that follows each step: per-step state, input or the camera.
    /// Every call costs the mod about a microsecond (a server copy of build
    /// 399), on every step, so a handler that can wait takes
    /// [`on_tick`](Self::on_tick).
    /// A panic in `handler` turns the mod off.
    ///
    /// # Errors
    ///
    /// [`Off`] after `init`.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_every_step(|tick| enshwick::debug!("step {}", tick.step))
    /// }
    /// ```
    pub fn on_every_step(
        &mut self,
        handler: impl Fn(&TickData) + Send + Sync + 'static,
    ) -> Result<(), Off> {
        self.on_raw_event(Event::TICKED, typed_tick(handler))
    }

    /// The reflected game event type `name`, such as
    /// `keen::ecs::EntityDiedEvent`, for
    /// [`on_raw_game_event`](Self::on_raw_game_event).
    ///
    /// # Errors
    ///
    /// [`Off`] when the type is not reflected on this build.
    ///
    /// # Examples
    ///
    /// See [`EventType`](crate::game_events::EventType).
    pub fn game_event_type(
        &self,
        name: &'static str,
    ) -> Result<crate::game_events::EventType, Off> {
        crate::game_events::EventType::resolve(self, name)
    }

    /// Calls `handler` on every game event of type `ty`, on the server's
    /// game thread before the systems of the next step run; see
    /// [`game_events`](crate::game_events). A panic in `handler` turns the
    /// mod off.
    ///
    /// # Errors
    ///
    /// [`Off`] after `init`.
    ///
    /// # Examples
    ///
    /// See [`EventType::field`](crate::game_events::EventType::field).
    pub fn on_raw_game_event<F>(
        &mut self,
        ty: crate::game_events::EventType,
        handler: F,
    ) -> Result<(), Off>
    where
        F: Fn(&crate::game_events::RawGameEvent<'_>) + Send + Sync + 'static,
    {
        self.game_events(ty.hash(), ty.name(), handler)
    }

    /// Calls `handler` on every game event of every type, for finding out
    /// what the game raises. Each event is copied for the mod, so leave this
    /// to development builds.
    ///
    /// # Errors
    ///
    /// As for [`on_raw_game_event`](Self::on_raw_game_event).
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::atomic::{AtomicU64, Ordering};
    ///
    /// static SEEN: AtomicU64 = AtomicU64::new(0);
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_every_raw_game_event(|_event| {
    ///         SEEN.fetch_add(1, Ordering::Relaxed);
    ///     })
    /// }
    /// ```
    pub fn on_every_raw_game_event<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: Fn(&crate::game_events::RawGameEvent<'_>) + Send + Sync + 'static,
    {
        self.game_events(0, "every type", handler)
    }

    pub(super) fn game_events<F>(
        &mut self,
        type_hash: u32,
        what: &str,
        handler: F,
    ) -> Result<(), Off>
    where
        F: Fn(&crate::game_events::RawGameEvent<'_>) + Send + Sync + 'static,
    {
        let api = self.api();
        let host = self.host.handle();
        hand_over(
            handler,
            |user| (api.game_event)(host, type_hash, deliver_game::<F>, user),
            &format!("game events of {what}"),
        )
    }

    /// Defines the mod event `E` for other mods to subscribe to, and returns
    /// the [`Emitter`] that sends it.
    ///
    /// `E` comes from [`#[event]`](macro@crate::event), named after this
    /// mod: `<id>:<what>`.
    ///
    /// # Errors
    ///
    /// [`Off`] when the name is outside this mod's id or defined twice.
    ///
    /// # Examples
    ///
    /// See [`Emitter`].
    pub fn define_event<E: ModEvent>(&mut self) -> Result<Emitter<E>, Off> {
        let spec = event_spec::<E>();
        let mut id = 0;
        let status = (self.api().define_event)(self.host.handle(), &raw const spec, &raw mut id);
        ok(status, Emitter::new(self.host, id), || {
            format!(
                "event {}: named after another mod or defined twice",
                E::NAME
            )
        })
    }

    /// Calls `handler` on every mod event `E` another mod sends, on the
    /// sending thread. A panic in `handler` turns this mod off.
    ///
    /// The loader matches the subscription once every mod is initialised:
    /// with no running mod defining `E`, or a definition that lacks or
    /// changed a field of `E`, it logs why and the handler stays idle.
    ///
    /// # Errors
    ///
    /// [`Off`] when called after `init`.
    ///
    /// # Examples
    ///
    /// ```
    /// #[enshwick::event(of = trading_post, name = "sold")]
    /// #[derive(Clone, Copy, Debug)]
    /// pub struct Sold {
    ///     pub item: u32,
    /// }
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_mod_event(|sold: &Sold| enshwick::info!("sold {}", sold.item))
    /// }
    /// ```
    pub fn on_mod_event<E: ModEvent>(
        &mut self,
        handler: impl Fn(&E) + Send + Sync + 'static,
    ) -> Result<(), Off> {
        subscribe_mod_event(self.host, handler)
    }
}

/// A tick handler on the raw data, which reads it as a [`TickData`].
fn typed_tick(handler: impl Fn(&TickData)) -> impl Fn(*const c_void) {
    move |raw| {
        // SAFETY: the loader passes a tick's data with every tick handler's
        // call, null or valid with no other thread writing it for the call.
        if let Some(tick) = unsafe { <Ticked as LoaderEvent>::data(raw) } {
            handler(&tick);
        }
    }
}

/// Subscribes `handler` on a period of `every`.
fn subscribe_tick<F>(host: Host, every: Duration, handler: F) -> Result<TickHandle, Off>
where
    F: Fn(*const c_void) + Send + Sync + 'static,
{
    let mut handle = TickHandle(0);
    let out = &raw mut handle;
    hand_over(
        handler,
        |user| {
            (host.api().subscribe_tick)(
                host.handle(),
                crate::events::nanos(every),
                deliver::<F>,
                user,
                out,
            )
        },
        "the tick",
    )?;
    Ok(handle)
}

/// Subscribes `handler` to the mod event `E`.
pub(super) fn subscribe_mod_event<E: ModEvent, F: Fn(&E) + Send + Sync + 'static>(
    host: Host,
    handler: F,
) -> Result<(), Off> {
    let spec = event_spec::<E>();
    hand_over(
        handler,
        |user| {
            (host.api().subscribe_event)(
                host.handle(),
                &raw const spec,
                deliver_mod_event::<E, F>,
                user,
            )
        },
        &format!("event {}", E::NAME),
    )
}

/// Runs a mod event handler; `user` is the handler boxed by
/// `Load::on_mod_event`, alive for the process.
pub(super) unsafe extern "system" fn deliver_mod_event<E: ModEvent, F: Fn(&E)>(
    data: *const c_void,
    user: *mut c_void,
) {
    // SAFETY: `user` is the leaked `Box<F>` this function was subscribed with.
    let handler = unsafe { &*user.cast::<F>() };
    // SAFETY: the loader checked the sender's layout starts with every
    // field of `E`, and the data lives for the call.
    let Some(event) = (unsafe { data.cast::<E>().as_ref() }) else {
        return;
    };
    crate::host::guard("panic in a mod event handler", || (), || handler(event));
}

/// Runs a subscribed handler; `user` is the handler boxed by
/// [`Load::on_raw_event`], alive for the process.
pub(super) unsafe extern "system" fn deliver<F: Fn(*const c_void)>(
    _event: Event,
    data: *const c_void,
    user: *mut c_void,
) {
    // SAFETY: `user` is the leaked `Box<F>` this function was subscribed with.
    let handler = unsafe { &*user.cast::<F>() };
    crate::host::guard("panic in an event handler", || (), || handler(data));
}

/// Runs a game event handler; `user` is the handler boxed by
/// `Load::game_events`, alive for the process.
pub(super) unsafe extern "system" fn deliver_game<F: Fn(&crate::game_events::RawGameEvent<'_>)>(
    event: *const crate::abi::GameEventData,
    user: *mut c_void,
) {
    // SAFETY: `user` is the leaked `Box<F>` this function was registered with.
    let handler = unsafe { &*user.cast::<F>() };
    // SAFETY: the loader passes its event, valid for the call.
    let Some(event) = (unsafe { crate::game_events::RawGameEvent::from_raw(event) }) else {
        return;
    };
    crate::host::guard("panic in a game event handler", || (), || handler(&event));
}
