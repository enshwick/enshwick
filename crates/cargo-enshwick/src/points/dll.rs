//! The schemas a mod DLL carries in its `.enshpt` section, read from the
//! file without running it.

use enshwick::schema_section::{Section, head_id};
use enshwick_gamedata::reflect::{Image, Layout};

use super::Error;

/// The section of a mod DLL that holds its point schemas.
pub const SECTION: &str = ".enshpt";

/// The schema text of the mod in the DLL `bytes`: the head its
/// `declare_mod!` wrote and the points, events and calls it exports;
/// `None` for a DLL without the section, its head or any of them.
///
/// # Errors
///
/// [`Error`] when the bytes are no x64 DLL, or a part of the section is
/// damaged or of an older enshwick.
///
/// # Examples
///
/// ```
/// use cargo_enshwick::points::from_dll;
///
/// assert!(from_dll(b"MZ").is_err());
/// ```
pub fn from_dll(bytes: &[u8]) -> Result<Option<String>, Error> {
    let image = Image::parse(bytes, Layout::File, None)?;
    let Some(section) = image.section(SECTION) else {
        return Ok(None);
    };
    let range = image.range(section);
    let raw = image
        .read(range.start, range.end - range.start)
        .ok_or(Error::CutShort)?;
    from_section(raw)
}

/// The schema text in `raw`, the bytes of a mod's `.enshpt` section: the
/// head `declare_mod!` wrote, then the mod's own declarations.
fn from_section(raw: &[u8]) -> Result<Option<String>, Error> {
    let section = Section::parse(raw)?;
    let Some(&head) = section.heads().first() else {
        return Ok(None);
    };
    let id = head_id(head).ok_or_else(|| Error::Toml("the head names no `mod`".to_owned()))?;
    let text = section.schema(head, id, None);
    Ok((!text.is_empty()).then_some(text))
}

#[cfg(test)]
mod tests {
    use enshwick::schema_section::ReadSectionError;

    use super::{Error, from_section};

    #[test]
    fn a_section_reads_as_its_head_and_the_mods_own_declarations() {
        let raw = b"\0\0[[point]]\nname = \"you.lockers:price\"\n\0\
            [[point]]\nname = \"you.other:price\"\n\0\
            schema = 1\nmod = \"you.lockers\"\nversion = \"0.1.0\"\n\0\0\0\
            [[event]]\nname = \"you.lockers:opened\"\n\0\0";
        assert_eq!(
            from_section(raw).unwrap().as_deref(),
            Some(
                "schema = 1\nmod = \"you.lockers\"\nversion = \"0.1.0\"\n\
                 [[point]]\nname = \"you.lockers:price\"\n\
                 [[event]]\nname = \"you.lockers:opened\"\n"
            )
        );
        assert_eq!(
            from_section(b"\0schema = 1\nmod = \"you.a\"\n\0").unwrap(),
            None
        );
        assert_eq!(from_section(b"\0\0").unwrap(), None);
    }

    #[test]
    fn a_damaged_or_older_section_is_refused_with_the_reason() {
        let damaged = from_section(b"schema = 1\nmod = \"you.a\"\n\0[[type]]\nname = \"T\"\n");
        assert!(
            matches!(damaged, Err(Error::Section(ReadSectionError::Damaged(26)))),
            "{damaged:?}"
        );
        let older = from_section(b"schema = 1\nmod = \"you.a\"\n[[point]]\nname = \"you.a:p\"\n")
            .unwrap_err()
            .to_string();
        assert!(older.contains("rebuild"), "{older}");
    }
}
