//! `cargo xtask close --date <YYYY-MM-DD> [--package <name>]...`: closes
//! every changelog for the alpha the workspace carries, in the pull request
//! that moves the crates to it, or with `--package` only the named
//! packages' changelogs, each at its own version, as a release pull request
//! of release-plz releases them. Each package's change files join its
//! changelog under `## [<version>] - <date>`, each heading's entries in the
//! files' name order, and the files go; a changelog without one gets the
//! heading alone. Nothing is written unless every changelog closes.

use std::path::{Path, PathBuf};

use cargo_enshwick::changelog::{CHANGELOG, CHANGES, Fragment, is_date, join};

use crate::affected::Package;
use crate::check::changelog::fragments;
use crate::commands::notes::release_version;

/// The command line `close` takes.
#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// The release date the sections carry, `YYYY-MM-DD`.
    #[arg(long, value_parser = date)]
    date: String,
    /// Closes only this package's changelog, at the package's own version;
    /// repeated for each package a release takes.
    #[arg(long = "package")]
    packages: Vec<String>,
}

fn date(text: &str) -> Result<String, String> {
    if is_date(text) {
        Ok(text.to_owned())
    } else {
        Err(format!("{text:?} is no date as YYYY-MM-DD"))
    }
}

/// Each package `args` close with the version it closes at: every package
/// at the loader's version, or the named ones at their own, each once.
///
/// # Errors
///
/// When no package is named and the loader is not among the packages, or a
/// named package is not in the workspace.
fn chosen<'a>(packages: &'a [Package], args: &Args) -> Result<Vec<(&'a Package, &'a str)>, String> {
    if args.packages.is_empty() {
        let version = release_version(packages)?;
        return Ok(packages.iter().map(|p| (p, version)).collect());
    }
    let mut chosen: Vec<(&Package, &str)> = Vec::new();
    for name in &args.packages {
        let package = packages
            .iter()
            .find(|p| &p.name == name)
            .ok_or_else(|| format!("no package {name} in the workspace"))?;
        if !chosen.iter().any(|(p, _)| std::ptr::eq(*p, package)) {
            chosen.push((package, package.version.as_str()));
        }
    }
    Ok(chosen)
}

/// Closes the changelogs of `packages` under `root` that `args` choose;
/// what it did.
///
/// # Errors
///
/// When the loader, or a package `args` name, is not among the packages, a
/// file does not read or write, or a changelog does not close ([`join`]).
pub(crate) fn run(root: &Path, packages: &[Package], args: &Args) -> Result<String, String> {
    let chosen = chosen(packages, args)?;
    let mut written = Vec::new();
    let mut gone: Vec<PathBuf> = Vec::new();
    let mut versions: Vec<&str> = Vec::new();
    for (package, version) in chosen {
        let dir = root.join(&package.dir);
        let path = dir.join(CHANGELOG);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        let files = fragments(&dir).map_err(|e| format!("{}/{CHANGES}: {e}", package.dir))?;
        let parsed = files
            .iter()
            .map(|(name, text)| {
                Fragment::parse(text).map_err(|e| format!("{}/{CHANGES}/{name}: {e}", package.dir))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let closed = join(&text, version, &args.date, &parsed)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        gone.extend(files.iter().map(|(name, _)| dir.join(CHANGES).join(name)));
        written.push((path, closed));
        if !versions.contains(&version) {
            versions.push(version);
        }
    }
    for (path, text) in &written {
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    for path in &gone {
        std::fs::remove_file(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if let Some(folder) = path.parent() {
            // Left only when another file is still in it.
            drop(std::fs::remove_dir(folder));
        }
    }
    Ok(format!(
        "closed {} changelogs for {}, joining {} change files",
        written.len(),
        versions.join(", "),
        gone.len()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::notes::LOADER;

    fn package(name: &str, dir: &str) -> Package {
        Package {
            name: name.to_owned(),
            version: "1.0.0-alpha.15".to_owned(),
            dir: dir.to_owned(),
            ..Package::default()
        }
    }

    #[test]
    fn each_changelog_closes_with_its_change_files_and_they_go() {
        let root = crate::scratch::Scratch::new("close").unwrap();
        let write = |path: &str, text: &str| {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        let head = "# Changelog\n\n## [Unreleased]\n\n## [1.0.0-alpha.14] - 2026-11-09\n\n- Old.\n";
        write("api/CHANGELOG.md", head);
        write("api/changes/b.md", "### Fixed\n\n- Fix b.\n");
        write("api/changes/a.md", "### Added\n\n- Add a.\n");
        write("loader/CHANGELOG.md", head);
        write("tool/Cargo.toml", "");
        let packages = [
            package(LOADER, "loader"),
            package("api", "api"),
            package("tool", "tool"),
        ];
        let args = Args {
            date: "2026-11-13".to_owned(),
            packages: Vec::new(),
        };
        let done = run(&root, &packages, &args).unwrap();
        let read = |path: &str| std::fs::read_to_string(root.join(path)).unwrap();
        let (api, loader) = (read("api/CHANGELOG.md"), read("loader/CHANGELOG.md"));
        let left = root.join("api/changes").exists();
        let tool_left_alone = !root.join("tool/CHANGELOG.md").exists();
        assert_eq!(
            done,
            "closed 2 changelogs for 1.0.0-alpha.15, joining 2 change files"
        );
        assert!(
            api.contains(
                "## [Unreleased]\n\n## [1.0.0-alpha.15] - 2026-11-13\n\n### Added\n\n- Add a.\n\n### Fixed\n\n- Fix b.\n\n## [1.0.0-alpha.14]"
            ),
            "{api}"
        );
        assert!(
            loader.contains(
                "## [Unreleased]\n\n## [1.0.0-alpha.15] - 2026-11-13\n\n## [1.0.0-alpha.14]"
            ),
            "{loader}"
        );
        assert!(!left, "the change files and their folder go");
        assert!(tool_left_alone, "a package without a changelog is skipped");
    }

    #[test]
    fn a_changelog_that_does_not_close_leaves_every_file_as_it_was() {
        let root = crate::scratch::Scratch::new("close-refused").unwrap();
        let write = |path: &str, text: &str| {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("api/CHANGELOG.md", "## [Unreleased]\n\n");
        write("api/changes/a.md", "### Added\n\n- Add a.\n");
        write("loader/CHANGELOG.md", "## [Unreleased]\n\n- By hand.\n");
        let packages = [package("api", "api"), package(LOADER, "loader")];
        let args = Args {
            date: "2026-11-13".to_owned(),
            packages: Vec::new(),
        };
        let error = run(&root, &packages, &args).unwrap_err();
        let api = std::fs::read_to_string(root.join("api/CHANGELOG.md")).unwrap();
        let kept = root.join("api/changes/a.md").exists();
        assert!(error.contains("Unreleased"), "{error}");
        assert_eq!(api, "## [Unreleased]\n\n");
        assert!(kept);
        assert!(date("13.11.2026").is_err());
    }

    #[test]
    fn named_packages_close_alone_each_at_its_own_version() {
        let root = crate::scratch::Scratch::new("close-named").unwrap();
        let write = |path: &str, text: &str| {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        let head = "# Changelog\n\n## [Unreleased]\n";
        write(
            "api/CHANGELOG.md",
            "# Changelog\n\n## [Unreleased]\n## [1.1.0] - 2026-11-12",
        );
        write("api/changes/a.md", "### Added\n\n- Add a.\n");
        write("tool/CHANGELOG.md", head);
        write("tool/changes/t.md", "### Fixed\n\n- Fix t.\n");
        let api = Package {
            version: "1.1.0".to_owned(),
            ..package("api", "api")
        };
        let packages = [api, package("tool", "tool")];
        let args = Args {
            date: "2026-11-13".to_owned(),
            packages: vec!["api".to_owned()],
        };
        let done = run(&root, &packages, &args).unwrap();
        let read = |path: &str| std::fs::read_to_string(root.join(path)).unwrap();
        let tool_kept = root.join("tool/changes/t.md").exists();
        assert_eq!(
            done,
            "closed 1 changelogs for 1.1.0, joining 1 change files"
        );
        assert_eq!(
            read("api/CHANGELOG.md"),
            "# Changelog\n\n## [Unreleased]\n\n## [1.1.0] - 2026-11-13\n\n### Added\n\n- Add a.\n\n"
        );
        assert_eq!(read("tool/CHANGELOG.md"), head);
        assert!(
            tool_kept,
            "a package the release does not take keeps its change files"
        );
        let twice = Args {
            date: "2026-11-13".to_owned(),
            packages: vec!["tool".to_owned(), "tool".to_owned()],
        };
        assert_eq!(
            run(&root, &packages, &twice).unwrap(),
            "closed 1 changelogs for 1.0.0-alpha.15, joining 1 change files",
            "a package named twice closes once"
        );
        let unknown = Args {
            date: "2026-11-13".to_owned(),
            packages: vec!["gone".to_owned()],
        };
        assert_eq!(
            run(&root, &packages, &unknown).unwrap_err(),
            "no package gone in the workspace"
        );
    }
}
