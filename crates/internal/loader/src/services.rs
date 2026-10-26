//! Services and events between mods. During init the loader keeps what mods
//! publish and define, and checks every user's layout against it field by
//! field; once every mod is initialised it seals the events, and a sent
//! event goes straight to its subscribers on the sending thread.

use std::sync::{Mutex, OnceLock, PoisonError};

use enshwick::abi::{ModEventFn, Status};
use enshwick::services::{LayoutError, compare};

/// A layout as `(field name, hash)` per field.
pub(crate) type Fields = Vec<(String, u64)>;

/// What a mod is called in messages: its id and version, and whether it is off.
pub(crate) trait Names {
    /// The id and version of the mod at `index`.
    fn name(&self, index: usize) -> (&str, &str);
    /// Whether the mod at `index` is off.
    fn off(&self, index: usize) -> bool;
}

/// A published service.
#[derive(Debug)]
struct Published {
    owner: usize,
    name: String,
    major: u32,
    fields: Fields,
    table: usize,
}

/// A defined mod event.
#[derive(Debug)]
struct Definition {
    owner: usize,
    name: String,
    fields: Fields,
}

/// A subscription waiting for the seal.
#[derive(Debug)]
struct Pending {
    owner: usize,
    name: String,
    fields: Fields,
    callback: ModEventFn,
    user: usize,
}

/// A service a user may call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Found {
    /// The publishing mod's index.
    pub(crate) owner: usize,
    /// The table, as an address.
    pub(crate) table: usize,
    /// How many fields the published table has.
    pub(crate) fields: u32,
}

/// What mods published, defined and subscribed during init.
#[derive(Debug, Default)]
pub(crate) struct Registry {
    published: Vec<Published>,
    defined: Vec<Definition>,
    pending: Vec<Pending>,
}

/// One subscriber of a sealed event.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Subscriber {
    /// The subscribing mod's index.
    pub(crate) owner: usize,
    /// The handler, inside the mod's DLL, which stays loaded.
    pub(crate) callback: ModEventFn,
    /// The mod's pointer for the handler, as an address.
    pub(crate) user: usize,
}

/// A sealed event: its definer and its subscribers.
#[derive(Debug)]
pub(crate) struct Slot {
    /// The defining mod's index, the one mod that sends it.
    pub(crate) owner: usize,
    /// The event's name.
    pub(crate) name: String,
    /// Its subscribers, in load order.
    pub(crate) subscribers: Box<[Subscriber]>,
}

impl Registry {
    /// Publishes a service.
    ///
    /// # Errors
    ///
    /// Why it is refused: the name and major are published already.
    pub(crate) fn publish(
        &mut self,
        owner: usize,
        name: &str,
        major: u32,
        fields: Fields,
        table: usize,
        names: &impl Names,
    ) -> Result<(), String> {
        if let Some(held) = self
            .published
            .iter()
            .find(|p| p.name == name && p.major == major)
        {
            let (id, version) = names.name(held.owner);
            return Err(if held.owner == owner {
                format!("service {name} v{major}: published twice")
            } else {
                format!("service {name} v{major}: {id} {version} publishes it already")
            });
        }
        self.published.push(Published {
            owner,
            name: name.to_owned(),
            major,
            fields,
            table,
        });
        Ok(())
    }

    /// The service `name` of `major` whose table starts with `fields`.
    ///
    /// # Errors
    ///
    /// [`Status::MISSING`] when no running mod publishes it, and
    /// [`Status::REFUSED`] when the published table does not serve
    /// `fields`, each with the reason.
    pub(crate) fn find(
        &self,
        name: &str,
        major: u32,
        fields: &[(String, u64)],
        names: &impl Names,
    ) -> Result<Found, (Status, String)> {
        let same_name = || self.published.iter().filter(|p| p.name == name);
        let Some(published) = same_name().find(|p| p.major == major) else {
            let others: Vec<String> = same_name()
                .filter(|p| !names.off(p.owner))
                .map(|p| {
                    let (id, version) = names.name(p.owner);
                    format!("v{} from {id} {version}", p.major)
                })
                .collect();
            return Err(if others.is_empty() {
                (Status::MISSING, "no running mod publishes it".to_owned())
            } else {
                (
                    Status::REFUSED,
                    format!(
                        "only {} is published; this mod was built for v{major}",
                        others.join(", ")
                    ),
                )
            });
        };
        let (id, version) = names.name(published.owner);
        if names.off(published.owner) {
            return Err((
                Status::MISSING,
                format!("{id} {version} publishes it, but is off"),
            ));
        }
        compare(&view(fields), &view(&published.fields)).map_err(|mismatch| {
            let advice = match mismatch {
                LayoutError::Newer { .. } => format!("; update {id}"),
                _ => "; both mods must compile one declaration".to_owned(),
            };
            (
                Status::REFUSED,
                format!("{id} {version}: {mismatch}{advice}"),
            )
        })?;
        Ok(Found {
            owner: published.owner,
            table: published.table,
            fields: u32::try_from(published.fields.len()).unwrap_or(u32::MAX),
        })
    }

    /// Defines a mod event named after its definer `owner_id`, and returns
    /// its id.
    ///
    /// # Errors
    ///
    /// Why it is refused: a name outside the definer's id, or defined twice.
    pub(crate) fn define(
        &mut self,
        owner: usize,
        owner_id: &str,
        name: &str,
        fields: Fields,
    ) -> Result<u32, String> {
        if name
            .strip_prefix(owner_id)
            .and_then(|rest| rest.strip_prefix(':'))
            .is_none_or(str::is_empty)
        {
            return Err(format!(
                "event {name}: an event is named `{owner_id}:<what>`"
            ));
        }
        if self.defined.iter().any(|d| d.name == name) {
            return Err(format!("event {name}: defined twice"));
        }
        let id = u32::try_from(self.defined.len()).map_err(|_| "too many events".to_owned())?;
        self.defined.push(Definition {
            owner,
            name: name.to_owned(),
            fields,
        });
        Ok(id)
    }

    /// Keeps a subscription for the seal.
    pub(crate) fn subscribe(
        &mut self,
        owner: usize,
        name: &str,
        fields: Fields,
        callback: ModEventFn,
        user: usize,
    ) {
        self.pending.push(Pending {
            owner,
            name: name.to_owned(),
            fields,
            callback,
            user,
        });
    }

    /// Matches every subscription with its definition: the sealed events by
    /// id, and why each subscription left out stays idle, by subscriber.
    pub(crate) fn seal(&mut self, names: &impl Names) -> (Box<[Slot]>, Vec<(usize, String)>) {
        let mut slots: Vec<(usize, String, Vec<Subscriber>)> = self
            .defined
            .iter()
            .map(|d| (d.owner, d.name.clone(), Vec::new()))
            .collect();
        let mut idle = Vec::new();
        for pending in std::mem::take(&mut self.pending) {
            let Some((id, definition)) = self
                .defined
                .iter()
                .enumerate()
                .find(|(_, d)| d.name == pending.name)
            else {
                idle.push((
                    pending.owner,
                    format!("event {}: no running mod defines it", pending.name),
                ));
                continue;
            };
            let (owner_id, version) = names.name(definition.owner);
            if names.off(definition.owner) {
                idle.push((
                    pending.owner,
                    format!(
                        "event {}: {owner_id} {version} defines it, but is off",
                        pending.name
                    ),
                ));
                continue;
            }
            if let Err(mismatch) = compare(&view(&pending.fields), &view(&definition.fields)) {
                let mismatch = mismatch.to_string().replace("publisher", "definer");
                idle.push((
                    pending.owner,
                    format!("event {}: {owner_id} {version}: {mismatch}", pending.name),
                ));
                continue;
            }
            if let Some(slot) = slots.get_mut(id) {
                slot.2.push(Subscriber {
                    owner: pending.owner,
                    callback: pending.callback,
                    user: pending.user,
                });
            }
        }
        let slots = slots
            .into_iter()
            .map(|(owner, name, subscribers)| Slot {
                owner,
                name,
                subscribers: subscribers.into_boxed_slice(),
            })
            .collect();
        (slots, idle)
    }
}

/// A layout as borrowed `(name, hash)` pairs.
fn view(fields: &[(String, u64)]) -> Vec<(&str, u64)> {
    fields
        .iter()
        .map(|(name, hash)| (name.as_str(), *hash))
        .collect()
}

/// The registry while mods initialise.
static REGISTRY: Mutex<Registry> = Mutex::new(Registry {
    published: Vec::new(),
    defined: Vec::new(),
    pending: Vec::new(),
});
/// The events once every mod is initialised, by id, read without a lock.
static SEALED: OnceLock<Box<[Slot]>> = OnceLock::new();

/// The registry, for the entry points.
pub(crate) fn registry() -> std::sync::MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Whether the events are sealed: subscribing and defining are over.
pub(crate) fn sealed() -> bool {
    SEALED.get().is_some()
}

/// Seals the events and logs every subscription that stays idle.
pub(crate) fn seal(names: &impl Names) {
    let (slots, idle) = registry().seal(names);
    for (owner, reason) in idle {
        let (id, _) = names.name(owner);
        crate::log::write(enshwick::abi::Level::WARN, id, &reason);
    }
    let count = slots.len();
    if SEALED.set(slots).is_ok() && count > 0 {
        crate::log!(INFO, "{count} mod events defined");
    }
}

/// A line per published service and defined event, for the status summary.
pub(crate) fn lines(names: &impl Names) -> Vec<String> {
    let mut lines: Vec<String> = registry()
        .published
        .iter()
        .map(|p| {
            let (id, version) = names.name(p.owner);
            let off = if names.off(p.owner) { ", off" } else { "" };
            format!(
                "service {} v{} from {id} {version}, {} fields{off}",
                p.name,
                p.major,
                p.fields.len()
            )
        })
        .collect();
    for slot in SEALED.get().map_or(&[][..], |slots| slots) {
        let (id, version) = names.name(slot.owner);
        let subscribers: Vec<&str> = slot
            .subscribers
            .iter()
            .map(|s| names.name(s.owner).0)
            .collect();
        lines.push(format!(
            "event {} from {id} {version} to {}",
            slot.name,
            if subscribers.is_empty() {
                "no one".to_owned()
            } else {
                subscribers.join(", ")
            }
        ));
    }
    lines
}

/// Hands `data` to every live subscriber of the event `id`, which the mod
/// at `sender` defined.
pub(crate) fn send(sender: usize, id: u32, data: *const core::ffi::c_void) -> Status {
    let Some(slots) = SEALED.get() else {
        return Status::OK;
    };
    let Some(slot) = slots.get(id as usize) else {
        return Status::BAD_ARGS;
    };
    if slot.owner != sender {
        return Status::REFUSED;
    }
    for subscriber in &slot.subscribers {
        if crate::host::is_off(subscriber.owner) {
            continue;
        }
        // SAFETY: the handler lives in the mod's DLL, which stays loaded; the
        // seal checked the sender's layout starts with the subscriber's, and
        // the safe layer catches the handler's panics.
        crate::profile::call(
            subscriber.owner,
            crate::profile::Feature::ModEvents,
            || unsafe {
                (subscriber.callback)(
                    data,
                    core::ptr::with_exposed_provenance_mut(subscriber.user),
                );
            },
        );
    }
    Status::OK
}

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;

    use super::{Names, Registry};

    struct Mods(Vec<(&'static str, &'static str, bool)>);

    impl Names for Mods {
        fn name(&self, index: usize) -> (&str, &str) {
            self.0.get(index).map_or(("?", "?"), |m| (m.0, m.1))
        }
        fn off(&self, index: usize) -> bool {
            self.0.get(index).is_some_and(|m| m.2)
        }
    }

    fn fields(names: &[&str]) -> Vec<(String, u64)> {
        names
            .iter()
            .zip(1..)
            .map(|(n, h)| ((*n).to_owned(), h))
            .collect()
    }

    unsafe extern "system" fn idle(_: *const core::ffi::c_void, _: *mut core::ffi::c_void) {}

    fn mods() -> Mods {
        Mods(vec![
            ("trading-post", "0.3.0", false),
            ("market-stalls", "0.1.0", false),
        ])
    }

    #[test]
    fn an_older_user_gets_a_newer_table() {
        let mut registry = Registry::default();
        assert_eq!(
            registry.publish(
                0,
                "trading-post:prices",
                1,
                fields(&["a", "b"]),
                0x10,
                &mods()
            ),
            Ok(())
        );
        let found = registry.find("trading-post:prices", 1, &fields(&["a"]), &mods());
        assert_eq!(found.map(|f| (f.table, f.fields)), Ok((0x10, 2)));
    }

    #[test]
    fn a_newer_user_is_told_to_update_the_publisher() {
        let mut registry = Registry::default();
        assert!(
            registry
                .publish(0, "trading-post:prices", 1, fields(&["a"]), 0x10, &mods())
                .is_ok()
        );
        let Err((status, reason)) =
            registry.find("trading-post:prices", 1, &fields(&["a", "b"]), &mods())
        else {
            panic!("served");
        };
        assert_eq!(status, Status::REFUSED);
        assert!(reason.contains("trading-post 0.3.0"), "{reason}");
        assert!(reason.contains("`b`"), "{reason}");
        assert!(reason.ends_with("update trading-post"), "{reason}");
    }

    #[test]
    fn another_major_is_named() {
        let mut registry = Registry::default();
        assert!(
            registry
                .publish(0, "trading-post:prices", 2, fields(&["a"]), 0x10, &mods())
                .is_ok()
        );
        let Err((status, reason)) =
            registry.find("trading-post:prices", 1, &fields(&["a"]), &mods())
        else {
            panic!("served");
        };
        assert_eq!(status, Status::REFUSED);
        assert!(reason.contains("v2 from trading-post 0.3.0"), "{reason}");
    }

    #[test]
    fn nothing_published_is_missing() {
        let registry = Registry::default();
        let found = registry.find("trading-post:prices", 1, &fields(&["a"]), &mods());
        assert!(matches!(found, Err((Status::MISSING, _))));
    }

    #[test]
    fn an_off_publisher_serves_nothing() {
        let mut registry = Registry::default();
        assert!(
            registry
                .publish(0, "trading-post:prices", 1, fields(&["a"]), 0x10, &mods())
                .is_ok()
        );
        let mut off = mods();
        off.0[0].2 = true;
        let found = registry.find("trading-post:prices", 1, &fields(&["a"]), &off);
        assert!(matches!(found, Err((Status::MISSING, reason)) if reason.contains("is off")));
    }

    #[test]
    fn a_name_and_major_publish_once() {
        let mut registry = Registry::default();
        assert!(
            registry
                .publish(0, "trading-post:prices", 1, fields(&["a"]), 0x10, &mods())
                .is_ok()
        );
        assert!(
            registry
                .publish(1, "trading-post:prices", 1, fields(&["a"]), 0x20, &mods())
                .is_err()
        );
        assert!(
            registry
                .publish(0, "trading-post:prices", 2, fields(&["a"]), 0x20, &mods())
                .is_ok()
        );
    }

    #[test]
    fn events_are_named_after_their_definer() {
        let mut registry = Registry::default();
        assert_eq!(
            registry.define(0, "trading-post", "trading-post:sold", fields(&["a"])),
            Ok(0)
        );
        assert!(
            registry
                .define(0, "trading-post", "trading-postx:sold", fields(&["a"]))
                .is_err()
        );
        assert!(
            registry
                .define(0, "trading-post", "trading-post:", fields(&["a"]))
                .is_err()
        );
        assert!(
            registry
                .define(0, "trading-post", "trading-post:sold", fields(&["a"]))
                .is_err()
        );
    }

    #[test]
    fn the_seal_keeps_matching_subscribers_and_names_the_rest() {
        let mut registry = Registry::default();
        assert!(
            registry
                .define(0, "trading-post", "trading-post:sold", fields(&["a", "b"]))
                .is_ok()
        );
        registry.subscribe(1, "trading-post:sold", fields(&["a"]), idle, 1);
        registry.subscribe(1, "trading-post:sold", fields(&["a", "b", "c"]), idle, 2);
        registry.subscribe(1, "trading-post:bought", fields(&["a"]), idle, 3);
        let (slots, idle) = registry.seal(&mods());
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].subscribers.len(), 1);
        assert_eq!(slots[0].subscribers[0].user, 1);
        assert_eq!(idle.len(), 2);
        assert!(
            idle[0].1.contains("`c`") && idle[0].1.contains("definer"),
            "{}",
            idle[0].1
        );
        assert!(
            idle[1].1.contains("no running mod defines it"),
            "{}",
            idle[1].1
        );
    }
}
