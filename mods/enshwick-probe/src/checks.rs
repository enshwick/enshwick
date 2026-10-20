//! The probe's checks of the world API, each logged as `PASS`, `FAIL` or
//! `SKIP`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use enshwick_loader_api::actions::{Actions, Teleport};
use enshwick_loader_api::ecs::Entity;
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

/// What the probe keeps per world: how often it checked it.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
struct Visits {
    checks: u32,
}

/// The checks and what they need.
#[derive(Debug)]
pub(crate) struct Checks {
    world: World,
    network_health: ComponentType,
    experience: ComponentType,
    store: WorldStore<Visits>,
    save_asked: AtomicBool,
    scheduler: Scheduler,
    /// Whether the world refused the replicated health as a writable type.
    copies_refused: bool,
    actions: Actions,
    died: EventType,
    /// Set from the ping the probe raised until the game hands it back.
    ping_sent: AtomicBool,
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
            experience: world.component_type(load, "keen::ecs::Experience")?,
            store: load.world_store("visits")?,
            world,
            save_asked: AtomicBool::new(false),
            scheduler: load.scheduler()?,
            copies_refused: world
                .writable_type(load, "keen::ecs::NetworkHealth")
                .is_err(),
            actions: load.actions()?,
            died: load.event_type("keen::ecs::EntityDiedEvent")?,
            ping_sent: AtomicBool::new(false),
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
        self.actions();
        match self.store.update(|visits| {
            visits.checks += 1;
            visits.checks
        }) {
            Some(n) if self.store.key() == key => {
                pass(&format!("world store open, check {n} of this world"));
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

    /// Writes an actor's health one below what it has, checks the read back
    /// and the replicated copy a moment later, then puts it back.
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
        let Some(entity) = actor else {
            enshwick_loader_api::info!("SKIP writes: no living actor with replicated health");
            return;
        };
        let Some(before) = self.world.vitals(entity).map(|v| v.health) else {
            return;
        };
        let target = before.current - 1;
        match self.world.set_health(entity, target) {
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
        match self.world.heal(entity, i32::MIN) {
            Ok(written) if written.current == 1 => {
                pass("heal by i32::MIN keeps health at 1");
            }
            other => fail(&format!("heal by i32::MIN: {other:?}")),
        }
        let _back_to_target = self.world.set_health(entity, target);
        let world = self.world;
        let network_health = self.network_health;
        self.scheduler.after(REPLICATION_WAIT, move || {
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
    fn actions(&self) {
        match self.actions.ping(Entity::NONE, PING_AT) {
            Ok(()) => {
                self.ping_sent.store(true, Ordering::Release);
                pass("ping raised; the game should hand it back next step");
            }
            Err(why) => fail(&format!("ping: {why}")),
        }
        let bytes = vec![0u8; self.died.size() as usize];
        match self.actions.emit_raw(self.died, &bytes) {
            Err(WorldError::Refused) => pass("emit refuses a type off the allow list"),
            other => fail(&format!("emit of a death: {other:?}")),
        }
        match self
            .actions
            .teleport(Entity(u32::MAX - 1), [0.0; 3], Teleport::new())
        {
            Err(WorldError::Gone) => pass("teleport refuses an entity not in the world"),
            other => fail(&format!("teleport of a missing entity: {other:?}")),
        }
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

    /// Compares `entity`'s vitals with the health the server replicates.
    fn compare(&self, entity: Entity) {
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
        match self.world.destroy(Entity::NONE) {
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

    /// `/probe vitals <id>`.
    pub(crate) fn vitals_of(&self, id: &str) -> String {
        let Some(entity) = parse(id) else {
            return format!("not an entity id: {id}");
        };
        self.compare(entity);
        format!(
            "vitals {:?}, progress {:?}",
            self.world.vitals(entity),
            self.world.progress(entity)
        )
    }

    /// `/probe destroy <id>`.
    pub(crate) fn destroy(&self, id: &str) -> String {
        let Some(entity) = parse(id) else {
            return format!("not an entity id: {id}");
        };
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

fn parse(id: &str) -> Option<Entity> {
    let id = id.trim();
    let value = match id.strip_prefix("0x") {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => id.parse().ok()?,
    };
    Some(Entity(value))
}
