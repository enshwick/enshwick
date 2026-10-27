//! Many patterns found in one pass over the code.

use aho_corasick::{AhoCorasick, AhoCorasickKind};
use rustc_hash::FxHashMap;

use super::{FindError, Pattern};

/// The fewest bytes worth a thread of their own.
const MIN_CHUNK: usize = 1 << 20;

/// The one offset in `code` where each of `patterns` matches, in their
/// order: one pass over `code` for all of them, where
/// [`Pattern::find_unique`] makes one pass per pattern.
///
/// The longest fixed run of every pattern goes into one automaton, the code
/// is split across the cores, and each pattern is compared in full where its
/// run occurs, so each result is what [`Pattern::find_unique`] gives.
///
/// # Examples
///
/// ```
/// use enshwick::code::{FindError, Pattern, find_each_unique};
///
/// let call = Pattern::parse("E8 ?? ?? ?? ?? C3")?;
/// let ret = Pattern::parse("C3")?;
/// let code = [0x90, 0xe8, 1, 2, 3, 4, 0xc3, 0xc3];
/// assert_eq!(
///     find_each_unique(&[&call, &ret], &code),
///     [Ok(1), Err(FindError::Many)]
/// );
/// # Ok::<(), enshwick::code::PatternError>(())
/// ```
#[must_use]
pub fn find_each_unique(patterns: &[&Pattern], code: &[u8]) -> Vec<Result<usize, FindError>> {
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    find_on(patterns, code, threads, MIN_CHUNK)
}

/// [`find_each_unique`] on at most `threads` threads, at most 16, each given
/// at least `min_chunk` bytes.
fn find_on(
    patterns: &[&Pattern],
    code: &[u8],
    threads: usize,
    min_chunk: usize,
) -> Vec<Result<usize, FindError>> {
    let Some(anchors) = Anchors::new(patterns) else {
        return patterns.iter().map(|p| p.find_unique(code)).collect();
    };
    let chunk = code
        .len()
        .div_ceil(threads.clamp(1, 16))
        .max(min_chunk.max(1));
    let tallies: Option<Vec<Vec<Seen>>> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..code.len().div_ceil(chunk).max(1))
            .map(|i| {
                let anchors = &anchors;
                let from = i * chunk;
                let to = from.saturating_add(chunk).min(code.len());
                scope.spawn(move || anchors.tally(code, from, to))
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().ok().flatten())
            .collect()
    });
    let Some(tallies) = tallies else {
        return patterns.iter().map(|p| p.find_unique(code)).collect();
    };
    let mut seen = vec![Seen::None; patterns.len()];
    for tally in tallies {
        for (total, part) in seen.iter_mut().zip(tally) {
            *total = total.and(part);
        }
    }
    seen.into_iter().map(Seen::unique).collect()
}

/// How often a pattern matched so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Seen {
    None,
    Once(usize),
    Many,
}

impl Seen {
    /// Both counts together; two parts never see the same offset.
    const fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::None, seen) | (seen, Self::None) => seen,
            _ => Self::Many,
        }
    }

    const fn unique(self) -> Result<usize, FindError> {
        match self {
            Self::None => Err(FindError::None),
            Self::Once(at) => Ok(at),
            Self::Many => Err(FindError::Many),
        }
    }
}

/// The distinct anchors of a set of patterns in one automaton.
struct Anchors<'p> {
    automaton: AhoCorasick,
    /// For each anchor, the patterns whose anchor it is.
    users: Vec<Vec<usize>>,
    patterns: &'p [&'p Pattern],
    /// The longest anchor, which a part's search runs past its end by.
    longest: usize,
}

impl<'p> Anchors<'p> {
    /// `None` when the automaton cannot be built, so each pattern searches
    /// on its own.
    fn new(patterns: &'p [&'p Pattern]) -> Option<Self> {
        let mut index: FxHashMap<Vec<u8>, usize> = FxHashMap::default();
        let mut needles: Vec<Vec<u8>> = Vec::new();
        let mut users: Vec<Vec<usize>> = Vec::new();
        for (i, pattern) in patterns.iter().enumerate() {
            let needle: Vec<u8> = pattern.bytes[pattern.anchor.clone()]
                .iter()
                .map(|b| b.unwrap_or(0))
                .collect();
            let anchor = *index.entry(needle.clone()).or_insert_with(|| {
                needles.push(needle);
                users.push(Vec::new());
                users.len() - 1
            });
            users.get_mut(anchor)?.push(i);
        }
        let longest = needles.iter().map(Vec::len).max().unwrap_or(0);
        // The DFA scans the code about three times as fast as the default
        // NFA and builds in about 2 ms for 600 anchors.
        let automaton = AhoCorasick::builder()
            .kind(Some(AhoCorasickKind::DFA))
            .build(&needles)
            .ok()?;
        Some(Self {
            automaton,
            users,
            patterns,
            longest,
        })
    }

    /// What each pattern matches in `code` whose anchor starts in
    /// `from..to`; `None` when the automaton refuses the search.
    fn tally(&self, code: &[u8], from: usize, to: usize) -> Option<Vec<Seen>> {
        let mut seen = vec![Seen::None; self.patterns.len()];
        let end = to.saturating_add(self.longest).min(code.len());
        let hay = code.get(from..end)?;
        for hit in self.automaton.try_find_overlapping_iter(hay).ok()? {
            let at = from + hit.start();
            if at >= to {
                continue;
            }
            for &i in self.users.get(hit.pattern().as_usize())? {
                let pattern = self.patterns.get(i)?;
                let Some(start) = at.checked_sub(pattern.anchor.start) else {
                    continue;
                };
                if code.get(start..).is_some_and(|rest| pattern.matches(rest)) {
                    let slot = seen.get_mut(i)?;
                    *slot = slot.and(Seen::Once(start));
                }
            }
        }
        Some(seen)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn each(
        patterns: &[Pattern],
        code: &[u8],
        threads: usize,
        min_chunk: usize,
    ) -> Vec<Result<usize, FindError>> {
        let refs: Vec<&Pattern> = patterns.iter().collect();
        find_on(&refs, code, threads, min_chunk)
    }

    fn one_by_one(patterns: &[Pattern], code: &[u8]) -> Vec<Result<usize, FindError>> {
        patterns.iter().map(|p| p.find_unique(code)).collect()
    }

    #[test]
    fn patterns_sharing_an_anchor_are_each_checked_in_full() {
        let patterns = [
            Pattern::parse("48 8B 05 ?? ?? ?? ?? C3").unwrap(),
            Pattern::parse("48 8B 05 ?? ?? ?? ?? 90").unwrap(),
            Pattern::parse("?? 48 8B 05").unwrap(),
        ];
        let code = [
            0xcc, 0x48, 0x8b, 0x05, 1, 2, 3, 4, 0xc3, 0x48, 0x8b, 0x05, 0, 0, 0, 0, 0x90,
        ];
        assert_eq!(
            each(&patterns, &code, 1, 1),
            [Ok(1), Ok(9), Err(FindError::Many)]
        );
    }

    #[test]
    fn a_match_across_a_part_boundary_counts_once() {
        let patterns = [Pattern::parse("11 22 33 44 ?? 55").unwrap()];
        let mut code = vec![0u8; 64];
        code[30..36].copy_from_slice(&[0x11, 0x22, 0x33, 0x44, 0, 0x55]);
        for chunk in 1..code.len() {
            assert_eq!(
                each(&patterns, &code, 16, chunk),
                [Ok(30)],
                "parts of {chunk}"
            );
        }
    }

    #[test]
    fn an_anchor_before_its_lead_bytes_is_no_match() {
        let patterns = [Pattern::parse("?? ?? C3 C3").unwrap()];
        assert_eq!(
            each(&patterns, &[0xc3, 0xc3, 0x90], 1, 1),
            [Err(FindError::None)]
        );
    }

    #[test]
    fn no_patterns_and_no_code_find_nothing() {
        assert!(each(&[], &[1, 2, 3], 4, 1).is_empty());
        let patterns = [Pattern::parse("C3").unwrap()];
        assert_eq!(each(&patterns, &[], 4, 1), [Err(FindError::None)]);
    }

    fn pattern() -> impl Strategy<Value = Pattern> {
        prop::collection::vec(prop::option::weighted(0.8, 0u8..4), 1..6).prop_filter_map(
            "a fixed byte",
            |bytes| {
                let text: Vec<String> = bytes
                    .iter()
                    .map(|b| b.map_or_else(|| "??".to_owned(), |b| format!("{b:02x}")))
                    .collect();
                Pattern::parse(&text.join(" ")).ok()
            },
        )
    }

    proptest! {
        #[test]
        fn one_pass_finds_what_each_pattern_finds_alone(
            patterns in prop::collection::vec(pattern(), 0..12),
            code in prop::collection::vec(0u8..4, 0..300),
            threads in 1usize..6,
            chunk in 1usize..80,
        ) {
            prop_assert_eq!(each(&patterns, &code, threads, chunk), one_by_one(&patterns, &code));
        }
    }
}
