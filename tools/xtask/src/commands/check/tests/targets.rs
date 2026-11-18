//! The lints for a target other than the host's.

use super::*;

#[test]
fn the_linux_lint_runs_when_the_change_reaches_a_package_it_lints_and_leaves_out_c() {
    let loader = package("enshwick-loader", "crates/runtime/loader", false);
    let mut luau = package("enshwick-luau", "crates/runtime/luau", false);
    luau.compiles_c = true;
    let stub = package("enshwick-loader-stub", "crates/runtime/stub", true);
    let abi = package("enshwick-abi", "crates/internal/abi", false);
    let packages = [loader, luau, stub, abi];
    let changed = |names: &[&str]| Scope::Some {
        code: BTreeSet::new(),
        docs: BTreeSet::new(),
        tested: names.iter().map(|name| (*name).to_owned()).collect(),
        built: BTreeSet::new(),
        wide: true,
    };
    let lint = "cargo clippy --target x86_64-unknown-linux-gnu --workspace --features \
                enshwick/test,enshwick/unstable,enshwick/typed,xtask/compat,enshwick-loader/test \
                --exclude enshwick-loader-stub --exclude enshwick-luau -- -D warnings";
    // A crate without target gates of its own still breaks on Linux when it
    // calls a dependency's gated item or gates a dependency in its manifest.
    for names in [&["enshwick-abi"][..], &["enshwick-abi", "enshwick-loader"]] {
        assert_eq!(
            linux_lint(&changed(names), &packages)
                .map(|step| step.line())
                .as_deref(),
            Some(lint),
            "{names:?}"
        );
    }
    assert!(linux_lint(&Scope::All, &packages).is_some());
    // Only code CI lints on Linux itself.
    assert!(
        linux_lint(
            &changed(&["enshwick-luau", "enshwick-loader-stub"]),
            &packages
        )
        .is_none()
    );
}
