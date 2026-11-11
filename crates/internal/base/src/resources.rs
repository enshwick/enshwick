//! Any game resource as typed values, read and written by its reflected type.
//!
//! The [`Registry`] holds the build's types; its codec reads a resource's
//! bytes into a [`Value`] tree and writes the tree back for
//! [`Load::serve`](../struct.Load.html#method.serve). Type names are the game's own
//! reflected names, given as data.
//!
//! # Examples
//!
//! ```
//! # extern crate enshwick_base as enshwick;
//! use enshwick::resources::{Field, Kind, Registry, Type, Value};
//!
//! let registry = Registry::new([
//!     Type::new("demo::float32", Kind::Float32, 4, 4),
//!     Type::new("demo::Table", Kind::Struct, 4, 4)
//!         .with_fields(vec![Field::new("damage", 0, "demo::float32")]),
//! ]);
//! let base = 2.5f32.to_le_bytes();
//! let mut table = registry.decode("demo::Table", &base)?;
//! *table.field_mut("damage").ok_or("no damage")? = Value::Float(5.0);
//! assert_eq!(registry.encode("demo::Table", &table, Some(&base))?, 5.0f32.to_le_bytes());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod codec;
mod kind;
mod read;
mod registry;
mod script;
mod serve;
mod table;
mod types;

pub use codec::CodecError;
#[doc(inline)]
pub use enshwick_values::resources::{Value, Variant};
pub use kind::Kind;
pub use registry::Registry;
pub use script::{Bytes, Resources};
#[doc(hidden)]
pub use serve::serve;
#[doc(hidden)]
pub use table::{TypeTable, declare, decode, serve_new, serve_value};
pub use types::{Attribute, Constant, Field, Type, crc32, crc64, fnv1a32};

#[cfg(test)]
mod tests;
