//! A world store as a script keeps it: one table of game data, saved as
//! TOML beside the world as a Rust mod's store is.

use super::{StoreError, WorldStore};
use crate::resources::Value as Data;
use crate::script::{Class, Error, Function, Object};

/// A [`WorldStore`] of game data, the store a script opens with
/// `world_store`.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct DataStore(WorldStore<toml::Table>);

impl Object for DataStore {
    const CLASS: &'static Class = &Class::of::<Self>(
        "WorldStore",
        "The mod's data for the world running now, saved beside it.",
    );
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_STORE];
}

/// `value` as game data.
fn toml_to_data(value: toml::Value) -> Data {
    match value {
        toml::Value::String(text) => Data::Text(text),
        toml::Value::Integer(i) => Data::Int(i),
        toml::Value::Float(f) => Data::Float(f),
        toml::Value::Boolean(b) => Data::Bool(b),
        toml::Value::Datetime(when) => Data::Text(when.to_string()),
        toml::Value::Array(items) => Data::List(items.into_iter().map(toml_to_data).collect()),
        toml::Value::Table(table) => Data::Struct(
            table
                .into_iter()
                .map(|(k, v)| (k, toml_to_data(v)))
                .collect(),
        ),
    }
}

/// `data` as TOML; nil, and what TOML cannot hold, is left out.
fn data_to_toml(data: Data) -> Option<toml::Value> {
    Some(match data {
        Data::Nil | Data::Bytes(_) => return None,
        Data::Bool(b) => toml::Value::Boolean(b),
        Data::Int(i) => toml::Value::Integer(i),
        Data::Uint(u) => toml::Value::Integer(i64::try_from(u).ok()?),
        Data::Float(f) => toml::Value::Float(f),
        Data::Name(text) | Data::Text(text) => toml::Value::String(text),
        Data::Guid(guid) => toml::Value::String(guid.to_string()),
        Data::Flags(names) => {
            toml::Value::Array(names.into_iter().map(toml::Value::String).collect())
        }
        Data::List(items) => {
            toml::Value::Array(items.into_iter().filter_map(data_to_toml).collect())
        }
        Data::Struct(fields) => toml::Value::Table(
            fields
                .into_iter()
                .filter_map(|(k, v)| Some((k, data_to_toml(v)?)))
                .collect(),
        ),
        Data::Variant(variant) => data_to_toml(variant.value().clone())?,
    })
}

#[crate::script(store)]
impl DataStore {
    /// What `read` answers for the data now; nil while no world runs.
    #[script(name = "with")]
    pub(crate) fn with_script(&self, read: impl Fn(Data) -> Result<Data, Error>) -> Option<Data> {
        self.0
            .with(|table| read(toml_to_data(toml::Value::Table(table.clone()))))
            .and_then(Result::ok)
    }

    /// Changes the data to what `change` returns for it, a table; nil or an
    /// error keeps it. False while no world runs.
    #[script(name = "update")]
    pub(crate) fn update_script(&self, change: impl Fn(Data) -> Result<Data, Error>) -> bool {
        self.0
            .update(|table| {
                let changed = change(toml_to_data(toml::Value::Table(table.clone())));
                if let Ok(data) = changed
                    && let Some(toml::Value::Table(new)) = data_to_toml(data)
                {
                    *table = new;
                }
            })
            .is_some()
    }

    /// Writes the data now, as `WorldStore::flush` does.
    ///
    /// # Errors
    ///
    /// As [`WorldStore::flush`].
    #[script(name = "flush")]
    pub(crate) fn flush_script(&self) -> Result<(), StoreError> {
        self.0.flush()
    }

    /// Why the store is shut, as `WorldStore::shut_reason` says.
    #[script(name = "shut_reason")]
    pub(crate) fn shut_reason_script(&self) -> Option<String> {
        self.0.shut_reason()
    }

    /// The world's key, as `WorldStore::key` gives it.
    #[script(name = "key")]
    pub(crate) fn key_script(&self) -> Option<String> {
        self.0.key()
    }
}

impl Object for super::Hold {
    const CLASS: &'static Class = &Class::of::<Self>(
        "Hold",
        "A mod's hold on a world's saves: while set, the world does not save.",
    );
    const PARTS: &'static [&'static [Function<Self>]] =
        &[Self::SCRIPT_HOLD, Self::SCRIPT_HOLD_TEXT];
}

#[crate::script(hold_text)]
impl super::Hold {
    /// Holds the saves for `reason`, with the text of the mod's texts key
    /// `key` and `args`, each a name and its value, for players, as
    /// `Hold::set_text` does.
    ///
    /// # Errors
    ///
    /// As [`super::Hold::set_text`].
    #[script(name = "set_text")]
    pub(crate) fn set_text_script(
        &self,
        reason: &str,
        key: &str,
        args: &[(String, String)],
    ) -> Result<(), StoreError> {
        let args: Vec<(&str, &str)> = args.iter().map(|(n, v)| (n.as_str(), v.as_str())).collect();
        self.set_text(reason, key, &args)
    }
}

/// The free functions of [`state`](super) as a script's namespace `state`.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct StateFns;

#[crate::script(state_fns)]
#[expect(
    clippy::unused_self,
    reason = "a script call takes its namespace, though a free function reads nothing of it"
)]
impl StateFns {
    /// The hold kept in the world's data folder `dir`, as `Hold::new` makes
    /// it.
    #[script]
    #[must_use]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "a script hands the path over as its own copy"
    )]
    pub fn hold(self, dir: std::path::PathBuf) -> super::Hold {
        super::Hold::new(&dir)
    }

    /// Every hold set in `dir`, each a mod and its reason, as
    /// [`super::holds`] lists them.
    #[script]
    #[must_use]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "a script hands the path over as its own copy"
    )]
    pub fn holds(self, dir: std::path::PathBuf) -> Vec<(String, String)> {
        super::holds(&dir)
    }
}

impl crate::script::Bound for StateFns {
    const NAME: &'static str = "state";
    const DOC: &'static str = "The data a mod keeps for a world, and holds on its saves.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_STATE_FNS];

    fn resolve(_load: &mut crate::Load<'_>) -> Result<Self, crate::Off> {
        Ok(Self)
    }
}

#[crate::script(state)]
impl crate::Load<'_> {
    /// The mod's store `name` for the world running now, of game data a
    /// script keeps, as `Load::world_store` opens a Rust mod's.
    ///
    /// # Errors
    ///
    /// As [`crate::Load::world_store`].
    #[script(name = "world_store")]
    pub(crate) fn world_store_script(&mut self, name: &str) -> Result<DataStore, crate::Off> {
        self.world_store(name).map(DataStore)
    }
}
