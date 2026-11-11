//! A mod's scripts in memory, and `require` among them.

use std::collections::BTreeMap;
use std::sync::Arc;

use mlua::Lua;
use mlua::luau::{NavigateError, Require};

use enshwick_core::packs::SCRIPT_MAIN as MAIN;

/// The script files of one mod, by path relative to its folder with `/`
/// between folders.
///
/// # Examples
///
/// ```
/// let scripts = enshwick_luau::Scripts::new()
///     .with("main.luau", "local greet = require('./greet')")
///     .with("greet.luau", "return 1");
/// assert_eq!(scripts.len(), 2);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scripts(BTreeMap<String, Arc<[u8]>>, String);

impl Scripts {
    /// No scripts.
    ///
    /// # Examples
    ///
    /// See [`Scripts`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The same with the script at `path` holding `source`.
    ///
    /// # Examples
    ///
    /// See [`Scripts`].
    #[must_use]
    pub fn with(mut self, path: &str, source: impl AsRef<[u8]>) -> Self {
        self.0.insert(path.to_owned(), Arc::from(source.as_ref()));
        self
    }

    /// How many scripts there are.
    ///
    /// # Examples
    ///
    /// See [`Scripts`].
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are none.
    ///
    /// # Examples
    ///
    /// ```
    /// assert!(enshwick_luau::Scripts::new().is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether they hold a `main.luau`, which makes a pack a Luau mod.
    ///
    /// # Examples
    ///
    /// ```
    /// assert!(enshwick_luau::Scripts::new().with("main.luau", "").has_main());
    /// ```
    #[must_use]
    pub fn has_main(&self) -> bool {
        self.0.contains_key(MAIN)
    }

    /// The same with each of `bindings` bound once `main.luau` ran: the
    /// global function it names runs on its item's actions of its kind, as
    /// `enshwick.items.on_item` binds it. A binding to a name `main.luau`
    /// leaves without a function stops the start with an error naming it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::packs::{Pack, PackFiles};
    ///
    /// let items = "[[item]]\nid = \"tonic\"\nlike = \"6943053a-32c1-4718-ae22-e61427ddf54d\"\non = { consume = \"drink\" }\n";
    /// let main = "function drink() end";
    /// let files = PackFiles::new().with("items.toml", items).with("main.luau", main);
    /// let pack = Pack::parse_with("you.tonics", &files, None).map_err(|e| e.to_string())?;
    /// let scripts = enshwick_luau::Scripts::new().with("main.luau", main).with_items(&pack.item_bindings());
    /// assert_eq!(scripts.len(), 1);
    /// # Ok::<(), String>(())
    /// ```
    #[must_use]
    pub fn with_items(mut self, bindings: &[enshwick_core::packs::ItemBinding]) -> Self {
        use core::fmt::Write as _;

        for b in bindings {
            let key = b.kind.to_ascii_lowercase();
            let _infallible = writeln!(
                self.1,
                "if type({f}) ~= \"function\" then error(\"item {id}: on.{key} names {f}, which main.luau does not define as a global function\", 0) end\nenshwick.items.on_item({item}, \"{kind}\", {f})",
                f = b.function,
                id = b.id,
                item = b.item.get(),
                kind = b.kind,
            );
        }
        self
    }

    /// The Luau that binds the items' functions, empty for none.
    pub(crate) fn bindings(&self) -> &str {
        &self.1
    }

    pub(crate) fn get(&self, path: &str) -> Option<&Arc<[u8]>> {
        self.0.get(path)
    }
}

/// Luau's `require` over a mod's own scripts: `./name` and `../name`
/// resolve beside the requiring script, nothing outside the mod's folder.
pub(crate) struct Requirer {
    scripts: Scripts,
    /// The module path the navigation points at, folders and the module's
    /// name without its extension.
    at: Vec<String>,
    /// The script file that path resolves to.
    resolved: Option<String>,
}

impl Requirer {
    pub(crate) fn new(scripts: Scripts) -> Self {
        Self {
            scripts,
            at: Vec::new(),
            resolved: None,
        }
    }

    fn resolve(&mut self) -> Result<(), NavigateError> {
        let path = self.at.join("/");
        let file = format!("{path}.luau");
        let folder = format!("{path}/init.luau");
        self.resolved = match (self.scripts.get(&file), self.scripts.get(&folder)) {
            (Some(_), Some(_)) => return Err(NavigateError::Ambiguous),
            (Some(_), None) => Some(file),
            (None, Some(_)) => Some(folder),
            (None, None) => None,
        };
        Ok(())
    }
}

impl Require for Requirer {
    fn is_require_allowed(&self, chunk_name: &str) -> bool {
        chunk_name.starts_with('@')
    }

    fn reset(&mut self, chunk_name: &str) -> Result<(), NavigateError> {
        let path = chunk_name
            .strip_prefix('@')
            .ok_or(NavigateError::NotFound)?;
        let path = path.strip_suffix(".luau").unwrap_or(path);
        let path = path.strip_suffix("/init").unwrap_or(path);
        self.at = path.split('/').map(str::to_owned).collect();
        self.resolve()
    }

    fn jump_to_alias(&mut self, _path: &str) -> Result<(), NavigateError> {
        Err(NavigateError::NotFound)
    }

    fn to_parent(&mut self) -> Result<(), NavigateError> {
        self.at.pop().ok_or(NavigateError::NotFound)?;
        self.resolve()
    }

    fn to_child(&mut self, name: &str) -> Result<(), NavigateError> {
        if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
            return Err(NavigateError::NotFound);
        }
        self.at.push(name.to_owned());
        self.resolve()
    }

    fn has_module(&self) -> bool {
        self.resolved.is_some()
    }

    fn cache_key(&self) -> String {
        self.resolved.clone().unwrap_or_default()
    }

    fn has_config(&self) -> bool {
        false
    }

    fn config(&self) -> std::io::Result<Vec<u8>> {
        Err(std::io::ErrorKind::NotFound.into())
    }

    fn loader(&self, lua: &Lua) -> mlua::Result<mlua::Function> {
        let path = self
            .resolved
            .as_deref()
            .ok_or_else(|| mlua::Error::runtime("no module here"))?;
        let source = self
            .scripts
            .get(path)
            .ok_or_else(|| mlua::Error::runtime(format!("{path} is gone")))?;
        lua.load(&source[..])
            .set_name(format!("@{path}"))
            .set_mode(mlua::chunk::ChunkMode::Text)
            .into_function()
    }
}
