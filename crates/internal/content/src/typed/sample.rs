//! A value of a build's type with every part filled, for the shape check
//! that turns a generated type off on a build it no longer fits.

use super::runtime::fields_of;
use crate::resources::{Registry, Value};

/// Arrays and optionals nested deeper than this are left empty in a sample.
const SAMPLE_DEPTH: usize = 8;

/// A value of the type `name` with every part a value can hold: one
/// element in each array, every optional present and each integer at the
/// largest positive value of its width, so reading it checks the shape of
/// every type it holds; `None` for a type the registry lacks.
#[doc(hidden)]
#[must_use]
pub fn sample(registry: &Registry<'_>, name: &str) -> Option<Value> {
    sample_at(registry, name, 0)
}

fn sample_at(registry: &Registry<'_>, name: &str, depth: usize) -> Option<Value> {
    use crate::resources::Kind;

    let ty = registry.named(name)?;
    let deeper = depth < SAMPLE_DEPTH;
    match ty.kind() {
        Kind::Typedef if deeper => sample_at(registry, ty.inner()?, depth + 1),
        Kind::Struct => {
            let mut fields = match ty.base() {
                Some(base) => fields_of(sample_at(registry, base, depth + 1)?),
                None => Vec::new(),
            };
            for field in ty.fields() {
                if let Some(of) = field.type_name() {
                    fields.push((field.name().to_owned(), sample_at(registry, of, depth + 1)?));
                }
            }
            Some(Value::Struct(fields))
        }
        Kind::StaticArray | Kind::BlobArray | Kind::DsArray => Some(Value::List(if deeper {
            vec![sample_at(registry, ty.inner()?, depth + 1)?]
        } else {
            Vec::new()
        })),
        Kind::BlobOptional | Kind::DsOptional if deeper => {
            sample_at(registry, ty.inner()?, depth + 1)
        }
        Kind::BlobOptional | Kind::DsOptional => Some(Value::Nil),
        _ => {
            let size = usize::try_from(ty.size()).ok()?;
            let mut bytes = vec![0; size];
            if matches!(
                ty.kind(),
                Kind::Uint8
                    | Kind::Uint16
                    | Kind::Uint32
                    | Kind::Uint64
                    | Kind::Sint8
                    | Kind::Sint16
                    | Kind::Sint32
                    | Kind::Sint64
            ) && let Some((top, low)) = bytes.split_last_mut()
            {
                // The largest positive value of its width: a narrower Rust
                // integer than the build's field cannot hold it.
                low.fill(0xff);
                *top = 0x7f;
            }
            registry.decode(name, &bytes).ok()
        }
    }
}
