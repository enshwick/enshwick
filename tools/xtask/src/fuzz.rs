//! Short fuzz runs for a change: the targets in `fuzz/` whose input the
//! change reaches, or every target in the full suite, sharing [`BUDGET`]
//! seconds, built by cargo-fuzz with the nightly compiler
//! [`TOOLCHAIN_VAR`] names. Longer runs stay local, as `fuzz/README.md` says.

use crate::Step;

/// Seconds of fuzzing shared by the targets a run fuzzes, so the part fits
/// the pull request lane however many a change reaches.
const BUDGET: u64 = 45;
/// Seconds a target runs at least, enough for its seed corpus and some
/// mutations; low, since a change to the fuzz crate reaches every target.
const LEAST: u64 = 2;
/// The variable naming the nightly toolchain to build with, which CI sets
/// to the dated one it installs.
const TOOLCHAIN_VAR: &str = "ENSH_FUZZ_TOOLCHAIN";

/// The `+<toolchain>` argument for cargo and rustc: [`TOOLCHAIN_VAR`], else
/// `nightly`.
fn toolchain() -> String {
    let name = std::env::var(TOOLCHAIN_VAR)
        .ok()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "nightly".to_owned());
    format!("+{name}")
}

/// Each target and the modules that read its input, as paths without `.rs`:
/// a change to the module file or below its folder reaches the target.
pub(crate) const TARGETS: &[(&str, &[&str])] = &[
    ("action_sheet", &["crates/enshwick/src/actions"]),
    ("item_records", &["crates/internal/loader/src/item_records"]),
    ("item_save", &["crates/internal/loader/src/item_records"]),
    (
        "join_lists",
        &[
            "crates/internal/loader/src/admission",
            "crates/internal/loader/src/join",
        ],
    ),
    ("migrations", &["crates/internal/loader/src/migrations"]),
    (
        "mod_manifests",
        &[
            "crates/enshwick/src/abi",
            "crates/internal/loader/src/manifest",
            "crates/internal/loader/src/resolve",
        ],
    ),
    (
        "notices",
        &[
            "crates/internal/loader/src/notice_queue",
            "crates/internal/loader/src/wire",
        ],
    ),
    ("pattern", &["crates/enshwick/src/code"]),
    ("point_schemas", &["crates/cargo-enshwick/src/points"]),
    ("reflection", &["crates/enshwick-gamedata/src/reflect"]),
    ("remote_edit", &["crates/enshwick/src/config"]),
    ("remote_message", &["crates/internal/loader/src/remote"]),
    ("server_rules", &["crates/internal/loader/src/join"]),
    (
        "settings_files",
        &[
            "crates/enshwick/src/config",
            "crates/enshwick/src/i18n",
            "crates/enshwick/src/packs",
            "crates/internal/loader/src/modlist",
        ],
    ),
    ("table", &["crates/enshwick-gamedata/src"]),
    ("wire_frames", &["crates/internal/loader/src/wire"]),
];

/// Every target.
pub(crate) fn all() -> Vec<&'static str> {
    TARGETS.iter().map(|(target, _)| *target).collect()
}

/// The targets the changed `files` reach: through a module that reads their
/// input, their own file or seed corpus, or the fuzz crate's shared code.
pub(crate) fn reached(files: &[String]) -> Vec<&'static str> {
    let under = |file: &str, path: &str| {
        file.strip_prefix(path)
            .is_some_and(|rest| rest == ".rs" || rest.starts_with('/'))
    };
    let shared = files.iter().any(|file| {
        file.starts_with("fuzz/src/") || file == "fuzz/Cargo.toml" || file == "fuzz/Cargo.lock"
    });
    TARGETS
        .iter()
        .filter(|(target, modules)| {
            shared
                || files.iter().any(|file| {
                    modules.iter().any(|module| under(file, module))
                        || *file == format!("fuzz/fuzz_targets/{target}.rs")
                        || file.starts_with(&format!("fuzz/corpus/{target}/"))
                })
        })
        .map(|(target, _)| *target)
        .collect()
}

/// Seconds each of `count` targets runs.
fn share(count: usize) -> u64 {
    let count = u64::try_from(count.max(1)).unwrap_or(u64::MAX);
    (BUDGET / count).max(LEAST)
}

/// The host triple of the nightly compiler, from `rustc -vV`.
fn host() -> Option<String> {
    let output = std::process::Command::new("rustc")
        .args([toolchain().as_str(), "-vV"])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(str::to_owned)
}

/// Runs each of `targets` for its share of the budget over its seed corpus;
/// whether none found a crash.
pub(crate) fn run(targets: &[&str]) -> bool {
    if targets.is_empty() {
        crate::say("no fuzz target reached; nothing to fuzz");
        return true;
    }
    // cargo-fuzz builds for the target it was itself built for, which for
    // its release binary is musl, without a standard library to sanitize.
    let Some(host) = host() else {
        crate::say(&format!(
            "rustc {toolchain} names no host; is that toolchain installed?",
            toolchain = toolchain()
        ));
        return false;
    };
    let seconds = format!("-max_total_time={}", share(targets.len()));
    let toolchain = toolchain();
    targets.iter().all(|target| {
        let found = format!("target/fuzz/corpus/{target}");
        if let Err(error) = std::fs::create_dir_all(&found) {
            crate::say(&format!("creating {found}: {error}"));
            return false;
        }
        Step::tool(
            "cargo",
            &[
                &toolchain,
                "fuzz",
                "run",
                // The workspace crates build anew in every CI run, since a
                // checkout gives their files new times; unoptimized they
                // build in about a quarter of the time, which a short run
                // needs more than speed per input.
                "--dev",
                "--target",
                &host,
                "--target-dir",
                "target/fuzz",
                target,
                &found,
                &format!("fuzz/corpus/{target}"),
                "--",
                &seconds,
            ],
        )
        .run(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|path| (*path).to_owned()).collect()
    }

    #[test]
    fn a_module_reaches_the_targets_that_read_it() {
        assert_eq!(
            reached(&files(&["crates/internal/loader/src/wire/frames.rs"])),
            ["notices", "wire_frames"]
        );
        assert_eq!(
            reached(&files(&["crates/internal/loader/src/join.rs"])),
            ["join_lists", "server_rules"]
        );
    }

    #[test]
    fn a_module_with_a_longer_name_reaches_nothing() {
        assert!(reached(&files(&["crates/internal/loader/src/joiner.rs"])).is_empty());
        assert!(reached(&files(&["crates/enshwick/src/lib.rs"])).is_empty());
    }

    #[test]
    fn a_target_s_own_file_or_corpus_reaches_it_and_shared_code_reaches_all() {
        assert_eq!(reached(&files(&["fuzz/fuzz_targets/table.rs"])), ["table"]);
        assert_eq!(reached(&files(&["fuzz/corpus/pattern/call"])), ["pattern"]);
        assert_eq!(reached(&files(&["fuzz/src/lib.rs"])), all());
    }

    #[test]
    fn the_budget_is_shared_but_each_target_runs_a_while() {
        assert_eq!(share(1), BUDGET);
        assert_eq!(share(3), 15);
        assert_eq!(share(TARGETS.len()), LEAST);
    }

    #[test]
    fn the_list_names_every_target_and_each_has_a_seed_corpus() {
        let root = crate::workspace_root();
        let mut found: Vec<String> = std::fs::read_dir(root.join("fuzz/fuzz_targets"))
            .unwrap()
            .filter_map(|entry| {
                let name = entry.unwrap().file_name().into_string().unwrap();
                name.strip_suffix(".rs").map(str::to_owned)
            })
            .collect();
        found.sort();
        assert_eq!(found, all());
        for target in all() {
            assert!(
                root.join(format!("fuzz/corpus/{target}")).is_dir(),
                "{target}"
            );
        }
    }
}
