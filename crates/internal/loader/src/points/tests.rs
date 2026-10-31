use std::sync::atomic::{AtomicU32, Ordering};

use enshwick::abi::{Level, PointMode, PointVerdict};

use crate::mod_index::ModIndex;
use crate::services::Names;

use super::{Declared, Mods, Pending, Registry, Sealed};

struct Loaded(Vec<(&'static str, &'static str, bool)>);

impl Names for Loaded {
    fn name(&self, index: usize) -> (&str, &str) {
        self.0.get(index).map_or(("?", "?"), |m| (m.0, m.1))
    }
    fn off(&self, index: usize) -> bool {
        self.0.get(index).is_some_and(|m| m.2)
    }
}

impl Mods for Loaded {
    fn find(&self, id: &str) -> Option<usize> {
        self.0.iter().position(|m| m.0 == id)
    }
}

fn loaded() -> Loaded {
    Loaded(vec![
        ("you.trading-post", "0.3.0", false),
        ("you.haggling", "0.1.0", false),
        ("you.discounts", "0.1.0", false),
        ("you.lockers", "0.1.0", false),
    ])
}

fn fields(names: &[&str]) -> Vec<(String, u64)> {
    names
        .iter()
        .zip(1..)
        .map(|(n, h)| ((*n).to_owned(), h))
        .collect()
}

fn access() -> Declared {
    Declared {
        name: "you.trading-post:price".to_owned(),
        input: fields(&["player", "owner"]),
        output: fields(&["output"]),
        mode: PointMode::OVERRIDE,
        refuse: true,
        input_size: 0,
        output_size: 0,
    }
}

/// The link answers what `user` encodes: the verdict in the low byte,
/// the value it sets above it.
unsafe extern "system" fn scripted(
    _input: *const core::ffi::c_void,
    value: *mut core::ffi::c_void,
    user: *mut core::ffi::c_void,
) -> PointVerdict {
    let script = user.addr();
    let verdict = PointVerdict(u32::try_from(script & 0xff).unwrap_or(0));
    if verdict == PointVerdict::SET {
        // SAFETY: the tests pass a `u32` value.
        unsafe { *value.cast::<u32>() = u32::try_from(script >> 8).unwrap_or(0) };
    }
    verdict
}

fn link(owner: ModIndex, script: usize) -> Pending {
    Pending {
        owner,
        declared: access(),
        priority: 0,
        before: Vec::new(),
        after: Vec::new(),
        callback: scripted,
        user: script,
    }
}

fn set(value: usize) -> usize {
    (value << 8) | PointVerdict::SET.0 as usize
}

fn seal(registry: &mut Registry) -> (Box<[Sealed]>, Vec<super::Inert>) {
    let (points, inert, cycles) = registry.seal(&loaded(), |_, _| false);
    assert!(cycles.is_empty(), "{cycles:?}");
    (points, inert)
}

fn ask(point: &Sealed, value: u32) -> (PointVerdict, u32) {
    let mut value = value;
    let verdict = point.run(
        core::ptr::null(),
        (&raw mut value).cast(),
        |_| false,
        |_, link| link(),
        |_, _| {},
    );
    (verdict, value)
}

fn defined(registry: &mut Registry) -> &'static AtomicU32 {
    registry
        .define(ModIndex::new(0), "you.trading-post", access())
        .unwrap()
        .1
}

/// A mod that defines points, and a mod that links them.
struct Traders;

impl Names for Traders {
    fn name(&self, index: usize) -> (&str, &str) {
        [("you.trading-post", "0.4.0"), ("you.haggling", "0.1.0")]
            .get(index)
            .copied()
            .unwrap_or(("?", "?"))
    }
    fn off(&self, _index: usize) -> bool {
        false
    }
}

impl Mods for Traders {
    fn find(&self, id: &str) -> Option<usize> {
        match id {
            "you.trading-post" => Some(0),
            "you.haggling" => Some(1),
            _ => None,
        }
    }
}

#[test]
fn a_link_reading_past_the_owner_s_input_or_answer_is_refused() {
    let sized = |input_size, output_size| Declared {
        input_size,
        output_size,
        ..access()
    };
    for (input, output) in [(24, 4), (16, 8)] {
        let mut registry = Registry::default();
        registry
            .define(ModIndex::new(0), "you.trading-post", sized(16, 4))
            .unwrap();
        let mut wide = link(ModIndex::new(1), set(7));
        wide.declared = sized(input, output);
        registry.attach(wide).unwrap();
        let (points, inert, _) = registry.seal(&Traders, |_, _| false);
        assert!(points[0].links.is_empty(), "{input} {output}");
        assert_eq!(inert.len(), 1);
    }
}

#[test]
fn a_link_is_checked_against_the_points_defined_so_far() {
    let mut registry = Registry::default();
    let mods = loaded();
    let absent = registry.unlinkable(&access(), &mods).unwrap_or_default();
    assert!(absent.contains("defines no such point"), "{absent}");
    let _live = defined(&mut registry);
    assert_eq!(registry.unlinkable(&access(), &mods), None);
    let advised = Declared {
        mode: PointMode::ADVISE,
        ..access()
    };
    let differs = registry.unlinkable(&advised, &mods).unwrap_or_default();
    assert!(
        differs.contains("must compile one declaration"),
        "{differs}"
    );
}

#[test]
fn points_are_named_after_their_owner_once() {
    let mut registry = Registry::default();
    assert!(
        registry
            .define(ModIndex::new(0), "you.trading-post", access())
            .is_ok()
    );
    assert!(
        registry
            .define(ModIndex::new(0), "you.trading-post", access())
            .is_err(),
        "twice"
    );
    let foreign = Declared {
        name: "you.lanterns:lit".to_owned(),
        ..access()
    };
    assert!(
        registry
            .define(ModIndex::new(0), "you.trading-post", foreign)
            .is_err()
    );
    let empty = Declared {
        name: "you.trading-post:".to_owned(),
        ..access()
    };
    assert!(
        registry
            .define(ModIndex::new(0), "you.trading-post", empty)
            .is_err()
    );
    assert!(registry.attach(link(ModIndex::new(1), 0)).is_ok());
    assert!(
        registry.attach(link(ModIndex::new(1), 0)).is_err(),
        "a mod links once"
    );
}

#[test]
fn links_run_in_their_asked_order_and_the_last_change_wins() {
    let mut registry = Registry::default();
    let live = defined(&mut registry);
    let mut first = link(ModIndex::new(3), set(30));
    first.before = vec!["you.haggling".to_owned()];
    let mut last = link(ModIndex::new(2), set(20));
    last.after = vec!["you.haggling".to_owned()];
    last.priority = -5;
    assert!(registry.attach(last).is_ok());
    assert!(registry.attach(link(ModIndex::new(1), set(10))).is_ok());
    assert!(registry.attach(first).is_ok());
    let (points, inert) = seal(&mut registry);
    assert!(inert.is_empty(), "{inert:?}");
    let order: Vec<usize> = points[0].links.iter().map(|l| l.owner.get()).collect();
    assert_eq!(order, [3, 1, 2]);
    points[0].arm(|_| false);
    assert_eq!(live.load(Ordering::Relaxed), 3);
    assert_eq!(ask(&points[0], 1), (PointVerdict::SET, 20));
    assert_eq!(points[0].links[2].undid.load(Ordering::Relaxed), 1);
    assert_eq!(points[0].links[2].undid_whom.load(Ordering::Relaxed), 1);
}

#[test]
fn priority_then_id_order_free_links() {
    let mut registry = Registry::default();
    defined(&mut registry);
    let mut early = link(ModIndex::new(3), 0);
    early.priority = -1;
    assert!(registry.attach(link(ModIndex::new(2), 0)).is_ok());
    assert!(registry.attach(link(ModIndex::new(1), 0)).is_ok());
    assert!(registry.attach(early).is_ok());
    let (points, _) = seal(&mut registry);
    let order: Vec<usize> = points[0].links.iter().map(|l| l.owner.get()).collect();
    // you.lockers first by priority, then you.discounts < you.haggling by id.
    assert_eq!(order, [3, 2, 1]);
}

#[test]
fn a_refusal_stops_the_chain() {
    let mut registry = Registry::default();
    defined(&mut registry);
    let mut refuses = link(ModIndex::new(1), PointVerdict::REFUSE.0 as usize);
    refuses.priority = -1;
    assert!(registry.attach(refuses).is_ok());
    assert!(registry.attach(link(ModIndex::new(2), set(9))).is_ok());
    let (points, _) = seal(&mut registry);
    assert_eq!(ask(&points[0], 1), (PointVerdict::REFUSE, 1));
    assert_eq!(points[0].links[1].changes.load(Ordering::Relaxed), 0);
}

#[test]
fn a_refusal_where_the_point_allows_none_is_a_pass() {
    let mut registry = Registry::default();
    let plain = Declared {
        refuse: false,
        ..access()
    };
    assert!(
        registry
            .define(ModIndex::new(0), "you.trading-post", plain.clone())
            .is_ok()
    );
    let mut refuses = link(ModIndex::new(1), PointVerdict::REFUSE.0 as usize);
    refuses.declared = plain.clone();
    let mut sets = link(ModIndex::new(2), set(4));
    sets.declared = plain;
    assert!(registry.attach(refuses).is_ok());
    assert!(registry.attach(sets).is_ok());
    let (points, _) = seal(&mut registry);
    assert_eq!(ask(&points[0], 1), (PointVerdict::SET, 4));
}

#[test]
fn an_observed_point_keeps_its_answer() {
    let mut registry = Registry::default();
    let observed = Declared {
        mode: PointMode::OBSERVE,
        refuse: false,
        ..access()
    };
    assert!(
        registry
            .define(ModIndex::new(0), "you.trading-post", observed.clone())
            .is_ok()
    );
    let mut watcher = link(ModIndex::new(1), PointVerdict::REFUSE.0 as usize);
    watcher.declared = observed;
    assert!(registry.attach(watcher).is_ok());
    let (points, _) = seal(&mut registry);
    assert_eq!(ask(&points[0], 7), (PointVerdict::PASS, 7));
}

#[test]
fn a_panicking_link_turns_off_and_the_point_runs_without_it() {
    let mut registry = Registry::default();
    let live = defined(&mut registry);
    let mut panics = link(ModIndex::new(1), PointVerdict::PANICKED.0 as usize);
    panics.priority = -1;
    assert!(registry.attach(panics).is_ok());
    assert!(registry.attach(link(ModIndex::new(2), set(5))).is_ok());
    let (points, _) = seal(&mut registry);
    points[0].arm(|_| false);
    let told = std::cell::Cell::new(0);
    let mut value = 1u32;
    let verdict = points[0].run(
        core::ptr::null(),
        (&raw mut value).cast(),
        |_| false,
        |_, link| link(),
        |_, link| told.set(told.get() + link.owner.get()),
    );
    assert_eq!((verdict, value), (PointVerdict::SET, 5));
    assert_eq!(told.get(), 1, "told once, of you.haggling");
    assert!(!points[0].links[0].is_on());
    assert_eq!(live.load(Ordering::Relaxed), 1);
    assert_eq!(ask(&points[0], 1), (PointVerdict::SET, 5));
    assert_eq!(told.get(), 1, "an off link is not asked again");
}

#[test]
fn a_linking_mod_that_turns_off_leaves_the_count() {
    let mut registry = Registry::default();
    let live = defined(&mut registry);
    assert!(registry.attach(link(ModIndex::new(1), set(5))).is_ok());
    assert!(registry.attach(link(ModIndex::new(2), set(6))).is_ok());
    let (points, _) = seal(&mut registry);
    points[0].arm(|_| false);
    assert_eq!(live.load(Ordering::Relaxed), 2);
    points[0].arm(|owner| owner == ModIndex::new(2));
    assert_eq!(live.load(Ordering::Relaxed), 1);
    points[0].arm(|owner| owner != ModIndex::new(0));
    assert_eq!(
        live.load(Ordering::Relaxed),
        0,
        "no link runs, the owner reads 0"
    );
}

#[test]
fn a_link_the_list_turns_off_stays_inert() {
    let mut registry = Registry::default();
    let live = defined(&mut registry);
    assert!(registry.attach(link(ModIndex::new(1), set(5))).is_ok());
    assert!(registry.attach(link(ModIndex::new(2), set(6))).is_ok());
    let (points, inert, _) = registry.seal(&loaded(), |id, point| {
        id == "you.haggling" && point == "you.trading-post:price"
    });
    points[0].arm(|_| false);
    assert_eq!(live.load(Ordering::Relaxed), 1);
    assert_eq!(points[0].links.len(), 1);
    assert_eq!(inert.len(), 1);
    assert_eq!(inert[0].owner, ModIndex::new(1));
    assert!(inert[0].reason.contains("mods.toml"), "{}", inert[0].reason);
    assert_eq!(ask(&points[0], 1), (PointVerdict::SET, 6));
}

#[test]
fn a_layout_mismatch_leaves_the_link_inert_with_the_field() {
    let mut registry = Registry::default();
    defined(&mut registry);
    let mut newer = link(ModIndex::new(1), 0);
    newer.declared.input = fields(&["player", "owner", "stall"]);
    let mut changed = link(ModIndex::new(2), 0);
    changed.declared.input = vec![("player".to_owned(), 1), ("owner".to_owned(), 99)];
    let mut other_answer = link(ModIndex::new(3), 0);
    other_answer.declared.output = vec![("output".to_owned(), 7)];
    assert!(registry.attach(newer).is_ok());
    assert!(registry.attach(changed).is_ok());
    assert!(registry.attach(other_answer).is_ok());
    let (points, inert) = seal(&mut registry);
    assert!(points[0].links.is_empty());
    let reasons: Vec<&str> = inert.iter().map(|n| n.reason.as_str()).collect();
    assert!(
        reasons[0].contains("`stall`") && reasons[0].ends_with("update you.trading-post"),
        "{}",
        reasons[0]
    );
    assert!(reasons[1].contains("field 1"), "{}", reasons[1]);
    assert!(reasons[2].contains("answer"), "{}", reasons[2]);
    assert!(inert.iter().all(|n| n.level == Level::WARN));
}

#[test]
fn another_mode_leaves_the_link_inert() {
    let mut registry = Registry::default();
    defined(&mut registry);
    let mut advises = link(ModIndex::new(1), 0);
    advises.declared.mode = PointMode::ADVISE;
    assert!(registry.attach(advises).is_ok());
    let (points, inert) = seal(&mut registry);
    assert!(points[0].links.is_empty());
    assert!(
        inert[0].reason.contains("advised, refusable"),
        "{}",
        inert[0].reason
    );
}

#[test]
fn a_missing_owner_leaves_the_link_inert_without_a_warning() {
    let mut registry = Registry::default();
    let mut gone = link(ModIndex::new(1), 0);
    gone.declared.name = "you.lanterns:lit".to_owned();
    let mut old = link(ModIndex::new(2), 0);
    old.declared.name = "you.trading-post:open".to_owned();
    assert!(registry.attach(gone).is_ok());
    assert!(registry.attach(old).is_ok());
    let (points, inert) = seal(&mut registry);
    assert!(points.is_empty());
    assert_eq!(inert[0].reason, "you.lanterns does not run");
    assert_eq!(inert[0].level, Level::INFO);
    assert!(inert[1].reason.contains("defines no such point"));
    assert_eq!(inert[1].level, Level::WARN);
    let mut off = loaded();
    off.0[0].2 = true;
    let mut registry = Registry::default();
    assert!(
        registry
            .define(ModIndex::new(0), "you.trading-post", access())
            .is_ok()
    );
    assert!(registry.attach(link(ModIndex::new(1), 0)).is_ok());
    let (points, inert, _) = registry.seal(&off, |_, _| false);
    assert!(points[0].links.is_empty());
    assert_eq!(inert[0].reason, "you.trading-post 0.3.0 is off");
}

#[test]
fn the_report_names_mods_changing_one_point_and_who_undid_whom() {
    let mut registry = Registry::default();
    defined(&mut registry);
    assert!(registry.attach(link(ModIndex::new(2), set(1))).is_ok());
    assert!(registry.attach(link(ModIndex::new(1), set(0))).is_ok());
    let mut gone = link(ModIndex::new(3), 0);
    gone.declared.name = "you.lanterns:lit".to_owned();
    assert!(registry.attach(gone).is_ok());
    let (points, inert) = seal(&mut registry);
    let _ = ask(&points[0], 1);
    assert_eq!(
        super::conflict_lines(&points, &loaded()),
        [
            "point you.trading-post:price: you.discounts then you.haggling, each may change the answer; you.haggling changed the answer you.discounts had set; times: 1"
        ]
    );
    let (defines, extends) = super::screen_lines(&points, &inert, 0, &loaded());
    assert_eq!(
        defines,
        ["you.trading-post:price (you.discounts, you.haggling)"]
    );
    assert!(extends.is_empty());
    let (_, extends) = super::screen_lines(&points, &inert, 3, &loaded());
    assert_eq!(extends, ["you.lanterns:lit (off)"]);
    let mut off = loaded();
    off.0[1].2 = true;
    assert!(super::conflict_lines(&points, &off).is_empty());
    let (defines, _) = super::screen_lines(&points, &inert, 0, &off);
    assert_eq!(defines, ["you.trading-post:price (you.discounts)"]);
    let (_, extends) = super::screen_lines(&points, &inert, 1, &off);
    assert_eq!(extends, ["you.trading-post:price (off)"]);
}

#[test]
fn a_links_off_entry_that_matches_no_link_is_named() {
    let mut registry = Registry::default();
    defined(&mut registry);
    assert!(registry.attach(link(ModIndex::new(1), 0)).is_ok());
    let (_, inert, _) = registry.seal(&loaded(), |id, point| {
        id == "you.haggling" && point == "you.trading-post:price"
    });
    let entries = [
        ("you.haggling", "you.trading-post:price"),
        ("you.haggling", "you.trading-post:gate"),
        ("you.trading-post", "you.trading-post:price"),
        ("not-installed", "you.trading-post:price"),
    ]
    .map(|(id, point)| (id.to_owned(), point.to_owned()));
    let unmatched: Vec<_> = super::unmatched_links_off(&entries, &inert, &loaded()).collect();
    assert_eq!(
        unmatched,
        [
            ("you.haggling", "you.trading-post:gate"),
            ("you.trading-post", "you.trading-post:price")
        ]
    );
}

#[test]
fn a_cycle_is_broken_by_priority_and_named() {
    let mut registry = Registry::default();
    defined(&mut registry);
    let mut a = link(ModIndex::new(1), 0);
    a.after = vec!["you.discounts".to_owned()];
    let mut b = link(ModIndex::new(2), 0);
    b.after = vec!["you.haggling".to_owned()];
    assert!(registry.attach(a).is_ok());
    assert!(registry.attach(b).is_ok());
    let (points, inert, cycles) = registry.seal(&loaded(), |_, _| false);
    assert_eq!(points[0].links.len(), 2);
    assert!(inert.is_empty());
    assert_eq!(cycles.len(), 1);
    assert!(cycles[0].reason.contains("cycle"));
}
