use super::join::{SHOWN_VALUES, shown};
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
        generation: 0,
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
fn a_server_s_join_values_stay_on_one_log_line() {
    assert_eq!(shown("{ a = \"Сейф\" }"), "{ a = \"Сейф\" }");
    assert_eq!(
        shown("{ a = 1 }\n12:00:00.000Z ERROR [enshwick] forged"),
        "{ a = 1 }\\n12:00:00.000Z ERROR [enshwick] forged"
    );
    let long = shown(&"x".repeat(SHOWN_VALUES * 4));
    assert_eq!(long.len(), SHOWN_VALUES + 3);
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
