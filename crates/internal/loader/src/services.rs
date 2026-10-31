//! Services and events between mods. During init the loader keeps what mods
//! publish and define, and checks every user's layout against it field by
//! field; once every mod is initialised it seals the events, and a sent
//! event goes straight to its subscribers on the sending thread.

use std::sync::{Mutex, OnceLock};

use enshwick::abi::{ModEventFn, Status};
use enshwick::services::{LayoutError, compare};

use crate::locks::Relock;
use crate::mod_index::ModIndex;

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
    owner: ModIndex,
    name: String,
    major: u32,
    fields: Fields,
    table: usize,
}

/// A defined mod event.
#[derive(Debug)]
struct Definition {
    owner: ModIndex,
    name: String,
    fields: Fields,
    /// The data's size in bytes, which a subscriber reads at most.
    size: u32,
}

/// A subscription waiting for the seal.
#[derive(Debug)]
struct Pending {
    owner: ModIndex,
    name: String,
    fields: Fields,
    /// The bytes the subscriber reads.
    size: u32,
    callback: ModEventFn,
    user: usize,
}

/// A service a user may call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Found {
    /// The publishing mod's index.
    pub(crate) owner: ModIndex,
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
    pub(crate) owner: ModIndex,
    /// The handler, inside the mod's DLL, which stays loaded.
    pub(crate) callback: ModEventFn,
    /// The mod's pointer for the handler, as an address.
    pub(crate) user: usize,
}

/// A sealed event: its definer and its subscribers.
#[derive(Debug)]
pub(crate) struct Slot {
    /// The defining mod's index, the one mod that sends it.
    pub(crate) owner: ModIndex,
    /// The event's name.
    pub(crate) name: String,
    /// Its subscribers, in load order.
    pub(crate) subscribers: Box<[Subscriber]>,
}

/// Why the loader refuses a service a mod publishes or an event it defines.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum DeclareServiceError {
    /// The mod publishes the name and major already.
    #[error("service {name} v{major}: published twice")]
    PublishedTwice { name: String, major: u32 },
    /// Another mod publishes the name and major already.
    #[error("service {name} v{major}: {id} {version} publishes it already")]
    PublishedBy {
        name: String,
        major: u32,
        id: String,
        version: String,
    },
    /// The event's name lies outside the defining mod's namespace.
    #[error("event {name}: an event is named `{owner}:<what>`")]
    Namespace { name: String, owner: String },
    /// An event of the name is defined already.
    #[error("event {name}: defined twice")]
    DefinedTwice { name: String },
    /// More events than an id holds.
    #[error("too many events")]
    TooMany,
}

/// Why a mod's lookup of a service finds nothing it can use; the mod reads
/// the message beside [`FindServiceError::status`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum FindServiceError {
    /// No running mod publishes the name.
    #[error("no running mod publishes it")]
    Unpublished,
    /// Running mods publish the name only at other majors, listed as
    /// `v<major> from <id> <version>`.
    #[error("only {others} is published; this mod was built for v{major}")]
    OtherMajors { others: String, major: u32 },
    /// The publishing mod is off.
    #[error("{id} {version} publishes it, but is off")]
    PublisherOff { id: String, version: String },
    /// The published table does not serve the fields the mod asks for.
    #[error("{id} {version}: {mismatch}; {}", advice(mismatch, id))]
    Layout {
        id: String,
        version: String,
        mismatch: LayoutError,
    },
    /// The publishing mod does not export the service to the asking mod.
    #[error("{reason}")]
    Unexported { reason: String },
}

impl FindServiceError {
    /// The status the mod's call returns: missing while nothing usable
    /// runs, refused when what runs does not fit or is not exported.
    pub(crate) const fn status(&self) -> Status {
        match self {
            Self::Unpublished | Self::PublisherOff { .. } => Status::MISSING,
            Self::OtherMajors { .. } | Self::Layout { .. } | Self::Unexported { .. } => {
                Status::REFUSED
            }
        }
    }
}

/// What the mod's author does about `mismatch` with the publisher `id`.
fn advice(mismatch: &LayoutError, id: &str) -> String {
    match mismatch {
        LayoutError::Newer { .. } => format!("update {id}"),
        _ => "both mods must compile one declaration".to_owned(),
    }
}

impl Registry {
    /// Publishes a service.
    ///
    /// # Errors
    ///
    /// Why it is refused: the name and major are published already.
    pub(crate) fn publish(
        &mut self,
        owner: ModIndex,
        name: &str,
        major: u32,
        fields: Fields,
        table: usize,
        names: &impl Names,
    ) -> Result<(), DeclareServiceError> {
        if let Some(held) = self
            .published
            .iter()
            .find(|p| p.name == name && p.major == major)
        {
            let (id, version) = names.name(held.owner.get());
            let name = name.to_owned();
            return Err(if held.owner == owner {
                DeclareServiceError::PublishedTwice { name, major }
            } else {
                DeclareServiceError::PublishedBy {
                    name,
                    major,
                    id: id.to_owned(),
                    version: version.to_owned(),
                }
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
    /// A [`FindServiceError`] when no running mod publishes it, its
    /// publisher is off or the published table does not serve `fields`.
    pub(crate) fn find(
        &self,
        name: &str,
        major: u32,
        fields: &[(String, u64)],
        names: &impl Names,
    ) -> Result<Found, FindServiceError> {
        let same_name = || self.published.iter().filter(|p| p.name == name);
        let Some(published) = same_name().find(|p| p.major == major) else {
            let others: Vec<String> = same_name()
                .filter(|p| !names.off(p.owner.get()))
                .map(|p| {
                    let (id, version) = names.name(p.owner.get());
                    format!("v{} from {id} {version}", p.major)
                })
                .collect();
            return Err(if others.is_empty() {
                FindServiceError::Unpublished
            } else {
                FindServiceError::OtherMajors {
                    others: others.join(", "),
                    major,
                }
            });
        };
        let (id, version) = names.name(published.owner.get());
        if names.off(published.owner.get()) {
            return Err(FindServiceError::PublisherOff {
                id: id.to_owned(),
                version: version.to_owned(),
            });
        }
        compare(&view(fields), &view(&published.fields)).map_err(|mismatch| {
            FindServiceError::Layout {
                id: id.to_owned(),
                version: version.to_owned(),
                mismatch,
            }
        })?;
        Ok(Found {
            owner: published.owner,
            table: published.table,
            fields: u32::try_from(published.fields.len()).unwrap_or(u32::MAX),
        })
    }

    /// Defines a mod event named after its definer, the mod `owner_id`, and
    /// returns its id.
    ///
    /// # Errors
    ///
    /// Why it is refused: a name outside the definer's namespace, or defined
    /// twice.
    pub(crate) fn define(
        &mut self,
        owner: ModIndex,
        owner_id: &str,
        name: &str,
        fields: Fields,
        size: u32,
    ) -> Result<u32, DeclareServiceError> {
        if !enshwick::__private::rules::is_named_after(name, owner_id) {
            return Err(DeclareServiceError::Namespace {
                name: name.to_owned(),
                owner: owner_id.to_owned(),
            });
        }
        if self.defined.iter().any(|d| d.name == name) {
            return Err(DeclareServiceError::DefinedTwice {
                name: name.to_owned(),
            });
        }
        let id = u32::try_from(self.defined.len()).map_err(|_| DeclareServiceError::TooMany)?;
        self.defined.push(Definition {
            owner,
            name: name.to_owned(),
            fields,
            size,
        });
        Ok(id)
    }

    /// Keeps a subscription for the seal.
    pub(crate) fn subscribe(
        &mut self,
        owner: ModIndex,
        name: &str,
        fields: Fields,
        size: u32,
        callback: ModEventFn,
        user: usize,
    ) {
        self.pending.push(Pending {
            owner,
            name: name.to_owned(),
            fields,
            size,
            callback,
            user,
        });
    }

    /// Matches every subscription with its definition: the sealed events by
    /// id, and why each subscription left out stays idle, by subscriber.
    pub(crate) fn seal(&mut self, names: &impl Names) -> (Box<[Slot]>, Vec<(ModIndex, String)>) {
        let mut slots: Vec<(ModIndex, String, Vec<Subscriber>)> = self
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
            let (owner_id, version) = names.name(definition.owner.get());
            if names.off(definition.owner.get()) {
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
            if pending.size > definition.size {
                idle.push((
                    pending.owner,
                    format!(
                        "event {}: it reads {} bytes, {owner_id} {version} sends {}",
                        pending.name, pending.size, definition.size
                    ),
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
    REGISTRY.relock()
}

/// Whether the events are sealed: subscribing and defining are over.
pub(crate) fn sealed() -> bool {
    SEALED.get().is_some()
}

/// Seals the events and logs every subscription that stays idle.
pub(crate) fn seal(names: &impl Names) {
    let (slots, idle) = registry().seal(names);
    for (owner, reason) in idle {
        let (id, _) = names.name(owner.get());
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
            let (id, version) = names.name(p.owner.get());
            let off = if names.off(p.owner.get()) {
                ", off"
            } else {
                ""
            };
            format!(
                "service {} v{} from {id} {version}, {} fields{off}",
                p.name,
                p.major,
                p.fields.len()
            )
        })
        .collect();
    for slot in SEALED.get().map_or(&[][..], |slots| slots) {
        let (id, version) = names.name(slot.owner.get());
        let subscribers: Vec<&str> = slot
            .subscribers
            .iter()
            .map(|s| names.name(s.owner.get()).0)
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
    if slot.owner != ModIndex::new(sender) {
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
    use crate::mod_index::ModIndex;

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
                ModIndex::new(0),
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
                .publish(
                    ModIndex::new(0),
                    "trading-post:prices",
                    1,
                    fields(&["a"]),
                    0x10,
                    &mods()
                )
                .is_ok()
        );
        let Err(refused) = registry.find("trading-post:prices", 1, &fields(&["a", "b"]), &mods())
        else {
            panic!("served");
        };
        assert_eq!(refused.status(), Status::REFUSED);
        let reason = refused.to_string();
        assert!(reason.contains("trading-post 0.3.0"), "{reason}");
        assert!(reason.contains("`b`"), "{reason}");
        assert!(reason.ends_with("update trading-post"), "{reason}");
    }

    #[test]
    fn another_major_is_named() {
        let mut registry = Registry::default();
        assert!(
            registry
                .publish(
                    ModIndex::new(0),
                    "trading-post:prices",
                    2,
                    fields(&["a"]),
                    0x10,
                    &mods()
                )
                .is_ok()
        );
        let Err(refused) = registry.find("trading-post:prices", 1, &fields(&["a"]), &mods()) else {
            panic!("served");
        };
        assert_eq!(refused.status(), Status::REFUSED);
        let reason = refused.to_string();
        assert!(reason.contains("v2 from trading-post 0.3.0"), "{reason}");
    }

    #[test]
    fn nothing_published_is_missing() {
        let registry = Registry::default();
        let found = registry.find("trading-post:prices", 1, &fields(&["a"]), &mods());
        assert_eq!(found.map(|_| ()), Err(super::FindServiceError::Unpublished));
    }

    #[test]
    fn an_off_publisher_serves_nothing() {
        let mut registry = Registry::default();
        assert!(
            registry
                .publish(
                    ModIndex::new(0),
                    "trading-post:prices",
                    1,
                    fields(&["a"]),
                    0x10,
                    &mods()
                )
                .is_ok()
        );
        let mut off = mods();
        off.0[0].2 = true;
        let found = registry.find("trading-post:prices", 1, &fields(&["a"]), &off);
        assert!(
            matches!(&found, Err(refused) if refused.status() == Status::MISSING && refused.to_string().contains("is off"))
        );
    }

    #[test]
    fn a_name_and_major_publish_once() {
        let mut registry = Registry::default();
        assert!(
            registry
                .publish(
                    ModIndex::new(0),
                    "trading-post:prices",
                    1,
                    fields(&["a"]),
                    0x10,
                    &mods()
                )
                .is_ok()
        );
        assert!(
            registry
                .publish(
                    ModIndex::new(1),
                    "trading-post:prices",
                    1,
                    fields(&["a"]),
                    0x20,
                    &mods()
                )
                .is_err()
        );
        assert!(
            registry
                .publish(
                    ModIndex::new(0),
                    "trading-post:prices",
                    2,
                    fields(&["a"]),
                    0x20,
                    &mods()
                )
                .is_ok()
        );
    }

    #[test]
    fn events_are_named_after_their_definer() {
        let mut registry = Registry::default();
        assert_eq!(
            registry.define(
                ModIndex::new(0),
                "trading-post",
                "trading-post:sold",
                fields(&["a"]),
                8,
            ),
            Ok(0)
        );
        assert!(
            registry
                .define(
                    ModIndex::new(0),
                    "trading-post",
                    "trading-postx:sold",
                    fields(&["a"]),
                    8,
                )
                .is_err()
        );
        assert!(
            registry
                .define(
                    ModIndex::new(0),
                    "trading-post",
                    "trading-post:",
                    fields(&["a"]),
                    8,
                )
                .is_err()
        );
        assert!(
            registry
                .define(
                    ModIndex::new(0),
                    "trading-post",
                    "trading-post:sold",
                    fields(&["a"]),
                    8,
                )
                .is_err()
        );
    }

    #[test]
    fn the_seal_keeps_matching_subscribers_and_names_the_rest() {
        let mut registry = Registry::default();
        assert!(
            registry
                .define(
                    ModIndex::new(0),
                    "trading-post",
                    "trading-post:sold",
                    fields(&["a", "b"]),
                    8,
                )
                .is_ok()
        );
        registry.subscribe(
            ModIndex::new(1),
            "trading-post:sold",
            fields(&["a"]),
            8,
            idle,
            1,
        );
        registry.subscribe(
            ModIndex::new(1),
            "trading-post:sold",
            fields(&["a", "b", "c"]),
            8,
            idle,
            2,
        );
        registry.subscribe(
            ModIndex::new(1),
            "trading-post:bought",
            fields(&["a"]),
            8,
            idle,
            3,
        );
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

    #[test]
    fn a_subscriber_reading_past_the_definer_s_data_stays_idle() {
        let mut registry = Registry::default();
        registry
            .define(
                ModIndex::new(0),
                "trading-post",
                "trading-post:sold",
                fields(&["a"]),
                4,
            )
            .unwrap();
        registry.subscribe(
            ModIndex::new(1),
            "trading-post:sold",
            fields(&["a"]),
            16,
            idle,
            1,
        );
        let (slots, idle) = registry.seal(&mods());
        assert!(slots[0].subscribers.is_empty());
        assert!(idle[0].1.contains("reads 16 bytes"), "{}", idle[0].1);
    }
}
