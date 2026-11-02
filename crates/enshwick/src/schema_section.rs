//! The `.enshpt` section of a mod binary, for the manifest and for
//! `cargo enshwick`, which read it alike. Each exported point, event and
//! call writes its schema there, each exported service its name, each other
//! declaration and content id of the mod `[[own]]` with its name and kind,
//! and `declare_mod!` its head.
//!
//! Entries are UTF-8 text separated by NULs, and a linker may pad between
//! them with more NULs. A head starts with `schema = ` and names its mod;
//! an entry starts with its table and holds its `name`. A table this reader
//! does not know is left out, so a newer kind reads in an older reader.

use crate::__private::is_own;

/// What an entry declares; entries of a schema text go in this order.
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
    /// A content id, whose entry carries no schema.
    Content,
}

impl Kind {
    /// Each kind of an exported entry with the table it starts with.
    const TABLES: [(Self, &'static str); 4] = [
        (Self::Point, "[[point]]\n"),
        (Self::Event, "[[event]]\n"),
        (Self::Call, "[[call]]\n"),
        (Self::Service, "[[service]]\n"),
    ];

    /// The table of an entry the mod keeps for itself.
    const OWN: &'static str = "[[own]]\n";

    /// Each kind with its name in an `[[own]]` entry.
    const LABELS: [(Self, &'static str); 5] = [
        (Self::Point, "point"),
        (Self::Event, "event"),
        (Self::Call, "call"),
        (Self::Service, "service"),
        (Self::Content, "content"),
    ];
}

/// One declaration or content id in the section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry<'a> {
    /// What it declares.
    pub kind: Kind,
    /// Its full name, `<mod id>:<what>`.
    pub name: &'a str,
    /// Its tables, as the schema text holds them.
    pub text: &'a str,
    /// Whether it is marked `export`; a content id is exported by the
    /// mod's `exports` instead.
    pub exported: bool,
}

impl<'a> Entry<'a> {
    /// The local name, after `<mod id>:`.
    fn local(&self) -> &'a str {
        self.name
            .split_once(':')
            .map_or(self.name, |(_, local)| local)
    }
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
    /// A part is neither a head naming its mod nor a table with a name.
    #[error("the point section's part at byte {0} is no head and no named table")]
    Damaged(usize),
    /// A head carries its tables itself, as a mod built before its
    /// declarations wrote their own entries does.
    #[error("the point section is of an older enshwick: rebuild its mod with this one")]
    Outdated,
}

impl<'a> Section<'a> {
    /// Splits `bytes` into heads and entries, leaving out tables of a kind
    /// this reader does not know.
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
    ///     b"\0schema = 1\nmod = \"you.a\"\n\0\0[[event]]\nname = \"you.a:sold\"\n\0[[future]]\nname = \"you.a:x\"\n",
    /// )?;
    /// assert_eq!(section.heads(), ["schema = 1\nmod = \"you.a\"\n"]);
    /// assert_eq!(section.entries().len(), 1);
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
                if head_id(text).is_none() {
                    return Err(ReadSectionError::Damaged(start));
                }
                section.heads.push(text);
                continue;
            }
            let name = value(text, "name")
                .filter(|name| text.starts_with("[[") && !name.is_empty())
                .ok_or(ReadSectionError::Damaged(start))?;
            let exported = Kind::TABLES
                .iter()
                .find(|(_, table)| text.starts_with(table))
                .map(|&(kind, _)| (kind, true));
            let own = || {
                let label = value(text, "kind")?;
                Kind::LABELS
                    .iter()
                    .find(|(_, l)| *l == label)
                    .map(|&(kind, _)| (kind, false))
            };
            let found = exported.or_else(|| text.starts_with(Kind::OWN).then(own).flatten());
            if let Some((kind, exported)) = found {
                section.entries.push(Entry {
                    kind,
                    name,
                    text,
                    exported,
                });
            }
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

    /// The local names the mod `id` exports: `listed`, the content names
    /// its `exports` lists, and those of its declarations marked `export`.
    ///
    /// A name that would also open one of the mod's own the mod does not
    /// export, such as an event beside an exported point of that name, is
    /// left out, since the loader matches exports by name alone.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::schema_section::Section;
    ///
    /// let section = Section::parse(
    ///     b"[[service]]\nname = \"you.a:prices\"\n\0[[point]]\nname = \"you.a:price\"\n\0[[own]]\nname = \"you.a:price\"\nkind = \"event\"\n",
    /// )?;
    /// assert_eq!(section.exports("you.a", &["locker"]), ["locker", "prices"]);
    /// # Ok::<(), enshwick::schema_section::ReadSectionError>(())
    /// ```
    #[must_use]
    pub fn exports(&self, id: &str, listed: &[&'a str]) -> Vec<&'a str> {
        let own: Vec<&Entry<'a>> = self.entries.iter().filter(|e| is_own(id, e.name)).collect();
        let open = |e: &Entry<'a>| {
            e.exported
                || (e.kind == Kind::Content && crate::rules::exports_content(listed, e.local()))
        };
        let reaches = |name: &str, e: &Entry<'a>| {
            if e.kind == Kind::Content {
                crate::rules::exports_content(&[name], e.local())
            } else {
                name == e.local()
            }
        };
        let mut names: Vec<&'a str> = listed.to_vec();
        names.extend(own.iter().filter(|e| e.exported).map(|e| e.local()));
        names.sort_unstable();
        names.dedup();
        names.retain(|name| !own.iter().any(|e| !open(e) && reaches(name, e)));
        names
    }

    /// The schema text of the mod `id`: `head`, then its exported points,
    /// events and calls whose local names `exports` holds, by kind, name and
    /// text; empty when it has none.
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
    ///     section.schema("schema = 1\n", "you.a", &["sold", "prices"]),
    ///     "schema = 1\n[[event]]\nname = \"you.a:sold\"\n",
    /// );
    /// assert_eq!(section.schema("schema = 1\n", "you.a", &[]), "");
    /// # Ok::<(), enshwick::schema_section::ReadSectionError>(())
    /// ```
    #[must_use]
    pub fn schema(&self, head: &str, id: &str, exports: &[&str]) -> String {
        let mut own: Vec<&Entry<'a>> = self
            .entries
            .iter()
            .filter(|e| {
                e.exported
                    && matches!(e.kind, Kind::Point | Kind::Event | Kind::Call)
                    && is_own(id, e.name)
                    && exports.contains(&e.local())
            })
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
/// assert_eq!(head_id("schema = 1\nmod = \"you.a\"\nversion = \"1.0.0\"\n"), Some("you.a"));
/// assert_eq!(head_id("schema = 1\n"), None);
/// ```
#[must_use]
pub fn head_id(head: &str) -> Option<&str> {
    value(head, "mod")
}

#[cfg(test)]
mod tests {
    use super::Section;

    const SECTION: &[u8] = b"[[point]]\nname = \"you.a:price\"\n\0\
        [[own]]\nname = \"you.a:price\"\nkind = \"event\"\n\0\
        [[own]]\nname = \"you.a:fee\"\nkind = \"call\"\n\0\
        [[event]]\nname = \"you.a:fee\"\n\0\
        [[point]]\nname = \"you.a:locker\"\n\0\
        [[own]]\nname = \"you.a:locker.recipe\"\nkind = \"content\"\n\0\
        [[call]]\nname = \"you.a:sold\"\n\0\
        [[event]]\nname = \"you.a:sold\"\n\0";

    #[test]
    fn an_exported_name_that_would_open_an_internal_event_or_call_is_left_out() {
        let section = Section::parse(SECTION).unwrap();
        let exports = section.exports("you.a", &[]);
        assert!(
            !exports.contains(&"price"),
            "the point would open the event"
        );
        assert!(!exports.contains(&"fee"), "the event would open the call");
        assert!(exports.contains(&"sold"), "both of its own are exported");
        let schema = section.schema("schema = 1\n", "you.a", &exports);
        assert!(
            !schema.contains("you.a:price") && !schema.contains("you.a:fee"),
            "{schema}"
        );
    }

    #[test]
    fn an_exported_point_does_not_open_content_the_mod_keeps() {
        let section = Section::parse(SECTION).unwrap();
        assert!(!section.exports("you.a", &[]).contains(&"locker"));
        assert!(section.exports("you.a", &["locker"]).contains(&"locker"));
    }

    #[test]
    fn a_table_of_an_unknown_kind_is_left_out_and_a_nameless_one_is_damaged() {
        let section = Section::parse(b"[[future]]\nname = \"you.a:x\"\nwhat = 1\n").unwrap();
        assert!(section.entries().is_empty());
        assert!(Section::parse(b"[[future]]\nwhat = 1\n").is_err());
        let own = Section::parse(b"[[own]]\nname = \"you.a:x\"\nkind = \"future\"\n").unwrap();
        assert!(own.entries().is_empty());
    }
}
