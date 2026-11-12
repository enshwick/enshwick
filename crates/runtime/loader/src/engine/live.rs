//! The check a part that writes game memory runs on one live object before
//! its first write: the offsets and addresses it found must give pointers
//! that read, counts under their bounds, vtables in the image's read-only
//! data and values that keep their invariants, or the part turns off alone.
//! Every read goes through [`crate::engine::pages::read`], so a wrong offset reads
//! nothing instead of faulting.

use windows_sys::Win32::System::Memory::PAGE_READONLY;

use crate::{engine::pages, engine::pe};

/// What did not hold on the live object, and the found entry it rests on;
/// shown as the entry and why, as [`crate::engine::fuse::Fuse::verify`] reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Broken {
    /// The build table or reflection entry whose value the check read
    /// through.
    pub(crate) entry: &'static str,
    what: What,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum What {
    Unreadable { at: usize },
    Null,
    Dangling { to: usize },
    Count { count: u64, max: u64 },
    Vtable { at: usize },
    Fails { wanted: &'static str },
}

impl Broken {
    const fn new(entry: &'static str, what: What) -> Self {
        Self { entry, what }
    }

    /// Whether the entry gave a null pointer, as an object the game has
    /// not made yet does.
    pub(crate) const fn is_null(&self) -> bool {
        matches!(self.what, What::Null)
    }

    /// What did not hold, worded to follow the entry.
    fn reason(&self) -> String {
        match self.what {
            What::Unreadable { at } => {
                format!("reads at {at:#x} on the live object, which does not read")
            }
            What::Null => "gives a null pointer on the live object".to_owned(),
            What::Dangling { to } => {
                format!("gives a pointer to {to:#x} on the live object, which does not read")
            }
            What::Count { count, max } => {
                format!("gives a count of {count} on the live object, past {max}")
            }
            What::Vtable { at } => format!(
                "gives a vtable at {at:#x} on the live object, outside the image's read-only data"
            ),
            What::Fails { wanted } => format!("does not give {wanted} on the live object"),
        }
    }
}

impl core::fmt::Display for Broken {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} {}", self.entry, self.reason())
    }
}

/// The game's image as it is mapped, for the vtable check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Image {
    base: usize,
    end: usize,
}

impl Image {
    /// The image mapped at `base`, its size read from its PE headers; `None`
    /// when they do not read.
    pub(crate) fn at(base: usize) -> Option<Self> {
        let u32_at = |at: usize| pages::read::<4>(at).map(u32::from_le_bytes);
        let signature = usize::try_from(u32_at(base.checked_add(pe::PE_POINTER)?)?).ok()?;
        let size_at = base
            .checked_add(signature)?
            .checked_add(pe::OPTIONAL + pe::SIZE_OF_IMAGE)?;
        let size = usize::try_from(u32_at(size_at)?).ok()?;
        Some(Self {
            base,
            end: base.checked_add(size)?,
        })
    }

    fn holds(&self, address: usize) -> bool {
        (self.base..self.end).contains(&address)
    }
}

/// The pointer-sized word at `offset` in `object`.
fn word(entry: &'static str, object: usize, offset: usize) -> Result<usize, Broken> {
    let at = object.wrapping_add(offset);
    pages::read::<8>(at)
        .map(usize::from_le_bytes)
        .ok_or(Broken::new(entry, What::Unreadable { at }))
}

/// The pointer at `entry`'s `offset` in `object`, which must be non-null
/// and point into memory that reads.
pub(crate) fn pointer(entry: &'static str, object: usize, offset: usize) -> Result<usize, Broken> {
    let to = word(entry, object, offset)?;
    if to == 0 {
        return Err(Broken::new(entry, What::Null));
    }
    if !pages::query(to).is_some_and(|region| region.reads()) {
        return Err(Broken::new(entry, What::Dangling { to }));
    }
    Ok(to)
}

/// The `u64` count at `entry`'s `offset` in `object`, which must be at most
/// `max`.
pub(crate) fn count(
    entry: &'static str,
    object: usize,
    offset: usize,
    max: u64,
) -> Result<u64, Broken> {
    let count = word(entry, object, offset)? as u64;
    if count > max {
        return Err(Broken::new(entry, What::Count { count, max }));
    }
    Ok(count)
}

/// Checks that `object`, found through `entry`, starts with a vtable: a
/// pointer into `image`'s read-only pages whose first slot is code of the
/// image that runs.
pub(crate) fn vtable(entry: &'static str, object: usize, image: Image) -> Result<(), Broken> {
    let at = word(entry, object, 0)?;
    let read_only = image.holds(at)
        && pages::query(at).is_some_and(|region| region.reads() && region.protect == PAGE_READONLY);
    let first_runs = || {
        pages::read::<8>(at)
            .map(usize::from_le_bytes)
            .is_some_and(|slot| {
                image.holds(slot) && pages::query(slot).is_some_and(|region| region.runs())
            })
    };
    if read_only && first_runs() {
        Ok(())
    } else {
        Err(Broken::new(entry, What::Vtable { at }))
    }
}

/// The `N` bytes at `entry`'s `offset` in `object`, which must all read.
pub(crate) fn bytes<const N: usize>(
    entry: &'static str,
    object: usize,
    offset: usize,
) -> Result<[u8; N], Broken> {
    let at = object.wrapping_add(offset);
    pages::read::<N>(at).ok_or(Broken::new(entry, What::Unreadable { at }))
}

/// Checks an invariant a part tested on what it read through `entry`:
/// `held`, or a failure saying the object did not give `wanted`.
pub(crate) const fn holds(
    entry: &'static str,
    held: bool,
    wanted: &'static str,
) -> Result<(), Broken> {
    if held {
        Ok(())
    } else {
        Err(Broken::new(entry, What::Fails { wanted }))
    }
}

#[cfg(test)]
mod tests {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

    use super::*;

    /// An object as the engine lays one out: a vtable, a pointer and a
    /// count.
    #[repr(C)]
    struct Object {
        vtable: usize,
        pointer: usize,
        count: u64,
    }

    const POINTER: usize = 8;
    const COUNT: usize = 16;

    fn first_slot() {}

    /// A vtable in the test executable's read-only data.
    static VTABLE: [fn(); 1] = [first_slot];
    /// Read-only data of the test executable that holds no code address.
    static NOT_CODE: [usize; 1] = [0x1234];
    /// A table in the test executable's writable data.
    static WRITABLE: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

    fn image() -> Image {
        // SAFETY: a null name asks for the executable's own module, which
        // stays loaded for the process.
        let base = unsafe { GetModuleHandleW(core::ptr::null()) };
        Image::at(base.addr()).expect("the test executable's headers read")
    }

    fn good(target: &u64) -> Object {
        Object {
            vtable: (&raw const VTABLE).addr(),
            pointer: (&raw const *target).addr(),
            count: 3,
        }
    }

    fn at(object: &Object) -> usize {
        (&raw const *object).addr()
    }

    #[test]
    fn a_good_object_passes_every_check() {
        let target = 7;
        let object = good(&target);
        assert_eq!(
            pointer("t.pointer", at(&object), POINTER),
            Ok(object.pointer)
        );
        assert_eq!(count("t.count", at(&object), COUNT, 4), Ok(3));
        assert_eq!(vtable("t.vtable", at(&object), image()), Ok(()));
    }

    #[test]
    fn a_broken_pointer_is_refused_without_a_fault() {
        let target = 7;
        let mut object = good(&target);
        object.pointer = 0;
        let null = pointer("t.pointer", at(&object), POINTER).unwrap_err();
        assert_eq!(null.entry, "t.pointer");
        assert!(null.reason().contains("null"), "{}", null.reason());
        object.pointer = 0x10;
        let dangling = pointer("t.pointer", at(&object), POINTER).unwrap_err();
        assert!(dangling.reason().contains("0x10"), "{}", dangling.reason());
        let unread = pointer("t.object", 0x10, POINTER).unwrap_err();
        assert!(unread.reason().contains("0x18"), "{}", unread.reason());
    }

    #[test]
    fn a_count_past_its_bound_is_refused() {
        let target = 7;
        let mut object = good(&target);
        object.count = 5000;
        let past = count("t.count", at(&object), COUNT, 4096).unwrap_err();
        assert!(
            past.reason().contains("5000") && past.reason().contains("past 4096"),
            "{}",
            past.reason()
        );
    }

    #[test]
    fn a_vtable_outside_the_image_s_read_only_data_is_refused() {
        let target = 7;
        let slots = [first_slot as fn()];
        let mut object = good(&target);
        object.vtable = (&raw const slots).addr();
        assert!(
            vtable("t.vtable", at(&object), image()).is_err(),
            "a stack table"
        );
        object.vtable = (&raw const target).addr();
        assert!(
            vtable("t.vtable", at(&object), image()).is_err(),
            "not a table"
        );
        object.vtable = (&raw const NOT_CODE).addr();
        assert!(
            vtable("t.vtable", at(&object), image()).is_err(),
            "read-only data whose first slot is no code"
        );
        WRITABLE.store(
            (first_slot as fn() as *const ()).addr(),
            core::sync::atomic::Ordering::Relaxed,
        );
        object.vtable = (&raw const WRITABLE).addr();
        assert!(
            vtable("t.vtable", at(&object), image()).is_err(),
            "a table in the image's writable data"
        );
    }

    #[test]
    fn a_count_at_its_bound_holds() {
        let target = 7;
        let object = good(&target);
        assert_eq!(count("t.count", at(&object), COUNT, 3), Ok(3));
    }

    #[test]
    fn an_invariant_that_fails_says_what_the_object_did_not_give() {
        assert_eq!(holds("t.range", true, "finite extents"), Ok(()));
        let failed = holds("t.range", false, "finite extents").unwrap_err();
        assert_eq!(
            failed.to_string(),
            "t.range does not give finite extents on the live object"
        );
        assert!(bytes::<4>("t.range", 0x10, 0).is_err());
    }

    #[test]
    fn a_broken_object_names_its_entry_first() {
        let broken = Broken::new("knowledge.state", What::Null);
        assert_eq!(
            broken.to_string(),
            "knowledge.state gives a null pointer on the live object"
        );
    }
}
