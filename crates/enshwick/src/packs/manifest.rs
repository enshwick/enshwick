use std::collections::BTreeMap;

use serde::Deserialize;

use super::MANIFEST_FILE;
use super::files::PackError;
use super::pack::Pack;

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
#[derive(Clone, Debug, PartialEq, Deserialize)]
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
    /// The pack's settings, in the order the Mods screen shows them; a Luau
    /// mod reads them, as a Rust mod reads its `Config`.
    #[serde(default)]
    pub setting: Vec<PackSetting>,
}

/// One setting of a pack, a `[[setting]]` table of `pack.toml`.
///
/// # Examples
///
/// ```
/// use enshwick::config::Stage;
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
/// stage = "startup"
/// "#)?;
/// let schema = manifest.settings_schema().ok_or("no settings")?;
/// assert_eq!(schema.fields[1].stage, Stage::Startup);
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
    /// When a change takes effect: `runtime` unless said.
    #[serde(default)]
    pub stage: crate::config::Stage,
    /// A `startup` setting a joined client takes from the server.
    #[serde(default)]
    pub synced: bool,
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

pub(super) const fn yes() -> bool {
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
        manifest.check_settings(text)?;
        Ok(manifest)
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

    /// Refuses a key that is not `[a-z0-9_]` or given twice, a default
    /// outside its own range or choices, and `synced` off `startup`.
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
            if setting.synced && setting.stage != crate::config::Stage::Startup {
                return problem(format!(
                    "setting {key:?}: synced goes with stage = \"startup\"; a sync stage follows the server already"
                ));
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
        crate::config::Field::new(&self.key, self.stage, &self.default)
            .with_range(self.min, self.max)
            .with_choices(&self.choices)
            .with_doc(&self.doc, &self.text)
            .with_synced(self.synced)
    }
}

impl PackManifest {
    /// Whether `pack` may run under this manifest, whose file reads `text`:
    /// a pack that [reaches players](Pack::reaches_players) keeps `join` on.
    ///
    /// # Errors
    ///
    /// The problem, at the line of `join`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::packs::{Pack, PackFiles, PackManifest};
    ///
    /// let text = "id = \"you.stacks\"\nversion = \"1.0.0\"\njoin = false\n";
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
        if self.join || !pack.reaches_players() {
            return Ok(());
        }
        Err(PackError::at(
            MANIFEST_FILE,
            text,
            find(text, "join"),
            "join = false, but the pack adds or changes what players' games read, so server and players must run it alike; remove the line",
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
