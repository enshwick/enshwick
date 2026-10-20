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
/// use enshwick_api::code::Pattern;
///
/// let pattern = Pattern::parse("B9 ?? 02 00 00").unwrap();
/// let code = [0x90, 0xb9, 0x00, 0x02, 0x00, 0x00, 0xc3];
/// assert_eq!(pattern.find_unique(&code), Ok(1));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    /// Each byte, `None` for a wildcard.
    bytes: Vec<Option<u8>>,
}

/// A pattern that does not parse.
///
/// # Examples
///
/// ```
/// use enshwick_api::code::Pattern;
///
/// assert!(Pattern::parse("B9 0").is_err());
/// ```
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
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
/// use enshwick_api::code::{Pattern, Unique};
///
/// let pattern = Pattern::parse("C3").unwrap();
/// assert_eq!(pattern.find_unique(&[0x90]), Err(Unique::None));
/// assert_eq!(pattern.find_unique(&[0xc3, 0xc3]), Err(Unique::Many));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Unique {
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
                _ if token.len() == 2 => u8::from_str_radix(token, 16)
                    .map(Some)
                    .map_err(|_| PatternError::Token(token.to_owned())),
                _ => Err(PatternError::Token(token.to_owned())),
            })
            .collect::<Result<Vec<_>, _>>()?;
        if bytes.iter().all(Option::is_none) {
            return Err(PatternError::Empty);
        }
        Ok(Self { bytes })
    }

    /// How many bytes the pattern spans.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_api::code::Pattern;
    ///
    /// assert_eq!(Pattern::parse("0F 84 ?? ?? ?? ??").unwrap().len(), 6);
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
    /// use enshwick_api::code::Pattern;
    ///
    /// assert!(!Pattern::parse("C3").unwrap().is_empty());
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
    /// use enshwick_api::code::Pattern;
    ///
    /// let pattern = Pattern::parse("48 ?? 05").unwrap();
    /// assert!(pattern.matches(&[0x48, 0x8d, 0x05, 0xff]));
    /// assert!(!pattern.matches(&[0x48, 0x8d]));
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
    /// use enshwick_api::code::Pattern;
    ///
    /// let pattern = Pattern::parse("CC ?? CC").unwrap();
    /// let found: Vec<usize> = pattern.find_all(&[0xcc, 1, 0xcc, 2, 0xcc]).collect();
    /// assert_eq!(found, [0, 2]);
    /// ```
    pub fn find_all<'a>(&'a self, code: &'a [u8]) -> impl Iterator<Item = usize> + 'a {
        // Anchor on the first fixed byte: a cheap scan, then the full compare.
        let (anchor, first) = self
            .bytes
            .iter()
            .enumerate()
            .find_map(|(i, b)| b.map(|b| (i, b)))
            .unwrap_or((0, 0));
        let starts = code
            .len()
            .checked_sub(self.bytes.len())
            .map_or(0, |last| last + 1);
        (0..starts).filter(move |&at| code[at + anchor] == first && self.matches(&code[at..]))
    }

    /// The one offset in `code` where the pattern matches.
    ///
    /// # Errors
    ///
    /// [`Unique::None`] when nothing matches, [`Unique::Many`] when more than
    /// one place does.
    ///
    /// # Examples
    ///
    /// See [`Pattern`].
    pub fn find_unique(&self, code: &[u8]) -> Result<usize, Unique> {
        let mut found = self.find_all(code);
        let first = found.next().ok_or(Unique::None)?;
        match found.next() {
            Some(_) => Err(Unique::Many),
            None => Ok(first),
        }
    }
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
        assert_eq!(pattern.find_unique(&code), Err(Unique::None));
    }

    #[test]
    fn a_second_match_is_ambiguous() {
        let pattern = Pattern::parse("66 3B C1").unwrap();
        let code = [0x66, 0x3b, 0xc1, 0x90, 0x66, 0x3b, 0xc1];
        assert_eq!(pattern.find_unique(&code), Err(Unique::Many));
        assert_eq!(pattern.find_all(&code).collect::<Vec<_>>(), [0, 4]);
    }

    #[test]
    fn a_match_may_end_at_the_last_byte_but_not_beyond() {
        let pattern = Pattern::parse("?? C3").unwrap();
        assert_eq!(pattern.find_unique(&[0x90, 0x90, 0xc3]), Ok(1));
        assert_eq!(pattern.find_unique(&[0xc3]), Err(Unique::None));
        assert_eq!(pattern.find_unique(&[]), Err(Unique::None));
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
        assert_eq!("e9 ?".parse::<Pattern>().unwrap().len(), 2);
    }
}
