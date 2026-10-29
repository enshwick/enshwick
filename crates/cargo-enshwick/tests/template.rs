//! `cargo enshwick new` against the mod template. With `ENSHWICK_TEMPLATE`
//! naming a checkout of the template repository, the test renders it with
//! copier (through `uvx`), compares the crate files with the ones `new`
//! writes and builds and tests both crates on this checkout of the platform;
//! without it the test passes at once.

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    const ID: &str = "you.street-lanterns";
    const CRATE: &str = "street-lanterns";
    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
    /// The copier release the organization's template check runs.
    const COPIER: &str = "copier@9.19.1";

    /// Files compared as text.
    const SAME_TEXT: [&str; 3] = ["src/lib.rs", "src/texts.toml", "rust-toolchain.toml"];

    fn run(command: &mut Command) {
        let status = command
            .status()
            .unwrap_or_else(|e| panic!("{command:?}: {e}"));
        assert!(status.success(), "{command:?} failed");
    }

    /// Renders the template at `template` into `out`, with `answers` beside
    /// the mod id and the crate name `new` gives it.
    fn render(template: &Path, out: &Path, answers: &[String]) {
        let mut copier = Command::new("uvx");
        copier.args([
            "--quiet",
            COPIER,
            "copy",
            "--quiet",
            "--trust",
            "--defaults",
            "--vcs-ref",
            "HEAD",
            "-d",
            &format!("mod_id={ID}"),
            "-d",
            &format!("crate_name={CRATE}"),
        ]);
        for answer in answers {
            copier.args(["-d", answer]);
        }
        run(copier.arg(template).arg(out));
    }

    /// Creates the crate with `cargo enshwick new` in `dir`.
    fn create(dir: &Path, args: &[&str]) {
        std::fs::create_dir_all(dir).unwrap();
        run(Command::new(env!("CARGO_BIN_EXE_cargo-enshwick"))
            .args(["enshwick", "new", ID, "--dir"])
            .arg(dir)
            .args(args));
    }

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    fn table(path: &Path) -> toml::Table {
        read(path)
            .parse()
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    /// `Cargo.toml` without what each crate fills in for itself: who wrote
    /// it, where it lives, the template's tools as members, and the version
    /// requirements of crates.io dependencies, which Renovate moves.
    fn manifest(path: &Path) -> toml::Table {
        let mut manifest = table(path);
        if let Some(package) = manifest
            .get_mut("package")
            .and_then(toml::Value::as_table_mut)
        {
            for key in ["authors", "repository", "homepage"] {
                package.remove(key);
            }
        }
        if let Some(workspace) = manifest
            .get_mut("workspace")
            .and_then(toml::Value::as_table_mut)
        {
            workspace.remove("members");
        }
        for kind in ["dependencies", "dev-dependencies"] {
            let Some(deps) = manifest.get_mut(kind).and_then(toml::Value::as_table_mut) else {
                continue;
            };
            for (_, spec) in deps.iter_mut() {
                match spec {
                    toml::Value::String(version) => "*".clone_into(version),
                    toml::Value::Table(spec) => {
                        spec.remove("version");
                    }
                    _ => {}
                }
            }
        }
        manifest
    }

    /// `.cargo/config.toml` without the template's `xtask` alias.
    fn config(path: &Path) -> toml::Table {
        let mut config = table(path);
        config.remove("alias");
        config
    }

    fn compare(new: &Path, template: &Path) {
        for file in SAME_TEXT {
            assert_eq!(
                read(&new.join(file)),
                read(&template.join(file)),
                "{file} differs between `new` and the template"
            );
        }
        assert_eq!(
            manifest(&new.join("Cargo.toml")),
            manifest(&template.join("Cargo.toml")),
            "Cargo.toml differs between `new` and the template"
        );
        assert_eq!(
            config(&new.join(".cargo/config.toml")),
            config(&template.join(".cargo/config.toml")),
            ".cargo/config.toml differs between `new` and the template"
        );
    }

    /// Builds and tests the crate at `dir` on this checkout's `enshwick`,
    /// with one target folder for every crate this test builds.
    fn build(dir: &Path) {
        let platform = Path::new(env!("CARGO_MANIFEST_DIR")).join("../enshwick");
        let platform = platform.canonicalize().unwrap();
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        run(Command::new(cargo)
            .current_dir(dir)
            .env(
                "CARGO_TARGET_DIR",
                Path::new(env!("CARGO_TARGET_TMPDIR")).join("template-crates"),
            )
            .args([
                "test",
                "--quiet",
                "--config",
                &format!(
                    "patch.crates-io.enshwick.path={:?}",
                    platform.display().to_string()
                ),
            ]));
    }

    #[test]
    fn a_new_crate_is_the_template_crate_and_builds() {
        let Some(template) = std::env::var_os("ENSHWICK_TEMPLATE") else {
            return;
        };
        let template = PathBuf::from(template).canonicalize().unwrap();
        // Outside every Cargo workspace, so `new` makes a crate of its own.
        let scratch =
            std::env::temp_dir().join(format!("enshwick-template-{}", std::process::id()));
        let _old = std::fs::remove_dir_all(&scratch);

        let released_new = scratch.join("new");
        let released_template = scratch.join("template");
        create(&released_new, &[]);
        render(&template, &released_template, &[]);
        compare(&released_new.join(CRATE), &released_template);

        let git_new = scratch.join("new-git");
        let git_template = scratch.join("template-git");
        create(&git_new, &["--enshwick-rev", COMMIT]);
        render(
            &template,
            &git_template,
            &[format!("enshwick_rev={COMMIT}")],
        );
        compare(&git_new.join(CRATE), &git_template);

        build(&released_new.join(CRATE));
        build(&released_template);
        std::fs::remove_dir_all(&scratch).unwrap();
    }
}
