//! The `.enshpt` section of a mod binary, where each shipped point, event
//! and call writes its schema and `declare_mod!` its head, for the
//! manifest and for `cargo enshwick`, which read it alike.
//!
//! Entries are UTF-8 text separated by NULs, and a linker may pad between
//! them with more NULs. A head starts with `schema = `; an entry with its
//! `[[point]]`, `[[event]]` or `[[call]]` table and its `name`.

use crate::__private::is_own;

/// What an entry declares; entries of a schema text go in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// An extension point.
    Point,
    /// A mod event.
    Event,
    /// A call.
    Call,
}

impl Kind {
    /// Each kind with the table its entry starts with.
    const TABLES: [(Self, &'static str); 3] = [
        (Self::Point, "[[point]]\n"),
        (Self::Event, "[[event]]\n"),
        (Self::Call, "[[call]]\n"),
    ];
}

/// One declaration's schema in the section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry<'a> {
    /// What it declares.
    pub kind: Kind,
    /// Its full name, `<mod id>:<what>`.
    pub name: &'a str,
    /// Its tables, as the schema text holds them.
    pub text: &'a str,
}

/// The section, split into heads and entries once.
#[derive(Clone, Debug, Default)]
pub struct Section<'a> {
    heads: Vec<&'a str>,
    entries: Vec<Entry<'a>>,
}

/// Why the bytes of a section do not read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ReadSectionError {
    /// A part is no UTF-8 text.
    #[error("the point section's part at byte {0} is no text")]
    NotText(usize),
    /// A part is neither a head nor a point, event or call with a name.
    #[error("the point section's part at byte {0} is no head and no named point, event or call")]
    Damaged(usize),
    /// A head carries its tables itself, as a mod built before its
    /// declarations wrote their own entries does.
    #[error("the point section is of an older enshwick: rebuild its mod with this one")]
    Outdated,
}

impl<'a> Section<'a> {
    /// Splits `bytes` into heads and entries.
    ///
    /// # Errors
    ///
    /// [`ReadSectionError`] for a part that is no text, no head and no
    /// named entry, or a head with tables of its own.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::schema_section::{Kind, Section};
    ///
    /// let section = Section::parse(b"\0schema = 1\nmod = \"you.a\"\n\0\0[[event]]\nname = \"you.a:sold\"\n\0")?;
    /// assert_eq!(section.heads(), ["schema = 1\nmod = \"you.a\"\n"]);
    /// assert_eq!(section.entries()[0].kind, Kind::Event);
    /// assert!(Section::parse(b"[[type]]\n").is_err());
    /// # Ok::<(), enshwick::schema_section::ReadSectionError>(())
    /// ```
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ReadSectionError> {
        let mut section = Self::default();
        let mut at = 0;
        for part in bytes.split(|&b| b == 0) {
            let start = at;
            at += part.len() + 1;
            if part.is_empty() {
                continue;
            }
            let text = core::str::from_utf8(part).map_err(|_| ReadSectionError::NotText(start))?;
            if text.starts_with("schema = ") {
                if text.contains("\n[[") {
                    return Err(ReadSectionError::Outdated);
                }
                section.heads.push(text);
                continue;
            }
            let kind = Kind::TABLES
                .iter()
                .find(|(_, table)| text.starts_with(table))
                .map(|&(kind, _)| kind)
                .ok_or(ReadSectionError::Damaged(start))?;
            let name = text
                .split_once("\nname = \"")
                .and_then(|(_, rest)| rest.split_once('"'))
                .map(|(name, _)| name)
                .filter(|name| !name.is_empty())
                .ok_or(ReadSectionError::Damaged(start))?;
            section.entries.push(Entry { kind, name, text });
        }
        Ok(section)
    }

    /// The heads, one per `declare_mod!` of the binary.
    #[must_use]
    pub fn heads(&self) -> &[&'a str] {
        &self.heads
    }

    /// The entries, in the section's order.
    #[must_use]
    pub fn entries(&self) -> &[Entry<'a>] {
        &self.entries
    }

    /// The schema text of the mod `id`: `head`, then its entries, those
    /// whose local name `exports` lists when it is given, by kind and
    /// name; empty when none is left.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::schema_section::Section;
    ///
    /// let section = Section::parse(
    ///     b"[[event]]\nname = \"you.a:sold\"\n\0[[point]]\nname = \"you.a:price\"\n\0[[point]]\nname = \"you.b:price\"\n",
    /// )?;
    /// assert_eq!(
    ///     section.schema("schema = 1\n", "you.a", Some(&["sold"])),
    ///     "schema = 1\n[[event]]\nname = \"you.a:sold\"\n",
    /// );
    /// assert_eq!(section.schema("schema = 1\n", "you.c", None), "");
    /// # Ok::<(), enshwick::schema_section::ReadSectionError>(())
    /// ```
    #[must_use]
    pub fn schema(&self, head: &str, id: &str, exports: Option<&[&str]>) -> String {
        let exported = |name: &str| {
            exports.is_none_or(|exports| {
                name.split_once(':')
                    .is_some_and(|(_, local)| exports.contains(&local))
            })
        };
        let mut own: Vec<&Entry<'a>> = self
            .entries
            .iter()
            .filter(|e| is_own(id, e.name) && exported(e.name))
            .collect();
        if own.is_empty() {
            return String::new();
        }
        own.sort_unstable_by_key(|e| (e.kind, e.name));
        let mut text = head.to_owned();
        for entry in own {
            text.push_str(entry.text);
        }
        text
    }
}

/// The mod id a head names, its `mod = "<id>"` line.
///
/// # Examples
///
/// ```
/// use enshwick::schema_section::head_id;
///
/// assert_eq!(head_id("schema = 1\nmod = \"you.a\"\nversion = \"1.0.0\"\n"), Some("you.a"));
/// assert_eq!(head_id("schema = 1\n"), None);
/// ```
#[must_use]
pub fn head_id(head: &str) -> Option<&str> {
    head.lines()
        .find_map(|line| line.strip_prefix("mod = \"")?.strip_suffix('"'))
}
