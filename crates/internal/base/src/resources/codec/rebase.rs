//! Offsets moved with the bytes that hold them.

use super::{CodecError, Encoder, Kind, MAX_DEPTH, Type, VARIANT_OFFSET, out_of_range, size_of};

impl<'a> Encoder<'_, 'a> {
    /// Moves every offset the value of `ty` at `at` holds by `delta`, after
    /// its bytes moved from `at + delta`, so each still points at its data.
    pub(super) fn rebase(
        &mut self,
        ty: &Type<'a>,
        at: usize,
        delta: isize,
        depth: usize,
    ) -> Result<(), CodecError> {
        if depth > MAX_DEPTH {
            return Err(CodecError::new("nested deeper than 64"));
        }
        let registry = self.registry;
        let ty = registry.underlying(ty)?;
        let from = match ty.kind() {
            Kind::BlobArray | Kind::BlobString | Kind::BlobOptional => at,
            Kind::BlobVariant => at + VARIANT_OFFSET,
            Kind::Struct => {
                for field in registry.struct_fields(ty) {
                    if let Some(field_ty) = field.type_name().and_then(|name| registry.named(name))
                    {
                        let offset =
                            usize::try_from(field.offset()).map_err(|_| out_of_range(at))?;
                        self.rebase(field_ty, at + offset, delta, depth + 1)?;
                    }
                }
                return Ok(());
            }
            Kind::StaticArray => {
                let inner = registry.inner_of(ty)?;
                let count = usize::try_from(ty.element_count().unwrap_or(0)).unwrap_or(0);
                let step = size_of(ty).checked_div(count).unwrap_or(0);
                for i in 0..count {
                    self.rebase(inner, at + i * step, delta, depth + 1)?;
                }
                return Ok(());
            }
            _ => return Ok(()),
        };
        let offset = i32::from_le_bytes(
            self.read(from, 4)
                .ok_or_else(|| out_of_range(from))?
                .try_into()
                .map_err(|_| out_of_range(from))?,
        );
        if offset == 0 {
            return Ok(());
        }
        let moved = i32::try_from(isize::try_from(offset).map_err(|_| out_of_range(from))? + delta)
            .map_err(|_| CodecError::new("the resource grew past 2 GiB"))?;
        self.write(from, &moved.to_le_bytes())
    }
}
