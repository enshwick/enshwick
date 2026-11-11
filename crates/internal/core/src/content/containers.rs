//! The slots of the containers items place, read and changed in the
//! template each item places.

use crate::Off;
use crate::engine_ids as ids;

use super::{Blob, EngineFacts};

/// A `BlobVariant` in a template's component list: type hash, offset from
/// its second word, size.
const VARIANT: usize = 12;

/// Where the fields a container edit touches sit on the running build,
/// resolved by the loader.
///
/// # Examples
///
/// See [`ContainerLayout::resolve`] and [`resize_container`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContainerLayout {
    /// A template's components.
    pub components: usize,
    /// The inventory setup component's type hash.
    pub setup: u32,
    /// Its count of generic slots, a `u8`.
    pub generic: usize,
    /// Its count of slots loot fills, a `u8`.
    pub available: usize,
    /// The setup override component's type hash and its count of generic
    /// slots, where the build has it.
    pub overwrite: Option<(u32, usize)>,
}

impl ContainerLayout {
    /// The layout `facts` give: the running game's through
    /// [`Load`](crate::Load), a game image's in the loader and in tools.
    ///
    /// # Errors
    ///
    /// The first [`Off`] a fact returns, when the build lacks it.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let layout = enshwick::content::containers::ContainerLayout::resolve(load)?;
    ///     enshwick::info!("generic slots at +{:#x}", layout.generic);
    ///     Ok(())
    /// }
    /// ```
    pub fn resolve(facts: &(impl EngineFacts + ?Sized)) -> Result<Self, Off> {
        let overwrite = facts
            .hash(ids::INVENTORY_SETUP_OVERWRITE)
            .and_then(|hash| {
                Ok((
                    hash,
                    facts.offset(ids::INVENTORY_SETUP_OVERWRITE_GENERIC_SLOTS)?,
                ))
            })
            .ok();
        Ok(Self {
            components: facts.offset(ids::TEMPLATE_COMPONENTS)?,
            setup: facts.hash(ids::INVENTORY_SETUP)?,
            generic: facts.offset(ids::INVENTORY_SETUP_GENERIC_SLOTS)?,
            available: facts.offset(ids::INVENTORY_SETUP_AVAILABLE_SLOTS)?,
            overwrite,
        })
    }
}

/// The data offset of each component of `template` with type hash `hash`.
fn components(layout: &ContainerLayout, blob: &Blob, hash: u32) -> Option<Vec<usize>> {
    template_components(blob, layout.components, hash)
}

/// The data offset of each component with type hash `hash` in the template
/// `blob`, whose component list sits at `list`.
pub(crate) fn template_components(blob: &Blob, list: usize, hash: u32) -> Option<Vec<usize>> {
    let (start, count) = blob.array(list)?;
    let mut found = Vec::new();
    for variant in (0..usize::try_from(count).ok()?).map(|i| start + i * VARIANT) {
        if blob.read_u32(variant)? == hash {
            let offset = usize::try_from(blob.read_u32(variant + 4)?).ok()?;
            found.push((variant + 4).checked_add(offset)?);
        }
    }
    Some(found)
}

/// How many generic slots the container `template` sets up; `None` for a
/// template without an inventory setup of its own.
///
/// # Examples
///
/// See [`resize_container`].
#[must_use]
pub fn container_slots(layout: &ContainerLayout, template: &[u8]) -> Option<u8> {
    let blob = Blob::new(template);
    let setup = *components(layout, &blob, layout.setup)?.first()?;
    blob.bytes().get(setup + layout.generic).copied()
}

/// The container `template` with `slots` generic slots; `None` for a
/// template without an inventory setup of its own.
///
/// It writes the inventory setup's generic and loot counts and its
/// override's count, which vanilla containers keep equal.
///
/// # Examples
///
/// ```
/// use enshwick::content::containers::{ContainerLayout, container_slots, resize_container};
///
/// fn doubled(layout: &ContainerLayout, template: &[u8]) -> Option<Vec<u8>> {
///     let slots = container_slots(layout, template)?;
///     resize_container(layout, template, slots.saturating_mul(2).min(128))
/// }
/// ```
#[must_use]
pub fn resize_container(layout: &ContainerLayout, template: &[u8], slots: u8) -> Option<Vec<u8>> {
    let mut blob = Blob::new(template);
    let setups = components(layout, &blob, layout.setup)?;
    if setups.is_empty() {
        return None;
    }
    for setup in setups {
        let written = blob.write(setup + layout.generic, &[slots])
            && blob.write(setup + layout.available, &[slots]);
        if !written {
            return None;
        }
    }
    if let Some((hash, generic)) = layout.overwrite {
        for overwrite in components(layout, &blob, hash)? {
            if !blob.write(overwrite + generic, &[slots]) {
                return None;
            }
        }
    }
    Some(blob.into_bytes())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const LAYOUT: ContainerLayout = ContainerLayout {
        components: 0,
        setup: 0xc24d_f82a,
        generic: 0x51,
        available: 0x52,
        overwrite: Some((0x1234_5678, 0)),
    };

    /// A template with an inventory setup of `slots`, its override, and an
    /// unrelated component.
    pub(crate) fn chest(slots: u8) -> Vec<u8> {
        let mut blob = Blob::new(&[0u8; 8]);
        let list_at = blob.append(&[0u8; 3 * VARIANT], 4);
        let mut setup = vec![0u8; 0x54];
        setup[0x51] = slots;
        setup[0x52] = slots;
        let setup_at = blob.append(&setup, 4);
        let overwrite_at = blob.append(&[slots, 0, 0, 0], 4);
        let other_at = blob.append(&[9; 4], 4);
        for (i, (hash, at, size)) in [
            (LAYOUT.setup, setup_at, 0x54u32),
            (0x1234_5678, overwrite_at, 4),
            (0x9999_9999, other_at, 4),
        ]
        .into_iter()
        .enumerate()
        {
            let variant = list_at + i * VARIANT;
            let offset = u32::try_from(at - (variant + 4)).unwrap();
            let bytes = [hash.to_le_bytes(), offset.to_le_bytes(), size.to_le_bytes()].concat();
            assert!(blob.write(variant, &bytes));
        }
        assert!(blob.set_array(0, list_at, 3));
        blob.into_bytes()
    }

    #[test]
    fn a_chest_s_slots_read_and_change_in_every_count() {
        let template = chest(24);
        assert_eq!(container_slots(&LAYOUT, &template), Some(24));
        let resized = resize_container(&LAYOUT, &template, 48).unwrap();
        assert_eq!(container_slots(&LAYOUT, &resized), Some(48));
        let blob = Blob::new(&resized);
        let setup = components(&LAYOUT, &blob, LAYOUT.setup).unwrap()[0];
        assert_eq!(resized[setup + 0x52], 48);
        let overwrite = components(&LAYOUT, &blob, 0x1234_5678).unwrap()[0];
        assert_eq!(resized[overwrite], 48);
        let other = components(&LAYOUT, &blob, 0x9999_9999).unwrap()[0];
        assert_eq!(resized[other..other + 4], [9; 4]);
    }

    #[test]
    fn a_template_without_an_inventory_has_no_slots() {
        let mut blob = Blob::new(&[0u8; 8]);
        assert!(blob.set_array(0, 8, 0));
        let empty = blob.into_bytes();
        assert_eq!(container_slots(&LAYOUT, &empty), None);
        assert_eq!(resize_container(&LAYOUT, &empty, 48), None);
    }
}
