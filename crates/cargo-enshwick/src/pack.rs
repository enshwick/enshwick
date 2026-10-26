//! `cargo enshwick pack`: checks a content pack folder as the loader reads
//! it, writes the JSON Schemas of its files for editors, and zips it for a
//! release.

use anyhow::Context as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use clap::Subcommand;
use enshwick::content::vanilla::Vanilla;
use enshwick::packs::{MANIFEST_FILE, Pack, PackFiles, PackManifest};

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
    /// Zips a pack folder for a release, its files under `mods/<id>/`, as
    /// players unpack them into the game's folder.
    Zip {
        /// The pack folder, holding `pack.toml`.
        dir: PathBuf,
        /// The archive; `<id>.zip` in the current folder when left out.
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
        Command::Zip { dir, out } => zip(dir, out.as_deref()),
    }
}

/// The vanilla content of the game in `game`: the layouts from its
/// executable's reflection, the data from its archive.
fn vanilla(game: &Path) -> anyhow::Result<Vanilla> {
    use enshwick::Off;
    use enshwick::content::items::ItemLayout;
    use enshwick::content::workshops::WorkshopLayout;
    use enshwick_reflect::{Image, Layout, Reflection};

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
    let field = |ty: &str, name: &str| {
        reflection
            .field_offset(ty, name)
            .ok()
            .and_then(|o| usize::try_from(o).ok())
            .ok_or_else(|| Off::new(format!("{ty}.{name} is not reflected")))
    };
    let size = |ty: &str| {
        reflection
            .type_info(ty)
            .ok()
            .and_then(|info| usize::try_from(info.size()).ok())
            .ok_or_else(|| Off::new(format!("{ty} is not reflected")))
    };
    let items = ItemLayout::with_lookup(field, size)?;
    let workshops = WorkshopLayout::with_lookup(field, size)?;
    let archive = enshwick_gamedata::archive::Archive::open(game, name)
        .with_context(|| format!("{}: {name}.kfc does not read", game.display()))?;
    Ok(Vanilla::new(items, workshops, move |key| {
        archive.resource(&key)
    }))
}

/// Reads and checks the pack in `dir`, with the game's vanilla names when
/// `vanilla` is given: a summary, its manifest and files.
fn check(
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
    let summary = format!(
        "{} {}: {} content ids, {} loot drops; join hash {:08x}",
        manifest.id,
        manifest.version,
        pack.ids().len(),
        pack.drops().len(),
        files.hash()
    );
    Ok((summary, manifest, files))
}

fn schema(out: &Path) -> anyhow::Result<String> {
    std::fs::create_dir_all(out).with_context(|| out.display().to_string())?;
    let mut written = Vec::new();
    for (file, schema) in enshwick::packs::schemas() {
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

fn zip(dir: &Path, out: Option<&Path>) -> anyhow::Result<String> {
    let (summary, manifest, files) = check(dir, None)?;
    let out = out.map_or_else(
        || PathBuf::from(format!("{}.zip", manifest.id)),
        Path::to_path_buf,
    );
    let file = std::fs::File::create(&out).with_context(|| out.display().to_string())?;
    let mut archive = ::zip::ZipWriter::new(file);
    let options = ::zip::write::SimpleFileOptions::default()
        .compression_method(::zip::CompressionMethod::Deflated);
    for name in files.names() {
        let entry = format!("mods/{}/{name}", manifest.id);
        archive
            .start_file(entry.as_str(), options)
            .and_then(|()| {
                archive
                    .write_all(files.get(name).unwrap_or_default())
                    .map_err(Into::into)
            })
            .with_context(|| entry.clone())?;
    }
    archive
        .finish()
        .with_context(|| out.display().to_string())?;
    Ok(format!("{summary}\nwrote {}", out.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(name: &str, items: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("enshwick-pack-{name}-{}", std::process::id()));
        let _fresh = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(MANIFEST_FILE),
            "id = \"lanterns\"\nversion = \"1.0.0\"\n",
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
            summary.starts_with("lanterns 1.0.0: 1 content ids"),
            "{summary}"
        );
        let out = dir.join("out.zip");
        zip(&dir, Some(&out)).unwrap();
        let archive = ::zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
        let mut names: Vec<String> = archive
            .file_names()
            .map(|name| name.unwrap().into_owned())
            .collect();
        names.sort_unstable();
        assert_eq!(
            names,
            ["mods/lanterns/items.toml", "mods/lanterns/pack.toml"]
        );
        let _gone = std::fs::remove_dir_all(&dir);
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
    fn the_game_s_data_checks_vanilla_names() {
        let Ok(game) = std::env::var("ENSHWICK_CLIENT_DIR") else {
            return;
        };
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
