use enshwick_core::abi::Str;

use super::*;

#[test]
fn a_raise_walks_its_event_s_subscriptions_in_their_order() {
    unsafe extern "system" fn handler(
        _: Event,
        _: *const core::ffi::c_void,
        _: *mut core::ffi::c_void,
    ) {
    }
    let made = |owner, event| Subscription {
        owner,
        event,
        callback: handler,
        user: 0,
    };
    let mut sealed = vec![
        made(ModIndex::new(2), Event::TICKED),
        made(ModIndex::new(0), Event::WORLD_SAVED),
        made(ModIndex::new(1), Event::TICKED),
        made(ModIndex::new(0), Event::TICKED),
        made(ModIndex::new(1), Event::SETTINGS_CHANGED),
    ];
    sealed.sort_by_key(|s| s.event.0);
    let owners = |event| {
        of_event(&sealed, event)
            .iter()
            .map(|s| s.owner)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        owners(Event::TICKED),
        [ModIndex::new(2), ModIndex::new(1), ModIndex::new(0)]
    );
    assert_eq!(owners(Event::WORLD_SAVED), [ModIndex::new(0)]);
    assert_eq!(owners(Event::SETTINGS_CHANGED), [ModIndex::new(1)]);
    assert!(owners(Event::SHUTDOWN_STARTED).is_empty());
}

fn read(text: Str) -> Option<&'static str> {
    // SAFETY: the tests read texts of stores that live to their end.
    unsafe { text.as_str() }
}

#[test]
fn a_poisoned_lock_is_taken_over_with_its_state() {
    let mutex = std::sync::Arc::new(Mutex::new(vec![1]));
    let held = std::sync::Arc::clone(&mutex);
    let panicked = std::thread::spawn(move || {
        let mut guard = held.lock().unwrap();
        guard.push(2);
        panic!("poisons the lock");
    })
    .join();
    assert!(panicked.is_err() && mutex.is_poisoned());
    mutex.relock().push(3);
    assert!(!mutex.is_poisoned());
    assert_eq!(*mutex.relock(), [1, 2, 3]);
}

#[test]
fn a_kept_text_outlives_newer_ones_and_an_equal_text_is_kept_once() {
    let kept = Kept::default();
    let first = kept.keep("limit = 1");
    for n in 2..100 {
        kept.keep(&format!("limit = {n}"));
    }
    let again = kept.keep("limit = 1");
    assert_eq!(read(first), Some("limit = 1"));
    assert_eq!(read(again), read(first));
    assert_eq!(kept.0.lock().unwrap().len(), 99);
}

#[test]
fn texts_kept_on_other_threads_leave_earlier_ones_readable() {
    let kept = std::sync::Arc::new(Kept::default());
    let first = kept.keep("a = 0");
    let writers: Vec<_> = (0..4)
        .map(|t| {
            let kept = std::sync::Arc::clone(&kept);
            std::thread::spawn(move || {
                for n in 0..200 {
                    kept.keep(&format!("a = {t}{n}"));
                }
            })
        })
        .collect();
    for _ in 0..200 {
        assert_eq!(read(first), Some("a = 0"));
    }
    for writer in writers {
        writer.join().unwrap();
    }
    assert_eq!(read(first), Some("a = 0"));
}

/// The world events each follower of [`FOLLOWING`] heard, a bit per event.
static HEARD: AtomicU64 = AtomicU64::new(0);
/// Whether [`FOLLOWING`]'s part says it follows the world.
static FOLLOWS: AtomicBool = AtomicBool::new(false);

fn heard(_: &Loader, event: Event) {
    HEARD.fetch_or(1 << event.0, Ordering::Relaxed);
}

static FOLLOWING: &[&crate::engine::parts::Part] = &[
    &crate::engine::parts::Part {
        world_event: Some(crate::engine::parts::fused!(
            "test world panics",
            |_, event| assert!(event != Event::WORLD_LOADED, "a corrupt file")
        )),
        ..crate::engine::parts::part!("panicking")
    },
    &crate::engine::parts::Part {
        world_event: Some(crate::engine::parts::fused!(
            "test world follower",
            |loader, event| heard(loader, event)
        )),
        follows: Some(|_| FOLLOWS.load(Ordering::Relaxed)),
        ..crate::engine::parts::part!("following")
    },
];
static LAYERS: &[(&str, &[&crate::engine::parts::Part])] = &[("test", FOLLOWING)];

fn following_loader() -> Loader {
    let mut loader = crate::start::tests::bare_loader();
    loader.parts = crate::engine::parts::Parts::new(LAYERS);
    loader
}

#[test]
fn the_parts_that_follow_the_world_hear_its_three_events_though_one_before_them_panics() {
    let loader = following_loader();
    for event in [
        Event::WORLD_LOADED,
        Event::WORLD_SAVED,
        Event::SHUTDOWN_STARTED,
        Event::TICKED,
    ] {
        loader.emit(event, None, core::ptr::null());
    }
    loader.emit(Event::PLAYER_JOINED, None, core::ptr::null());
    loader.emit(Event::WORLD_SAVED, Some(0), core::ptr::null());
    let wanted = (1 << Event::WORLD_LOADED.0)
        | (1 << Event::WORLD_SAVED.0)
        | (1 << Event::SHUTDOWN_STARTED.0);
    assert_eq!(HEARD.load(Ordering::Relaxed), wanted);
}

#[test]
fn a_part_that_follows_the_world_has_its_three_events_linked_and_no_other() {
    let loader = following_loader();
    assert!(
        !loader.subscribed(Event::WORLD_LOADED),
        "it follows nothing yet"
    );
    FOLLOWS.store(true, Ordering::Relaxed);
    for event in [
        Event::WORLD_LOADED,
        Event::WORLD_SAVED,
        Event::SHUTDOWN_STARTED,
    ] {
        assert!(loader.subscribed(event), "{event:?}");
    }
    assert!(!loader.subscribed(Event::TICKED));
    assert!(!loader.subscribed(Event::PLAYER_LEFT));
}
