//! Writing a value back into resource bytes, into new bytes or a served
//! resource's lent output.

use super::{CodecError, Encoder, size_of};
use crate::resources::{Grow, Output, Registry, Value};

/// Why an encode into a refused [`Output`] fails.
pub(super) const REFUSED: &str = "the loader lent no buffer for the output";

impl Registry<'_> {
    /// Writes `value`, of the type qualified as `type_name`, as resource
    /// bytes: over `base`, the bytes it was read from, keeping every byte
    /// whose value did not change, or into new bytes for `None`. A struct may
    /// name only the fields it changes; the rest keep `base`'s bytes, or
    /// zero.
    ///
    /// # Errors
    ///
    /// [`CodecError`] for a value of another kind than its type, a number
    /// out of its type's range, a name no constant or field carries, a static
    /// array of another length, and a `base` too short for the type.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::{Field, Kind, Registry, Type, Value};
    ///
    /// let registry = Registry::new([
    ///     Type::new("demo::float32", Kind::Float32, 4, 4),
    ///     Type::new("demo::Table", Kind::Struct, 8, 4).with_fields(vec![
    ///         Field::new("damage", 0, "demo::float32"),
    ///         Field::new("speed", 4, "demo::float32"),
    ///     ]),
    /// ]);
    /// let base = [2.5f32.to_le_bytes(), 1.0f32.to_le_bytes()].concat();
    /// let doubled = Value::Struct(vec![("damage".into(), Value::Float(5.0))]);
    /// let bytes = registry.encode("demo::Table", &doubled, Some(&base))?;
    /// assert_eq!(bytes, [5.0f32.to_le_bytes(), 1.0f32.to_le_bytes()].concat());
    /// # Ok::<(), enshwick::resources::CodecError>(())
    /// ```
    pub fn encode(
        &self,
        type_name: &str,
        value: &Value,
        base: Option<&[u8]>,
    ) -> Result<Vec<u8>, CodecError> {
        let mut bytes = Vec::new();
        self.encode_in(type_name, value, base, &mut bytes)?;
        Ok(bytes)
    }

    /// Writes `value` as [`Registry::encode`] does, straight into a served
    /// resource's [`Output`], whose bytes it replaces, so a builder's
    /// resource is built where the loader lends it.
    ///
    /// # Errors
    ///
    /// As [`Registry::encode`], and when the loader lends `out` no buffer;
    /// `out` then holds what was written so far.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::ResourceKey;
    /// use enshwick::resources::{Registry, Value};
    ///
    /// fn double_damage(
    ///     load: &mut enshwick::Load<'_>,
    ///     key: ResourceKey,
    ///     table: &'static str,
    /// ) -> Result<(), enshwick::Off> {
    ///     let registry = Registry::game()?;
    ///     load.serve(key, key, move |base, out| {
    ///         let mut value = registry.decode(table, base).ok()?;
    ///         if let Some(Value::Float(damage)) = value.field_mut("damage") {
    ///             *damage *= 2.0;
    ///         }
    ///         registry.encode_into(table, &value, Some(base), out).ok()
    ///     })
    /// }
    /// ```
    pub fn encode_into(
        &self,
        type_name: &str,
        value: &Value,
        base: Option<&[u8]>,
        out: &mut Output<'_>,
    ) -> Result<(), CodecError> {
        out.resize(0);
        self.encode_in(type_name, value, base, out)
    }

    /// Writes `value` into the empty `bytes`, as [`Registry::encode`].
    pub(in crate::resources) fn encode_in<B: Grow>(
        &self,
        type_name: &str,
        value: &Value,
        base: Option<&[u8]>,
        bytes: &mut B,
    ) -> Result<(), CodecError> {
        let ty = self.require(type_name)?;
        let size = size_of(ty);
        let fresh = match base {
            Some(base) if base.len() >= size => {
                bytes.put(base);
                false
            }
            Some(base) => {
                return Err(CodecError::new(format!(
                    "the base holds {} bytes, {} needs {size}",
                    base.len(),
                    ty.qualified_name()
                )));
            }
            None => {
                bytes.grow(size);
                true
            }
        };
        if bytes.len() < size {
            return Err(CodecError::new(REFUSED));
        }
        Encoder {
            registry: self,
            bytes,
        }
        .value(ty, value, 0, fresh, 0)
    }
}
