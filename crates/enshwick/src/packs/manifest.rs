use std::collections::BTreeMap;

use serde::Deserialize;

use super::MANIFEST_FILE;
use super::files::PackError;
use super::pack::Pack;
use crate::abi::Relation;

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
    /// Where the world runs and on every client playing in it, which must
    /// run the same pack, files and all: the default, as both read content.
    #[default]
    Both,
    /// Only where the world runs: the dedicated server, or a client that
    /// hosts or plays alone, such as a pack of loot alone.
    Server,
}

/// A pack folder's `pack.toml`: what the loader reads before it runs the
/// pack, as a mod's `[package.metadata.enshwick]` says it for a DLL.
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
/// exports = ["event:lit"]
///
/// [requires]
/// market = { id = "you.trading-post", version = ">=0.5, <1.0" }
/// "#)?;
/// assert_eq!(manifest.id, "you.lanterns");
/// assert_eq!(manifest.exports, ["event:lit"]);
/// assert_eq!(manifest.side, PackSide::Both);
/// assert_eq!(manifest.requires["market"].id, "you.trading-post");
/// # Ok::<(), enshwick::packs::PackError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct PackManifest {
    /// The pack id, `<author>.<name>` like a mod's; it names the folder, the
    /// namespace of the pack's content and its settings folder in each world.
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
    /// Mods and packs it needs, by the aliases its files and scripts know
    /// them by; the loader runs them first and leaves the pack off without
    /// them.
    #[serde(default)]
    pub requires: BTreeMap<String, PackDependency>,
    /// Mods it loads after when they run, by alias; they take no version.
    #[serde(default)]
    pub after: BTreeMap<String, PackDependency>,
    /// Mods it cannot run beside, by alias; with a `version`, only those
    /// versions.
    #[serde(default)]
    pub breaks: BTreeMap<String, PackDependency>,
    /// The Enshwick releases it is made for, as a Cargo-style version
    /// requirement such as `=1.0.0-alpha.12` (empty for any); a loader of
    /// another release skips the pack and names both.
    #[serde(default)]
    pub enshwick: String,
    /// What other mods may use, each `event:<name>` for an event its scripts
    /// define, `point:<name>` for a point of `[[point]]` or `content:<name>`
    /// for its content: `event:greeted` lets them hear `<id>:greeted`. The
    /// loader refuses them anything else of the pack's.
    #[serde(default)]
    pub exports: Vec<String>,
    /// The pack's settings, in the order the Mods screen shows them; a Luau
    /// mod reads them, as a Rust mod reads its `Config`.
    #[serde(default)]
    pub setting: Vec<PackSetting>,
    /// The extension points its scripts own and define at start.
    #[serde(default)]
    pub point: Vec<super::PackPoint>,
}

/// One setting of a pack, a `[[setting]]` table of `pack.toml`.
///
/// # Examples
///
/// ```
/// use enshwick::packs::PackManifest;
///
/// let manifest = PackManifest::parse(r#"
/// id = "you.greeter"
/// version = "1.0.0"
///
/// [[setting]]
/// key = "greeting"
/// default = "Welcome"
/// doc = "What a player who joins reads."
///
/// [[setting]]
/// key = "every"
/// default = 30
/// min = 1
/// "#)?;
/// let schema = manifest.settings_schema().ok_or("no settings")?;
/// assert_eq!(schema.fields[1].min, Some(1.0));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct PackSetting {
    /// The key in the settings file, of `[a-z0-9_]`.
    pub key: String,
    /// The value on first start; its type is the setting's.
    #[cfg_attr(feature = "schema", schemars(with = "serde_json::Value"))]
    pub default: toml::Value,
    /// What it does, for the file's comments and the Mods screen.
    #[serde(default)]
    pub doc: String,
    /// The smallest number it accepts.
    pub min: Option<f64>,
    /// The largest number it accepts.
    pub max: Option<f64>,
    /// The values it accepts; empty for any.
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Vec<serde_json::Value>"))]
    pub choices: Vec<toml::Value>,
    /// The key of its translated label in the pack's `texts.toml`;
    /// `config.<key>` unless said.
    #[serde(default)]
    pub text: String,
}

/// One dependency of a pack, the value under its alias in `requires`,
/// `after` or `breaks`: a mod id, or a table with the `id`
/// and a `version` requirement.
///
/// # Examples
///
/// ```
/// use enshwick::packs::PackManifest;
///
/// let manifest = PackManifest::parse(r#"
/// id = "you.lanterns"
/// version = "1.0.0"
///
/// [requires]
/// market = { id = "you.trading-post", version = ">=0.5, <1.0" }
/// lockers = "you.lockers"
/// "#)?;
/// assert_eq!(manifest.requires["market"].id, "you.trading-post");
/// assert_eq!(manifest.requires["market"].version, ">=0.5, <1.0");
/// assert_eq!(manifest.requires["lockers"].version, "");
/// # Ok::<(), enshwick::packs::PackError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(from = "DependencyEntry")]
#[cfg_attr(feature = "schema", schemars(with = "DependencyEntry"))]
#[non_exhaustive]
pub struct PackDependency {
    /// The mod's id.
    pub id: String,
    /// A Cargo-style version requirement; empty for any.
    pub version: String,
}

/// How `pack.toml` writes a [`PackDependency`].
#[derive(Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged, expecting = "a mod id, or a table with `id` and `version`")]
enum DependencyEntry {
    /// The mod's id alone, for any version.
    Id(String),
    /// The mod's id with a version requirement.
    Table(DependencyTable),
}

/// The table form of a [`DependencyEntry`].
#[derive(Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
struct DependencyTable {
    /// The mod's id.
    id: String,
    /// A Cargo-style version requirement; empty for any.
    #[serde(default)]
    version: String,
}

impl From<DependencyEntry> for PackDependency {
    fn from(entry: DependencyEntry) -> Self {
        match entry {
            DependencyEntry::Id(id) => Self {
                id,
                version: String::new(),
            },
            DependencyEntry::Table(DependencyTable { id, version }) => Self { id, version },
        }
    }
}

/// The names Luau keeps for itself, which no alias takes, so a script
/// reaches every dependency as `enshwick.deps.<alias>`.
const LUAU_KEYWORDS: [&str; 21] = [
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in", "local",
    "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

/// The module of `enshwick.deps` and `deps!` that names the game's own
/// content, which no alias takes.
const VANILLA: &str = "vanilla";

/// The table of `pack.toml` that gives dependencies of `relation`.
const fn table_of(relation: Relation) -> &'static str {
    match relation {
        Relation::AFTER => "after",
        Relation::BREAKS => "breaks",
        _ => "requires",
    }
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
        manifest.check_settings(text)?;
        manifest.check_points(text)?;
        manifest.check_exports(text)?;
        manifest.check_dependencies(text)?;
        Ok(manifest)
    }

    /// Every dependency by alias, with the relation of the table that names
    /// it: `requires`, `after` or `breaks`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Relation;
    /// use enshwick::packs::PackManifest;
    ///
    /// let manifest = PackManifest::parse(r#"
    /// id = "you.lanterns"
    /// version = "1.0.0"
    ///
    /// [requires]
    /// lockers = "you.lockers"
    ///
    /// [after]
    /// stalls = "you.market-stalls"
    /// "#)?;
    /// let aliases: Vec<_> = manifest.dependencies().map(|(relation, alias, _)| (relation, alias)).collect();
    /// assert_eq!(aliases, [(Relation::REQUIRES, "lockers"), (Relation::AFTER, "stalls")]);
    /// # Ok::<(), enshwick::packs::PackError>(())
    /// ```
    pub fn dependencies(&self) -> impl Iterator<Item = (Relation, &str, &PackDependency)> {
        [
            (Relation::REQUIRES, &self.requires),
            (Relation::AFTER, &self.after),
            (Relation::BREAKS, &self.breaks),
        ]
        .into_iter()
        .flat_map(|(relation, entries)| {
            entries
                .iter()
                .map(move |(alias, dependency)| (relation, alias.as_str(), dependency))
        })
    }

    /// Refuses an alias that is no `[a-z][a-z0-9_]*`, a Luau keyword,
    /// `vanilla` or given twice across the tables, a dependency that is no
    /// mod id and a version on `after`, as `declare_mod!` refuses them in
    /// `[package.metadata.enshwick]`.
    fn check_dependencies(&self, text: &str) -> Result<(), PackError> {
        let mut seen = std::collections::BTreeSet::new();
        for (relation, alias, dependency) in self.dependencies() {
            let at = format!("`{}.{alias}`", table_of(relation));
            let first = alias.bytes().next();
            let bare = first.is_some_and(|b| b.is_ascii_lowercase())
                && alias
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
            let problem = if !bare || LUAU_KEYWORDS.contains(&alias) {
                format!("{at}: an alias is [a-z][a-z0-9_]* and no Luau keyword")
            } else if alias == VANILLA {
                format!("{at}: `{VANILLA}` names the game's own content; choose another alias")
            } else if !seen.insert(alias) {
                format!("{at}: the alias names another dependency already")
            } else if !crate::abi::is_valid_id(&dependency.id)
                && !crate::abi::is_valid_old_form_id(&dependency.id)
            {
                format!(
                    "{at}: {:?} is not <author>.<name> or [a-z0-9-]+",
                    dependency.id
                )
            } else if relation == Relation::AFTER && !dependency.version.is_empty() {
                format!("{at}: `after` orders by id alone and takes no `version`")
            } else {
                continue;
            };
            let line = find(text, &format!("{alias} ="));
            return Err(PackError::at(MANIFEST_FILE, text, line, &problem));
        }
        Ok(())
    }

    /// The schema of the pack's settings; `None` without any.
    ///
    /// # Examples
    ///
    /// See [`PackSetting`].
    #[must_use]
    pub fn settings_schema(&self) -> Option<crate::config::Schema> {
        if self.setting.is_empty() {
            return None;
        }
        let fields = self.setting.iter().map(PackSetting::field).collect();
        Some(crate::config::Schema::new(
            &self.description,
            "config",
            fields,
        ))
    }

    /// The settings of the `pack.toml` among `files` and their schema; none
    /// without one or when it does not read, whose problems the pack's
    /// check reports.
    pub(super) fn settings_of(
        files: &super::PackFiles,
    ) -> (Vec<PackSetting>, Option<crate::config::Schema>) {
        files
            .text(MANIFEST_FILE)
            .ok()
            .flatten()
            .and_then(|text| Self::parse(text).ok())
            .map(|manifest| (manifest.settings_schema(), manifest.setting))
            .map_or_else(Default::default, |(schema, settings)| (settings, schema))
    }

    /// The kinds a pack declares and so may export: its scripts' events and
    /// points, and its content.
    const EXPORT_KINDS: [crate::schema_section::Kind; 3] = [
        crate::schema_section::Kind::Event,
        crate::schema_section::Kind::Point,
        crate::schema_section::Kind::Content,
    ];

    /// Refuses an export that is not `<kind>:<local name>` of a kind in
    /// [`EXPORT_KINDS`](Self::EXPORT_KINDS), a point the pack does not
    /// declare, or given twice,
    /// as `declare_mod!` refuses one in `[package.metadata.enshwick]`.
    fn check_exports(&self, text: &str) -> Result<(), PackError> {
        let mut seen = std::collections::BTreeSet::new();
        for name in &self.exports {
            let parsed = crate::schema_section::Kind::of_export(name);
            let problem = if parsed.is_none_or(|(kind, _)| !Self::EXPORT_KINDS.contains(&kind)) {
                let kinds: Vec<&str> = Self::EXPORT_KINDS.iter().map(|kind| kind.word()).collect();
                format!(
                    "exports: {name:?} is not `<kind>:<name>`, the kind one a pack declares: {}",
                    kinds.join(", ")
                )
            } else if !parsed
                .is_some_and(|(_, local)| crate::content::is_id(&format!("{}:{local}", self.id)))
            {
                format!(
                    "exports: {name:?}: the name is 1 to 128 of [a-z0-9_./-], without the pack's id"
                )
            } else if let Some((crate::schema_section::Kind::Point, local)) = parsed
                && !self.point.iter().any(|point| point.name == local)
            {
                format!("exports: {name:?} names no [[point]] of this pack")
            } else if !seen.insert(name) {
                format!("exports: {name:?} is given twice")
            } else {
                continue;
            };
            return Err(PackError::at(
                MANIFEST_FILE,
                text,
                find(text, "exports"),
                &problem,
            ));
        }
        Ok(())
    }

    /// Refuses a key that is not `[a-z0-9_]` or given twice, a default
    /// outside its own range or choices.
    fn check_settings(&self, text: &str) -> Result<(), PackError> {
        let mut seen = std::collections::BTreeSet::new();
        for setting in &self.setting {
            let at = find(text, &format!("key = \"{}\"", setting.key));
            let problem = |why: String| Err(PackError::at(MANIFEST_FILE, text, at, &why));
            let key = &setting.key;
            let bare = !key.is_empty()
                && key
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
            if !bare {
                return problem(format!("setting {key:?}: a key is of [a-z0-9_]"));
            }
            if !seen.insert(key.as_str()) {
                return problem(format!("setting {key:?} is given twice"));
            }
            if let Err(why) = setting.field().check(&setting.default) {
                return problem(format!("setting {key:?}: the default is refused: {why}"));
            }
        }
        Ok(())
    }
}

impl PackSetting {
    /// The setting as a field of a settings schema.
    fn field(&self) -> crate::config::Field {
        crate::config::Field::new(&self.key, &self.default)
            .with_range(self.min, self.max)
            .with_choices(&self.choices)
            .with_doc(&self.doc, &self.text)
    }
}

impl PackManifest {
    /// Whether `pack` may run under this manifest, whose file reads `text`:
    /// a pack that [reaches players](Pack::reaches_players) runs on both
    /// sides.
    ///
    /// # Errors
    ///
    /// The problem, at the line of `side`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::packs::{Pack, PackFiles, PackManifest};
    ///
    /// let text = "id = \"you.stacks\"\nversion = \"1.0.0\"\nside = \"server\"\n";
    /// let manifest = PackManifest::parse(text)?;
    /// let files = PackFiles::new().with("items.toml", r#"
    /// [[change]]
    /// items = ["6943053a-32c1-4718-ae22-e61427ddf54d"]
    /// max_stack = 50
    /// "#);
    /// let pack = Pack::parse("you.stacks", &files).map_err(|e| e.list()[0].clone())?;
    /// let problem = manifest.check(text, &pack).unwrap_err();
    /// assert_eq!(problem.line, 3);
    /// # Ok::<(), enshwick::packs::PackError>(())
    /// ```
    pub fn check(&self, text: &str, pack: &Pack) -> Result<(), PackError> {
        if self.side == PackSide::Both || !pack.reaches_players() {
            return Ok(());
        }
        Err(PackError::at(
            MANIFEST_FILE,
            text,
            find(text, "side"),
            "side = \"server\", but the pack adds or changes what players' games read, so server and players must run it alike; make it \"both\"",
        ))
    }
}

/// The byte offset of the line that sets `key`, for an error about it.
pub(super) fn find(text: &str, key: &str) -> usize {
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if line.trim_start().starts_with(key) {
            return at + line.len() - line.trim_start().len();
        }
        at += line.len();
    }
    0
}
