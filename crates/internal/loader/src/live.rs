//! The check a part that writes game memory runs on one live object before
//! its first write: the offsets and addresses it found must give pointers
//! that read, counts under their bounds, vtables in the image's read-only
//! data and values that keep their invariants, or the part turns off alone.
//! Every read goes through [`crate::pages::read`], so a wrong offset reads
//! nothing instead of faulting.

use windows_sys::Win32::System::Memory::PAGE_READONLY;

use crate::{pages, pe};

/// What did not hold on the live object, and the found entry it rests on;
/// shown as the entry and why, as [`crate::fuse::Fuse::verify`] reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Broken {
    /// The build table or reflection entry whose value the check read
    /// through.
    pub(crate) entry: &'static str,
    what: What,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum What {
    Unreadable {
        at: usize,
    },
    Null,
    Dangling {
        to: usize,
    },
    Count {
        count: u64,
        max: u64,
    },
    Vtable {
        at: usize,
    },
    Untyped {
        at: usize,
        wanted: &'static str,
    },
    /// A vtable of another class, by its type descriptor: the name is read
    /// when shown, which keeps a failure small.
    Class {
        at: usize,
        descriptor: usize,
        wanted: &'static str,
    },
    Fails {
        wanted: &'static str,
    },
}

/// MSVC's x64 run-time type information, which the slot before each
/// vtable points to: its complete object locator holds image-relative
/// offsets to the type descriptor, whose decorated name follows two
/// pointers, and to the class hierarchy, whose base class array lists the
/// class itself and every base, each entry's first field a type descriptor.
mod rtti {
    /// The signature of an x64 locator, whose offsets are image-relative.
    pub(super) const SIGNATURE: u32 = 1;
    pub(super) const LOCATOR_TYPE: usize = 12;
    pub(super) const LOCATOR_HIERARCHY: usize = 16;
    /// The locator's own offset, which holds only for a real locator.
    pub(super) const LOCATOR_SELF: usize = 20;
    pub(super) const TYPE_NAME: usize = 16;
    pub(super) const HIERARCHY_COUNT: usize = 8;
    pub(super) const HIERARCHY_BASES: usize = 12;
    /// More bases than any class of the game has, so a wrong count stops.
    pub(super) const MAX_BASES: u32 = 64;
}

/// The longest decorated class name read; a longer one is no class.
const MAX_NAME: usize = 128;

/// A class name from the type information, as MSVC decorates it
/// (`.?AVAllocator@keen@@`), shown as the source names it (`keen::Allocator`)
/// where it is a plain name and as decorated otherwise, such as a template.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Class {
    name: [u8; MAX_NAME],
    len: usize,
}

impl Class {
    /// The class the type descriptor at `descriptor` names; `None` when its
    /// name does not read, ends past [`MAX_NAME`] or is no class name.
    fn read(descriptor: usize) -> Option<Self> {
        let start = descriptor.checked_add(rtti::TYPE_NAME)?;
        let mut name = [0; MAX_NAME];
        for (i, byte) in name.iter_mut().enumerate() {
            let [read] = pages::read::<1>(start.checked_add(i)?)?;
            if read == 0 {
                let class = Self { name, len: i };
                return class.decorated().starts_with(".?A").then_some(class);
            }
            *byte = read;
        }
        None
    }

    fn decorated(&self) -> &str {
        core::str::from_utf8(&self.name[..self.len]).unwrap_or_default()
    }

    /// The parts of a plain class or struct name, outermost namespace first;
    /// `None` for a template or another special name.
    fn parts(&self) -> Option<impl Iterator<Item = &str>> {
        let decorated = self.decorated();
        let stem = decorated
            .strip_prefix(".?AV")
            .or_else(|| decorated.strip_prefix(".?AU"))?
            .strip_suffix("@@")?;
        (!stem.is_empty() && !stem.contains(['?', '$'])).then(|| stem.split('@').rev())
    }

    /// Whether this is the class `wanted` names, such as `keen::Allocator`.
    fn is(&self, wanted: &str) -> bool {
        self.parts()
            .is_some_and(|parts| parts.eq(wanted.split("::")))
    }
}

impl core::fmt::Display for Class {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let Some(parts) = self.parts() else {
            return f.write_str(self.decorated());
        };
        for (i, part) in parts.enumerate() {
            if i > 0 {
                f.write_str("::")?;
            }
            f.write_str(part)?;
        }
        Ok(())
    }
}

impl core::fmt::Debug for Class {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self}")
    }
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
            What::Untyped { at, wanted } => format!(
                "gives a vtable at {at:#x} on the live object without the type information of {wanted}"
            ),
            What::Class {
                at,
                descriptor,
                wanted,
            } => {
                let found = Class::read(descriptor).map_or_else(
                    || format!("the type described at {descriptor:#x}"),
                    |class| class.to_string(),
                );
                format!("gives a vtable of {found} at {at:#x} on the live object, wanted {wanted}")
            }
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

/// Checks that `object`, found through `entry`, starts with a vtable of the
/// class `wanted` names (`keen::Allocator`) or a class derived from it: a
/// pointer into `image`'s read-only pages whose first slot is code of the
/// image that runs, and whose type information in the image names the class.
pub(crate) fn vtable(
    entry: &'static str,
    object: usize,
    image: Image,
    wanted: &'static str,
) -> Result<(), Broken> {
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
    if !(read_only && first_runs()) {
        return Err(Broken::new(entry, What::Vtable { at }));
    }
    match class_of(at, image, wanted) {
        Some((_, true)) => Ok(()),
        Some((descriptor, false)) => Err(Broken::new(
            entry,
            What::Class {
                at,
                descriptor,
                wanted,
            },
        )),
        None => Err(Broken::new(entry, What::Untyped { at, wanted })),
    }
}

/// The type descriptor of the class the type information before the vtable
/// at `at` names, and whether the class is or derives from `wanted`; `None`
/// when the slot before the vtable holds no locator of `image` or its class
/// name does not read.
fn class_of(at: usize, image: Image, wanted: &str) -> Option<(usize, bool)> {
    let locator = pages::read::<8>(at.checked_sub(8)?).map(usize::from_le_bytes)?;
    let u32_at = |at: usize| pages::read::<4>(at).map(u32::from_le_bytes);
    let in_image = |at: usize| {
        let offset = pages::read::<4>(at).map(u32::from_le_bytes)?;
        let to = image.base.checked_add(usize::try_from(offset).ok()?)?;
        image.holds(to).then_some(to)
    };
    if !image.holds(locator)
        || u32_at(locator)? != rtti::SIGNATURE
        || in_image(locator.checked_add(rtti::LOCATOR_SELF)?)? != locator
    {
        return None;
    }
    let descriptor = in_image(locator.checked_add(rtti::LOCATOR_TYPE)?)?;
    Class::read(descriptor)?;
    let hierarchy = in_image(locator.checked_add(rtti::LOCATOR_HIERARCHY)?)?;
    let count = u32_at(hierarchy.checked_add(rtti::HIERARCHY_COUNT)?)?.min(rtti::MAX_BASES);
    let bases = in_image(hierarchy.checked_add(rtti::HIERARCHY_BASES)?)?;
    let derives = (0..count as usize).any(|i| {
        bases
            .checked_add(4 * i)
            .and_then(in_image)
            .and_then(in_image)
            .and_then(Class::read)
            .is_some_and(|base| base.is(wanted))
    });
    Some((descriptor, derives))
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

/// A vtable with MSVC's type information in the test executable, which
/// rustc gives none: the vtable in the image's read-only data, the locator,
/// hierarchy and type descriptors it points to in its writable data, filled
/// at run time since their offsets are image-relative.
#[cfg(test)]
pub(crate) mod fake {
    use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
    use std::sync::{Mutex, MutexGuard};

    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

    use crate::locks::Relock as _;

    /// A vtable and, in the slot before it, its locator.
    #[repr(C)]
    struct Table {
        locator: &'static [AtomicU32; 6],
        first: fn(),
    }

    fn first_slot() {}

    const NAME: usize = super::rtti::TYPE_NAME + super::MAX_NAME;

    static LOCATOR: [AtomicU32; 6] = [const { AtomicU32::new(0) }; 6];
    static TABLE: Table = Table {
        locator: &LOCATOR,
        first: first_slot,
    };
    static HIERARCHY: [AtomicU32; 4] = [const { AtomicU32::new(0) }; 4];
    static BASES: [AtomicU32; 2] = [const { AtomicU32::new(0) }; 2];
    /// Two base class descriptors, each only its type descriptor's offset.
    static DESCRIPTORS: [AtomicU32; 2] = [const { AtomicU32::new(0) }; 2];
    static TYPES: [[AtomicU8; NAME]; 2] = [const { [const { AtomicU8::new(0) }; NAME] }; 2];
    /// One test at a time fills the statics.
    static FILLING: Mutex<()> = Mutex::new(());

    /// The address of a vtable whose type information names `class`, a
    /// class deriving from `base`, both spelled as `keen::Allocator`; the
    /// guard keeps it so while held.
    pub(crate) fn vtable(class: &str, base: &str) -> (MutexGuard<'static, ()>, usize) {
        let guard = FILLING.relock();
        // SAFETY: a null name asks for the executable's own module, which
        // stays loaded for the process.
        let image = unsafe { GetModuleHandleW(core::ptr::null()) }.addr();
        let offset = |at: usize| u32::try_from(at - image).expect("in the image");
        let address = |cell: &AtomicU32| (&raw const *cell).addr();
        for (name, cells) in [class, base].into_iter().zip(&TYPES) {
            let parts: Vec<&str> = name.split("::").collect();
            let decorated = format!(
                ".?AV{}@@",
                parts.into_iter().rev().collect::<Vec<_>>().join("@")
            );
            let bytes = decorated.bytes().chain(core::iter::repeat(0));
            for (cell, byte) in cells[super::rtti::TYPE_NAME..].iter().zip(bytes) {
                cell.store(byte, Ordering::Relaxed);
            }
        }
        let types = TYPES
            .each_ref()
            .map(|cells| offset((&raw const *cells).addr()));
        for (cell, ty) in DESCRIPTORS.iter().zip(types) {
            cell.store(ty, Ordering::Relaxed);
        }
        for (cell, descriptor) in BASES.iter().zip(&DESCRIPTORS) {
            cell.store(offset(address(descriptor)), Ordering::Relaxed);
        }
        let hierarchy = [0, 0, 2, offset(address(&BASES[0]))];
        for (cell, value) in HIERARCHY.iter().zip(hierarchy) {
            cell.store(value, Ordering::Relaxed);
        }
        let locator = [
            super::rtti::SIGNATURE,
            0,
            0,
            types[0],
            offset(address(&HIERARCHY[0])),
            offset(address(&LOCATOR[0])),
        ];
        for (cell, value) in LOCATOR.iter().zip(locator) {
            cell.store(value, Ordering::Relaxed);
        }
        (guard, (&raw const TABLE.first).addr())
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

    /// A vtable in the test executable's read-only data, with no locator in
    /// the slot before it.
    #[repr(C)]
    struct Bare {
        locator: usize,
        first: fn(),
    }

    static BARE: Bare = Bare {
        locator: 0,
        first: first_slot,
    };
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
            vtable: (&raw const BARE.first).addr(),
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
        let mut object = good(&target);
        let (_filled, table) = fake::vtable("keen::WorldAllocator", "keen::Allocator");
        object.vtable = table;
        assert_eq!(
            pointer("t.pointer", at(&object), POINTER),
            Ok(object.pointer)
        );
        assert_eq!(count("t.count", at(&object), COUNT, 4), Ok(3));
        let typed = |wanted| vtable("t.vtable", at(&object), image(), wanted);
        assert_eq!(typed("keen::WorldAllocator"), Ok(()));
        assert_eq!(typed("keen::Allocator"), Ok(()), "a base of its class");
    }

    #[test]
    fn a_vtable_of_another_class_or_none_is_refused_by_name() {
        let target = 7;
        let mut object = good(&target);
        let (_filled, table) = fake::vtable("keen::ZoneAllocator", "keen::Allocator");
        object.vtable = table;
        let other = vtable("t.vtable", at(&object), image(), "keen::Mutex").unwrap_err();
        assert_eq!(
            other.to_string(),
            format!(
                "t.vtable gives a vtable of keen::ZoneAllocator at {table:#x} on the live object, wanted keen::Mutex"
            )
        );
        object.vtable = (&raw const BARE.first).addr();
        let untyped = vtable("t.vtable", at(&object), image(), "keen::Mutex").unwrap_err();
        assert!(
            untyped
                .reason()
                .contains("without the type information of keen::Mutex"),
            "{}",
            untyped.reason()
        );
    }

    fn class(decorated: &str) -> Class {
        let mut name = [0; MAX_NAME];
        name[..decorated.len()].copy_from_slice(decorated.as_bytes());
        Class {
            name,
            len: decorated.len(),
        }
    }

    #[test]
    fn a_plain_class_shows_as_the_source_names_it_and_a_template_as_decorated() {
        let nested = class(".?AVZone@World@keen@@");
        assert_eq!(nested.to_string(), "keen::World::Zone");
        assert!(nested.is("keen::World::Zone"));
        assert!(!nested.is("keen::Zone"));
        assert_eq!(class(".?AUPlain@@").to_string(), "Plain");
        let template = class(".?AV?$Array@H@keen@@");
        assert_eq!(template.to_string(), ".?AV?$Array@H@keen@@");
        assert!(!template.is("keen::Array"));
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
            vtable("t.vtable", at(&object), image(), "keen::Allocator").is_err(),
            "a stack table"
        );
        object.vtable = (&raw const target).addr();
        assert!(
            vtable("t.vtable", at(&object), image(), "keen::Allocator").is_err(),
            "not a table"
        );
        object.vtable = (&raw const NOT_CODE).addr();
        assert!(
            vtable("t.vtable", at(&object), image(), "keen::Allocator").is_err(),
            "read-only data whose first slot is no code"
        );
        WRITABLE.store(
            (first_slot as fn() as *const ()).addr(),
            core::sync::atomic::Ordering::Relaxed,
        );
        object.vtable = (&raw const WRITABLE).addr();
        assert!(
            vtable("t.vtable", at(&object), image(), "keen::Allocator").is_err(),
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
