//! Effects in the server's world: game events a mod raises, teleports and
//! spawns.
//!
//! An event goes where the game's own events go: the loader puts it into
//! the stream the engine moves at the start of the next step, and the
//! systems that read its type act on it as on one the game raised. Only
//! types the server's own systems raise are allowed, each with a typed call
//! here; the loader refuses any other. A teleport is the game's admin
//! teleport: the entity moves after the next step, and clients see it jump.
//! A spawn is the game's admin spawn: the new entity's id comes back at
//! once, and the entity is in the world after the next step.
//!
//! Every call belongs on the server's game thread, from a game event, a
//! [`Scheduler`](crate::schedule::Scheduler) task or an action; on a client,
//! while it hosts a game.
//!
//! # Examples
//!
//! ```
//! use enshwick_loader_api::actions::{Action, Outcome};
//! use enshwick_loader_api::effects::Teleport;
//!
//! fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
//!     let effects = load.effects();
//!     let actions = load.actions(enshwick_loader_api::i18n::Texts::default())?;
//!     actions.add(Action::new("home", "Go to the world's centre"), move |request| {
//!         let me = request.player.entity;
//!         match effects.teleport(me, [0.0, 100.0, 0.0], Teleport::new().best_spawn()) {
//!             Ok(()) => Outcome::done("On your way"),
//!             Err(why) => Outcome::refused(format!("No teleport: {why}")),
//!         }
//!     })
//! }
//! ```

use crate::abi::{Header, SpawnRequest, TeleportRequest};
use crate::ecs::Entity;
use crate::game_events::{EventType, Field, Scalar};
use crate::world::WorldError;
use crate::{Host, Load, Off};

/// How an entity arrives, for [`Effects::teleport`].
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct Teleport {
    /// Let the game look for a free spot near the target to stand on, as it
    /// does for respawns.
    pub best_spawn: bool,
    /// The orientation to arrive in, a quaternion `x, y, z, w`; `None`
    /// keeps the entity's own.
    pub facing: Option<[f32; 4]>,
}

impl Teleport {
    /// Straight to the target, keeping the entity's orientation.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            best_spawn: false,
            facing: None,
        }
    }

    /// Let the game find a free spot near the target.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn best_spawn(mut self) -> Self {
        self.best_spawn = true;
        self
    }

    /// Arrive facing `yaw` radians about the vertical axis.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::effects::Teleport;
    ///
    /// let north = Teleport::new().facing_yaw(0.0);
    /// assert_eq!(north.facing, Some([0.0, 0.0, 0.0, 1.0]));
    /// ```
    #[must_use]
    pub fn facing_yaw(mut self, yaw: f32) -> Self {
        let half = yaw / 2.0;
        self.facing = Some([0.0, half.sin(), 0.0, half.cos()]);
        self
    }
}

/// How an entity starts, for [`Effects::spawn`].
///
/// # Examples
///
/// ```
/// use core::time::Duration;
/// use enshwick_loader_api::effects::Spawn;
///
/// let brief = Spawn::new().level(10).despawn_after(Duration::from_secs(60));
/// assert_eq!(brief.level, Some(10));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct Spawn {
    /// The level the game scales the entity to, as for enemies; `None` for
    /// the template's own.
    pub level: Option<u32>,
    /// Game time after which the game removes the entity; `None` keeps it.
    pub despawn_after: Option<core::time::Duration>,
    /// The orientation, a quaternion `x, y, z, w`; `None` for none turned.
    pub facing: Option<[f32; 4]>,
}

impl Spawn {
    /// The template as it is.
    ///
    /// # Examples
    ///
    /// See [`Spawn`].
    #[must_use]
    pub const fn new() -> Self {
        Self {
            level: None,
            despawn_after: None,
            facing: None,
        }
    }

    /// Spawn at `level`.
    ///
    /// # Examples
    ///
    /// See [`Spawn`].
    #[must_use]
    pub const fn level(mut self, level: u32) -> Self {
        self.level = Some(level);
        self
    }

    /// Let the game remove the entity after `time` of game time.
    ///
    /// # Examples
    ///
    /// See [`Spawn`].
    #[must_use]
    pub const fn despawn_after(mut self, time: core::time::Duration) -> Self {
        self.despawn_after = Some(time);
        self
    }

    /// Turn the entity `yaw` radians about the vertical axis.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::effects::Spawn;
    ///
    /// assert!(Spawn::new().facing_yaw(1.0).facing.is_some());
    /// ```
    #[must_use]
    pub fn facing_yaw(mut self, yaw: f32) -> Self {
        let half = yaw / 2.0;
        self.facing = Some([0.0, half.sin(), 0.0, half.cos()]);
        self
    }
}

/// The fields of a `keen::WorldTransform` inside an event.
#[derive(Clone, Copy, Debug)]
struct TransformAt {
    position: u64,
    orientation: u64,
    scale: u64,
}

/// `NpcCommandEvent`: a survivor walks somewhere or resets.
#[derive(Clone, Copy, Debug)]
struct NpcCommand {
    ty: EventType,
    kind: Field<u8>,
    npc: Field<u32>,
    target: TransformAt,
}

/// `SetToggleStateEvent`: a signal switches logic on or off.
#[derive(Clone, Copy, Debug)]
struct SetToggle {
    ty: EventType,
    sender: Field<Entity>,
    signal: Field<u32>,
    state: Field<bool>,
}

/// `AddThreatTargetEvent`: an enemy takes notice of a target.
#[derive(Clone, Copy, Debug)]
struct AddThreat {
    ty: EventType,
    attacker: Field<Entity>,
    target: Field<Entity>,
    threat: Field<f32>,
}

/// `UiPingEvent`: a ping on the map and in the world.
#[derive(Clone, Copy, Debug)]
struct Ping {
    ty: EventType,
    sender: Field<Entity>,
    position: u64,
}

/// What a survivor does on [`Effects::npc_command`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::effects::NpcOrder;
///
/// let order = NpcOrder::GoTo([10.0, 50.0, -3.0]);
/// assert_ne!(order, NpcOrder::Reset);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum NpcOrder {
    /// Walk to a point, in metres.
    GoTo([f64; 3]),
    /// Drop what it was told and go back to its routine.
    Reset,
}

/// The effects a mod has on the server's world; get them from
/// [`Load::effects`].
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug)]
pub struct Effects {
    host: Host,
    npc: Option<NpcCommand>,
    toggle: Option<SetToggle>,
    threat: Option<AddThreat>,
    ping: Option<Ping>,
}

impl Effects {
    pub(crate) fn resolve(load: &Load<'_>) -> Self {
        let host = load.host();
        let transform = |base: u64| -> Result<TransformAt, Off> {
            Ok(TransformAt {
                position: base + load.field_offset("keen::WorldTransform", "position")?,
                orientation: base + load.field_offset("keen::WorldTransform", "orientation")?,
                scale: base + load.field_offset("keen::WorldTransform", "scale")?,
            })
        };
        let npc = (|| {
            let ty = load.event_type("keen::ecs::NpcCommandEvent")?;
            Ok::<_, Off>(NpcCommand {
                ty,
                kind: ty.field(load, "type")?,
                npc: ty.field(load, "npcId")?,
                target: transform(load.field_offset(ty.name(), "targetTransform")?)?,
            })
        })();
        let toggle = (|| {
            let ty = load.event_type("keen::ecs::SetToggleStateEvent")?;
            Ok::<_, Off>(SetToggle {
                ty,
                sender: ty.field(load, "senderId")?,
                signal: ty.field(load, "signalId")?,
                state: ty.field(load, "newState")?,
            })
        })();
        let threat = (|| {
            let ty = load.event_type("keen::ecs::AddThreatTargetEvent")?;
            Ok::<_, Off>(AddThreat {
                ty,
                attacker: ty.field(load, "attackerId")?,
                target: ty.field(load, "targetId")?,
                threat: ty.field(load, "threatValue")?,
            })
        })();
        let ping = (|| {
            let ty = load.event_type("keen::ecs::UiPingEvent")?;
            Ok::<_, Off>(Ping {
                ty,
                sender: ty.field(load, "senderEntityId")?,
                position: load.field_offset(ty.name(), "position")?,
            })
        })();
        Self {
            host,
            npc: keep("survivor orders", npc),
            toggle: keep("toggles", toggle),
            threat: keep("threat", threat),
            ping: keep("pings", ping),
        }
    }

    /// Orders survivor `npc`, by its survivor id, as the game does when a
    /// player sends it somewhere; the survivor's own behaviour carries the
    /// order out.
    ///
    /// # Errors
    ///
    /// [`WorldError::Missing`] on a build without the event,
    /// [`WorldError::WrongThread`] off the game thread and
    /// [`WorldError::Refused`] when the step holds no room for more events.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::effects::{Effects, NpcOrder};
    ///
    /// fn gather(effects: &Effects, survivor: u32, at: [f64; 3]) {
    ///     if let Err(why) = effects.npc_command(survivor, NpcOrder::GoTo(at)) {
    ///         enshwick_loader_api::debug!("survivor {survivor} stays: {why}");
    ///     }
    /// }
    /// ```
    pub fn npc_command(&self, npc: u32, order: NpcOrder) -> Result<(), WorldError> {
        let event = self.npc.ok_or(WorldError::Missing)?;
        let mut bytes = blank(event.ty)?;
        let (kind, at) = match order {
            NpcOrder::Reset => (0, None),
            NpcOrder::GoTo(at) => (1, Some(at)),
        };
        put(&mut bytes, event.kind, kind)?;
        put(&mut bytes, event.npc, npc)?;
        put_quaternion(&mut bytes, event.target.orientation, [0.0, 0.0, 0.0, 1.0])?;
        put_floats(&mut bytes, event.target.scale, [1.0; 3])?;
        if let Some(at) = at {
            put_position(&mut bytes, event.target.position, at)?;
        }
        self.emit_raw(event.ty, &bytes)
    }

    /// Switches the logic signal `signal` on or off as `sender` would, the
    /// way levers and pressure plates drive doors and lights.
    ///
    /// # Errors
    ///
    /// As for [`Effects::npc_command`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn open(effects: &enshwick_loader_api::effects::Effects, lever: enshwick_loader_api::ecs::Entity, signal: u32) {
    ///     let _sent = effects.set_toggle(lever, signal, true);
    /// }
    /// ```
    pub fn set_toggle(&self, sender: Entity, signal: u32, on: bool) -> Result<(), WorldError> {
        let event = self.toggle.ok_or(WorldError::Missing)?;
        let mut bytes = blank(event.ty)?;
        put(&mut bytes, event.sender, sender)?;
        put(&mut bytes, event.signal, signal)?;
        put(&mut bytes, event.state, on)?;
        self.emit_raw(event.ty, &bytes)
    }

    /// Makes the enemy `attacker` take notice of `target` with `threat`, as
    /// a hit or a call for help does.
    ///
    /// # Errors
    ///
    /// As for [`Effects::npc_command`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::ecs::Entity;
    ///
    /// fn taunt(effects: &enshwick_loader_api::effects::Effects, enemy: Entity, player: Entity) {
    ///     let _noticed = effects.add_threat(enemy, player, 100.0);
    /// }
    /// ```
    pub fn add_threat(
        &self,
        attacker: Entity,
        target: Entity,
        threat: f32,
    ) -> Result<(), WorldError> {
        let event = self.threat.ok_or(WorldError::Missing)?;
        let mut bytes = blank(event.ty)?;
        put(&mut bytes, event.attacker, attacker)?;
        put(&mut bytes, event.target, target)?;
        put(&mut bytes, event.threat, threat)?;
        self.emit_raw(event.ty, &bytes)
    }

    /// Pings `position`, in metres, on the map as `sender` would; the
    /// players see it as a ping from them.
    ///
    /// # Errors
    ///
    /// As for [`Effects::npc_command`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn mark(effects: &enshwick_loader_api::effects::Effects, player: &enshwick_loader_api::world::Player, at: [f64; 3]) {
    ///     let _pinged = effects.ping(player.entity, at);
    /// }
    /// ```
    pub fn ping(&self, sender: Entity, position: [f64; 3]) -> Result<(), WorldError> {
        let event = self.ping.ok_or(WorldError::Missing)?;
        let mut bytes = blank(event.ty)?;
        put(&mut bytes, event.sender, sender)?;
        put_position(&mut bytes, event.position, position)?;
        self.emit_raw(event.ty, &bytes)
    }

    /// Raises an event of type `ty` from its whole reflected layout, `bytes`
    /// of [`EventType::size`]; the loader stamps the time. The loader allows
    /// the types the typed calls raise and refuses any other.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for a type the loader does not allow, a size
    /// that is not the type's, or a full step; [`WorldError::WrongThread`]
    /// off the game thread.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::actions::{Action, Outcome};
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let effects = load.effects();
    ///     let ping = load.event_type("keen::ecs::UiPingEvent")?;
    ///     let actions = load.actions(enshwick_loader_api::i18n::Texts::default())?;
    ///     actions.add(Action::new("ping-origin", "Ping the world origin").admins(), move |_request| {
    ///         let bytes = vec![0u8; ping.size() as usize];
    ///         Outcome::done(format!("{:?}", effects.emit_raw(ping, &bytes)))
    ///     })
    /// }
    /// ```
    pub fn emit_raw(&self, ty: EventType, bytes: &[u8]) -> Result<(), WorldError> {
        let size = u32::try_from(bytes.len()).map_err(|_| WorldError::Refused)?;
        let status = (self.host.api().emit)(self.host.handle(), ty.hash(), bytes.as_ptr(), size);
        WorldError::from_status(status)
    }

    /// Asks the server to move `entity` to `position`, in metres, after the
    /// next step, as its admin teleport does: the entity jumps there and
    /// clients see it arrive.
    ///
    /// # Errors
    ///
    /// [`WorldError::Gone`] for an entity not in the world or without a
    /// place, [`WorldError::Refused`] for one the game's teleport does not
    /// move, such as a prop (it moves actors and characters),
    /// [`WorldError::WrongThread`] off the game thread and
    /// [`WorldError::Missing`] on a build without teleports.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn teleport(
        &self,
        entity: Entity,
        position: [f64; 3],
        how: Teleport,
    ) -> Result<(), WorldError> {
        let mut flags = 0;
        if how.best_spawn {
            flags |= TeleportRequest::BEST_SPAWN;
        }
        if how.facing.is_some() {
            flags |= TeleportRequest::ORIENTED;
        }
        let request = TeleportRequest {
            header: Header::of::<TeleportRequest>(),
            entity: entity.0,
            flags,
            position: crate::ecs::fixed(position),
            orientation: how.facing.unwrap_or([0.0, 0.0, 0.0, 1.0]),
        };
        WorldError::check(
            (self.host.api().teleport)(self.host.handle(), &raw const request),
            WorldError::Gone,
        )
    }
}

impl Effects {
    /// Asks the server to spawn an entity of `template`, its GUID as
    /// [`World::template`](crate::world::World::template) gives it, at
    /// `position` in metres, as its admin spawn does: an enemy, an animal,
    /// a survivor or a prop. The new entity's id comes back at once; the
    /// entity is in the world after the next step, and a template the game
    /// does not know spawns nothing.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for a zero template or a request the world
    /// turns away and [`WorldError::WrongThread`] off the game thread.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::actions::{Action, Arg, Outcome, Value};
    /// use enshwick_loader_api::effects::Spawn;
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let effects = load.effects();
    ///     let world = load.world()?;
    ///     let actions = load.actions(enshwick_loader_api::i18n::Texts::default())?;
    ///     let clone = Action::new("clone", "Spawn a copy of an entity beside you")
    ///         .admins()
    ///         .arg(Arg::number("Entity", 0.0..=f64::from(u32::MAX), 1.0));
    ///     actions.add(clone, move |request| {
    ///         let [Value::Number(id)] = request.args else {
    ///             return Outcome::refused("Name an entity");
    ///         };
    ///         let source = enshwick_loader_api::ecs::Entity(*id as u32);
    ///         let (Some(template), Some([x, y, z])) = (world.template(source), world.position(request.player.entity)) else {
    ///             return Outcome::refused("Nothing to copy, or you are nowhere");
    ///         };
    ///         Outcome::done(format!("{:?}", effects.spawn(template, [x + 2.0, y, z], Spawn::new())))
    ///     })
    /// }
    /// ```
    pub fn spawn(
        &self,
        template: [u8; 16],
        position: [f64; 3],
        how: Spawn,
    ) -> Result<Entity, WorldError> {
        let api = self.host.api();
        let lifetime = how.despawn_after.map_or(0, |time| {
            u64::try_from(time.as_nanos()).unwrap_or(u64::MAX).max(1)
        });
        let request = SpawnRequest {
            header: Header::of::<SpawnRequest>(),
            template,
            position: crate::ecs::fixed(position),
            orientation: how.facing.unwrap_or([0.0, 0.0, 0.0, 1.0]),
            level: how.level.unwrap_or(0),
            reserved: 0,
            lifetime_ns: lifetime,
        };
        let mut entity = 0u32;
        WorldError::from_status((api.spawn)(
            self.host.handle(),
            &raw const request,
            &raw mut entity,
        ))?;
        Ok(Entity(entity))
    }
}

/// `found`, or `None` with a debug line saying why.
fn keep<T>(what: &str, found: Result<T, Off>) -> Option<T> {
    found
        .inspect_err(|off| crate::debug!("{what} off: {off}"))
        .ok()
}

/// The result of a world call that returned `status`.
/// An all-zero event of type `ty`.
fn blank(ty: EventType) -> Result<Vec<u8>, WorldError> {
    usize::try_from(ty.size())
        .map(|size| vec![0u8; size])
        .map_err(|_| WorldError::Missing)
}

fn put<T: Scalar>(bytes: &mut [u8], field: Field<T>, value: T) -> Result<(), WorldError> {
    put_at(bytes, u64::from(field.offset()), value)
}

fn put_at<T: Scalar>(bytes: &mut [u8], at: u64, value: T) -> Result<(), WorldError> {
    let written = usize::try_from(at)
        .ok()
        .and_then(|at| bytes.get_mut(at..))
        .is_some_and(|out| value.to_bytes(out));
    written.then_some(()).ok_or(WorldError::Missing)
}

fn put_position(bytes: &mut [u8], at: u64, position: [f64; 3]) -> Result<(), WorldError> {
    for (i, axis) in crate::ecs::fixed(position).into_iter().enumerate() {
        put_at(bytes, at + 8 * i as u64, axis)?;
    }
    Ok(())
}

fn put_floats<const N: usize>(
    bytes: &mut [u8],
    at: u64,
    values: [f32; N],
) -> Result<(), WorldError> {
    for (i, value) in values.into_iter().enumerate() {
        put_at(bytes, at + 4 * i as u64, value)?;
    }
    Ok(())
}

fn put_quaternion(bytes: &mut [u8], at: u64, value: [f32; 4]) -> Result<(), WorldError> {
    put_floats(bytes, at, value)
}

impl Load<'_> {
    /// The mod's [`Effects`] in the server's world: game events it raises
    /// and teleports; on a client, the built-in server's while it hosts a
    /// game. An action whose event this build lacks answers
    /// [`WorldError::Missing`].
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn effects(&mut self) -> Effects {
        Effects::resolve(self)
    }
}
