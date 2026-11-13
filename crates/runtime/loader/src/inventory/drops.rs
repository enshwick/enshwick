//! Loose item stacks mods drop on the ground, on the server's game thread
//! between steps.
//!
//! A drop is the game's generic pickup template, spawned with the item's
//! `PickupItem` among its initial components, as the game spills loot (server
//! 26430253, `sys.random_pickup_loot_spawner`): the template carries the
//! model setup, the interaction offer any player takes it by, gravity and
//! `DoNotSave`, and `sys.pickup_item_setup` fills the rest from the item.
//!
//! The template is found by its name, which reads every template's name
//! from the archive, about a tenth of a second on a dedicated server: the
//! loader reads it on a thread of its own as the world loads, and only a
//! drop asked for before the read ends waits for the rest of it.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick_core::abi::{DropRequest, Event, Header, SpawnRequest, Status};
use enshwick_core::affixes::ItemRarity;

use crate::engine::loader::Loader;

/// The template the game spills loose loot with.
const TEMPLATE: &str = "Base_LootPickup_Generic_Gravity";
/// The component a pickup holds its item in.
const PICKUP: &str = "keen::ecs::PickupItem";

/// The pickup template and where `PickupItem` keeps its fields.
#[derive(Debug, Clone, Copy)]
struct Layout {
    template: [u8; 16],
    hash: u32,
    size: usize,
    item: usize,
    level: usize,
    rarity: usize,
    amount: usize,
}

/// The pickup's layout, read once for the process off the game thread; a
/// failure stays, as the reflection and the archive it comes from do.
#[derive(Debug)]
struct Reading {
    started: AtomicBool,
    layout: OnceLock<Option<Layout>>,
}

static READING: Reading = Reading::new();

impl Reading {
    const fn new() -> Self {
        Self {
            started: AtomicBool::new(false),
            layout: OnceLock::new(),
        }
    }

    /// Runs `read` on a thread of its own, unless a read started before.
    fn start(
        &'static self,
        read: impl FnOnce() -> Result<Layout, ReadLayoutError> + Send + 'static,
    ) {
        if self.started.load(Ordering::Acquire) || self.started.swap(true, Ordering::AcqRel) {
            return;
        }
        crate::engine::off_thread::read("item drops", read, |read, took| {
            self.finish(read, took);
        });
    }

    /// Starts `read` when `event` is the world's load.
    fn at(
        &'static self,
        event: Event,
        read: impl FnOnce() -> Result<Layout, ReadLayoutError> + Send + 'static,
    ) {
        if event == Event::WORLD_LOADED {
            self.start(read);
        }
    }

    /// Keeps `read` for the process, then logs it and turns drops off on a
    /// failure: kept first, so a drop waiting in [`Reading::layout`] wakes
    /// even if the log panics.
    fn finish(&self, read: Result<Layout, ReadLayoutError>, took: std::time::Duration) {
        let why = match read {
            Ok(layout) => {
                let _first = self.layout.set(Some(layout));
                None
            }
            Err(why) => {
                let _first = self.layout.set(None);
                Some(why)
            }
        };
        match why {
            None => crate::log!(
                INFO,
                "item drops ready: the pickup read in {} ms off the game thread",
                took.as_millis()
            ),
            Some(why) => {
                crate::log!(WARN, "item drops off: {why}");
                crate::engine::diag::part_off("item drops", &why.to_string());
            }
        }
    }

    /// The layout, waiting for a read [`Reading::start`] began, so a drop
    /// right after the world loads waits out the rest of the read instead
    /// of failing; [`Status::MISSING`] when the read failed.
    fn layout(&self) -> Result<&Layout, Status> {
        self.layout.wait().as_ref().ok_or(Status::MISSING)
    }
}

/// Starts reading the layout as a world loads, where a server runs.
pub(crate) fn world_event(event: Event) {
    if crate::engine::world::server().is_err() {
        return;
    }
    if let Some(loader) = crate::engine::loader::loader() {
        READING.at(event, move || read_layout(loader));
    }
}

fn read_layout(loader: &Loader) -> Result<Layout, ReadLayoutError> {
    let reflection = &loader.reflection;
    let info = reflection.type_info(PICKUP)?;
    // A field whose type the build widened or narrowed would take its
    // neighbour's bytes: each must hold what `pickup` writes into it.
    let field = |name: &'static str, width: u32| {
        let mut found = info.fields().iter().filter(|f| f.name() == name);
        let (Some(held), None) = (found.next(), found.next()) else {
            return Err(ReadLayoutError::Field(name));
        };
        let size = held
            .type_name()
            .and_then(|ty| reflection.type_info(ty).ok())
            .map(enshwick_gamedata::reflect::TypeInfo::size);
        let at = held.offset();
        if size != Some(width) || at + u64::from(width) > u64::from(info.size()) {
            return Err(ReadLayoutError::Width {
                field: name,
                at,
                size,
                width,
            });
        }
        usize::try_from(at).map_err(|_| ReadLayoutError::Field(name))
    };
    let template = match crate::engine::archive::vanilla(loader).map(|v| v.template(TEMPLATE)) {
        Ok(Ok(guid)) => guid.0,
        why => return Err(ReadLayoutError::Template(format!("{why:?}"))),
    };
    Ok(Layout {
        template,
        hash: crate::engine::world::type_hash(info.va()).ok_or(ReadLayoutError::Type)?,
        size: usize::try_from(info.size()).map_err(|_| ReadLayoutError::Type)?,
        item: field("itemId", 4)?,
        level: field("level", 4)?,
        rarity: field("rarity", 1)?,
        amount: field("pickupAmount", 2)?,
    })
}

/// Why the pickup's layout does not read on this build.
#[derive(Debug, thiserror::Error)]
enum ReadLayoutError {
    /// The reflection lacks the pickup type.
    #[error(transparent)]
    Reflection(#[from] enshwick_gamedata::reflect::LookupError),
    /// The pickup type has no hash or size the loader can use.
    #[error("{pickup} has no usable type hash or size", pickup = PICKUP)]
    Type,
    /// A field is missing or more than one carries its name.
    #[error("{pickup}.{0} missing or ambiguous", pickup = PICKUP)]
    Field(&'static str),
    /// A field's type is not as wide as what a drop writes into it.
    #[error("{pickup}.{field} at {at} holds {size:?} bytes, not {width}", pickup = PICKUP)]
    Width {
        field: &'static str,
        at: u64,
        size: Option<u32>,
        width: u32,
    },
    /// The template is not found by its name in the game's data.
    #[error("{template}: {0}", template = TEMPLATE)]
    Template(String),
    /// The read panicked.
    #[error("the read of the pickup panicked")]
    Panicked,
    /// No thread could be started for the read.
    #[error("no thread to read the pickup")]
    Thread,
}

impl crate::engine::off_thread::ReadFailure for ReadLayoutError {
    const PANICKED: Self = Self::Panicked;
    const THREAD: Self = Self::Thread;
}

/// `PickupItem`'s bytes for `request` on `layout`, `None` for a count or a
/// rarity the game does not hold.
fn pickup(layout: &Layout, request: &DropRequest) -> Option<Vec<u8>> {
    let amount = u16::try_from(request.count).ok().filter(|&n| n != 0)?;
    let rarity = u8::try_from(request.rarity)
        .ok()
        .filter(|&r| ItemRarity::from_raw(r).is_some())?;
    Some(crate::inventory::spawns::bytes(
        layout.size,
        &[
            (layout.item, &request.item.get().to_le_bytes()),
            (layout.level, &request.level.to_le_bytes()),
            (layout.rarity, &[rarity]),
            (layout.amount, &amount.to_le_bytes()),
        ],
    ))
}

/// Drops the stack `request` asks for; the pickup's entity.
pub(crate) fn drop_items(loader: &'static Loader, request: &DropRequest) -> Result<u32, Status> {
    crate::engine::world::server()?;
    // A drop before any world load reached the parts starts the read itself.
    READING.start(move || read_layout(loader));
    let layout = READING.layout()?;
    let bytes = pickup(layout, request).ok_or(Status::BAD_ARGS)?;
    crate::inventory::transactions::item_info(loader, request.item.get())
        .map_err(|_| Status::REFUSED)?;
    let spawn = SpawnRequest {
        header: Header::of::<SpawnRequest>(),
        template: layout.template,
        position: request.position,
        orientation: [0.0, 0.0, 0.0, 1.0],
        level: 0,
        reserved: 0,
        lifetime_ns: 0,
        flags: 0,
    };
    crate::inventory::spawns::spawn_with(loader, &spawn, vec![(layout.hash, bytes)], 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT: Layout = Layout {
        template: [1; 16],
        hash: 7,
        size: 28,
        item: 0,
        level: 20,
        rarity: 24,
        amount: 26,
    };

    fn request(count: u32, rarity: u32) -> DropRequest {
        DropRequest {
            header: Header::of::<DropRequest>(),
            item: enshwick_core::ItemId::new(0x2a),
            count,
            position: [0; 3],
            level: 9,
            rarity,
        }
    }

    #[test]
    fn a_pickup_holds_the_item_level_rarity_and_count_where_the_game_reads_them() {
        let bytes = pickup(&LAYOUT, &request(300, 2)).unwrap();
        assert_eq!(bytes.len(), 28);
        assert_eq!(bytes[0..4], 0x2a_u32.to_le_bytes());
        assert_eq!(bytes[4..20], [0; 16], "no item reference");
        assert_eq!(bytes[20..24], 9_u32.to_le_bytes());
        assert_eq!(bytes[24], 2);
        assert_eq!(bytes[26..28], 300_u16.to_le_bytes());
    }

    #[test]
    fn a_drop_without_a_server_world_hands_out_no_entity() {
        let loader: &'static Loader = Box::leak(Box::new(crate::start::tests::bare_loader()));
        assert!(drop_items(loader, &request(5, 0)).is_err());
    }

    /// A reading of its own, which the test's threads share.
    fn reading() -> &'static Reading {
        Box::leak(Box::new(Reading::new()))
    }

    /// Waits for `reading`'s thread to keep what it read.
    fn finished(reading: &Reading) {
        let began = std::time::Instant::now();
        while reading.layout.get().is_none() {
            assert!(began.elapsed().as_secs() < 10, "the read never finished");
            std::thread::yield_now();
        }
    }

    #[test]
    fn the_layout_is_read_once_and_a_failed_read_turns_drops_off_for_good() {
        let reading = reading();
        assert!(reading.layout.get().is_none(), "not started");
        let reads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        for _ in 0..3 {
            let reads = std::sync::Arc::clone(&reads);
            reading.start(move || {
                reads.fetch_add(1, Ordering::Relaxed);
                Err(ReadLayoutError::Type)
            });
        }
        finished(reading);
        assert_eq!(reads.load(Ordering::Relaxed), 1);
        assert_eq!(reading.layout().err(), Some(Status::MISSING));
        reading.start(|| Ok(LAYOUT));
        assert_eq!(
            reading.layout().err(),
            Some(Status::MISSING),
            "no second read"
        );
    }

    #[test]
    fn a_drop_during_the_read_waits_for_it_instead_of_failing() {
        let reading = reading();
        reading.start(|| {
            std::thread::sleep(std::time::Duration::from_millis(200));
            Ok(LAYOUT)
        });
        assert_eq!(reading.layout().map(|l| l.hash), Ok(LAYOUT.hash));
    }

    #[test]
    fn only_the_world_s_load_starts_the_read() {
        let reading = reading();
        for event in [Event::WORLD_SAVED, Event::SHUTDOWN_STARTED, Event::TICKED] {
            reading.at(event, || Ok(LAYOUT));
        }
        assert!(!reading.started.load(Ordering::Acquire));
        reading.at(Event::WORLD_LOADED, || Ok(LAYOUT));
        finished(reading);
        assert_eq!(reading.layout().map(|l| l.hash), Ok(LAYOUT.hash));
    }

    #[test]
    fn a_read_layout_answers_every_drop_and_a_panicking_read_turns_drops_off() {
        let read = reading();
        read.start(|| Ok(LAYOUT));
        finished(read);
        assert_eq!(read.layout().map(|l| l.hash), Ok(LAYOUT.hash));
        let panicked = reading();
        panicked.start(|| panic!("a read that panics"));
        finished(panicked);
        assert_eq!(panicked.layout().err(), Some(Status::MISSING));
    }

    #[test]
    fn a_count_of_none_or_past_a_pickup_and_an_unknown_rarity_are_refused() {
        assert!(pickup(&LAYOUT, &request(0, 0)).is_none());
        assert!(pickup(&LAYOUT, &request(65_536, 0)).is_none());
        assert!(pickup(&LAYOUT, &request(65_535, 0)).is_some());
        assert!(pickup(&LAYOUT, &request(1, 200)).is_none());
    }
}
