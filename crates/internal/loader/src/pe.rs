//! The game's executable names and the PE header fields the loader reads.

use enshwick::abi::Side;

/// The dedicated server's executable, lowercase.
const SERVER_EXE: &str = "enshrouded_server.exe";
/// The client's executable, lowercase.
const CLIENT_EXE: &str = "enshrouded.exe";

/// The offset of the word that locates the PE signature.
pub(crate) const PE_POINTER: usize = 0x3c;
/// From the PE signature: the section count in the file header.
pub(crate) const SECTION_COUNT: usize = 6;
/// From the PE signature: the optional header's size in the file header.
pub(crate) const OPTIONAL_SIZE: usize = 20;
/// From the PE signature: the optional header, past the signature and the
/// file header.
pub(crate) const OPTIONAL: usize = 24;
/// From the optional header of a 64-bit image: the preferred base.
pub(crate) const IMAGE_BASE: usize = 24;
/// From the optional header: the mapped image's size.
pub(crate) const SIZE_OF_IMAGE: usize = 56;
/// From the optional header: the size of all headers.
pub(crate) const SIZE_OF_HEADERS: usize = 60;
/// The length of one section header.
pub(crate) const SECTION_HEADER: usize = 40;
/// From a section header: where the section is mapped.
pub(crate) const SECTION_ADDRESS: usize = 12;
/// From a section header: the section's size in the file.
pub(crate) const SECTION_RAW_SIZE: usize = 16;
/// From a section header: where the section starts in the file.
pub(crate) const SECTION_RAW_AT: usize = 20;

/// The side a game executable runs, from its file name; `None` for any
/// other executable.
pub(crate) fn side_of(file_name: &str) -> Option<Side> {
    match file_name.to_ascii_lowercase().as_str() {
        SERVER_EXE => Some(Side::SERVER),
        CLIENT_EXE => Some(Side::CLIENT),
        _ => None,
    }
}

/// The little-endian `u32` at `at` in `bytes`.
pub(crate) fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    bytes
        .get(at..at.checked_add(4)?)?
        .try_into()
        .ok()
        .map(u32::from_le_bytes)
}

/// The fields of a 64-bit executable's headers that mapping it needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Headers {
    /// The preferred base.
    pub(crate) base: u64,
    /// The mapped image's size.
    pub(crate) size: usize,
    /// The size of all headers, copied to the image's start.
    header_size: usize,
    /// Each section's `(mapped at, file offset, file size)`.
    sections: Vec<(usize, usize, usize)>,
}

impl Headers {
    /// Reads the headers of the executable `bytes`.
    ///
    /// # Errors
    ///
    /// The name of the first field the bytes lack.
    pub(crate) fn read(bytes: &[u8]) -> Result<Self, &'static str> {
        let word = |at: usize, name| {
            u32_at(bytes, at)
                .and_then(|word| usize::try_from(word).ok())
                .ok_or(name)
        };
        let half = |at: usize, name| {
            bytes
                .get(at..at.saturating_add(2))
                .and_then(|b| b.try_into().ok())
                .map(u16::from_le_bytes)
                .map(usize::from)
                .ok_or(name)
        };
        let pe = word(PE_POINTER, "PE header")?;
        let optional = pe + OPTIONAL;
        let base = bytes
            .get(optional + IMAGE_BASE..optional + IMAGE_BASE + 8)
            .and_then(|b| b.try_into().ok())
            .map(u64::from_le_bytes)
            .ok_or("image base")?;
        let size = word(optional + SIZE_OF_IMAGE, "image size")?;
        let headers = word(optional + SIZE_OF_HEADERS, "header size")?;
        let count = half(pe + SECTION_COUNT, "section count")?;
        let table = optional + half(pe + OPTIONAL_SIZE, "optional header")?;
        let sections = (0..count)
            .filter_map(|i| {
                let header = table + SECTION_HEADER * i;
                let field = |at| u32_at(bytes, header + at).and_then(|w| usize::try_from(w).ok());
                Some((
                    field(SECTION_ADDRESS)?,
                    field(SECTION_RAW_AT)?,
                    field(SECTION_RAW_SIZE)?,
                ))
            })
            .collect();
        Ok(Self {
            base,
            size,
            header_size: headers,
            sections,
        })
    }

    /// Copies the headers and sections of the executable `bytes` into
    /// `image` where Windows would map them; a part that does not fit is
    /// left out.
    pub(crate) fn copy(&self, bytes: &[u8], image: &mut [u8]) {
        let mut copy = |to: usize, from: usize, len: usize| {
            if let (Some(dst), Some(src)) = (
                image.get_mut(to..to.saturating_add(len)),
                bytes.get(from..from.saturating_add(len)),
            ) {
                dst.copy_from_slice(src);
            }
        };
        copy(0, 0, self.header_size.min(self.size));
        for &(at, from, len) in &self.sections {
            copy(at, from, len);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_game_executables_name_their_side_in_any_case() {
        assert_eq!(side_of("enshrouded_server.exe"), Some(Side::SERVER));
        assert_eq!(side_of("Enshrouded.exe"), Some(Side::CLIENT));
        assert_eq!(side_of("notepad.exe"), None);
    }

    /// A minimal 64-bit executable: headers in the first 0x200 bytes and one
    /// section of four bytes at file offset 0x200, mapped at 0x1000.
    fn executable() -> Vec<u8> {
        let mut bytes = vec![0u8; 0x204];
        let pe = 0x80;
        bytes[PE_POINTER..PE_POINTER + 4].copy_from_slice(&0x80u32.to_le_bytes());
        bytes[pe + SECTION_COUNT..pe + SECTION_COUNT + 2].copy_from_slice(&1u16.to_le_bytes());
        bytes[pe + OPTIONAL_SIZE..pe + OPTIONAL_SIZE + 2].copy_from_slice(&0xf0u16.to_le_bytes());
        let optional = pe + OPTIONAL;
        bytes[optional + IMAGE_BASE..optional + IMAGE_BASE + 8]
            .copy_from_slice(&0x1_4000_0000u64.to_le_bytes());
        bytes[optional + SIZE_OF_IMAGE..optional + SIZE_OF_IMAGE + 4]
            .copy_from_slice(&0x2000u32.to_le_bytes());
        bytes[optional + SIZE_OF_HEADERS..optional + SIZE_OF_HEADERS + 4]
            .copy_from_slice(&0x200u32.to_le_bytes());
        let section = optional + 0xf0;
        for (at, value) in [
            (SECTION_ADDRESS, 0x1000u32),
            (SECTION_RAW_SIZE, 4),
            (SECTION_RAW_AT, 0x200),
        ] {
            bytes[section + at..section + at + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[0x200..0x204].copy_from_slice(b"code");
        bytes
    }

    #[test]
    fn headers_name_the_base_size_and_sections() {
        let headers = Headers::read(&executable());
        assert_eq!(
            headers,
            Ok(Headers {
                base: 0x1_4000_0000,
                size: 0x2000,
                header_size: 0x200,
                sections: vec![(0x1000, 0x200, 4)],
            })
        );
    }

    #[test]
    fn a_section_lands_where_its_header_maps_it() -> Result<(), &'static str> {
        let bytes = executable();
        let headers = Headers::read(&bytes)?;
        let mut image = vec![0u8; headers.size];
        headers.copy(&bytes, &mut image);
        assert_eq!(&image[0x1000..0x1004], b"code");
        assert_eq!(image[..0x200], bytes[..0x200]);
        Ok(())
    }

    #[test]
    fn truncated_headers_name_the_missing_field() {
        assert_eq!(Headers::read(&[0u8; 8]), Err("PE header"));
        assert_eq!(Headers::read(&executable()[..0x90]), Err("image base"));
    }

    #[test]
    fn a_word_past_the_end_reads_as_none() {
        assert_eq!(u32_at(&[1, 0, 0, 0], 0), Some(1));
        assert_eq!(u32_at(&[1, 0, 0], 0), None);
        assert_eq!(u32_at(&[], usize::MAX), None);
    }
}
