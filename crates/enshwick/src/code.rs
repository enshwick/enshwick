//! Byte patterns that find game code without fixed addresses.
//!
//! A pattern is hex bytes separated by spaces, with `??` for a byte that may
//! be anything, such as the parts of an instruction that change between game
//! builds. [`Load::find_code`](crate::Load::find_code) searches the game's
//! code for one and asks for exactly one match, so a patch never lands on the
//! wrong place.

/// A byte pattern with wildcards.
///
/// # Examples
///
/// ```
/// use enshwick::code::Pattern;
///
/// let pattern = Pattern::parse("B9 ?? 02 00 00")?;
/// let code = [0x90, 0xb9, 0x00, 0x02, 0x00, 0x00, 0xc3];
/// assert_eq!(pattern.find_unique(&code), Ok(1));
/// # Ok::<(), enshwick::code::PatternError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    /// Each byte, `None` for a wildcard.
    bytes: Vec<Option<u8>>,
    /// The longest run of fixed bytes, which a search looks for first.
    anchor: core::ops::Range<usize>,
}

/// A pattern that does not parse.
///
/// # Examples
///
/// ```
/// use enshwick::code::Pattern;
///
/// assert!(Pattern::parse("B9 0").is_err());
/// ```
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PatternError {
    /// No byte at all, or only wildcards.
    #[error("a pattern needs at least one fixed byte")]
    Empty,
    /// A token that is neither two hex digits nor `??`.
    #[error("{0:?} is neither two hex digits nor ??")]
    Token(String),
}

/// Why a search did not find exactly one match.
///
/// # Examples
///
/// ```
/// use enshwick::code::{Pattern, FindError};
///
/// let pattern = Pattern::parse("C3")?;
/// assert_eq!(pattern.find_unique(&[0x90]), Err(FindError::None));
/// assert_eq!(pattern.find_unique(&[0xc3, 0xc3]), Err(FindError::Many));
/// # Ok::<(), enshwick::code::PatternError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum FindError {
    /// Nothing matches.
    #[error("no match")]
    None,
    /// More than one place matches.
    #[error("more than one match")]
    Many,
}

impl Pattern {
    /// Parses hex bytes separated by spaces, `??` or `?` for any byte.
    ///
    /// # Errors
    ///
    /// [`PatternError`] for a malformed token or a pattern without a fixed
    /// byte.
    ///
    /// # Examples
    ///
    /// See [`Pattern`].
    pub fn parse(text: &str) -> Result<Self, PatternError> {
        let bytes = text
            .split_ascii_whitespace()
            .map(|token| match token {
                "?" | "??" => Ok(None),
                // `from_str_radix` takes a sign, as in `+F`.
                _ if token.len() == 2 && token.bytes().all(|b| b.is_ascii_hexdigit()) => {
                    u8::from_str_radix(token, 16)
                        .map(Some)
                        .map_err(|_| PatternError::Token(token.to_owned()))
                }
                _ => Err(PatternError::Token(token.to_owned())),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let anchor = longest_run(&bytes).ok_or(PatternError::Empty)?;
        Ok(Self { bytes, anchor })
    }

    /// Whether the byte at `at` must match; `None` past the pattern's end.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::code::Pattern;
    ///
    /// let pattern = Pattern::parse("E8 ?? C3")?;
    /// assert_eq!(pattern.is_fixed(0), Some(true));
    /// assert_eq!(pattern.is_fixed(1), Some(false));
    /// assert_eq!(pattern.is_fixed(3), None);
    /// # Ok::<(), enshwick::code::PatternError>(())
    /// ```
    #[must_use]
    pub fn is_fixed(&self, at: usize) -> Option<bool> {
        self.bytes.get(at).map(Option::is_some)
    }

    /// How many bytes the pattern spans.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::code::Pattern;
    ///
    /// assert_eq!(Pattern::parse("0F 84 ?? ?? ?? ??")?.len(), 6);
    /// # Ok::<(), enshwick::code::PatternError>(())
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Always false: a parsed pattern has a fixed byte.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::code::Pattern;
    ///
    /// assert!(!Pattern::parse("C3")?.is_empty());
    /// # Ok::<(), enshwick::code::PatternError>(())
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Whether `code` starts with the pattern.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::code::Pattern;
    ///
    /// let pattern = Pattern::parse("48 ?? 05")?;
    /// assert!(pattern.matches(&[0x48, 0x8d, 0x05, 0xff]));
    /// assert!(!pattern.matches(&[0x48, 0x8d]));
    /// # Ok::<(), enshwick::code::PatternError>(())
    /// ```
    #[must_use]
    pub fn matches(&self, code: &[u8]) -> bool {
        code.len() >= self.bytes.len()
            && self
                .bytes
                .iter()
                .zip(code)
                .all(|(want, &have)| want.is_none_or(|want| want == have))
    }

    /// Every offset in `code` where the pattern matches, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::code::Pattern;
    ///
    /// let pattern = Pattern::parse("CC ?? CC")?;
    /// let found: Vec<usize> = pattern.find_all(&[0xcc, 1, 0xcc, 2, 0xcc]).collect();
    /// assert_eq!(found, [0, 2]);
    /// # Ok::<(), enshwick::code::PatternError>(())
    /// ```
    pub fn find_all<'a>(&'a self, code: &'a [u8]) -> impl Iterator<Item = usize> + 'a {
        // The longest fixed run through memchr's SIMD search, then the full
        // compare at each hit.
        let needle: Vec<u8> = self.bytes[self.anchor.clone()]
            .iter()
            .map(|b| b.unwrap_or(0))
            .collect();
        let lead = self.anchor.start;
        let hay = code.get(lead..).unwrap_or_default();
        let finder = memchr::memmem::Finder::new(&needle).into_owned();
        let mut from = 0;
        core::iter::from_fn(move || {
            while let Some(hit) = hay.get(from..).and_then(|rest| finder.find(rest)) {
                let start = from + hit;
                from = start + 1;
                if code.get(start..).is_some_and(|rest| self.matches(rest)) {
                    return Some(start);
                }
            }
            None
        })
    }

    /// The one offset in `code` where the pattern matches.
    ///
    /// # Errors
    ///
    /// [`FindError::None`] when nothing matches, [`FindError::Many`] when more than
    /// one place does.
    ///
    /// # Examples
    ///
    /// See [`Pattern`].
    pub fn find_unique(&self, code: &[u8]) -> Result<usize, FindError> {
        let mut found = self.find_all(code);
        let first = found.next().ok_or(FindError::None)?;
        match found.next() {
            Some(_) => Err(FindError::Many),
            None => Ok(first),
        }
    }
}

/// The longest run of fixed bytes in `bytes`; `None` when every byte is a
/// wildcard.
fn longest_run(bytes: &[Option<u8>]) -> Option<core::ops::Range<usize>> {
    let mut best: Option<core::ops::Range<usize>> = None;
    let mut start = None;
    for (i, byte) in bytes.iter().chain([&None]).enumerate() {
        match (byte, start) {
            (Some(_), None) => start = Some(i),
            (None, Some(from)) => {
                if best.as_ref().is_none_or(|b| i - from > b.len()) {
                    best = Some(from..i);
                }
                start = None;
            }
            _ => {}
        }
    }
    best
}

impl core::str::FromStr for Pattern {
    type Err = PatternError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards_match_any_byte_and_fixed_bytes_only_themselves() {
        let pattern = Pattern::parse("4C 8D 05 ?? ?? ?? ?? 33 C9").unwrap();
        let mut code = vec![0x90; 64];
        code[20..29].copy_from_slice(&[0x4c, 0x8d, 0x05, 1, 2, 3, 4, 0x33, 0xc9]);
        assert_eq!(pattern.find_unique(&code), Ok(20));
        code[28] = 0xca;
        assert_eq!(pattern.find_unique(&code), Err(FindError::None));
    }

    #[test]
    fn a_second_match_is_ambiguous() {
        let pattern = Pattern::parse("66 3B C1").unwrap();
        let code = [0x66, 0x3b, 0xc1, 0x90, 0x66, 0x3b, 0xc1];
        assert_eq!(pattern.find_unique(&code), Err(FindError::Many));
        assert_eq!(pattern.find_all(&code).collect::<Vec<_>>(), [0, 4]);
    }

    #[test]
    fn a_match_may_end_at_the_last_byte_but_not_beyond() {
        let pattern = Pattern::parse("?? C3").unwrap();
        assert_eq!(pattern.find_unique(&[0x90, 0x90, 0xc3]), Ok(1));
        assert_eq!(pattern.find_unique(&[0xc3]), Err(FindError::None));
        assert_eq!(pattern.find_unique(&[]), Err(FindError::None));
    }

    #[test]
    fn malformed_patterns_are_refused() {
        assert_eq!(Pattern::parse(""), Err(PatternError::Empty));
        assert_eq!(Pattern::parse("?? ??"), Err(PatternError::Empty));
        assert_eq!(
            Pattern::parse("0F 8"),
            Err(PatternError::Token("8".to_owned()))
        );
        assert_eq!(
            Pattern::parse("0G"),
            Err(PatternError::Token("0G".to_owned()))
        );
        assert_eq!(
            Pattern::parse("90 +F"),
            Err(PatternError::Token("+F".to_owned()))
        );
        assert_eq!("e9 ?".parse::<Pattern>().unwrap().len(), 2);
    }
}
