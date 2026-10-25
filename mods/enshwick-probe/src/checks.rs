//! The probe's checks of the world API, each logged as `PASS`, `FAIL` or
//! `SKIP`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use enshwick_loader_api::ecs::Entity;
use enshwick_loader_api::effects::{Effects, Spawn, Teleport};
use enshwick_loader_api::entity_data::{EntityData, Replication};
use enshwick_loader_api::game_events::{EventType, RawGameEvent};
use enshwick_loader_api::schedule::Scheduler;
use enshwick_loader_api::state::WorldStore;
use enshwick_loader_api::world::{ComponentType, World, WorldError};
use enshwick_loader_api::{Load, Off};

/// Entities whose stats a check compares.
const SAMPLE: usize = 5;
/// How long a destroyed entity may take to leave the world.
const DESTROY_WAIT: Duration = Duration::from_secs(2);
/// How long a written component may take to reach its replicated copy.
const REPLICATION_WAIT: Duration = Duration::from_secs(1);
/// Where the probe pings, far from anything a player would see.
const PING_AT: [f64; 3] = [-12345.0, -500.0, 777.0];
/// When the probe looks at a teleported entity's components.
const TELEPORT_PEEK: Duration = Duration::from_millis(500);

/// How often the position is logged after it.
const SELF_TELEPORT_STEP: Duration = Duration::from_millis(250);
/// How many times: five seconds.
const SELF_TELEPORT_STEPS: u32 = 20;

/// What the probe keeps per world: how often it checked it.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(default)]
struct Visits {
    checks: u32,
    /// Placed entities seen at the last check, with their template's first
    /// bytes, for telling whether ids stay with entities across loads.
    seen: Vec<(u32, u64)>,
}

/// How many placed entities the id comparison keeps.
const ID_SAMPLE: usize = 500;

/// The checks and what they need.
#[derive(Debug)]
pub(crate) struct Checks {
    world: World,
    network_health: ComponentType,
    health: ComponentType,
    /// `Teleport`, `TeleportCount` and `LastTransform`, which the game's
    /// teleport reads and writes.
    teleport_parts: [ComponentType; 3],
    experience: ComponentType,
    store: WorldStore<Visits>,
    save_asked: AtomicBool,
    scheduler: Scheduler,
    /// Whether the world refused the replicated health as a writable type.
    copies_refused: bool,
    effects: Effects,
    died: EventType,
    /// Set from the ping the probe raised until the game hands it back.
    ping_sent: AtomicBool,
    /// The check each sampled entity was marked at.
    marks: EntityData<u32>,
}

fn pass(what: &str) {
    enshwick_loader_api::info!("PASS {what}");
}

fn fail(what: &str) {
    enshwick_loader_api::warn!("FAIL {what}");
}

impl Checks {
    pub(crate) fn new(load: &mut Load<'_>) -> Result<Self, Off> {
        let world = load.world()?;
        Ok(Self {
            network_health: world.component_type(load, "keen::ecs::NetworkHealth")?,
            health: world.component_type(load, "keen::ecs::Health")?,
            teleport_parts: [
                world.component_type(load, "keen::ecs::Teleport")?,
                world.component_type(load, "keen::ecs::TeleportCount")?,
                world.component_type(load, "keen::ecs::LastTransform")?,
            ],
            experience: world.component_type(load, "keen::ecs::Experience")?,
            store: load.world_store("visits")?,
            world,
            save_asked: AtomicBool::new(false),
            scheduler: load.scheduler()?,
            copies_refused: world
                .writable_type(load, "keen::ecs::NetworkHealth")
                .is_err(),
            effects: load.effects(),
            died: load.event_type("keen::ecs::EntityDiedEvent")?,
            ping_sent: AtomicBool::new(false),
            marks: load.entity_data("mark", Replication::Server)?,
        })
    }

    /// Runs every check on the game thread.
    pub(crate) fn run(&self) {
        let key = self.world.key();
        match &key {
            Some(key) => pass(&format!("world key {key}")),
            None => fail("world key: none"),
        }
        self.queries();
        self.stats();
        self.writes();
        self.effects();
        self.spawns();
        let now: Vec<(u32, u64)> = self
            .world
            .entities(self.world.transform_type())
            .iter()
            .take(ID_SAMPLE)
            .filter_map(|placed| {
                let template = self.world.template(placed.entity)?;
                let head = u64::from_le_bytes(template[..8].try_into().ok()?);
                Some((placed.entity.0, head))
            })
            .collect();
        match self.store.update(|visits| {
            visits.checks += 1;
            let before = std::mem::replace(&mut visits.seen, now.clone());
            (visits.checks, before)
        }) {
            Some((n, before)) if self.store.key() == key => {
                pass(&format!("world store open, check {n} of this world"));
                ids_across_loads(&before, &now);
                self.entity_data(n, &before, &now);
            }
            _ => fail("world store: not open for the running world"),
        }
        self.refusals();
        match self.world.save() {
            Ok(()) => {
                self.save_asked.store(true, Ordering::Release);
                pass("save asked; WorldSaved should follow");
            }
            Err(why) => fail(&format!("save: {why}")),
        }
    }

    fn queries(&self) {
        let placed = self.world.entities(self.world.transform_type());
        if placed.is_empty() {
            fail("entities: no placed entity");
            return;
        }
        pass(&format!("entities: {} placed", placed.len()));
        let Some(first) = placed.first() else { return };
        match self.world.position(first.entity) {
            Some(at) if first.distance(at) < 0.01 => pass(&format!(
                "entity {} placed at {at:?}, as its transform says",
                first.entity.0
            )),
            other => fail(&format!(
                "entity {} placed at {:?}, its transform says {other:?}",
                first.entity.0, first.position
            )),
        }
        let near = self
            .world
            .near(first.position, 5.0, self.world.transform_type());
        if near.first().map(|p| p.entity) == Some(first.entity) {
            pass(&format!(
                "near: {} within 5 m of entity {}, itself first",
                near.len(),
                first.entity.0
            ));
        } else {
            fail("near: the entity is not first near itself");
        }
    }

    fn stats(&self) {
        let actors = self.world.entities(self.network_health);
        if actors.is_empty() {
            enshwick_loader_api::info!(
                "SKIP vitals: no actor with replicated health in the loaded world"
            );
        }
        for placed in actors.iter().take(SAMPLE) {
            self.compare(placed.entity);
        }
        match self.world.entities(self.experience).first() {
            Some(placed) => match self.world.progress(placed.entity) {
                Some(progress) => pass(&format!(
                    "progress of entity {}: {progress:?}",
                    placed.entity.0
                )),
                None => fail(&format!("progress of entity {}: none", placed.entity.0)),
            },
            None => enshwick_loader_api::info!("SKIP progress: no character in the world"),
        }
    }

    /// Checks that the replicated health is refused as a writable type, and
    /// writes the health of an actor in the world, if there is one.
    fn writes(&self) {
        if self.copies_refused {
            pass("writes: the replicated health is refused as a writable type");
        } else {
            fail("writes: the replicated health is writable");
        }
        let actor = self
            .world
            .entities(self.network_health)
            .into_iter()
            .map(|placed| placed.entity)
            .find(|&entity| {
                self.world
                    .vitals(entity)
                    .is_some_and(|v| v.health.current > 1)
            });
        match actor {
            Some(entity) => {
                Self::write_health(self.world, self.scheduler, self.network_health, entity);
            }
            None => enshwick_loader_api::info!(
                "SKIP writes: no living actor with replicated health; the spawn check tries its own"
            ),
        }
    }

    /// Writes `entity`'s health one below what it has, checks the read back
    /// and the replicated copy a moment later, then puts it back.
    fn write_health(
        world: World,
        scheduler: Scheduler,
        network_health: ComponentType,
        entity: Entity,
    ) {
        let Some(before) = world.vitals(entity).map(|v| v.health) else {
            return;
        };
        let target = before.current - 1;
        match world.set_health(entity, target) {
            Ok(written) if written.current == target => pass(&format!(
                "set_health of entity {} to {target}: {written:?}",
                entity.0
            )),
            other => {
                fail(&format!(
                    "set_health of entity {} to {target}: {other:?}",
                    entity.0
                ));
                return;
            }
        }
        match world.heal(entity, i32::MIN) {
            Ok(written) if written.current == 1 => {
                pass("heal by i32::MIN keeps health at 1");
            }
            other => fail(&format!("heal by i32::MIN: {other:?}")),
        }
        let _back_to_target = world.set_health(entity, target);
        scheduler.after(REPLICATION_WAIT, move || {
            let replicated = world.component(entity, network_health).map(|data| {
                // SAFETY: NetworkHealth is health then its maximum, two u32,
                // read at once on the game thread.
                unsafe { data.cast::<[u32; 2]>().read_unaligned() }
            });
            match replicated {
                Some([health, _]) if health == target.cast_unsigned() => pass(&format!(
                    "the replicated health of entity {} follows the write: {health}",
                    entity.0
                )),
                other => fail(&format!(
                    "the replicated health of entity {} after the write: {other:?}, expected {target}",
                    entity.0
                )),
            }
            match world.set_health(entity, before.current) {
                Ok(_) => pass(&format!("health of entity {} put back", entity.0)),
                Err(why) => fail(&format!("health of entity {} not put back: {why}", entity.0)),
            }
        });
    }

    /// Raises a ping and checks two refusals: a type off the allow list and
    /// a teleport of an entity not in the world.
    fn effects(&self) {
        match self.effects.ping(Entity(0), PING_AT) {
            Ok(()) => {
                self.ping_sent.store(true, Ordering::Release);
                pass("ping raised; the game should hand it back next step");
            }
            Err(why) => fail(&format!("ping: {why}")),
        }
        let bytes = vec![0u8; self.died.size() as usize];
        match self.effects.emit_raw(self.died, &bytes) {
            Err(WorldError::Refused) => pass("emit refuses a type off the allow list"),
            other => fail(&format!("emit of a death: {other:?}")),
        }
        match self
            .effects
            .teleport(Entity(u32::MAX - 1), [0.0; 3], Teleport::new())
        {
            Err(WorldError::Gone) => pass("teleport refuses an entity not in the world"),
            other => fail(&format!("teleport of a missing entity: {other:?}")),
        }
    }

    /// Spawns a copy of an entity in the world three metres above it,
    /// preferring one with health, then checks it arrived, writes its
    /// health, teleports it and destroys it.
    fn spawns(&self) {
        let source = self
            .world
            .entities(self.health)
            .into_iter()
            .chain(self.world.entities(self.world.transform_type()))
            .find_map(|placed| Some((self.world.template(placed.entity)?, placed.position)));
        let Some((template, [x, y, z])) = source else {
            enshwick_loader_api::info!("SKIP spawns: no entity with a template in the world");
            return;
        };
        self.spawn(template, [x, y + 3.0, z]);
    }

    /// Spawns one entity of `template` at `at` and follows it through
    /// writes, a teleport and its destruction.
    pub(crate) fn spawn(&self, template: [u8; 16], at: [f64; 3]) -> String {
        let entity = match self.effects.spawn(template, at, Spawn::new()) {
            Ok(entity) => entity,
            Err(why) => {
                fail(&format!("spawn of {template:02x?}: {why}"));
                return format!("{why}");
            }
        };
        pass(&format!(
            "spawn of {template:02x?} at {at:?} handed out entity {}",
            entity.0
        ));
        let world = self.world;
        let effects = self.effects;
        let scheduler = self.scheduler;
        let network_health = self.network_health;
        let teleport_parts = self.teleport_parts;
        let marks = self.marks;
        self.scheduler.after(DESTROY_WAIT, move || {
            if world.template(entity) == Some(template) {
                pass(&format!("spawned entity {} is in the world", entity.0));
            } else {
                fail(&format!("spawned entity {} is not in the world", entity.0));
                return;
            }
            match world.position(entity) {
                Some(p) if (0..3).all(|i| (p[i] - at[i]).abs() < 0.5) => {
                    pass(&format!("spawned entity {} stands at {p:?}", entity.0));
                }
                other => fail(&format!(
                    "spawned entity {} stands at {other:?}, asked {at:?}",
                    entity.0
                )),
            }
            let vitals = world.vitals(entity);
            enshwick_loader_api::info!("spawned entity {} has vitals {vitals:?}", entity.0);
            if vitals.is_some_and(|v| v.health.current > 1) {
                Self::write_health(world, scheduler, network_health, entity);
            }
            let to = [at[0] + 5.0, at[1], at[2]];
            let movable = world.component(entity, teleport_parts[1]).is_some();
            match effects.teleport(entity, to, Teleport::new()) {
                Ok(()) if movable => pass(&format!("teleport of entity {} asked", entity.0)),
                Err(WorldError::Refused) if !movable => {
                    pass(&format!(
                        "teleport of entity {} refused: the game's teleport does not move it",
                        entity.0
                    ));
                    destroy_marked(world, scheduler, marks, entity);
                    return;
                }
                other => {
                    fail(&format!("teleport of entity {} (movable {movable}): {other:?}", entity.0));
                    return;
                }
            }
            scheduler.after(TELEPORT_PEEK, move || {
                enshwick_loader_api::info!(
                    "entity {} a moment after its teleport: Teleport {}, TeleportCount {}, LastTransform {}",
                    entity.0,
                    world.component(entity, teleport_parts[0]).is_some(),
                    world.component(entity, teleport_parts[1]).is_some(),
                    world.component(entity, teleport_parts[2]).is_some(),
                );
            });
            scheduler.after(DESTROY_WAIT, move || {
                match world.position(entity) {
                    Some(p) if (p[0] - to[0]).abs() < 0.5 => {
                        pass(&format!("teleported entity {} stands at {p:?}", entity.0));
                    }
                    other => fail(&format!(
                        "teleported entity {} stands at {other:?}, asked {to:?}",
                        entity.0
                    )),
                }
                destroy_marked(world, scheduler, marks, entity);
            });
        });
        format!("entity {}", entity.0)
    }

    /// Notes a ping the game raised, and whether it is the probe's.
    pub(crate) fn pinged(&self, event: &RawGameEvent<'_>) {
        let x = event.read::<i64>(16);
        if x == Some(enshwick_loader_api::ecs::fixed(PING_AT)[0])
            && self.ping_sent.swap(false, Ordering::AcqRel)
        {
            pass("the game handed back the probe's ping as a game event");
        }
    }

    /// Compares `entity`'s vitals with the health the server replicates,
    /// and logs its time in the shroud.
    fn compare(&self, entity: Entity) {
        enshwick_loader_api::info!(
            "shroud of entity {}: {:?}, in it {:?}; skills {:?}",
            entity.0,
            self.world.shroud_time(entity),
            self.world.in_shroud(entity),
            self.world.skills(entity)
        );
        let vitals = self.world.vitals(entity);
        let replicated = self
            .world
            .component(entity, self.network_health)
            .map(|data| {
                // SAFETY: NetworkHealth is health then its maximum, two u32,
                // read at once on the game thread.
                unsafe { data.cast::<[u32; 2]>().read_unaligned() }
            });
        match (vitals, replicated) {
            (Some(v), Some([health, max]))
                if v.health.current.cast_unsigned() == health
                    && v.health.max.cast_unsigned() == max =>
            {
                pass(&format!(
                    "vitals of entity {}: {v:?}, as replicated",
                    entity.0
                ));
            }
            (Some(v), None) => pass(&format!(
                "vitals of entity {}: {v:?}, none replicated",
                entity.0
            )),
            (v, r) => fail(&format!(
                "vitals of entity {}: {v:?}, replicated {r:?}",
                entity.0
            )),
        }
    }

    fn refusals(&self) {
        match self.world.destroy(Entity(0)) {
            Err(WorldError::Refused) => pass("destroy refuses entity 0"),
            other => fail(&format!("destroy of entity 0: {other:?}")),
        }
        match self.world.destroy(Entity(u32::MAX - 1)) {
            Err(WorldError::Gone) => pass("destroy refuses an entity not in the world"),
            other => fail(&format!("destroy of a missing entity: {other:?}")),
        }
    }

    /// Notes a finished save.
    pub(crate) fn saved(&self) {
        if self.save_asked.swap(false, Ordering::AcqRel) {
            pass("WorldSaved after the save asked");
        }
    }

    /// The `vitals` action: an entity's vitals and progress.
    pub(crate) fn vitals_of(&self, entity: Entity) -> String {
        self.compare(entity);
        format!(
            "vitals {:?}, progress {:?}",
            self.world.vitals(entity),
            self.world.progress(entity)
        )
    }

    /// The `spawn` action: a copy of an entity three metres above it.
    pub(crate) fn spawn_copy(&self, entity: Entity) -> String {
        match (self.world.template(entity), self.world.position(entity)) {
            (Some(template), Some([x, y, z])) => self.spawn(template, [x, y + 3.0, z]),
            _ => format!("entity {} has no template or place", entity.0),
        }
    }

    /// The `notice` action: shows the text to the player who asked.
    pub(crate) fn notice(&self, player: &enshwick_loader_api::world::Player, text: &str) -> String {
        let how = self.world.notify(player, text);
        enshwick_loader_api::info!("notice to {}: {how:?}", player.name);
        format!("{how:?}")
    }

    /// The `teleport` action: moves the asking player's character by `by`,
    /// in metres, and logs where it stands four times a second for five
    /// seconds: whether it stays or snaps back.
    pub(crate) fn teleport_self(&self, entity: Entity, by: [f64; 3]) -> String {
        let Some(from) = self.world.position(entity) else {
            return format!("entity {} has no place", entity.0);
        };
        let to = [from[0] + by[0], from[1] + by[1], from[2] + by[2]];
        if let Err(why) = self.effects.teleport(entity, to, Teleport::new()) {
            fail(&format!(
                "R7 teleport of entity {} to {to:?}: {why}",
                entity.0
            ));
            return format!("{why}");
        }
        enshwick_loader_api::info!(
            "R7 teleport of entity {} asked from {from:?} to {to:?}",
            entity.0
        );
        let world = self.world;
        let parts = self.teleport_parts;
        for step in 1..=SELF_TELEPORT_STEPS {
            self.scheduler.after(SELF_TELEPORT_STEP * step, move || {
                let at = world.position(entity);
                let distance = |p: [f64; 3], q: [f64; 3]| {
                    ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
                };
                let place = at.map_or_else(
                    || "nowhere".to_owned(),
                    |p| {
                        format!(
                            "{p:?}, {:.2} m from the target, {:.2} m from the start",
                            distance(p, to),
                            distance(p, from)
                        )
                    },
                );
                enshwick_loader_api::info!(
                    "R7 +{} ms: entity {} at {place}; Teleport {}, TeleportCount {}, LastTransform {}",
                    (SELF_TELEPORT_STEP * step).as_millis(),
                    entity.0,
                    world.component(entity, parts[0]).is_some(),
                    world.component(entity, parts[1]).is_some(),
                    world.component(entity, parts[2]).is_some(),
                );
                if step == SELF_TELEPORT_STEPS {
                    let verdict = match at {
                        Some(p) if distance(p, to) < 1.0 => "stays at the target",
                        Some(p) if distance(p, from) < 1.0 => "is back where it started",
                        Some(_) => "is elsewhere (it may have walked)",
                        None => "has no place",
                    };
                    enshwick_loader_api::info!("R7 after 5 s entity {} {verdict}", entity.0);
                }
            });
        }
        format!("teleport asked to {to:?}; see probe.log for the next 5 s")
    }

    /// The `destroy` action.
    pub(crate) fn destroy(&self, entity: Entity) -> String {
        let template = self.world.template(entity);
        let outcome = self.world.destroy(entity);
        enshwick_loader_api::info!(
            "destroy of entity {} (template {template:02x?}): {outcome:?}",
            entity.0
        );
        if outcome.is_ok() {
            let world = self.world;
            self.scheduler.after(DESTROY_WAIT, move || {
                if world.template(entity).is_none() {
                    pass(&format!("entity {} gone after its destroy", entity.0));
                } else {
                    fail(&format!(
                        "entity {} still there after its destroy",
                        entity.0
                    ));
                }
            });
        }
        format!("{outcome:?}")
    }
}

impl Checks {
    /// Reads the marks the last check left on entities and marks this
    /// check's: entity data across loads.
    fn entity_data(&self, check: u32, before: &[(u32, u64)], now: &[(u32, u64)]) {
        if !before.is_empty() {
            let kept = before
                .iter()
                .filter(|(id, _)| self.marks.get(Entity(*id)) == Some(check - 1))
                .count();
            enshwick_loader_api::info!(
                "entity data across loads: {kept} of {} marked entities keep their mark",
                before.len()
            );
        }
        let marked = now
            .iter()
            .filter(|(id, _)| self.marks.set(Entity(*id), &check).is_ok())
            .count();
        let read = now
            .iter()
            .filter(|(id, _)| self.marks.get(Entity(*id)) == Some(check))
            .count();
        if marked == now.len() && read == marked {
            pass(&format!(
                "entity data: {marked} entities marked and read back"
            ));
        } else {
            fail(&format!(
                "entity data: {marked} of {} marked, {read} read back",
                now.len()
            ));
        }
        match self.marks.set(Entity(u32::MAX - 1), &check) {
            Err(_) => pass("entity data: a missing entity is refused"),
            Ok(()) => fail("entity data: a missing entity took a value"),
        }
    }
}

/// Logs how many entities of the last check have the same id and template
/// now: whether entity ids stay with entities across loads.
fn ids_across_loads(before: &[(u32, u64)], now: &[(u32, u64)]) {
    if before.is_empty() {
        enshwick_loader_api::info!("SKIP ids across loads: the first check of this world");
        return;
    }
    let same = before.iter().filter(|seen| now.contains(seen)).count();
    let moved = before
        .iter()
        .filter(|(_, template)| !now.iter().any(|(_, t)| t == template))
        .count();
    enshwick_loader_api::info!(
        "ids across loads: {same} of {} sampled entities keep their id and template ({moved} templates gone)",
        before.len()
    );
}

/// Marks a spawned entity with entity data, destroys it and checks the
/// mark is gone a moment later, as the loader drops a destroyed entity's
/// data.
fn destroy_marked(world: World, scheduler: Scheduler, marks: EntityData<u32>, entity: Entity) {
    let marked = marks.set(entity, &7).is_ok();
    let destroyed = world.destroy(entity);
    enshwick_loader_api::info!("spawned entity {} destroyed: {destroyed:?}", entity.0);
    if !marked || destroyed.is_err() {
        return;
    }
    scheduler.after(DESTROY_WAIT, move || match marks.get(entity) {
        None => pass(&format!(
            "entity data: destroyed entity {} lost its mark",
            entity.0
        )),
        Some(mark) => fail(&format!(
            "entity data: destroyed entity {} keeps its mark {mark}",
            entity.0
        )),
    });
}
