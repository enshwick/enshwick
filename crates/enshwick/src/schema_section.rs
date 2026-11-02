//! The `.enshpt` section of a mod binary, for the manifest and for
//! `cargo enshwick`, which read it alike. Each exported point, event and
//! call writes its schema there, each exported service its name, and
//! `declare_mod!` its head, which names the mod and the content and stat
//! kinds its `exports` lists.
//!
//! Entries are UTF-8 text separated by NULs, and a linker may pad between
//! them with more NULs. A head starts with `schema = ` and names its mod;
//! an entry starts with its table and holds its `name`. A new kind of
//! table comes with a new [`FORMAT`], so a table this reader does not know
//! is damage.

use crate::__private::is_own;

/// The schema format a head states, which `cargo enshwick` checks.
pub const FORMAT: u32 = 2;

/// What a mod exports, and what an entry declares; entries of a schema text
/// go in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Kind {
    /// An extension point.
    Point,
    /// A mod event.
    Event,
    /// A call.
    Call,
    /// A service table, whose entry carries no schema.
    Service,
    /// A content id, exported by `exports` and never an entry.
    Content,
    /// A stat kind, exported by `exports` and never an entry.
    Stat,
}

impl Kind {
    /// Every kind, in order.
    pub const ALL: [Self; 6] = [
        Self::Point,
        Self::Event,
        Self::Call,
        Self::Service,
        Self::Content,
        Self::Stat,
    ];

    /// Each kind of an entry with the table it starts with.
    const TABLES: [(Self, &'static str); 4] = [
        (Self::Point, "[[point]]\n"),
        (Self::Event, "[[event]]\n"),
        (Self::Call, "[[call]]\n"),
        (Self::Service, "[[service]]\n"),
    ];

    /// The word before `:` in an export of this kind, as `point:price`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::schema_section::Kind;
    ///
    /// assert_eq!(Kind::Content.word(), "content");
    /// ```
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Point => "point",
            Self::Event => "event",
            Self::Call => "call",
            Self::Service => "service",
            Self::Content => "content",
            Self::Stat => "stat",
        }
    }

    /// What a message calls a name of this kind, as
    /// `content id you.lockers:key`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::schema_section::Kind;
    ///
    /// assert_eq!(Kind::Content.label(), "content id");
    /// ```
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Content => "content id",
            other => other.word(),
        }
    }

    /// The kind whose word is `word`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::schema_section::Kind;
    ///
    /// assert_eq!(Kind::of_word("stat"), Some(Kind::Stat));
    /// assert_eq!(Kind::of_word("greeting"), None);
    /// ```
    #[must_use]
    pub fn of_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.word() == word)
    }

    /// The kind and local name of `export`, `<kind>:<local name>`; `None`
    /// when it names no kind of [`ALL`](Self::ALL).
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::schema_section::Kind;
    ///
    /// assert_eq!(Kind::of_export("event:sold"), Some((Kind::Event, "sold")));
    /// assert_eq!(Kind::of_export("sold"), None);
    /// ```
    #[must_use]
    pub fn of_export(export: &str) -> Option<(Self, &str)> {
        let (word, local) = export.split_once(':')?;
        Some((Self::of_word(word)?, local))
    }

    /// Every kind's word, joined by `, `, for messages.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::schema_section::Kind;
    ///
    /// assert!(Kind::words().starts_with("point, event"));
    /// ```
    #[must_use]
    pub fn words() -> String {
        Self::ALL.map(Self::word).join(", ")
    }
}

/// One exported declaration in the section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry<'a> {
    /// What it declares.
    pub kind: Kind,
    /// Its full name, `<mod id>:<what>`.
    pub name: &'a str,
    /// Its tables, as the schema text holds them.
    pub text: &'a str,
}

impl<'a> Entry<'a> {
    /// The local name, after `<mod id>:`.
    ///
    /// # Examples
    ///
    /// See [`export`](Self::export).
    #[must_use]
    pub fn local(&self) -> &'a str {
        self.name
            .split_once(':')
            .map_or(self.name, |(_, local)| local)
    }

    /// The export of it, `<kind>:<local name>`, as a manifest lists it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::schema_section::Section;
    ///
    /// let section = Section::parse(b"[[call]]\nname = \"you.a:fee\"\n")?;
    /// let fee = section.entries()[0];
    /// assert_eq!(fee.local(), "fee");
    /// assert_eq!(fee.export(), "call:fee");
    /// # Ok::<(), enshwick::schema_section::ReadSectionError>(())
    /// ```
    #[must_use]
    pub fn export(&self) -> String {
        format!("{}:{}", self.kind.word(), self.local())
    }
}

/// The head one `declare_mod!` wrote, which names its mod and the content
/// and stat kinds it exports.
///
/// # Examples
///
/// ```
/// use enshwick::schema_section::Section;
///
/// let section = Section::parse(b"schema = 2\nmod = \"you.a\"\nexports = [\"content:locker\"]\n")?;
/// let head = section.heads()[0];
/// assert_eq!(head.id, "you.a");
/// assert_eq!(head.exports(), ["content:locker"]);
/// # Ok::<(), enshwick::schema_section::ReadSectionError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Head<'a> {
    /// Its text, the start of the mod's schema text.
    pub text: &'a str,
    /// The mod id it names.
    pub id: &'a str,
}

impl Head<'_> {
    /// The content and stat kinds its `exports = [..]` line names, each
    /// `<kind>:<name>`; empty without the line.
    ///
    /// # Examples
    ///
    /// See [`Head`].
    #[must_use]
    pub fn exports(&self) -> Vec<String> {
        toml::from_str::<toml::Table>(self.text)
            .ok()
            .and_then(|head| match head.get("exports")? {
                toml::Value::Array(names) => Some(
                    names
                        .iter()
                        .filter_map(|name| name.as_str().map(str::to_owned))
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default()
    }
}

/// The section, split into heads and entries once.
#[derive(Clone, Debug, Default)]
pub struct Section<'a> {
    heads: Vec<Head<'a>>,
    entries: Vec<Entry<'a>>,
}

/// Why the bytes of a section do not read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ReadSectionError {
    /// A part is no UTF-8 text.
    #[error("the point section's part at byte {0} is no text")]
    NotText(usize),
    /// A part is neither a head naming its mod nor a table with a name.
    #[error("the point section's part at byte {0} is no head and no named table")]
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
    /// [`ReadSectionError`] for a part that is no text, no head naming its
    /// mod and no named table, or a head with tables of its own.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::schema_section::{Kind, Section};
    ///
    /// let section = Section::parse(
    ///     b"\0schema = 2\nmod = \"you.a\"\n\0\0[[event]]\nname = \"you.a:sold\"\n\0",
    /// )?;
    /// assert_eq!(section.heads()[0].id, "you.a");
    /// assert_eq!(section.entries()[0].kind, Kind::Event);
    /// assert!(Section::parse(b"[[future]]\nname = \"you.a:x\"\n").is_err());
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
                let id = head_id(text).ok_or(ReadSectionError::Damaged(start))?;
                section.heads.push(Head { text, id });
                continue;
            }
            // The entry's own name, before its `[[type]]` and other tables.
            let own = text.split("\n[").next().unwrap_or(text);
            let name = value(own, "name")
                .filter(|name| text.starts_with("[[") && !name.is_empty())
                .ok_or(ReadSectionError::Damaged(start))?;
            // A new kind comes with a new format, which the head reports, so
            // an unknown table of this format is damage.
            let &(kind, _) = Kind::TABLES
                .iter()
                .find(|(_, table)| text.starts_with(table))
                .ok_or(ReadSectionError::Damaged(start))?;
            section.entries.push(Entry { kind, name, text });
        }
        Ok(section)
    }

    /// The heads, one per `declare_mod!` of the binary.
    #[must_use]
    pub fn heads(&self) -> &[Head<'a>] {
        &self.heads
    }

    /// The entries, in the section's order.
    #[must_use]
    pub fn entries(&self) -> &[Entry<'a>] {
        &self.entries
    }

    /// The exports the mod `id`'s entries make, each `<kind>:<local name>`,
    /// sorted and each once; its content and stat exports are not entries
    /// and are not among them.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::schema_section::Section;
    ///
    /// let section = Section::parse(
    ///     b"[[service]]\nname = \"you.a:prices\"\n\0[[point]]\nname = \"you.a:price\"\n\0[[point]]\nname = \"you.b:fee\"\n",
    /// )?;
    /// assert_eq!(section.exports("you.a"), ["point:price", "service:prices"]);
    /// # Ok::<(), enshwick::schema_section::ReadSectionError>(())
    /// ```
    #[must_use]
    pub fn exports(&self, id: &str) -> Vec<String> {
        let mut names: Vec<String> = self
            .entries
            .iter()
            .filter(|e| is_own(id, e.name))
            .map(Entry::export)
            .collect();
        names.sort_unstable();
        names.dedup();
        names
    }

    /// The schema text of the mod `id`: `head`, then its points, events and
    /// calls by kind, name and text; empty when it has none.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::schema_section::Section;
    ///
    /// let section = Section::parse(
    ///     b"[[event]]\nname = \"you.a:sold\"\n\0[[service]]\nname = \"you.a:prices\"\n\0[[point]]\nname = \"you.b:price\"\n",
    /// )?;
    /// assert_eq!(
    ///     section.schema("schema = 2\n", "you.a"),
    ///     "schema = 2\n[[event]]\nname = \"you.a:sold\"\n",
    /// );
    /// assert_eq!(section.schema("schema = 2\n", "you.c"), "");
    /// # Ok::<(), enshwick::schema_section::ReadSectionError>(())
    /// ```
    #[must_use]
    pub fn schema(&self, head: &str, id: &str) -> String {
        let mut own: Vec<&Entry<'a>> = self
            .entries
            .iter()
            .filter(|e| e.kind != Kind::Service && is_own(id, e.name))
            .collect();
        if own.is_empty() {
            return String::new();
        }
        // Two entries of one name, as two majors of a call, keep an order
        // of their own text, whatever the linker's.
        own.sort_unstable_by_key(|e| (e.kind, e.name, e.text));
        own.dedup_by_key(|e| e.text);
        let mut text = head.to_owned();
        for entry in own {
            text.push_str(entry.text);
        }
        text
    }
}

/// The text of the first `<key> = "<value>"` line of `text`.
fn value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        line.strip_prefix(key)?
            .strip_prefix(" = \"")?
            .strip_suffix('"')
    })
}

/// The mod id a head names, its `mod = "<id>"` line.
///
/// # Examples
///
/// ```
/// use enshwick::schema_section::head_id;
///
/// assert_eq!(head_id("schema = 2\nmod = \"you.a\"\nversion = \"1.0.0\"\n"), Some("you.a"));
/// assert_eq!(head_id("schema = 2\n"), None);
/// ```
#[must_use]
pub fn head_id(head: &str) -> Option<&str> {
    value(head, "mod")
}

#[cfg(test)]
mod tests {
    use super::Section;

    #[test]
    fn the_macros_know_every_export_kind_and_no_other() {
        assert_eq!(
            enshwick_macros::__export_kinds!(),
            super::Kind::ALL.map(super::Kind::word)
        );
    }

    #[test]
    fn a_table_of_an_unknown_kind_or_without_a_name_is_damaged() {
        assert!(Section::parse(b"[[future]]\nname = \"you.a:x\"\nwhat = 1\n").is_err());
        assert!(Section::parse(b"[[future]]\nwhat = 1\n").is_err());
    }

    #[test]
    fn a_table_whose_name_is_only_in_a_nested_table_is_damaged() {
        let nested = b"[[point]]\nmode = \"override\"\n[[type]]\nname = \"you.a:price\"\n";
        assert!(Section::parse(nested).is_err());
        let own =
            Section::parse(b"[[point]]\nname = \"you.a:price\"\n[[type]]\nname = \"Price\"\n")
                .unwrap();
        assert_eq!(own.entries()[0].name, "you.a:price");
    }
}
