//! The live check the base grouping runs on the bases object before it
//! first writes to it.

use super::Fields;
use crate::base_groups::window;
use crate::live::{self, Broken};
use crate::pinned::bases;

/// Reads the live bases object `obj` the way
/// [`super::members::let_zone_list_grow`] and [`super::members::settle`]
/// do, through reads that cannot fault, before the first write: the count
/// is one the game's array holds, the zone list's length is within its
/// capacity with its data readable, and the world allocator, where there is
/// one, is an object of the game's image with a vtable.
pub(super) fn object_holds(
    obj: usize,
    fields: &Fields,
    image: Option<live::Image>,
) -> Result<(), Broken> {
    let count = u32::from_le_bytes(live::bytes(bases::COUNT.id(), obj, fields.count)?);
    live::holds(
        bases::COUNT.id(),
        count as usize <= window::VANILLA,
        "a count of 16 at most",
    )?;
    let list = obj.wrapping_add(fields.zone_list);
    let len = live::count(
        bases::CONTAINER_LEN.id(),
        list,
        fields.container.len,
        u64::from(u32::MAX),
    )?;
    let cap = live::count(
        bases::CONTAINER_CAP.id(),
        list,
        fields.container.cap,
        u64::from(u32::MAX),
    )?;
    live::holds(
        bases::CONTAINER_LEN.id(),
        len <= cap,
        "a zone list length within its capacity",
    )?;
    if len > 0 {
        live::pointer(bases::CONTAINER_DATA.id(), list, fields.container.data)?;
    }
    live::bytes::<8>(
        bases::ZONE_LIST_ALLOCATOR.id(),
        list,
        fields.container.allocator,
    )?;
    let allocator =
        usize::from_le_bytes(live::bytes(bases::ALLOCATOR.id(), obj, fields.allocator)?);
    if allocator == 0 {
        return Ok(());
    }
    match image {
        Some(image) => live::vtable(bases::ALLOCATOR.id(), allocator, image),
        None => live::holds(
            bases::ALLOCATOR.id(),
            false,
            "a game image whose headers read",
        ),
    }
}

#[cfg(test)]
mod tests {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

    use super::*;
    use crate::bases::fake::{FIELDS, Fake, square};

    fn first_slot() {}

    /// An allocator's vtable in the test executable's read-only data.
    static VTABLE: [fn(); 1] = [first_slot];

    #[test]
    fn a_live_bases_object_holds_only_with_its_count_list_and_allocator_in_shape() {
        // SAFETY: a null name asks for the executable's own module, which
        // stays loaded for the process.
        let base = unsafe { GetModuleHandleW(core::ptr::null()) };
        let image = live::Image::at(base.addr());
        let mut fake = Fake::new(1, &[square(0.0, 0.0)]);
        // The fake leaves the capacity out; the game's list has inline room
        // for 16.
        fake.obj.0[0x1340..0x1348].copy_from_slice(&16u64.to_le_bytes());
        // Each check takes the object's address anew, so the writes to the
        // fake before it are seen.
        let obj = |fake: &mut Fake| fake.ptr().addr();
        assert_eq!(object_holds(obj(&mut fake), &FIELDS, image), Ok(()));
        let allocator = [(&raw const VTABLE).addr()];
        let at = allocator.as_ptr().addr();
        fake.obj.0[..8].copy_from_slice(&at.to_le_bytes());
        assert_eq!(object_holds(obj(&mut fake), &FIELDS, image), Ok(()));
        let entry =
            |fake: &mut Fake, image| object_holds(obj(fake), &FIELDS, image).unwrap_err().entry;
        assert_eq!(
            entry(&mut fake, None),
            bases::ALLOCATOR.id(),
            "no image to hold the vtable"
        );
        let stack_table = [first_slot as fn()];
        let not_a_vtable = [(&raw const stack_table).addr()];
        fake.obj.0[..8].copy_from_slice(&not_a_vtable.as_ptr().addr().to_le_bytes());
        assert_eq!(entry(&mut fake, image), bases::ALLOCATOR.id());
        fake.obj.0[..8].copy_from_slice(&[0; 8]);
        fake.obj.0[0x24..0x28].copy_from_slice(&17u32.to_le_bytes());
        assert_eq!(entry(&mut fake, image), bases::COUNT.id());
        fake.obj.0[0x24..0x28].copy_from_slice(&1u32.to_le_bytes());
        fake.obj.0[0x1338..0x1340].copy_from_slice(&u64::MAX.to_le_bytes());
        assert_eq!(
            entry(&mut fake, image),
            bases::CONTAINER_LEN.id(),
            "a length past every list"
        );
    }
}
