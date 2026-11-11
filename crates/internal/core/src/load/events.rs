//! `Load`'s methods of a module below core, forwarded to the base handle.

use core::ffi::c_void;
use core::time::Duration;

use crate::abi::{Event, TickData};
use crate::events::TickPeriod;
use crate::services::{Emitter, ModEvent};
use crate::{Load, Off};

#[enshwick_macros::forward(base)]
#[crate::script(ticks)]
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
    ) -> Result<(), Off>;

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
        F: Fn(*const c_void) + Send + Sync + 'static;

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
    #[script]
    pub fn on_tick(
        &mut self,
        every: Duration,
        handler: impl Fn(&TickData) + Send + Sync + 'static,
    ) -> Result<TickPeriod, Off>;

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
    #[script]
    pub fn on_every_step(
        &mut self,
        handler: impl Fn(&TickData) + Send + Sync + 'static,
    ) -> Result<(), Off>;

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
    pub fn game_event_type(&self, name: &'static str)
    -> Result<crate::game_events::EventType, Off>;

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
        F: Fn(&crate::game_events::RawGameEvent<'_>) + Send + Sync + 'static;

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
        F: Fn(&crate::game_events::RawGameEvent<'_>) + Send + Sync + 'static;

    /// Defines the mod event `E` for other mods to subscribe to, and returns
    /// the [`Emitter`] that sends it.
    ///
    /// `E` comes from [`#[event]`](macro@crate::event), named after this
    /// mod: `<id>:<what>`, see
    /// [how ids are namespaced](https://github.com/enshwick/enshwick/blob/main/book/src/concepts/ids.md).
    ///
    /// # Errors
    ///
    /// [`Off`] when the name is outside this mod's id or defined twice.
    ///
    /// # Examples
    ///
    /// See [`Emitter`].
    pub fn define_event<E: ModEvent>(&mut self) -> Result<Emitter<E>, Off>;

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
    ) -> Result<(), Off>;
}
