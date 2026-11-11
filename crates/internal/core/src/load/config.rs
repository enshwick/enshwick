//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::abi::{Event, Level, Status, Str};
use crate::config::{ConfigError, Schema, reason};
use crate::host::Host;
use crate::{Load, Off};
use crate::{Load, Off};
use core::ffi::c_void;
use core::fmt::Write as _;
use std::sync::Arc;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// Declares the mod's settings described by `schema`, the world's
    /// `settings.toml`, and returns its defaults: the values the mod runs on
    /// while no world runs. [`TableConfig::on_change`] follows the world's.
    ///
    /// The fields are top-level keys: a schema with groups is refused.
    ///
    /// # Errors
    ///
    /// [`Off`] for a schema with groups, dotted or empty keys, or when the
    /// loader refuses it.
    ///
    /// # Examples
    ///
    /// See [`TableConfig`].
    pub fn table_config(&mut self, schema: Schema) -> Result<(TableConfig, toml::Table), Off>;
}
