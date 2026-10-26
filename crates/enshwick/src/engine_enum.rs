//! The engine's value of each variant of an enum the API names, read by the
//! engine constant's name through the loader, so no engine number is
//! compiled into a mod.

use crate::Load;

/// The engine value of each variant of `T`; a variant whose constant the
/// build lacks has none and is never decoded.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Values<T: 'static, const N: usize>([(T, Option<u64>); N]);

impl<T: Copy + PartialEq, const N: usize> Values<T, N> {
    /// Each variant with its engine value as given.
    #[cfg(test)]
    pub(crate) const fn new(values: [(T, Option<u64>); N]) -> Self {
        Self(values)
    }

    /// Each variant with the value the loader answers for its id.
    pub(crate) fn resolve(load: &Load<'_>, ids: [(T, &str); N]) -> Self {
        Self(ids.map(|(variant, id)| (variant, load.constant(id).ok())))
    }

    /// The variant whose engine value is `raw`.
    pub(crate) fn variant(&self, raw: u64) -> Option<T> {
        self.0
            .iter()
            .find(|&&(_, value)| value == Some(raw))
            .map(|&(variant, _)| variant)
    }
}

/// The API's bits for each bit of an engine mask of up to 16 bits, from the
/// engine constants that name the bit indices.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Bits([u16; 16]);

impl Bits {
    /// The mapping for `ids`: the API's bit and the id of its engine bit.
    pub(crate) fn resolve<const N: usize>(load: &Load<'_>, ids: [(u16, &str); N]) -> Self {
        Self::from_indices(
            ids.into_iter()
                .map(|(ours, id)| (ours, load.constant(id).ok())),
        )
    }

    fn from_indices(bits: impl Iterator<Item = (u16, Option<u64>)>) -> Self {
        let mut table = [0u16; 16];
        for (ours, index) in bits {
            if let Some(entry) = index
                .and_then(|i| usize::try_from(i).ok())
                .and_then(|slot| table.get_mut(slot))
            {
                *entry |= ours;
            }
        }
        Self(table)
    }

    /// The API's bits for the engine's mask `raw`; engine bits the API does
    /// not name are dropped.
    pub(crate) fn map(&self, raw: u16) -> u16 {
        let mut rest = raw;
        let mut out = 0;
        while rest != 0 {
            out |= self.0[rest.trailing_zeros() as usize];
            rest &= rest - 1;
        }
        out
    }
}

/// Asserts that every variant of `ids` is named after the engine constant
/// its id stands for, as the catalogue names each id after its constant.
#[cfg(test)]
pub(crate) fn assert_named_after_engine<T: core::fmt::Debug>(ids: &[(T, &str)]) {
    let plain = |name: &str| -> String {
        name.chars()
            .filter(|&c| c != '_')
            .map(|c| c.to_ascii_lowercase())
            .collect()
    };
    for (variant, id) in ids {
        let (_, constant) = id.rsplit_once('.').unwrap_or(("", id));
        assert_eq!(plain(&format!("{variant:?}")), plain(constant), "{id}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Side {
        Left,
        Right,
        Up,
    }

    #[test]
    fn a_variant_decodes_from_its_engine_value() {
        let values = Values([
            (Side::Left, Some(7)),
            (Side::Right, Some(2)),
            (Side::Up, None),
        ]);
        assert_eq!(values.variant(2), Some(Side::Right));
        assert_eq!(values.variant(9), None);
        assert_eq!(values.variant(7), Some(Side::Left));
    }

    #[test]
    fn engine_bits_map_to_the_api_s_bits_and_unnamed_bits_drop() {
        let bits =
            Bits::from_indices([(1, Some(3)), (2, Some(0)), (4, None), (8, Some(99))].into_iter());
        assert_eq!(bits.map(0b1001), 0b11);
        assert_eq!(bits.map(0b0100), 0, "bit 2 has no name");
        assert_eq!(bits.map(0x8000), 0);
        assert_eq!(bits.map(0), 0);
    }
}
