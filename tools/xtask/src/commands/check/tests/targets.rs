//! The lints for a target other than the host's.

use super::*;

#[test]
fn the_linux_lint_runs_when_the_change_reaches_target_gated_code_and_leaves_out_c() {
    let loader = package("enshwick-loader", "crates/runtime/loader", false);
    let mut luau = package("enshwick-luau", "crates/runtime/luau", false);
    luau.compiles_c = true;
    let stub = package("enshwick-loader-stub", "crates/runtime/stub", true);
    let abi = package("enshwick-abi", "crates/internal/abi", false);
    let packages = [loader, luau, stub, abi];
    let gated = |package: &Package| package.name != "enshwick-abi";
    let changed = |names: &[&str]| Scope::Some {
        code: BTreeSet::new(),
        docs: BTreeSet::new(),
        tested: names.iter().map(|name| (*name).to_owned()).collect(),
        built: BTreeSet::new(),
        wide: true,
    };
    let step = linux_lint(
        &changed(&["enshwick-abi", "enshwick-loader"]),
        &packages,
        gated,
    )
    .map(|step| step.line());
    assert_eq!(
        step.as_deref(),
        Some(
            "cargo clippy --target x86_64-unknown-linux-gnu --workspace --features \
             enshwick/test,enshwick/unstable,enshwick/typed,xtask/compat,enshwick-loader/test \
             --exclude enshwick-loader-stub --exclude enshwick-luau -- -D warnings"
        )
    );
    assert!(linux_lint(&Scope::All, &packages, gated).is_some());
    // No gate Linux reads differently, or only code CI lints there itself.
    for names in [
        &["enshwick-abi"][..],
        &["enshwick-luau", "enshwick-loader-stub"],
    ] {
        assert!(
            linux_lint(&changed(names), &packages, gated).is_none(),
            "{names:?}"
        );
    }
}
