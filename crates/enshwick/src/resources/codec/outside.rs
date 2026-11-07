//! The fields whose data lies outside a value's own bytes.

use super::{Kind, MAX_DEPTH, Registry, Type};

impl<'a> Registry<'a> {
    /// The path, by the game's field names, of the first field of `ty` at
    /// any depth of its own bytes whose data lies outside them: a blob kind,
    /// whose offset points past them in a resource, or a designer data kind,
    /// which points to the heap; a copy of a live value's own bytes holds
    /// neither's data.
    pub(crate) fn outside_field(&self, ty: &Type<'a>) -> Option<String> {
        self.outside(ty, 0)
    }

    fn outside(&self, ty: &Type<'a>, depth: usize) -> Option<String> {
        let ty = self.underlying(ty).ok()?;
        if depth > MAX_DEPTH {
            return None;
        }
        let join = |head: &str, rest: String| {
            if rest.is_empty() {
                head.to_owned()
            } else {
                format!("{head}.{rest}")
            }
        };
        match ty.kind() {
            Kind::BlobArray
            | Kind::BlobString
            | Kind::BlobOptional
            | Kind::BlobVariant
            | Kind::DsArray
            | Kind::DsString
            | Kind::DsOptional
            | Kind::DsVariant => Some(String::new()),
            Kind::Struct => self.struct_fields(ty).into_iter().find_map(|field| {
                let inner = self.named(field.type_name()?)?;
                self.outside(inner, depth + 1)
                    .map(|rest| join(field.name(), rest))
            }),
            Kind::StaticArray => {
                let inner = self.inner_of(ty).ok()?;
                self.outside(inner, depth + 1).map(|rest| join("0", rest))
            }
            _ => None,
        }
    }
}
