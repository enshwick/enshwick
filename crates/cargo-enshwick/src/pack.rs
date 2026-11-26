//! `cargo enshwick pack`: checks a data pack folder as the loader reads
//! it, writes the JSON Schemas of its files for editors, and zips it for a
//! release.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use clap::Subcommand;
use enshwick_core::content::vanilla::Vanilla;
use enshwick_core::packs::{MANIFEST_FILE, Pack, PackFiles, PackManifest};

/// What `cargo enshwick pack` does.
#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Checks a pack folder as the loader reads it and lists every problem
    /// with its file, line and column.
    Check {
        /// The pack folder, holding `pack.toml`.
        dir: PathBuf,
        /// The game folder, holding `enshrouded.exe` or
        /// `enshrouded_server.exe`, whose data checks the vanilla debug
        /// names and stations the pack names; `ENSHROUDED_DIR` when omitted.
        #[arg(long, env = "ENSHROUDED_DIR")]
        game: Option<PathBuf>,
    },
    /// Writes the JSON Schemas of a pack's files, `<file>.schema.json`, for
    /// editors that check TOML against them.
    Schema {
        /// The folder the schemas go into.
        #[arg(default_value = ".")]
        out: PathBuf,
    },
    /// Zips a mod for a release, its files under `mods/<id>/`, as players
    /// unpack them into the game's folder: a mod crate built in release for
    /// Windows, its manifest checked against `Cargo.toml`, or a pack folder
    /// checked as the loader reads it. Prints the archive's SHA-256.
    Zip {
        /// The mod's folder: a crate, or a data pack or Luau mod holding
        /// `pack.toml`.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// The package to zip, when the workspace has several mods.
        #[arg(long, short)]
        package: Option<String>,
        /// The archive; `<id>-<version>.zip` in the current folder when left
        /// out.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

/// Runs `command`; what it did, or why it failed.
pub(crate) fn run(command: &Command) -> anyhow::Result<String> {
    match command {
        Command::Check { dir, game } => {
            let vanilla = game.as_deref().map(vanilla).transpose()?;
            check(dir, vanilla.as_ref()).map(|(summary, _, _)| summary)
        }
        Command::Schema { out } => schema(out),
        Command::Zip { dir, package, out } => zip(dir, package.as_deref(), out.as_deref()),
    }
}

/// The vanilla content of the game in `game`: the layouts from its
/// executable's reflection, the data from its archive.
pub(crate) fn vanilla(game: &Path) -> anyhow::Result<Vanilla> {
    use enshwick_gamedata::catalogue::Reflected;
    use enshwick_gamedata::reflect::{Image, Layout, Reflection};

    let (exe, name) = ["enshrouded", "enshrouded_server"]
        .into_iter()
        .map(|name| (game.join(format!("{name}.exe")), name))
        .find(|(exe, _)| exe.is_file())
        .with_context(|| {
            format!(
                "{}: no enshrouded.exe or enshrouded_server.exe",
                game.display()
            )
        })?;
    let bytes = std::fs::read(&exe).with_context(|| exe.display().to_string())?;
    let image = Image::parse(&bytes, Layout::File, None)?;
    let reflection = Reflection::scan(&image);
    let archive = enshwick_gamedata::archive::Archive::open(game, name)
        .with_context(|| format!("{}: {name}.kfc does not read", game.display()))?;
    Ok(Vanilla::new(&Reflected(&reflection), move |key| {
        archive.resource(&key)
    })?)
}

/// Reads and checks the pack in `dir`, with the game's vanilla names when
/// `vanilla` is given: a summary, its manifest and files.
pub(crate) fn check(
    dir: &Path,
    vanilla: Option<&Vanilla>,
) -> anyhow::Result<(String, PackManifest, PackFiles)> {
    let manifest_path = dir.join(MANIFEST_FILE);
    let text = std::fs::read_to_string(&manifest_path)
        .with_context(|| manifest_path.display().to_string())?;
    let manifest = PackManifest::parse(&text)?;
    semver::Version::parse(&manifest.version)
        .with_context(|| format!("{MANIFEST_FILE}: version {:?}", manifest.version))?;
    let files = PackFiles::read_dir(dir)?;
    let pack = Pack::parse_with(&manifest.id, &files, vanilla)?;
    manifest.check(&text, &pack)?;
    let summary = format!(
        "{} {}: {} content ids, {} loot drops, {} changes; join hash {:08x}",
        manifest.id,
        manifest.version,
        pack.ids().len(),
        pack.drops().len(),
        pack.changes().len(),
        files.hash()
    );
    let summary = pack
        .skipped()
        .iter()
        .fold(summary, |out, skipped| format!("{out}\nwarning: {skipped}"));
    Ok((summary, manifest, files))
}

fn schema(out: &Path) -> anyhow::Result<String> {
    std::fs::create_dir_all(out).with_context(|| out.display().to_string())?;
    let mut written = Vec::new();
    for (file, schema) in enshwick_core::packs::schemas() {
        let name = file.replace(".toml", ".schema.json");
        let path = out.join(&name);
        std::fs::write(&path, schema).with_context(|| path.display().to_string())?;
        written.push(name);
    }
    Ok(format!(
        "wrote {} in {}; name one at the top of a file: #:schema ./{}",
        written.join(", "),
        out.display(),
        written.first().map_or("", String::as_str)
    ))
}

/// Builds or checks the mod in `dir` and writes its release archive; what it
/// found and the archive's path and SHA-256.
fn zip(dir: &Path, package: Option<&str>, out: Option<&Path>) -> anyhow::Result<String> {
    archive(&crate::install::build(dir, package)?, out)
}

/// Writes the release archive of `built`, a crate's only once its DLL's
/// manifest matches the crate where it can be read.
fn archive(built: &crate::install::Built, out: Option<&Path>) -> anyhow::Result<String> {
    let mut said = vec![built.summary.clone()];
    if let Some((dll, side)) = &built.dll {
        let expected = crate::manifest::Expected {
            id: Some(&built.id),
            version: Some(&built.version),
            side: side.as_deref(),
        };
        if cfg!(windows) {
            crate::manifest::show(dll, &expected).map_err(anyhow::Error::msg)?;
            said.push(format!(
                "its manifest says {} {}, as Cargo.toml does",
                built.id, built.version
            ));
        } else {
            said.push(
                "warning: the DLL's manifest is read on Windows only; check it there with `cargo enshwick manifest --expect-version`"
                    .to_owned(),
            );
        }
    }
    let out = out.map_or_else(
        || PathBuf::from(format!("{}-{}.zip", built.id, built.version)),
        Path::to_path_buf,
    );
    let entries = built
        .files
        .iter()
        .map(|(name, bytes)| (format!("mods/{}/{name}", built.id), bytes.as_slice()));
    let sha = write_zip(&out, entries)?;
    said.push(format!("wrote {}\nSHA-256 {sha}", out.display()));
    Ok(said.join("\n"))
}

/// Writes `entries`, by their path in the archive, to the zip `out` so that
/// the same entries always give the same bytes: sorted, dated 1980-01-01,
/// one mode and one compression; the archive's SHA-256 in hex.
pub(crate) fn write_zip<'a>(
    out: &Path,
    entries: impl IntoIterator<Item = (String, &'a [u8])>,
) -> anyhow::Result<String> {
    use sha2::Digest as _;

    let mut entries: Vec<(String, &[u8])> = entries.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let options = ::zip::write::SimpleFileOptions::default()
        .compression_method(::zip::CompressionMethod::Deflated)
        .compression_level(Some(6))
        .last_modified_time(::zip::DateTime::default())
        .unix_permissions(0o644);
    let mut archive = ::zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        archive
            .start_file(name.as_str(), options)
            .and_then(|()| archive.write_all(bytes).map_err(Into::into))
            .with_context(|| name.clone())?;
    }
    let bytes = archive
        .finish()
        .with_context(|| out.display().to_string())?
        .into_inner();
    std::fs::write(out, &bytes).with_context(|| out.display().to_string())?;
    Ok(crate::loader::hex(&sha2::Sha256::digest(&bytes)))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A scratch pack folder `name` with `items` as its `items.toml`.
    pub(crate) fn pack(name: &str, items: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("enshwick-pack-{name}-{}", std::process::id()));
        let _fresh = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(MANIFEST_FILE),
            "id = \"you.lanterns\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("items.toml"), items).unwrap();
        dir
    }

    #[test]
    fn a_good_pack_checks_and_zips_under_its_folder() {
        let dir = pack(
            "good",
            "[[item]]\nid = \"lantern\"\nlike = \"6943053a-32c1-4718-ae22-e61427ddf54d\"\n",
        );
        let (summary, _, _) = check(&dir, None).unwrap();
        assert!(
            summary.starts_with("you.lanterns 1.0.0: 1 content ids"),
            "{summary}"
        );
        let out = dir.join("out.zip");
        let said = zip(&dir, None, Some(&out)).unwrap();
        let again = dir.join("again.zip");
        let said_again = zip(&dir, None, Some(&again)).unwrap();
        assert_eq!(
            said.lines().last(),
            said_again.lines().last(),
            "same SHA-256"
        );
        assert_eq!(std::fs::read(&out).unwrap(), std::fs::read(&again).unwrap());
        assert!(
            zip(&dir, Some("lanterns"), None)
                .unwrap_err()
                .to_string()
                .contains("--package names a crate")
        );
        let unpacked = dir.join("game");
        ::zip::ZipArchive::new(std::fs::File::open(&out).unwrap())
            .unwrap()
            .extract(&unpacked)
            .unwrap();
        let read = PackFiles::read_dir(&unpacked.join("mods/you.lanterns")).unwrap();
        assert_eq!(read.hash(), PackFiles::read_dir(&dir).unwrap().hash());
        let archive = ::zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
        let mut names: Vec<String> = archive
            .file_names()
            .map(|name| name.unwrap().into_owned())
            .collect();
        names.sort_unstable();
        assert_eq!(
            names,
            [
                "mods/you.lanterns/items.toml",
                "mods/you.lanterns/pack.toml"
            ]
        );
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_crate_s_archive_holds_its_dll_in_the_folder_of_its_id_once_its_manifest_is_checked() {
        let dir = crate::scratch("crate-zip");
        let dll = dir.join("stalls.dll");
        std::fs::write(&dll, b"MZ").unwrap();
        let built = crate::install::Built {
            id: "you.stalls".to_owned(),
            version: "0.2.0".to_owned(),
            summary: format!("built {}", dll.display()),
            files: vec![("stalls.dll".to_owned(), b"MZ".to_vec())],
            dll: Some((dll, Some("both".to_owned()))),
        };
        let out = dir.join("you.stalls-0.2.0.zip");
        let said = archive(&built, Some(&out));
        if cfg!(windows) {
            // A DLL whose manifest does not read is never archived.
            assert!(said.unwrap_err().to_string().contains("stalls.dll"));
            assert!(!out.exists());
            write_zip(
                &out,
                [("mods/you.stalls/stalls.dll".to_owned(), &b"MZ"[..])],
            )
            .unwrap();
        } else {
            assert!(said.unwrap().contains("read on Windows only"));
        }
        ::zip::ZipArchive::new(std::fs::File::open(&out).unwrap())
            .unwrap()
            .extract(dir.join("game"))
            .unwrap();
        assert_eq!(
            std::fs::read(dir.join("game/mods/you.stalls/stalls.dll")).unwrap(),
            b"MZ"
        );
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_pack_that_changes_items_runs_on_both_sides() {
        let dir = pack(
            "join",
            "[[change]]\nitems = [\"6943053a-32c1-4718-ae22-e61427ddf54d\"]\nmax_stack = 50\n",
        );
        let (summary, _, _) = check(&dir, None).unwrap();
        assert!(summary.contains("1 changes"), "{summary}");
        std::fs::write(
            dir.join(MANIFEST_FILE),
            "id = \"you.lanterns\"\nversion = \"1.0.0\"\nside = \"server\"\n",
        )
        .unwrap();
        let error = check(&dir, None).unwrap_err().to_string();
        assert!(
            error.starts_with("pack.toml:3:1: side = \"server\""),
            "{error}"
        );
        let _gone = std::fs::remove_dir_all(&dir);
    }

    /// Every data pack among the examples, by folder.
    pub(crate) fn example_packs() -> Vec<PathBuf> {
        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        let mut packs: Vec<PathBuf> = std::fs::read_dir(examples)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|dir| dir.join(MANIFEST_FILE).is_file())
            .collect();
        packs.sort();
        packs
    }

    #[test]
    fn every_example_pack_checks_clean_against_the_game_s_data_when_it_is_at_hand() {
        let packs = example_packs();
        assert!(!packs.is_empty());
        let game = std::env::var_os("ENSHROUDED_DIR").map(PathBuf::from);
        let vanilla = game.as_deref().map(|dir| vanilla(dir).unwrap());
        for dir in packs {
            match check(&dir, vanilla.as_ref()) {
                Ok(_) => {}
                // Without the game's data, only the game's names are left
                // to check.
                Err(error) if vanilla.is_none() => {
                    let text = error.to_string();
                    assert!(
                        text.lines().all(|line| line.contains("the game's data")),
                        "{}: {text}",
                        dir.display()
                    );
                }
                Err(error) => panic!("{}: {error}", dir.display()),
            }
        }
    }

    #[test]
    fn a_bad_pack_names_the_line() {
        let dir = pack("bad", "[[item]]\nid = \"lantern\"\nlike = \"torch\"\n");
        let why = format!("{:#}", check(&dir, None).unwrap_err());
        assert!(
            why.starts_with("items.toml:1:1: no item \"torch\""),
            "{why}"
        );
        let _gone = std::fs::remove_dir_all(&dir);
    }

    /// The installed client, when `ENSHWICK_CLIENT_DIR` names its folder
    /// (build 1269579): vanilla names and a forge clone check against its
    /// data, and a misspelt name is explained.
    #[test]
    #[ignore = "needs ENSHWICK_CLIENT_DIR"]
    fn the_game_s_data_checks_vanilla_names() {
        let game = std::env::var("ENSHWICK_CLIENT_DIR")
            .expect("ENSHWICK_CLIENT_DIR names the client folder");
        let vanilla = vanilla(Path::new(&game)).unwrap();
        let dir = pack(
            "names",
            "[[item]]\nid = \"bench\"\nlike = \"Craftingstation_Workbench\"\n",
        );
        std::fs::write(
            dir.join("workshops.toml"),
            "[[workshop]]\nid = \"forge\"\nlike = \"Prop_Production_T1_Forge\"\n",
        )
        .unwrap();
        let (summary, _, _) = check(&dir, Some(&vanilla)).unwrap();
        assert!(summary.contains("2 content ids"), "{summary}");
        std::fs::write(
            dir.join("items.toml"),
            "[[item]]\nid = \"bench\"\nlike = \"Craftingstation_Workbenhc\"\n",
        )
        .unwrap();
        let why = format!("{:#}", check(&dir, Some(&vanilla)).unwrap_err());
        assert!(why.contains("near: Craftingstation_Workbench"), "{why}");
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn schemas_are_written_per_file() {
        let dir = std::env::temp_dir().join(format!("enshwick-schemas-{}", std::process::id()));
        schema(&dir).unwrap();
        let items = std::fs::read_to_string(dir.join("items.schema.json")).unwrap();
        assert!(items.contains("\"like\""), "{items}");
        let _gone = std::fs::remove_dir_all(&dir);
    }
}
