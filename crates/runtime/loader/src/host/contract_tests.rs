//! The loader's half of its contract with the fake host: each entry's caller
//! kind and its answer to a call with zeroed arguments under each
//! precondition, written as the table `crates/internal/fake/contract/loader.snap`
//! that the fake host's own `contract_tests` hold the fake to, on its own
//! feature set.

use std::sync::atomic::Ordering;

use enshwick_abi::contract::{
    self, ENTRIES, Entry, ME, PACK, PRECONDITIONS, Precondition, Row, UNKNOWN,
};
use enshwick_core::abi::{ModHandle, Status};

use super::API;
use super::layer::Callers;
use crate::engine::loader::{Loader, initialising, install_for_test};

/// [`CALLERS`], every entry's caller kind by name, in field order.
macro_rules! callers {
    ($($field:ident)*) => {
        const CALLERS: &[(&str, &str)] = &[$((stringify!($field), Callers::$field),)*];
    };
}

enshwick_abi::__host_entries!(callers);

/// The ids of the loader's mod and data pack, in the order that gives them
/// [`ME`] and [`PACK`].
const IDS: [&str; 2] = ["you.probe", "you.probe-pack"];

/// A fresh loader with no game running the mod [`ME`] and the server's data
/// pack [`PACK`].
fn fresh() -> &'static Loader {
    let mut loader = crate::start::tests::loader_running(&IDS);
    let ids: Vec<&str> = loader.mods.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, IDS, "the mod is first and the pack second");
    loader.mods[PACK.0 - 1].pack = true;
    install_for_test(loader)
}

/// The bare loader's answer to `entry` under `precondition`.
///
/// After init the registries mods fill during init are sealed, as start
/// seals them; the seal is the process's for good, so every call after init
/// comes after every other.
fn loader_answers(entry: Entry, precondition: Precondition) -> Status {
    static SEALED: std::sync::Once = std::sync::Once::new();
    let loader = fresh();
    match precondition {
        Precondition::UnknownMod => entry(&API, UNKNOWN),
        Precondition::ModOff => {
            loader.mods[0].off.store(true, Ordering::Release);
            entry(&API, ME)
        }
        Precondition::InInit => initialising(0, || entry(&API, ME)),
        Precondition::Pack => initialising(1, || entry(&API, PACK)),
        Precondition::Loader => entry(&API, ModHandle::NONE),
        Precondition::AfterInit => {
            SEALED.call_once(|| crate::start::seal(loader));
            let _first = loader.delivery.set(Box::default());
            entry(&API, ME)
        }
    }
}

/// Set in the child process that writes the table.
const CHILD: &str = "ENSHWICK_CONTRACT_CHILD";

/// The child test that writes the table.
const WRITTEN: &str = "host::contract_tests::the_table_is_written_in_a_process_of_its_own";

#[test]
fn the_loader_s_answer_and_caller_of_every_entry_match_the_contract_table() {
    // The calls fill the loader's process-wide registries and seal them for
    // good, which the other tests of this process read, so they run apart.
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", WRITTEN, "--ignored", "--nocapture"])
        .args(["--test-threads", "1"])
        .env(CHILD, "1")
        .output()
        .unwrap();
    let out = format!(
        "{}{}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    assert!(child.status.success(), "{out}");
    assert!(out.contains("1 passed"), "the table was written: {out}");
}

#[test]
#[ignore = "runs only as that test's child, in a process of its own"]
fn the_table_is_written_in_a_process_of_its_own() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    let mut rows: Vec<Row> = ENTRIES
        .iter()
        .zip(CALLERS)
        .map(|(&(entry, _), &(named, caller))| {
            assert_eq!(entry, named);
            Row {
                entry: entry.to_owned(),
                caller: caller.to_owned(),
                answers: [Status::OK; PRECONDITIONS.len()],
            }
        })
        .collect();
    for (column, precondition) in PRECONDITIONS.into_iter().enumerate() {
        for (row, &(_, entry)) in rows.iter_mut().zip(ENTRIES) {
            row.answers[column] = loader_answers(entry, precondition);
        }
    }
    // In the fake's folder, so a change of the table puts the fake's half in
    // the change's scope.
    insta::with_settings!({
        snapshot_path => "../../../../internal/fake/contract",
        prepend_module_to_snapshot => false,
    }, {
        insta::assert_snapshot!("loader", contract::render(&rows));
    });
}
