//! Content packs: items, recipes, workshops, loot and texts described in
//! TOML files instead of code.
//!
//! A pack is a set of files: `items.toml`, `recipes.toml`, `workshops.toml`,
//! `loot.toml` and `texts.toml`, each optional, and the PNG icons the items
//! name. [`Pack::parse`] reads and checks them against the types below and
//! reports every problem with its file, line and column; [`Load::add_pack`]
//! adds what they describe through the same calls code uses, beside vanilla
//! content, never in its place. A mod embeds a pack with
//! [`PackFiles::with`]; a folder `mods/<id>/` with a `pack.toml`
//! ([`PackManifest`]) and no DLL is a pack the loader runs on its own.
//!
//! Every name a pack defines lives in its namespace: `lantern` in the pack
//! `lanterns` is the content id `lanterns:lantern`. A reference is a name of
//! the pack, a full content id of another mod (`safes:safe`), or a vanilla
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
//! let pack = Pack::parse("lanterns", &files)?;
//! assert_eq!(pack.ids(), ["lanterns:lantern"]);
//! # Ok::<(), enshwick::packs::ParsePackError>(())
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;
use toml::Spanned;

use crate::content::icons::{Icon, Pixels, Tint};
use crate::content::items::{Meter, NewItem, Stat, StatFormat};
use crate::content::recipes::NewRecipe;
use crate::content::vanilla::Vanilla;
use crate::content::workshops::{NewWorkshop, Station, StationRecipe, WorkshopId, stations};
use crate::content::{Guid, Id};
use crate::i18n::Texts;
use crate::loot::{Drop, Kinds, Source};
use crate::{Load, Off};

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
/// let errors = Pack::parse("lanterns", &files).unwrap_err();
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
/// id = "lanterns"
/// version = "1.0.0"
/// name = "Lanterns"
///
/// [requires]
/// safes = ">=0.5, <1.0"
/// "#)?;
/// assert_eq!(manifest.id, "lanterns");
/// assert_eq!(manifest.side, PackSide::Both);
/// assert!(manifest.join);
/// assert_eq!(manifest.requires["safes"], ">=0.5, <1.0");
/// # Ok::<(), enshwick::packs::PackError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct PackManifest {
    /// The pack id, 1 to 64 of `[a-z0-9-]`; it names the folder, the
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
        let id_ok = (1..=64).contains(&manifest.id.len())
            && manifest
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if !id_ok {
            return Err(PackError::at(
                MANIFEST_FILE,
                text,
                find(text, "id"),
                "id is 1 to 64 of [a-z0-9-]",
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

/// The number format of a tooltip line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
enum FormatEntry {
    #[default]
    Number,
    Signed,
    Percent,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
struct StatEntry {
    /// The text key of the line's label.
    label: String,
    value: i32,
    #[serde(default)]
    format: FormatEntry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
struct MeterEntry {
    /// The most it holds, 1 or more.
    max: u16,
    /// What a new item holds; `max` when left out.
    start: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
struct IconEntry {
    /// Recolour the vanilla icon toward this colour, red, green, blue.
    tint: Option<[u8; 3]>,
    /// How far toward the colour, in percent; 50 when left out.
    amount: Option<u8>,
    /// The vanilla icon to recolour; the item's `like` when left out.
    like: Option<String>,
    /// A PNG of the pack's own under `icons/`, such as `icons/lantern.png`.
    png: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
struct ItemEntry {
    /// The item's name in the pack.
    id: String,
    /// The item it is cloned from: a vanilla GUID or a content id.
    like: String,
    /// The text key of its name.
    name: Option<String>,
    /// The text key of its description.
    description: Option<String>,
    /// Its name in the registry's debug list.
    debug_name: Option<String>,
    meter: Option<MeterEntry>,
    #[serde(default)]
    stats: Vec<StatEntry>,
    icon: Option<IconEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
struct RecipeEntry {
    /// The recipe's name in the pack.
    id: String,
    /// The item whose recipe is copied.
    like: String,
    /// The item it makes.
    output: String,
    /// How many it makes; 1 when left out.
    #[serde(default = "one")]
    count: u32,
    /// The copied ingredient counts times this; 1 when left out.
    #[serde(default = "one")]
    cost: u32,
    /// Where it is made: a workshop of the pack, or a workshop's GUID or
    /// content id; the copied recipe's when left out.
    workshop: Option<String>,
}

const fn one() -> u32 {
    1
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
struct StationRecipeEntry {
    /// The vanilla station's ingredient counts times this.
    cost: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
struct WorkshopEntry {
    /// The station's name in the pack; it also names its item.
    id: String,
    /// The vanilla station cloned: `workbench`.
    like: String,
    /// The text key of the station's name, which also names the workshop.
    name: Option<String>,
    /// The text key of the station item's description.
    description: Option<String>,
    /// How the station is crafted; not at all when left out.
    recipe: Option<StationRecipeEntry>,
    /// The item's name in the registry's debug list.
    debug_name: Option<String>,
}

/// Where a drop comes from: one name or several.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
enum FromEntry {
    One(String),
    Many(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
struct DropEntry {
    /// The items, one picked per drop.
    items: Vec<String>,
    /// The chance per roll, from 0 to 1.
    chance: f64,
    /// `bosses`, `locked-chests`, `corpses`, `chests`, `containers`,
    /// `destroyed`, or a list of the last four.
    from: FromEntry,
    /// How many per drop, at least and at most; 1 when left out.
    count: Option<[u16; 2]>,
    /// How full each dropped item's meter starts, at least and at most.
    meter: Option<[u16; 2]>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemsFile {
    #[serde(default)]
    item: Vec<Spanned<ItemEntry>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecipesFile {
    #[serde(default)]
    recipe: Vec<Spanned<RecipeEntry>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkshopsFile {
    #[serde(default)]
    workshop: Vec<Spanned<WorkshopEntry>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct LootFile {
    #[serde(default)]
    drop: Vec<Spanned<DropEntry>>,
}

/// The JSON Schemas of a pack's files, for editors: `(file, schema)`.
#[cfg(feature = "schema")]
#[must_use]
pub fn schemas() -> Vec<(&'static str, String)> {
    #[derive(schemars::JsonSchema)]
    #[schemars(deny_unknown_fields)]
    #[expect(dead_code, reason = "only its schema is read")]
    struct Items {
        item: Option<Vec<ItemEntry>>,
    }
    #[derive(schemars::JsonSchema)]
    #[schemars(deny_unknown_fields)]
    #[expect(dead_code, reason = "only its schema is read")]
    struct Recipes {
        recipe: Option<Vec<RecipeEntry>>,
    }
    #[derive(schemars::JsonSchema)]
    #[schemars(deny_unknown_fields)]
    #[expect(dead_code, reason = "only its schema is read")]
    struct Workshops {
        workshop: Option<Vec<WorkshopEntry>>,
    }
    #[derive(schemars::JsonSchema)]
    #[schemars(deny_unknown_fields)]
    #[expect(dead_code, reason = "only its schema is read")]
    struct Loot {
        drop: Option<Vec<DropEntry>>,
    }
    let json = |schema: schemars::Schema| serde_json::to_string_pretty(&schema).unwrap_or_default();
    vec![
        (MANIFEST_FILE, json(schemars::schema_for!(PackManifest))),
        (ITEMS_FILE, json(schemars::schema_for!(Items))),
        (RECIPES_FILE, json(schemars::schema_for!(Recipes))),
        (WORKSHOPS_FILE, json(schemars::schema_for!(Workshops))),
        (LOOT_FILE, json(schemars::schema_for!(Loot))),
    ]
}

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

/// A pack, read and checked: what [`Load::add_pack`] adds.
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

/// Collects a pack's problems as it is read.
struct Reader<'a> {
    namespace: &'a str,
    files: &'a PackFiles,
    errors: Vec<PackError>,
    /// Names the pack defines, with what each is.
    defined: BTreeMap<String, &'static str>,
    /// The game's content, for vanilla names.
    vanilla: Option<&'a Vanilla>,
}

impl Reader<'_> {
    /// The parsed `name`, or `T::default` when the pack lacks it; problems
    /// are noted.
    fn file<T: for<'de> Deserialize<'de> + Default>(&mut self, name: &str) -> (T, String) {
        let text = match self.files.text(name) {
            Ok(Some(text)) => text.to_owned(),
            Ok(None) => return (T::default(), String::new()),
            Err(error) => {
                self.errors.push(error);
                return (T::default(), String::new());
            }
        };
        match toml::from_str(&text) {
            Ok(parsed) => (parsed, text),
            Err(error) => {
                self.errors.push(PackError::of(name, &text, &error));
                (T::default(), text)
            }
        }
    }

    fn error(&mut self, file: &str, text: &str, at: usize, message: impl AsRef<str>) {
        self.errors
            .push(PackError::at(file, text, at, message.as_ref()));
    }

    /// The full id of the pack's own `name`: `<namespace>:<name>`.
    fn own(&self, name: &str) -> String {
        format!("{}:{name}", self.namespace)
    }

    /// Notes that the pack defines `name` as a `what`; a problem when it
    /// is no valid name or is defined already.
    fn define(&mut self, file: &str, text: &str, at: usize, name: &str, what: &'static str) {
        let full = self.own(name);
        if name.contains(':') || Guid::try_named(&full).is_none() {
            self.error(
                file,
                text,
                at,
                format!("{what} id {name:?} is no name: 1 to 128 of [a-z0-9_./-]"),
            );
            return;
        }
        if let Some(earlier) = self.defined.insert(full, what) {
            self.error(
                file,
                text,
                at,
                format!("{what} id {name:?} is taken by a {earlier} already"),
            );
        }
    }

    /// The GUID `reference` names, and whether the pack defines it; a
    /// pack name must be a `what` of the pack.
    fn reference(
        &mut self,
        file: &str,
        text: &str,
        at: usize,
        reference: &str,
        what: &'static str,
        suffix: &str,
    ) -> Option<(Guid, Ref)> {
        if let Some(guid) = Guid::try_parse(reference) {
            return Some((guid, Ref::Elsewhere));
        }
        if reference.contains(':') {
            let guid = Guid::try_named(reference);
            if guid.is_none() {
                self.error(
                    file,
                    text,
                    at,
                    format!("{reference:?} is no content id: <mod>:<name>"),
                );
            }
            return guid.map(|g| (g, Ref::Elsewhere));
        }
        let full = self.own(reference);
        if self.defined.get(&full) == Some(&what) {
            return Guid::try_named(&format!("{full}{suffix}")).map(|g| (g, Ref::Own));
        }
        let found = match self.vanilla {
            None => Err(format!(
                "no {what} {reference:?} in this pack; name another mod's as <mod>:<name>, or a vanilla one by its GUID or, where the game's data is at hand, its debug name"
            )),
            Some(vanilla) => vanilla
                .item(reference)
                .and_then(|item| match what {
                    "workshop" => vanilla.workshop(item).map(WorkshopId::guid),
                    _ => Ok(item),
                })
                .map_err(|why| format!("no {what} {reference:?} in this pack, and {why}")),
        };
        match found {
            Ok(guid) => Some((guid, Ref::Elsewhere)),
            Err(why) => {
                self.error(file, text, at, why);
                None
            }
        }
    }

    /// The vanilla station `like` names: `workbench`, a station item's debug
    /// name or its GUID.
    fn station(
        &mut self,
        file: &str,
        text: &str,
        at: usize,
        id: &str,
        like: &str,
    ) -> Option<Station> {
        if like == "workbench" {
            return Some(stations::WORKBENCH);
        }
        let found = match (self.vanilla, Guid::try_parse(like)) {
            (_, Some(item)) if item == stations::WORKBENCH.item => Ok(stations::WORKBENCH),
            (None, _) => Err(format!(
                "like {like:?}: a station other than the workbench needs the game's data at hand"
            )),
            (Some(vanilla), Some(item)) => vanilla.station(item).map_err(|why| why.to_string()),
            (Some(vanilla), None) => vanilla
                .item(like)
                .and_then(|item| vanilla.station(item))
                .map_err(|why| why.to_string()),
        };
        found
            .map_err(|why| self.error(file, text, at, format!("workshop {id}: {why}")))
            .ok()
    }

    /// A text key the pack's texts must hold.
    fn text_key(
        &mut self,
        file: &str,
        text: &str,
        at: usize,
        key: &str,
        texts: Option<&Texts>,
    ) -> Option<String> {
        let tag = self.own(&format!("text/{key}"));
        if Guid::try_named(&tag).is_none() {
            self.error(
                file,
                text,
                at,
                format!("text key {key:?} is no name: 1 to 123 of [a-z0-9_./-]"),
            );
            return None;
        }
        if texts.is_some_and(|t| t.translations(key).next().is_none()) {
            self.error(
                file,
                text,
                at,
                format!("text key {key:?} has no text in {TEXTS_FILE}"),
            );
        }
        Some(key.to_owned())
    }
}

/// The kinds and filters of a drop's `from`.
fn source_of(from: &FromEntry) -> Result<Source, String> {
    let kind = |name: &str| match name {
        "corpses" => Some(Kinds::CORPSES),
        "chests" => Some(Kinds::CHESTS),
        "containers" => Some(Kinds::CONTAINERS),
        "destroyed" => Some(Kinds::DESTROYED),
        _ => None,
    };
    let unknown = |name: &str| {
        format!("from {name:?}: bosses, locked-chests, corpses, chests, containers or destroyed")
    };
    match from {
        FromEntry::One(name) if name == "bosses" => Ok(Source::bosses()),
        FromEntry::One(name) if name == "locked-chests" => Ok(Source::locked_chests()),
        FromEntry::One(name) => kind(name).map(Source::kinds).ok_or_else(|| unknown(name)),
        FromEntry::Many(names) if names.is_empty() => Err("from names no kind".to_owned()),
        FromEntry::Many(names) => names
            .iter()
            .try_fold(Source::kinds(Kinds::default()), |s, n| {
                kind(n)
                    .map(|k| Source::kinds(s.kinds.with(k)))
                    .ok_or_else(|| unknown(n))
            }),
    }
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
    /// items = ["safes:safe"]
    /// chance = 0.1
    /// from = "bosses"
    /// "#);
    /// let pack = Pack::parse("lanterns", &files)?;
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
    ///     "[[drop]]\nitems = [\"safes:safe\"]\nchance = {chance}\nfrom = \"bosses\"\n"
    /// ));
    /// let before = Pack::parse("lanterns", &loot("0.1"))?;
    /// let after = Pack::parse("lanterns", &loot("0.5"))?;
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

/// The checked items of `items_file`.
fn items(
    reader: &mut Reader<'_>,
    items_file: &ItemsFile,
    items_text: &str,
    texts: Option<&Texts>,
) -> Vec<PackItem> {
    let files = reader.files;
    let mut out = Vec::new();
    let file = ITEMS_FILE;
    let text = items_text;
    for entry in &items_file.item {
        let at = entry.span().start;
        let e = entry.get_ref();
        let like = reader.reference(file, text, at, &e.like, "item", "");
        let name = e
            .name
            .as_deref()
            .and_then(|k| reader.text_key(file, text, at, k, texts));
        let description = e
            .description
            .as_deref()
            .and_then(|k| reader.text_key(file, text, at, k, texts));
        let meter = e.meter.map(|m| (m.max, m.start.unwrap_or(m.max)));
        if let Some((max, start)) = meter
            && (max == 0 || start > max)
        {
            reader.error(
                file,
                text,
                at,
                format!(
                    "item {}: a meter holds 1 or more and starts at most full",
                    e.id
                ),
            );
        }
        let mut stats = Vec::new();
        for stat in &e.stats {
            if let Some(label) = reader.text_key(file, text, at, &stat.label, texts) {
                let format = match stat.format {
                    FormatEntry::Number => StatFormat::Number,
                    FormatEntry::Signed => StatFormat::Signed,
                    FormatEntry::Percent => StatFormat::Percent,
                };
                stats.push((label, stat.value, format));
            }
        }
        let icon = e
            .icon
            .as_ref()
            .and_then(|icon| match (&icon.tint, &icon.png) {
                (Some(colour), None) => {
                    let like = match &icon.like {
                        Some(r) => reader.reference(file, text, at, r, "item", "")?.0,
                        None => like?.0,
                    };
                    let amount = icon.amount.unwrap_or(50).min(100);
                    Some(PackIcon::Tint {
                        like,
                        tint: Tint {
                            colour: *colour,
                            amount,
                        },
                    })
                }
                (None, Some(png)) => match files.get(png) {
                    Some(bytes) if png.starts_with("icons/") => Some(PackIcon::Png(bytes.to_vec())),
                    _ => {
                        reader.error(
                            file,
                            text,
                            at,
                            format!("item {}: no icon {png:?} under icons/", e.id),
                        );
                        None
                    }
                },
                _ => {
                    reader.error(
                        file,
                        text,
                        at,
                        format!("item {}: an icon has either tint or png", e.id),
                    );
                    None
                }
            });
        if let Some((like, _)) = like {
            out.push(PackItem {
                id: reader.own(&e.id),
                like,
                name,
                description,
                debug_name: e.debug_name.clone().unwrap_or_else(|| reader.own(&e.id)),
                meter,
                stats,
                icon,
            });
        }
    }
    out
}

/// The checked workshops of `workshops_file`.
fn workshops(
    reader: &mut Reader<'_>,
    workshops_file: &WorkshopsFile,
    workshops_text: &str,
    texts: Option<&Texts>,
) -> Vec<PackWorkshop> {
    let mut out = Vec::new();
    let file = WORKSHOPS_FILE;
    let text = workshops_text;
    for entry in &workshops_file.workshop {
        let at = entry.span().start;
        let e = entry.get_ref();
        let like = reader.station(file, text, at, &e.id, &e.like);
        let name = e
            .name
            .as_deref()
            .and_then(|k| reader.text_key(file, text, at, k, texts));
        let description = e
            .description
            .as_deref()
            .and_then(|k| reader.text_key(file, text, at, k, texts));
        if let Some(like) = like {
            out.push(PackWorkshop {
                id: reader.own(&e.id),
                like,
                name,
                description,
                cost: e.recipe.map(|r| r.cost),
                debug_name: e.debug_name.clone().unwrap_or_else(|| reader.own(&e.id)),
            });
        }
    }
    out
}

/// The checked recipes of `recipes_file`.
fn recipes(
    reader: &mut Reader<'_>,
    recipes_file: &RecipesFile,
    recipes_text: &str,
) -> Vec<PackRecipe> {
    let mut out = Vec::new();
    let file = RECIPES_FILE;
    let text = recipes_text;
    for entry in &recipes_file.recipe {
        let at = entry.span().start;
        let e = entry.get_ref();
        let like = reader.reference(file, text, at, &e.like, "item", "");
        let output = reader.reference(file, text, at, &e.output, "item", "");
        let workshop = e
            .workshop
            .as_deref()
            .map(|w| reader.reference(file, text, at, w, "workshop", ".workshop"));
        if e.count == 0 || e.cost == 0 {
            reader.error(
                file,
                text,
                at,
                format!("recipe {}: count and cost are 1 or more", e.id),
            );
        }
        let workshop = match workshop {
            None => Some(None),
            Some(Some((guid, _))) => Some(Some(guid)),
            Some(None) => None,
        };
        if let (Some((like, _)), Some((output, _)), Some(workshop)) = (like, output, workshop) {
            out.push(PackRecipe {
                id: reader.own(&e.id),
                like,
                output,
                count: e.count,
                cost: e.cost,
                workshop,
            });
        }
    }
    out
}
/// The checked drops of `loot`.
fn drops(reader: &mut Reader<'_>, loot: &LootFile, text: &str, items: &[PackItem]) -> Vec<Drop> {
    let file = LOOT_FILE;
    let mut drops = Vec::new();
    for entry in &loot.drop {
        let at = entry.span().start;
        let e = entry.get_ref();
        let mut ids = Vec::new();
        let mut meter_of = Vec::new();
        for item in &e.items {
            if let Some((guid, kind)) = reader.reference(file, text, at, item, "item", "") {
                ids.push(guid.id());
                let own = (kind == Ref::Own)
                    .then(|| items.iter().find(|i| Guid::try_named(&i.id) == Some(guid)))
                    .flatten();
                meter_of.push(own.and_then(|i| i.meter).map(|(max, _)| max));
            }
        }
        if e.items.is_empty() {
            reader.error(file, text, at, "a drop names no item");
        }
        if !(0.0..=1.0).contains(&e.chance) {
            reader.error(
                file,
                text,
                at,
                format!("chance {} is not between 0 and 1", e.chance),
            );
        }
        let from = match source_of(&e.from) {
            Ok(from) => from,
            Err(why) => {
                reader.error(file, text, at, why);
                continue;
            }
        };
        let [min, max] = e.count.unwrap_or([1, 1]);
        if min == 0 || min > max {
            reader.error(file, text, at, format!("count {min}..{max} is empty"));
        }
        let mut drop = Drop::one_of(&ids, e.chance, from).count(min, max);
        if let Some([low, high]) = e.meter {
            match meter_of.first().copied().flatten() {
                Some(of) if meter_of.iter().all(|m| *m == Some(of)) && low <= high && high <= of => {
                    drop = drop.meter(low, high, of);
                }
                _ => reader.error(
                    file,
                    text,
                    at,
                    format!("meter {low}..{high}: every item must be one of the pack's with a meter of the same maximum, at least {high}"),
                ),
            }
        }
        if ids.len() == e.items.len() {
            drops.push(drop);
        }
    }
    drops
}

/// A content id the pack built at run time; ids live for the process.
fn id(full: &str) -> Result<Id, Off> {
    let name: &'static str = Box::leak(full.to_owned().into_boxed_str());
    Id::try_new(name).ok_or_else(|| Off::new(format!("content pack: {full:?} is no content id")))
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

impl Load<'_> {
    /// Adds what `pack` describes, on the server and the client alike:
    /// its workshops, items, recipes, texts and loot, each through the call
    /// code uses, beside vanilla content.
    ///
    /// # Errors
    ///
    /// [`Off`] as the calls it makes return it, such as for a content id
    /// the loader refuses.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::packs::{Pack, PackFiles};
    ///
    /// const ITEMS: &str = r#"
    /// [[item]]
    /// id = "lantern"
    /// like = "6943053a-32c1-4718-ae22-e61427ddf54d"
    /// "#;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let files = PackFiles::new().with("items.toml", ITEMS);
    ///     load.add_pack(&Pack::parse("my-mod", &files)?)
    /// }
    /// ```
    pub fn add_pack(&mut self, pack: &Pack) -> Result<(), Off> {
        let texts = self.texts(&pack.texts)?;
        let tag = |load: &mut Self, key: &Option<String>| -> Result<Option<Guid>, Off> {
            key.as_ref()
                .map(|key| {
                    let tag = id(&format!("{}:text/{key}", pack.namespace))?;
                    load.add_texts(&texts, &[(key, tag)])?;
                    Ok(tag.guid())
                })
                .transpose()
        };
        for workshop in &pack.workshops {
            let base = &workshop.id;
            let name = tag(self, &workshop.name)?;
            let description = tag(self, &workshop.description)?;
            let station_recipe = workshop
                .cost
                .map(|cost| -> Result<StationRecipe, Off> {
                    Ok(StationRecipe {
                        id: id(&format!("{base}.recipe"))?,
                        cost,
                    })
                })
                .transpose()?;
            self.add_workshop(NewWorkshop {
                like: workshop.like,
                item: id(base)?,
                template: id(&format!("{base}.template"))?,
                workshop: id(&format!("{base}.workshop"))?,
                name,
                description,
                station_recipe,
                debug_name: workshop.debug_name.clone(),
            })?;
        }
        for item in &pack.items {
            let own = id(&item.id)?;
            let name = tag(self, &item.name)?;
            let description = tag(self, &item.description)?;
            let mut stats = Vec::new();
            for (label, value, format) in &item.stats {
                let label = tag(self, &Some(label.clone()))?.unwrap_or(Guid([0; 16]));
                stats.push(Stat {
                    label,
                    value: *value,
                    format: *format,
                });
            }
            let meter = item
                .meter
                .map(|(max, start)| -> Result<Meter, Off> {
                    Ok(Meter {
                        template: id(&format!("{}.meter", item.id))?,
                        max,
                        start,
                    })
                })
                .transpose()?;
            let icon = item
                .icon
                .as_ref()
                .map(|icon| -> Result<Icon, Off> {
                    let (like, pixels) = match icon {
                        PackIcon::Tint { like, tint } => (*like, Pixels::Tint(*tint)),
                        PackIcon::Png(bytes) => {
                            let bytes: &'static [u8] = Box::leak(bytes.clone().into_boxed_slice());
                            (item.like, Pixels::Png(bytes))
                        }
                    };
                    Ok(Icon {
                        id: id(&format!("{}.icon", item.id))?,
                        like,
                        pixels,
                    })
                })
                .transpose()?;
            self.add_item(NewItem {
                name,
                description,
                debug_name: item.debug_name.clone(),
                meter,
                stats,
                icon,
                ..NewItem::new(own, item.like)
            })?;
        }
        for recipe in &pack.recipes {
            self.add_recipe(NewRecipe {
                id: id(&recipe.id)?,
                like: recipe.like,
                output: recipe.output,
                output_count: recipe.count,
                cost: recipe.cost,
                workshop: recipe.workshop.map(WorkshopId::new),
            })?;
        }
        let group = group(&pack.namespace);
        for drop in &pack.drops {
            self.add_loot_in(group, drop.clone())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHEST: &str = "6943053a-32c1-4718-ae22-e61427ddf54d";

    fn files(items: &str) -> PackFiles {
        PackFiles::new()
            .with(ITEMS_FILE, items)
            .with(TEXTS_FILE, "[en]\n\"lantern.name\" = \"Lantern\"\n")
    }

    #[test]
    fn an_item_takes_the_pack_s_namespace() {
        let pack = Pack::parse(
            "lanterns",
            &files(&format!(
                "[[item]]\nid = \"lantern\"\nlike = \"{CHEST}\"\nname = \"lantern.name\"\n"
            )),
        )
        .unwrap();
        assert_eq!(pack.ids(), ["lanterns:lantern"]);
        assert_eq!(pack.items[0].like, Guid::parse(CHEST));
    }

    #[test]
    fn problems_name_their_file_line_and_column() {
        let errors = Pack::parse(
            "lanterns",
            &files(&format!(
                "[[item]]\nid = \"lantern\"\nlike = \"{CHEST}\"\n\n[[item]]\nid = \"lantern\"\nlike = \"torch\"\nname = \"missing\"\n"
            )),
        )
        .unwrap_err();
        let lines: Vec<String> = errors.list().iter().map(ToString::to_string).collect();
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("items.toml:5:1: item id \"lantern\" is taken")),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.contains("no item \"torch\" in this pack")),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.contains("text key \"missing\" has no text")),
            "{lines:?}"
        );
    }

    #[test]
    fn vanilla_content_is_named_by_its_debug_name() {
        let torch = Guid([1; 16]);
        let vanilla = crate::content::vanilla::tests::game(&[
            (torch, "Torch"),
            (Guid([5; 16]), "Forge"),
            (Guid([7; 16]), "Prop"),
        ]);
        let items = "[[item]]\nid = \"lantern\"\nlike = \"Torch\"\nname = \"lantern.name\"\n";
        let workshops = "[[workshop]]\nid = \"forge\"\nlike = \"Forge\"\n\n\
                         [[workshop]]\nid = \"bench\"\nlike = \"workbench\"\n";
        let recipes = "[[recipe]]\nid = \"lantern.recipe\"\nlike = \"Torch\"\n\
                       output = \"lantern\"\nworkshop = \"Forge\"\n";
        let named = files(items)
            .with(WORKSHOPS_FILE, workshops)
            .with(RECIPES_FILE, recipes);
        let pack = Pack::parse_with("lights", &named, Some(&vanilla)).unwrap();
        assert_eq!(pack.items[0].like, torch);
        assert_eq!(pack.workshops[0].like.template, Guid([8; 16]));
        assert_eq!(pack.workshops[1].like, stations::WORKBENCH);
        assert_eq!(pack.recipes[0].workshop, Some(Guid([10; 16])));

        let wrong = files("[[item]]\nid = \"lantern\"\nlike = \"Torchh\"\n").with(
            WORKSHOPS_FILE,
            "[[workshop]]\nid = \"forge\"\nlike = \"Prop\"\n",
        );
        let errors = Pack::parse_with("lights", &wrong, Some(&vanilla)).unwrap_err();
        let lines: Vec<String> = errors.list().iter().map(ToString::to_string).collect();
        let unknown = "items.toml:1:1: no item \"Torchh\" in this pack, \
                       and no vanilla item \"Torchh\"; near: Torch";
        assert!(lines.iter().any(|l| l == unknown), "{lines:?}");
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("workshops.toml:1:1: workshop forge:")
                    && l.contains("no crafting station")),
            "{lines:?}"
        );
        let errors = Pack::parse_with("lights", &wrong, None).unwrap_err();
        assert!(
            errors.to_string().contains("needs the game's data"),
            "{errors}"
        );
    }

    #[test]
    fn an_unknown_field_is_a_problem_at_its_line() {
        let errors = Pack::parse(
            "lanterns",
            &files(&format!(
                "[[item]]\nid = \"lantern\"\nlike = \"{CHEST}\"\ncolour = 3\n"
            )),
        )
        .unwrap_err();
        let first = &errors.list()[0];
        assert_eq!(
            (first.file.as_str(), first.line),
            (ITEMS_FILE, 4),
            "{errors}"
        );
        assert!(first.message.contains("colour"), "{errors}");
    }

    #[test]
    fn recipes_reach_the_pack_s_items_and_workshops() {
        let files = files(&format!("[[item]]\nid = \"lantern\"\nlike = \"{CHEST}\"\n"))
            .with(WORKSHOPS_FILE, "[[workshop]]\nid = \"bench\"\nlike = \"workbench\"\nrecipe = { cost = 2 }\n")
            .with(
                RECIPES_FILE,
                format!("[[recipe]]\nid = \"lantern.recipe\"\nlike = \"{CHEST}\"\noutput = \"lantern\"\nworkshop = \"bench\"\n"),
            );
        let pack = Pack::parse("lanterns", &files).unwrap();
        let recipe = &pack.recipes[0];
        assert_eq!(recipe.output, Guid::named("lanterns:lantern"));
        assert_eq!(
            recipe.workshop,
            Some(Guid::named("lanterns:bench.workshop"))
        );
        assert_eq!(
            pack.ids(),
            [
                "lanterns:bench",
                "lanterns:lantern",
                "lanterns:lantern.recipe"
            ]
        );
    }

    #[test]
    fn a_drop_s_meter_follows_its_items() {
        let items =
            format!("[[item]]\nid = \"staff\"\nlike = \"{CHEST}\"\nmeter = {{ max = 3 }}\n");
        let good = files(&items).with(LOOT_FILE, "[[drop]]\nitems = [\"staff\"]\nchance = 0.5\nfrom = [\"corpses\", \"chests\"]\nmeter = [1, 3]\n");
        let pack = Pack::parse("staffs", &good).unwrap();
        let drop = &pack.drops()[0];
        assert_eq!(drop.items, [Guid::named("staffs:staff").id()]);
        assert_eq!(drop.from.kinds, Kinds::CONTAINERS);
        assert_eq!(drop.meter.map(|m| m.of), Some(3));
        let bad = files(&items).with(
            LOOT_FILE,
            "[[drop]]\nitems = [\"staff\"]\nchance = 1.5\nfrom = \"moon\"\nmeter = [1, 4]\n",
        );
        let errors = Pack::parse("staffs", &bad).unwrap_err().to_string();
        assert!(errors.contains("loot.toml:1:1: chance 1.5"), "{errors}");
        assert!(errors.contains("from \"moon\""), "{errors}");
    }

    #[test]
    fn loot_alone_keeps_the_same_content() {
        let a = Pack::parse("p", &PackFiles::new()).unwrap();
        let b = Pack::parse("p", &PackFiles::new().with(TEXTS_FILE, "[en]\na = \"b\"\n")).unwrap();
        assert!(a.same_content(&a.clone()));
        assert!(!a.same_content(&b));
    }

    #[test]
    fn a_manifest_names_its_problem_line() {
        let error = PackManifest::parse("version = \"1.0.0\"\nid = \"Bad Id\"\n").unwrap_err();
        assert_eq!((error.line, error.column), (2, 1), "{error}");
        let error = PackManifest::parse("id = \"a\"\nversion = \"1\"\nextra = 1\n").unwrap_err();
        assert_eq!(error.line, 3, "{error}");
    }

    #[test]
    fn groups_differ_by_pack_and_are_never_a_mod_s() {
        assert_ne!(group("a"), group("b"));
        assert_ne!(group("a"), 0);
    }
}
