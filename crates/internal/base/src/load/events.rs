use core::ffi::c_void;
use core::time::Duration;

use crate::abi::{Event, TickData, TickHandle};
use crate::events::{LoaderEvent, TickPeriod, Ticked};
use crate::host::Host;
use crate::services::{Emitter, ModEvent};

use super::{BaseLoad, Off, event_spec, hand_over, ok};

impl BaseLoad {
    /// As `Load::on`.
    pub fn on<E: crate::events::Kind>(
        &mut self,
        handler: impl Fn(&E::Data) + Send + Sync + 'static,
    ) -> Result<(), Off> {
        E::subscribe(self, handler)
    }

    /// As `Load::on_raw_event`.
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

    /// As `Load::on_tick`.
    pub fn on_tick(
        &mut self,
        every: Duration,
        handler: impl Fn(&TickData) + Send + Sync + 'static,
    ) -> Result<TickPeriod, Off> {
        let host = self.host;
        let handle = subscribe_tick(host, every, typed_tick(handler))?;
        Ok(TickPeriod::new(host, handle))
    }

    /// As `Load::on_every_step`.
    pub fn on_every_step(
        &mut self,
        handler: impl Fn(&TickData) + Send + Sync + 'static,
    ) -> Result<(), Off> {
        self.on_raw_event(Event::TICKED, typed_tick(handler))
    }

    /// As `Load::game_event_type`.
    pub fn game_event_type(
        &self,
        name: &'static str,
    ) -> Result<crate::game_events::EventType, Off> {
        crate::game_events::EventType::resolve(self, name)
    }

    /// As `Load::on_raw_game_event`.
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

    /// As `Load::on_every_raw_game_event`.
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
        game_events_of(self.host, type_hash, what, handler)
    }

    /// As `Load::define_event`.
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

    /// As `Load::on_mod_event`.
    pub fn on_mod_event<E: ModEvent>(
        &mut self,
        handler: impl Fn(&E) + Send + Sync + 'static,
    ) -> Result<(), Off> {
        subscribe_mod_event(self.host, handler)
    }
}

/// [`Load::on_raw_game_event`] for a runtime that holds the mod's host
/// during its init, such as a script's: `handler` on every game event of the
/// type hashed `type_hash`, `what` naming it in a refusal.
pub fn game_events_of<F>(host: Host, type_hash: u32, what: &str, handler: F) -> Result<(), Off>
where
    F: Fn(&crate::game_events::RawGameEvent<'_>) + Send + Sync + 'static,
{
    let api = host.api();
    let handle = host.handle();
    hand_over(
        handler,
        |user| (api.game_event)(handle, type_hash, deliver_game::<F>, user),
        &format!("game events of {what}"),
    )
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
