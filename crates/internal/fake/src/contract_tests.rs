//! The fake host's half of its contract with the loader: each entry takes
//! the loader's caller kind and answers a call with zeroed arguments as the
//! loader does under each precondition, by the table the loader's own
//! `host::contract_tests` write from the bare loader.

use std::cell::Cell;
use std::path::Path;

use enshwick_abi::contract::{
    self, CallerKind, ENTRIES, Entry, ME, PACK, PRECONDITIONS, Precondition, UNKNOWN,
};
use enshwick_core::abi::{HostApi, ModHandle, Side, Status};

use crate::test::table::base::caller::Callers;

/// [`CALLERS`], every entry's caller kind by name, in field order.
macro_rules! callers {
    ($($field:ident)*) => {
        const CALLERS: &[(&str, CallerKind)] = &[$((stringify!($field), Callers::$field),)*];
    };
}

enshwick_abi::__host_entries!(callers);

/// The loader's table, from this crate's folder.
const TABLE: &str = "contract/loader.snap";

/// A difference the hosts keep on purpose: the entry, the precondition,
/// the loader's answer, the fake's, and why.
type Kept = (&'static str, Precondition, Status, Status, &'static str);

/// The differences the hosts keep on purpose besides [`no_game`].
const KEPT: &[Kept] = &[(
    "item_data_get",
    Precondition::AfterInit,
    Status::MISSING,
    Status::BAD_ARGS,
    "the loader's kinds are the process's: an earlier call of its test defined one, \
     where each fake starts empty",
)];

/// Whether the loader's answer is that it runs no game: the bare loader of
/// its test has none, so it stops where the fake's game goes on, and
/// nothing off Windows can say what a game would answer there. Many
/// entries read their arguments only past the game check, so a refusal of
/// the caller cannot be told apart there either: the caller kinds are
/// compared on their own for that.
fn no_game(loader: Status) -> bool {
    loader == Status::UNAVAILABLE
}

thread_local! {
    /// The entry [`probe`] calls from the mod's init, the handle it calls
    /// by, and its answer.
    static PROBED: Cell<Option<(Entry, ModHandle)>> = const { Cell::new(None) };
    static ANSWER: Cell<Status> = const { Cell::new(Status::OK) };
}

/// An init that calls the entry [`PROBED`] holds and keeps its answer.
///
/// # Safety
///
/// `api` points at a `HostApi` that lives for the call, as the fake's
/// static table does.
unsafe extern "system" fn probe(api: *const HostApi, _me: ModHandle) -> Status {
    if let Some((entry, by)) = PROBED.take() {
        // SAFETY: the caller promises `api` points at a live table.
        ANSWER.set(entry(unsafe { &*api }, by));
    }
    Status::OK
}

/// An init or ready that agrees.
///
/// # Safety
///
/// None: it reads neither argument.
unsafe extern "system" fn agree(_api: *const HostApi, _me: ModHandle) -> Status {
    Status::OK
}

/// An init that fails, so the fake turns the mod off.
///
/// # Safety
///
/// None: it reads neither argument.
unsafe extern "system" fn refuse(_api: *const HostApi, _me: ModHandle) -> Status {
    Status::REFUSED
}

/// The answer of `entry` called by `by` while the fake runs the mod's init.
fn in_init(fake: &crate::test::Installed, entry: Entry, by: ModHandle) -> Status {
    PROBED.set(Some((entry, by)));
    ANSWER.set(Status::OK);
    // SAFETY: `probe` takes the table and handle the fake passes.
    let _ran = unsafe { fake.init(probe) };
    assert!(PROBED.take().is_none(), "the fake ran no init");
    ANSWER.get()
}

/// The fake host's answer to `entry` under `precondition`, on a fresh fake
/// with its mod and the server's data pack.
fn fake_answers(entry: Entry, precondition: Precondition) -> Status {
    let fake = crate::test::FakeHost::new()
        .pack(PACK, Side::SERVER)
        .install();
    assert_eq!(fake.handle(), ME);
    // SAFETY: the fake's table is a static.
    let api = unsafe { &*fake.api() };
    match precondition {
        Precondition::UnknownMod => entry(api, UNKNOWN),
        Precondition::ModOff => {
            // SAFETY: `refuse` takes the table and handle the fake passes.
            let _off = unsafe { fake.init(refuse) };
            entry(api, ME)
        }
        Precondition::InInit => in_init(&fake, entry, ME),
        Precondition::Pack => in_init(&fake, entry, PACK),
        Precondition::Loader => entry(api, ModHandle::NONE),
        Precondition::AfterInit => {
            // SAFETY: `agree` takes the table and handle the fake passes.
            let _init = unsafe { fake.init(agree) };
            // SAFETY: `agree` reads neither the table nor the handle the
            // fake passes, and the mod's init answered OK before this ready.
            let _ready = unsafe { fake.ready(agree) };
            entry(api, ME)
        }
    }
}

/// The child test that compares the hosts.
const COMPARED: &str = "contract_tests::every_entry_is_compared_in_a_process_of_its_own";

#[test]
fn every_entry_takes_the_loader_s_caller_and_answers_as_the_loader_s_table_says() {
    // Each call installs a fresh fake, which the other tests of this process
    // would see, so they run apart.
    contract::run_apart(COMPARED).unwrap();
}

#[test]
#[ignore = "runs only as that test's child, in a process of its own"]
fn every_entry_is_compared_in_a_process_of_its_own() {
    if !contract::apart() {
        return;
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(TABLE);
    let rows = contract::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let listed: Vec<&str> = rows.iter().map(|row| row.entry.as_str()).collect();
    let entries: Vec<&str> = ENTRIES.iter().map(|&(name, _)| name).collect();
    assert_eq!(
        listed,
        entries,
        "{} lists every entry once, in field order",
        path.display()
    );
    let mut differences = Vec::new();
    for (row, &(name, fake)) in rows.iter().zip(CALLERS) {
        if row.caller != fake {
            differences.push(format!(
                "{name} caller: loader {}, fake {}",
                row.caller.name(),
                fake.name()
            ));
        }
    }
    for (column, precondition) in PRECONDITIONS.into_iter().enumerate() {
        for (row, &(name, entry)) in rows.iter().zip(ENTRIES) {
            let loader = row.answers[column];
            let fake = fake_answers(entry, precondition);
            let kept = no_game(loader)
                || KEPT
                    .iter()
                    .any(|kept| *kept == (name, precondition, loader, fake, kept.4));
            if loader != fake && !kept {
                differences.push(format!(
                    "{name} {precondition:?}: loader {loader:?}, fake {fake:?}"
                ));
            }
        }
    }
    assert!(
        differences.is_empty(),
        "{} differences:\n{}",
        differences.len(),
        differences.join("\n")
    );
}
