//! The search indexes the rustdoc steps clear before they run.

use super::*;

#[test]
fn a_doc_step_clears_the_search_index_of_the_folder_cargo_writes_and_no_other_step_any() {
    let (root, cwd) = (Path::new("/w"), Path::new("/w/crates/a"));
    let index = |step: &Step, inherited| step.search_index(root, cwd, inherited);
    let doc = Step::new(&["doc", "--no-deps", "-p", "enshwick"]);
    assert_eq!(
        index(&doc, None),
        Some(root.join("target/doc/search.index"))
    );
    assert_eq!(
        index(&doc, Some("/t/outer")),
        Some(Path::new("/t/outer/doc/search.index").to_path_buf())
    );
    let own = Step::new(&["doc", "--no-deps", "--target-dir", "/t/paths"])
        .then(&["--target", WINDOWS_TARGET]);
    assert_eq!(
        index(&own, Some("/t/outer")),
        Some(
            Path::new("/t/paths")
                .join(WINDOWS_TARGET)
                .join("doc/search.index")
        )
    );
    let joined = Step::new(&["doc", "--target-dir=rel", "--target=x"]);
    assert_eq!(
        index(&joined, None),
        Some(cwd.join("rel/x/doc/search.index"))
    );
    let env = Step::new(&["doc"]).env("CARGO_TARGET_DIR", "/t/step");
    assert_eq!(
        index(&env, Some("/t/outer")),
        Some(Path::new("/t/step/doc/search.index").to_path_buf())
    );
    for other in [
        Step::new(&["clippy", "--all-targets"]),
        Step::new(&["test", "--doc"]),
        Step::tool("cargo-machete", &["doc"]),
    ] {
        assert_eq!(index(&other, None), None, "{}", other.line());
    }
}
