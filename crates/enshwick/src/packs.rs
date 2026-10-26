//! Content packs: items, recipes, workshops, loot and texts described in
//! TOML files instead of code.
//!
//! A pack is a set of files: `items.toml`, `recipes.toml`, `workshops.toml`,
//! `loot.toml` and `texts.toml`, each optional, and the PNG icons the items
//! name. [`Pack::parse`] reads and checks them against the types below and
//! reports every problem with its file, line and column; [`Load::add_pack`](crate::Load::add_pack)
//! adds what they describe through the same calls code uses, beside vanilla
//! content, never in its place. A mod embeds a pack with
//! [`PackFiles::with`]; a folder `mods/<id>/` with a `pack.toml`
//! ([`PackManifest`]) and no DLL is a pack the loader runs on its own.
//!
//! Every name a pack defines lives in its namespace: `lantern` in the pack
//! `you.lanterns` is the content id `you.lanterns:lantern`. A reference is a name of
//! the pack, a full content id of another mod (`you.lockers:locker`), or a vanilla
//! GUID (`6943053a-32c1-4718-ae22-e61427ddf54d`).
//!
//! # Examples
//!
//! ```
//! use enshwick::packs::{Pack, PackFiles};
//!
//! let files = PackFiles::new()
//!     .with("items.toml", r#"
//! [[item]]
//! id = "lantern"
//! like = "6943053a-32c1-4718-ae22-e61427ddf54d"
//! name = "lantern.name"
//! "#)
//!     .with("texts.toml", "[en]\n\"lantern.name\" = \"Lantern\"\n");
//! let pack = Pack::parse("you.lanterns", &files)?;
//! assert_eq!(pack.ids(), ["you.lanterns:lantern"]);
//! # Ok::<(), enshwick::packs::ParsePackError>(())
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::Off;
use crate::content::Guid;
use crate::content::icons::Tint;
use crate::content::items::StatFormat;
use crate::content::vanilla::Vanilla;
use crate::content::workshops::Station;
use crate::i18n::Texts;
use crate::loot::Drop;

/// The manifest file of a pack folder.
pub const MANIFEST_FILE: &str = "pack.toml";
/// The items file.
pub const ITEMS_FILE: &str = "items.toml";
/// The recipes file.
pub const RECIPES_FILE: &str = "recipes.toml";
/// The workshops file.
pub const WORKSHOPS_FILE: &str = "workshops.toml";
/// The loot file.
pub const LOOT_FILE: &str = "loot.toml";
/// The texts file, in the form of a mod's [texts](crate::i18n).
pub const TEXTS_FILE: &str = "texts.toml";
/// Every file a pack folder may hold besides its icons, in the order the
/// loader reads them.
pub const FILES: [&str; 6] = [
    MANIFEST_FILE,
    ITEMS_FILE,
    RECIPES_FILE,
    WORKSHOPS_FILE,
    LOOT_FILE,
    TEXTS_FILE,
];
/// The largest file a pack may hold: 4 MiB.
pub const MAX_FILE: u64 = 4 << 20;

/// One problem in a pack: where it is and what is wrong.
///
/// # Examples
///
/// ```
/// use enshwick::packs::{Pack, PackFiles};
///
/// let files = PackFiles::new().with("items.toml", "[[item]]\nid = \"lantern\"\n");
/// let errors = Pack::parse("you.lanterns", &files).unwrap_err();
/// assert_eq!(errors.list()[0].line, 1);
/// assert!(errors.to_string().starts_with("items.toml:1:1: "), "{errors}");
/// ```
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{file}:{line}:{column}: {message}")]
pub struct PackError {
    /// The file, as the pack names it, such as `items.toml`.
    pub file: String,
    /// The line, from 1.
    pub line: usize,
    /// The column, from 1.
    pub column: usize,
    /// What is wrong.
    pub message: String,
}

/// Every problem [`Pack::parse`] found, in file order.
///
/// # Examples
///
/// See [`PackError`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{}", self.lines())]
pub struct ParsePackError(Vec<PackError>);

impl ParsePackError {
    /// The problems.
    ///
    /// # Examples
    ///
    /// See [`PackError`].
    #[must_use]
    pub fn list(&self) -> &[PackError] {
        &self.0
    }

    fn lines(&self) -> String {
        self.0
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl From<ParsePackError> for Off {
    fn from(errors: ParsePackError) -> Self {
        Self::new(format!("content pack: {errors}"))
    }
}

/// Why [`PackFiles::read_dir`] could not read a pack's folder.
///
/// # Examples
///
/// ```
/// use enshwick::packs::ReadPackError;
///
/// let large = ReadPackError::TooLarge { file: "items.toml".into() };
/// assert_eq!(large.to_string(), "items.toml: larger than 4194304 bytes");
/// ```
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReadPackError {
    /// A file could not be read.
    #[error("{file}: {source}")]
    Io {
        /// The file, as the pack names it.
        file: String,
        /// Why.
        source: std::io::Error,
    },
    /// A file is larger than [`MAX_FILE`].
    #[error("{file}: larger than {MAX_FILE} bytes")]
    TooLarge {
        /// The file, as the pack names it.
        file: String,
    },
}

/// The files of a pack: their names, such as `items.toml` or
/// `icons/lantern.png`, and their bytes.
///
/// # Examples
///
/// ```
/// use enshwick::packs::PackFiles;
///
/// let files = PackFiles::new().with("texts.toml", "[en]\n");
/// assert_eq!(files.get("texts.toml"), Some(&b"[en]\n"[..]));
/// assert_ne!(files.hash(), PackFiles::new().hash());
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackFiles {
    files: BTreeMap<String, Vec<u8>>,
}

impl PackFiles {
    /// No files.
    ///
    /// # Examples
    ///
    /// See [`PackFiles`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The same with the file `name` holding `bytes`.
    ///
    /// # Examples
    ///
    /// See [`PackFiles`].
    #[must_use]
    pub fn with(mut self, name: &str, bytes: impl Into<Vec<u8>>) -> Self {
        self.files.insert(name.to_owned(), bytes.into());
        self
    }

    /// The pack files in the folder `dir`: the files of [`FILES`] it holds
    /// and every PNG under `dir/icons`, each at most [`MAX_FILE`] bytes.
    ///
    /// # Errors
    ///
    /// [`ReadPackError`] naming the file that could not be read, or is
    /// larger than [`MAX_FILE`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::packs::PackFiles;
    ///
    /// let files = PackFiles::read_dir(std::path::Path::new("no such folder"))?;
    /// assert!(files.names().next().is_none());
    /// # Ok::<(), enshwick::packs::ReadPackError>(())
    /// ```
    pub fn read_dir(dir: &Path) -> Result<Self, ReadPackError> {
        let mut files = Self::new();
        let read = |path: &Path, name: &str, files: &mut Self| -> Result<(), ReadPackError> {
            let io = |source| ReadPackError::Io {
                file: name.to_owned(),
                source,
            };
            let meta = std::fs::metadata(path).map_err(io)?;
            if meta.len() > MAX_FILE {
                return Err(ReadPackError::TooLarge {
                    file: name.to_owned(),
                });
            }
            let bytes = std::fs::read(path).map_err(io)?;
            files.files.insert(name.to_owned(), bytes);
            Ok(())
        };
        for name in FILES {
            let path = dir.join(name);
            if path.is_file() {
                read(&path, name, &mut files)?;
            }
        }
        if let Ok(entries) = std::fs::read_dir(dir.join("icons")) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                let png = path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("png"));
                if let (true, Some(file)) = (png, path.file_name().and_then(|f| f.to_str())) {
                    read(&path, &format!("icons/{file}"), &mut files)?;
                }
            }
        }
        Ok(files)
    }

    /// The bytes of the file `name`.
    ///
    /// # Examples
    ///
    /// See [`PackFiles`].
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.files.get(name).map(Vec::as_slice)
    }

    /// The names of the files, sorted.
    ///
    /// # Examples
    ///
    /// See [`PackFiles::read_dir`].
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }

    /// A hash of every file's name and bytes, the same for the same files:
    /// FNV-1a 32, as eight hex digits go into the join check.
    ///
    /// # Examples
    ///
    /// See [`PackFiles`].
    #[must_use]
    pub fn hash(&self) -> u32 {
        let mut hash: u32 = 0x811c_9dc5;
        let mut eat = |bytes: &[u8]| {
            for &b in bytes {
                hash ^= u32::from(b);
                hash = hash.wrapping_mul(0x0100_0193);
            }
        };
        for (name, bytes) in &self.files {
            eat(name.as_bytes());
            eat(&[0]);
            eat(&u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
            eat(bytes);
        }
        hash
    }

    fn text(&self, name: &str) -> Result<Option<&str>, PackError> {
        self.get(name)
            .map(|bytes| {
                core::str::from_utf8(bytes)
                    .map_err(|e| PackError::at(name, "", 0, &format!("not UTF-8: {e}")))
            })
            .transpose()
    }
}

/// Where a pack runs, in `pack.toml`.
///
/// # Examples
///
/// See [`PackManifest`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum PackSide {
    /// The server and the clients: the default, as both read content.
    #[default]
    Both,
    /// The dedicated server and hosting clients only, such as a pack of
    /// loot alone.
    Server,
    /// Clients only.
    Client,
}

/// A pack folder's `pack.toml`: what the loader reads before it runs the
/// pack, as a mod's [`manifest!`](crate::manifest) says it for a DLL.
///
/// # Examples
///
/// ```
/// use enshwick::packs::{PackManifest, PackSide};
///
/// let manifest = PackManifest::parse(r#"
/// id = "you.lanterns"
/// version = "1.0.0"
/// name = "Lanterns"
///
/// [requires]
/// "you.trading-post" = ">=0.5, <1.0"
/// "#)?;
/// assert_eq!(manifest.id, "you.lanterns");
/// assert_eq!(manifest.side, PackSide::Both);
/// assert!(manifest.join);
/// assert_eq!(manifest.requires["you.trading-post"], ">=0.5, <1.0");
/// # Ok::<(), enshwick::packs::PackError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct PackManifest {
    /// The pack id, `<author>.<name>` like a mod's; it names the folder, the
    /// namespace of the pack's content and its texts override file.
    pub id: String,
    /// Its SemVer version.
    pub version: String,
    /// The name players read; the id when empty.
    #[serde(default)]
    pub name: String,
    /// One sentence on what it adds.
    #[serde(default)]
    pub description: String,
    /// The authors' names.
    #[serde(default)]
    pub authors: Vec<String>,
    /// An SPDX licence expression.
    #[serde(default)]
    pub license: String,
    /// Where players read about it, an `https://` address.
    #[serde(default)]
    pub homepage: String,
    /// Where it runs.
    #[serde(default)]
    pub side: PackSide,
    /// Whether server and clients must run the same pack, files and all, to
    /// play together; on unless the pack says otherwise.
    #[serde(default = "yes")]
    pub join: bool,
    /// Mods and packs it needs, by id, each with a Cargo-style version
    /// requirement (empty for any).
    #[serde(default)]
    pub requires: BTreeMap<String, String>,
}

const fn yes() -> bool {
    true
}

impl PackManifest {
    /// Reads a `pack.toml`.
    ///
    /// # Errors
    ///
    /// The first problem, with its line and column.
    ///
    /// # Examples
    ///
    /// See [`PackManifest`].
    pub fn parse(text: &str) -> Result<Self, PackError> {
        let manifest: Self =
            toml::from_str(text).map_err(|e| PackError::of(MANIFEST_FILE, text, &e))?;
        if !crate::abi::is_valid_id(&manifest.id) {
            return Err(PackError::at(
                MANIFEST_FILE,
                text,
                find(text, "id"),
                "id is <author>.<name>, each of [a-z0-9-]",
            ));
        }
        Ok(manifest)
    }
}

/// The byte offset of the line that sets `key`, for an error about it.
fn find(text: &str, key: &str) -> usize {
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if line.trim_start().starts_with(key) {
            return at + line.len() - line.trim_start().len();
        }
        at += line.len();
    }
    0
}

impl PackError {
    /// The error at byte `offset` of `text`, the contents of `file`.
    fn at(file: &str, text: &str, offset: usize, message: &str) -> Self {
        let before = text.get(..offset.min(text.len())).unwrap_or("");
        let line = before.matches('\n').count() + 1;
        let column = before
            .rsplit_once('\n')
            .map_or(before, |(_, last)| last)
            .chars()
            .count()
            + 1;
        Self {
            file: file.to_owned(),
            line,
            column,
            message: message.to_owned(),
        }
    }

    /// A TOML error of `file`.
    fn of(file: &str, text: &str, error: &toml::de::Error) -> Self {
        let offset = error.span().map_or(0, |span| span.start);
        Self::at(file, text, offset, error.message().trim_end())
    }
}

mod add;
mod entries;
mod read;
#[cfg(test)]
mod tests;

#[cfg(feature = "schema")]
pub use entries::schemas;
use entries::{ItemsFile, LootFile, RecipesFile, WorkshopsFile};
use read::{Reader, drops, items, recipes, workshops};

/// A reference in a pack, resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ref {
    /// A name the pack defines, as its full id.
    Own,
    /// Another mod's content id, or a vanilla GUID.
    Elsewhere,
}

/// A pack item, ready to add.
#[derive(Clone, Debug, PartialEq)]
struct PackItem {
    id: String,
    like: Guid,
    name: Option<String>,
    description: Option<String>,
    debug_name: String,
    meter: Option<(u16, u16)>,
    stats: Vec<(String, i32, StatFormat)>,
    icon: Option<PackIcon>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PackIcon {
    Tint { like: Guid, tint: Tint },
    Png(Vec<u8>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PackRecipe {
    id: String,
    like: Guid,
    output: Guid,
    count: u32,
    cost: u32,
    workshop: Option<Guid>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PackWorkshop {
    id: String,
    like: Station,
    name: Option<String>,
    description: Option<String>,
    cost: Option<u32>,
    debug_name: String,
}

/// A pack, read and checked: what [`Load::add_pack`](crate::Load::add_pack) adds.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Debug, PartialEq)]
pub struct Pack {
    namespace: String,
    items: Vec<PackItem>,
    recipes: Vec<PackRecipe>,
    workshops: Vec<PackWorkshop>,
    drops: Vec<Drop>,
    texts: String,
    hash: u32,
}

impl Pack {
    /// Reads and checks the pack `files` in the namespace `namespace`, the
    /// id of the mod or pack that adds it, with the running game's vanilla
    /// names ([`Vanilla::game`]) when there is a game.
    ///
    /// # Errors
    ///
    /// Every problem found, each with its file, line and column: a file
    /// that is no TOML of its form, an unknown field, a name defined twice,
    /// a reference to a name neither the pack nor the game has, or that
    /// several vanilla items carry, a station that cannot be cloned, a text
    /// key without text, an icon the pack lacks, a chance outside 0 to 1.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn parse(namespace: &str, files: &PackFiles) -> Result<Self, ParsePackError> {
        Self::parse_with(namespace, files, Vanilla::game())
    }

    /// Reads and checks a pack as [`Pack::parse`] does, with the vanilla
    /// names of `vanilla`; without it, a pack names vanilla content by GUID
    /// only and clones only the workbench.
    ///
    /// # Errors
    ///
    /// As [`Pack::parse`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::packs::{Pack, PackFiles};
    ///
    /// let files = PackFiles::new().with("items.toml", "[[item]]\nid = \"torch\"\nlike = \"Torch\"\n");
    /// let errors = Pack::parse_with("lights", &files, None).err().ok_or("parsed")?;
    /// assert!(errors.to_string().contains("items.toml:1"));
    /// # Ok::<(), &str>(())
    /// ```
    pub fn parse_with(
        namespace: &str,
        files: &PackFiles,
        vanilla: Option<&Vanilla>,
    ) -> Result<Self, ParsePackError> {
        let mut reader = Reader {
            namespace,
            files,
            errors: Vec::new(),
            defined: BTreeMap::new(),
            vanilla,
        };
        let texts_text = match files.text(TEXTS_FILE) {
            Ok(text) => text.unwrap_or("").to_owned(),
            Err(error) => {
                reader.errors.push(error);
                String::new()
            }
        };
        let texts = match toml::from_str::<BTreeMap<String, BTreeMap<String, String>>>(&texts_text)
        {
            Err(error) => {
                reader
                    .errors
                    .push(PackError::of(TEXTS_FILE, &texts_text, &error));
                None
            }
            Ok(_) => match Texts::parse(&texts_text) {
                Ok(texts) => Some(texts),
                Err(off) => {
                    reader.error(TEXTS_FILE, &texts_text, 0, off.to_string());
                    None
                }
            },
        };
        let (items_file, items_text): (ItemsFile, String) = reader.file(ITEMS_FILE);
        let (workshops_file, workshops_text): (WorkshopsFile, String) = reader.file(WORKSHOPS_FILE);
        let (recipes_file, recipes_text): (RecipesFile, String) = reader.file(RECIPES_FILE);
        let (loot_file, loot_text): (LootFile, String) = reader.file(LOOT_FILE);
        for entry in &items_file.item {
            let at = entry.span().start;
            reader.define(ITEMS_FILE, &items_text, at, &entry.get_ref().id, "item");
        }
        for entry in &workshops_file.workshop {
            let at = entry.span().start;
            reader.define(
                WORKSHOPS_FILE,
                &workshops_text,
                at,
                &entry.get_ref().id,
                "workshop",
            );
        }
        for entry in &recipes_file.recipe {
            let at = entry.span().start;
            reader.define(
                RECIPES_FILE,
                &recipes_text,
                at,
                &entry.get_ref().id,
                "recipe",
            );
        }
        let mut pack = Self {
            namespace: namespace.to_owned(),
            items: Vec::new(),
            recipes: Vec::new(),
            workshops: Vec::new(),
            drops: Vec::new(),
            texts: texts_text,
            hash: files.hash(),
        };
        pack.items = items(&mut reader, &items_file, &items_text, texts.as_ref());
        pack.workshops = workshops(
            &mut reader,
            &workshops_file,
            &workshops_text,
            texts.as_ref(),
        );
        pack.recipes = recipes(&mut reader, &recipes_file, &recipes_text);
        pack.drops = drops(&mut reader, &loot_file, &loot_text, &pack.items);
        if reader.errors.is_empty() {
            Ok(pack)
        } else {
            Err(ParsePackError(reader.errors))
        }
    }

    /// The content ids the pack defines, in the order it adds them.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn ids(&self) -> Vec<&str> {
        self.workshops
            .iter()
            .map(|w| w.id.as_str())
            .chain(self.items.iter().map(|i| i.id.as_str()))
            .chain(self.recipes.iter().map(|r| r.id.as_str()))
            .collect()
    }

    /// The pack's loot drops.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::packs::{Pack, PackFiles};
    ///
    /// let files = PackFiles::new().with("loot.toml", r#"
    /// [[drop]]
    /// items = ["you.lockers:locker"]
    /// chance = 0.1
    /// from = "bosses"
    /// "#);
    /// let pack = Pack::parse("you.lanterns", &files)?;
    /// assert_eq!(pack.drops().len(), 1);
    /// # Ok::<(), enshwick::packs::ParsePackError>(())
    /// ```
    #[must_use]
    pub fn drops(&self) -> &[Drop] {
        &self.drops
    }

    /// Whether `other` adds the same content as this pack, its loot aside:
    /// what a running game can take without a restart.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::packs::{Pack, PackFiles};
    ///
    /// let loot = |chance: &str| PackFiles::new().with("loot.toml", format!(
    ///     "[[drop]]\nitems = [\"you.lockers:locker\"]\nchance = {chance}\nfrom = \"bosses\"\n"
    /// ));
    /// let before = Pack::parse("you.lanterns", &loot("0.1"))?;
    /// let after = Pack::parse("you.lanterns", &loot("0.5"))?;
    /// assert!(before.same_content(&after));
    /// # Ok::<(), enshwick::packs::ParsePackError>(())
    /// ```
    #[must_use]
    pub fn same_content(&self, other: &Self) -> bool {
        self.namespace == other.namespace
            && self.items == other.items
            && self.recipes == other.recipes
            && self.workshops == other.workshops
            && self.texts == other.texts
    }

    /// The hash of the files the pack was read from, as
    /// [`PackFiles::hash`] gives it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::packs::{Pack, PackFiles};
    ///
    /// let files = PackFiles::new();
    /// assert_eq!(Pack::parse("empty", &files)?.hash(), files.hash());
    /// # Ok::<(), enshwick::packs::ParsePackError>(())
    /// ```
    #[must_use]
    pub fn hash(&self) -> u32 {
        self.hash
    }
}

/// The group of a pack's drops, which a reload replaces.
#[must_use]
pub(crate) fn group(namespace: &str) -> u64 {
    namespace
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, b| {
            (hash ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
        })
        | 1
}

/// Replaces the drops of the pack `namespace` with `drops`, as the loader
/// does when a pack's `loot.toml` changes.
///
/// # Errors
///
/// Why they take a restart instead.
#[doc(hidden)]
pub fn reload_loot(namespace: &str, drops: Vec<Drop>) -> Result<(), String> {
    crate::loot::replace_group(group(namespace), drops)
}
