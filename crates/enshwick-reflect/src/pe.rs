//! Enough of the PE format to address an image by virtual address, whether it
//! is a file on disk or a module mapped into the process.

use std::ops::Range;

/// How an image's bytes are laid out, which decides how a virtual address maps
/// to an offset.
///
/// # Examples
///
/// ```
/// use enshwick_reflect::Layout;
///
/// assert_ne!(Layout::File, Layout::Mapped);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Layout {
    /// A file: sections sit at their raw file offsets.
    File,
    /// A loaded module: byte `i` is at virtual address `base + i`.
    Mapped,
}

/// One section header, the unit an address is resolved against.
///
/// # Examples
///
/// ```no_run
/// use enshwick_reflect::{Image, Layout};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let bytes = std::fs::read("enshrouded_server.exe")?;
/// let image = Image::parse(&bytes, Layout::File, None)?;
/// let rdata = image.section(".rdata").ok_or("no .rdata")?;
/// assert!(rdata.virtual_size() > 0);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    name: String,
    rva: u64,
    virtual_size: u64,
    raw_offset: u64,
    raw_size: u64,
}

impl Section {
    /// The section's name, such as `.rdata`.
    ///
    /// # Examples
    ///
    /// See [`Section`].
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Where the section starts, relative to the image base.
    ///
    /// # Examples
    ///
    /// See [`Section`].
    #[must_use]
    pub fn rva(&self) -> u64 {
        self.rva
    }

    /// How many bytes the section covers once mapped.
    ///
    /// # Examples
    ///
    /// See [`Section`].
    #[must_use]
    pub fn virtual_size(&self) -> u64 {
        self.virtual_size
    }
}

/// A build's identity: the CodeView record's GUID, in file order, and age.
///
/// The game's version string repeats across builds; this does not, so the
/// loader pins addresses to it.
///
/// # Examples
///
/// ```
/// use enshwick_reflect::Fingerprint;
///
/// let build = Fingerprint::new([0x1c; 16], 387);
/// assert_eq!(build.age(), 387);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Fingerprint {
    guid: [u8; 16],
    age: u32,
}

impl Fingerprint {
    /// A fingerprint from its parts, as a build table stores it.
    ///
    /// # Examples
    ///
    /// See [`Fingerprint`].
    #[must_use]
    pub const fn new(guid: [u8; 16], age: u32) -> Self {
        Self { guid, age }
    }

    /// The GUID in file order.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_reflect::Fingerprint;
    ///
    /// assert_eq!(Fingerprint::new([7; 16], 1).guid(), [7; 16]);
    /// ```
    #[must_use]
    pub const fn guid(&self) -> [u8; 16] {
        self.guid
    }

    /// The age, bumped by the linker on every incremental link.
    ///
    /// # Examples
    ///
    /// See [`Fingerprint`].
    #[must_use]
    pub const fn age(&self) -> u32 {
        self.age
    }
}

/// Why bytes are not an image this crate can read.
///
/// # Examples
///
/// ```
/// use enshwick_reflect::{Image, ImageError, Layout};
///
/// let error = Image::parse(b"not a pe", Layout::File, None).unwrap_err();
/// assert_eq!(error, ImageError::NotPe);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ImageError {
    /// The DOS or PE signature is missing.
    #[error("not a pe image")]
    NotPe,
    /// A header runs past the end of the bytes.
    #[error("the image is truncated")]
    Truncated,
    /// The optional header is not PE32+.
    #[error("not an x64 image")]
    NotX64,
}

/// A PE32+ image, addressed by virtual address.
///
/// # Examples
///
/// ```no_run
/// use enshwick_reflect::{Image, Layout};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let bytes = std::fs::read("enshrouded_server.exe")?;
/// let image = Image::parse(&bytes, Layout::File, None)?;
/// let header = image.read(image.base(), 2).ok_or("unreadable")?;
/// assert_eq!(header, b"MZ");
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct Image<'a> {
    bytes: &'a [u8],
    layout: Layout,
    base: u64,
    sections: Vec<Section>,
    fingerprint: Option<Fingerprint>,
}

const PE_OFFSET: usize = 0x3c;
const SECTION_HEADER: usize = 40;
const OPTIONAL_MAGIC_PE32_PLUS: u16 = 0x20b;
const OPTIONAL_IMAGE_BASE: usize = 24;
const OPTIONAL_DEBUG_DIRECTORY: usize = 112 + 6 * 8;
const DEBUG_ENTRY: u64 = 28;
const DEBUG_TYPE_CODEVIEW: u32 = 2;

impl<'a> Image<'a> {
    /// Reads the headers of `bytes`.
    ///
    /// For [`Layout::Mapped`], `base` is where the module is loaded; for
    /// [`Layout::File`], `None` uses the preferred base.
    ///
    /// # Errors
    ///
    /// [`ImageError`] when the bytes are not a whole PE32+ header.
    ///
    /// # Examples
    ///
    /// See [`Image`] and [`ImageError`].
    pub fn parse(bytes: &'a [u8], layout: Layout, base: Option<u64>) -> Result<Self, ImageError> {
        if bytes.get(..2) != Some(b"MZ") {
            return Err(ImageError::NotPe);
        }
        let pe = u32_at(bytes, PE_OFFSET).ok_or(ImageError::Truncated)? as usize;
        if bytes.get(pe..pe + 4) != Some(b"PE\0\0") {
            return Err(ImageError::NotPe);
        }
        let section_count = u16_at(bytes, pe + 6).ok_or(ImageError::Truncated)? as usize;
        let optional_size = u16_at(bytes, pe + 20).ok_or(ImageError::Truncated)? as usize;
        let optional = pe + 24;
        if u16_at(bytes, optional) != Some(OPTIONAL_MAGIC_PE32_PLUS) {
            return Err(ImageError::NotX64);
        }
        let preferred =
            u64_at(bytes, optional + OPTIONAL_IMAGE_BASE).ok_or(ImageError::Truncated)?;

        let sections = (0..section_count)
            .map(|i| {
                let at = optional + optional_size + i * SECTION_HEADER;
                bytes
                    .get(at..at + SECTION_HEADER)
                    .and_then(section)
                    .ok_or(ImageError::Truncated)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut image = Self {
            bytes,
            layout,
            base: base.unwrap_or(preferred),
            sections,
            fingerprint: None,
        };
        let debug_rva = u32_at(bytes, optional + OPTIONAL_DEBUG_DIRECTORY).unwrap_or(0);
        let debug_size = u32_at(bytes, optional + OPTIONAL_DEBUG_DIRECTORY + 4).unwrap_or(0);
        image.fingerprint = image.codeview(u64::from(debug_rva), u64::from(debug_size));
        Ok(image)
    }

    /// Where the image is, or for a file where it would be at its preferred
    /// base; pointers inside the image are relative to it.
    ///
    /// # Examples
    ///
    /// See [`Image`].
    #[must_use]
    pub fn base(&self) -> u64 {
        self.base
    }

    /// The build fingerprint, when the image carries a CodeView record.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use enshwick_reflect::{Image, Layout};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let bytes = std::fs::read("enshrouded_server.exe")?;
    /// let image = Image::parse(&bytes, Layout::File, None)?;
    /// assert!(image.fingerprint().is_some());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn fingerprint(&self) -> Option<Fingerprint> {
        self.fingerprint
    }

    /// The section named `name`.
    ///
    /// # Examples
    ///
    /// See [`Section`].
    #[must_use]
    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.name == name)
    }

    /// The virtual addresses a section's readable bytes cover.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use enshwick_reflect::{Image, Layout};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let bytes = std::fs::read("enshrouded_server.exe")?;
    /// let image = Image::parse(&bytes, Layout::File, None)?;
    /// let text = image.range(image.section(".text").ok_or("no .text")?);
    /// assert!(text.start > image.base());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn range(&self, section: &Section) -> Range<u64> {
        let len = match self.layout {
            Layout::File => section.raw_size,
            Layout::Mapped => section.virtual_size,
        };
        let start = self.base + section.rva;
        start..start + len
    }

    /// `len` bytes at virtual address `va`, when they lie inside the image
    /// (inside one section, for a file).
    ///
    /// # Examples
    ///
    /// See [`Image`].
    #[must_use]
    pub fn read(&self, va: u64, len: u64) -> Option<&'a [u8]> {
        let rva = va.checked_sub(self.base)?;
        let offset = match self.layout {
            Layout::Mapped => rva,
            Layout::File => {
                let s = self
                    .sections
                    .iter()
                    .find(|s| s.rva <= rva && rva < s.rva + s.raw_size)?;
                if rva + len > s.rva + s.raw_size {
                    return None;
                }
                s.raw_offset + (rva - s.rva)
            }
        };
        let start = usize::try_from(offset).ok()?;
        let end = start.checked_add(usize::try_from(len).ok()?)?;
        self.bytes.get(start..end)
    }

    /// The little-endian `u64` at `va`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use enshwick_reflect::{Image, Layout};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let bytes = std::fs::read("enshrouded_server.exe")?;
    /// let image = Image::parse(&bytes, Layout::File, None)?;
    /// assert!(image.u64(image.base()).is_some());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn u64(&self, va: u64) -> Option<u64> {
        u64_at(self.read(va, 8)?, 0)
    }

    /// The little-endian `u32` at `va`.
    ///
    /// # Examples
    ///
    /// Works as [`Image::u64`] does.
    #[must_use]
    pub fn u32(&self, va: u64) -> Option<u32> {
        u32_at(self.read(va, 4)?, 0)
    }

    /// The little-endian `u16` at `va`.
    ///
    /// # Examples
    ///
    /// Works as [`Image::u64`] does.
    #[must_use]
    pub fn u16(&self, va: u64) -> Option<u16> {
        u16_at(self.read(va, 2)?, 0)
    }

    fn codeview(&self, rva: u64, size: u64) -> Option<Fingerprint> {
        (0..size / DEBUG_ENTRY).find_map(|entry| {
            let record = self.read(self.base + rva + entry * DEBUG_ENTRY, DEBUG_ENTRY)?;
            if u32_at(record, 12)? != DEBUG_TYPE_CODEVIEW {
                return None;
            }
            let data = self.read(self.base + u64::from(u32_at(record, 20)?), 24)?;
            if data.get(..4)? != b"RSDS" {
                return None;
            }
            Some(Fingerprint {
                guid: data.get(4..20)?.try_into().ok()?,
                age: u32_at(data, 20)?,
            })
        })
    }
}

fn section(raw: &[u8]) -> Option<Section> {
    let name = raw.get(..8)?;
    let name_len = name.iter().position(|&c| c == 0).unwrap_or(8);
    Some(Section {
        name: String::from_utf8_lossy(&name[..name_len]).into_owned(),
        virtual_size: u64::from(u32_at(raw, 8)?),
        rva: u64::from(u32_at(raw, 12)?),
        raw_size: u64::from(u32_at(raw, 16)?),
        raw_offset: u64::from(u32_at(raw, 20)?),
    })
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}
