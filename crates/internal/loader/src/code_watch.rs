//! Game code changed around the API, put down to the mod that changed it.
//!
//! A copy of the game's `.text`, taken as the loader starts, is compared
//! after each mod loads, initialises and gets ready. The copy lives only
//! while the loader starts; the changes found stay for the conflict report.

/// Changed bytes closer than this join one range: a jump and the bytes it
/// pads count once.
const JOIN_GAP: usize = 16;
/// Bytes compared at once before a byte-by-byte look.
const CHUNK: usize = 4096;
/// More ranges than this between the image on disk and the code in memory
/// mean the file holds packed or encrypted code, not changes.
const PACKED_RANGES: usize = 512;
/// Changed bytes past one in this many of the code also mean packed code.
const PACKED_SHARE: usize = 100;

/// When a change was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Before the loader started: by a DLL the stub chained in, or by another
    /// program.
    BeforeStart,
    /// While a DLL in `mods/` loaded.
    Load,
    /// In a mod's `init`.
    Init,
    /// While the loader unloaded the DLLs it skips, such as an older copy of
    /// a mod: by one of their detach routines.
    Skip,
    /// After every `init`, before the loader armed its hooks: by a thread of
    /// some mod or another program.
    BeforeArming,
    /// In a mod's `ready`.
    Ready,
}

impl Stage {
    /// When, in words, after the one who changed it.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::BeforeStart => "before enshwick started",
            Self::Load => "while it loaded",
            Self::Skip => "while enshwick unloaded the DLLs it skips",
            Self::Init => "in its init",
            Self::BeforeArming => "after every init, before enshwick armed its hooks",
            Self::Ready => "in its ready",
        }
    }

    /// The key of [`describe`](Self::describe)'s text, for reports in the
    /// reader's language.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::BeforeStart => "watch.before-start",
            Self::Load => "watch.load",
            Self::Skip => "watch.skip",
            Self::Init => "watch.init",
            Self::BeforeArming => "watch.before-arming",
            Self::Ready => "watch.ready",
        }
    }
}

/// One range of game code a mod or another program changed outside the API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeChange {
    /// Its first byte, from the start of `.text`.
    pub offset: usize,
    /// How many bytes, changed ones and the few between them.
    pub len: usize,
    /// Who changed it: a mod id, a DLL's file name, or empty when unknown.
    pub by: String,
    /// When.
    pub stage: Stage,
}

impl CodeChange {
    /// Who changed it, in words.
    #[must_use]
    pub fn who(&self) -> &str {
        if self.by.is_empty() {
            "another program"
        } else {
            &self.by
        }
    }
}

/// The ranges where `old` and `new` differ, as offset and length; ranges
/// closer than [`JOIN_GAP`] join. Only the common length is compared.
///
/// # Examples
///
/// ```
/// use enshwick_loader::code_watch::diff;
///
/// assert_eq!(diff(b"abcdef", b"abXdeY"), [(2, 4)]);
/// assert!(diff(b"same", b"same").is_empty());
/// ```
#[must_use]
pub fn diff(old: &[u8], new: &[u8]) -> Vec<(usize, usize)> {
    let len = old.len().min(new.len());
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut at = 0;
    while at < len {
        let end = (at + CHUNK).min(len);
        if old[at..end] != new[at..end] {
            for i in at..end {
                if old[i] == new[i] {
                    continue;
                }
                match ranges.last_mut() {
                    Some((start, n)) if i - (*start + *n) < JOIN_GAP => *n = i + 1 - *start,
                    _ => ranges.push((i, 1)),
                }
            }
        }
        at = end;
    }
    ranges
}

/// The copy of the game's code the loader compares against while it starts,
/// and the changes found so far.
#[derive(Debug)]
pub struct Watch {
    copy: Vec<u8>,
    /// Ranges the loader itself writes while it starts, as offset and
    /// length, such as the stub's hold on the server's entry point.
    ignored: Vec<(usize, usize)>,
    changes: Vec<CodeChange>,
}

impl Watch {
    /// Starts from the code as it is `now`, leaving out the `ignored`
    /// ranges.
    #[must_use]
    pub fn new(now: &[u8], ignored: &[(usize, usize)]) -> Self {
        Self {
            copy: now.to_vec(),
            ignored: ignored.to_vec(),
            changes: Vec::new(),
        }
    }

    /// Starts from `disk`, the code as the image on disk holds it once
    /// relocated, and puts what differs `now` down to programs that ran
    /// before the loader; `None` when they differ so much that the file's
    /// code is packed or encrypted rather than changed.
    #[must_use]
    pub fn from_disk(disk: Vec<u8>, now: &[u8], ignored: &[(usize, usize)]) -> Option<Self> {
        if disk.len() != now.len() {
            return None;
        }
        let ranges = diff(&disk, now);
        let changed: usize = ranges.iter().map(|&(_, len)| len).sum();
        if ranges.len() > PACKED_RANGES || changed > now.len() / PACKED_SHARE {
            return None;
        }
        let mut watch = Self {
            copy: disk,
            ignored: ignored.to_vec(),
            changes: Vec::new(),
        };
        watch.note(now, &ranges, "", Stage::BeforeStart);
        Some(watch)
    }

    /// Compares the code `now` with the copy and puts each new change down to
    /// `by` at `stage`; the number of changes noted.
    pub fn check(&mut self, now: &[u8], by: &str, stage: Stage) -> usize {
        let before = self.changes.len();
        let ranges = diff(&self.copy, now);
        self.note(now, &ranges, by, stage);
        self.changes.len() - before
    }

    /// Takes the code `now` as the copy without noting anything: the
    /// loader's own hooks and the patches mods asked for through the API.
    pub fn absorb(&mut self, now: &[u8]) {
        for (offset, len) in diff(&self.copy, now) {
            self.copy[offset..offset + len].copy_from_slice(&now[offset..offset + len]);
        }
    }

    /// The changes found, in the order they were found.
    #[must_use]
    pub fn changes(&self) -> &[CodeChange] {
        &self.changes
    }

    /// Ends the watch: the changes found, without the copy.
    #[must_use]
    pub fn into_changes(self) -> Vec<CodeChange> {
        self.changes
    }

    fn note(&mut self, now: &[u8], ranges: &[(usize, usize)], by: &str, stage: Stage) {
        for &(offset, len) in ranges {
            let parts: Vec<(usize, usize)> = outside(offset, len, &self.ignored)
                .into_iter()
                .filter_map(|(at, n)| {
                    let (mut start, mut end) = (at, at + n);
                    while start < end && self.copy[start] == now[start] {
                        start += 1;
                    }
                    while end > start && self.copy[end - 1] == now[end - 1] {
                        end -= 1;
                    }
                    (start < end).then_some((start, end - start))
                })
                .collect();
            self.copy[offset..offset + len].copy_from_slice(&now[offset..offset + len]);
            for (offset, len) in parts {
                self.changes.push(CodeChange {
                    offset,
                    len,
                    by: by.to_owned(),
                    stage,
                });
            }
        }
    }
}

/// The parts of the range at `offset`, `len` bytes long, that no range of
/// `ignored` covers.
fn outside(offset: usize, len: usize, ignored: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut parts = vec![(offset, offset + len)];
    for &(skip, skip_len) in ignored {
        let skip_end = skip + skip_len;
        parts = parts
            .into_iter()
            .flat_map(|(start, end)| {
                [(start, end.min(skip)), (start.max(skip_end), end)]
                    .into_iter()
                    .filter(|(a, b)| a < b)
            })
            .collect();
    }
    parts.into_iter().map(|(a, b)| (a, b - a)).collect()
}

/// Moves the 64-bit addresses in a section's bytes as Windows does for an
/// image mapped `delta` bytes away from its preferred base.
///
/// `text` starts at `text_rva`; `fixups` are the RVAs of the words to move.
///
/// # Examples
///
/// ```
/// use enshwick_loader::code_watch::relocate;
///
/// let mut text = 0x1_4000_0010u64.to_le_bytes().to_vec();
/// relocate(&mut text, 0x1000, [0x1000], 0x10_0000);
/// assert_eq!(text, 0x1_4010_0010u64.to_le_bytes());
/// ```
pub fn relocate(text: &mut [u8], text_rva: u64, fixups: impl IntoIterator<Item = u64>, delta: u64) {
    for rva in fixups {
        let Some(at) = rva
            .checked_sub(text_rva)
            .and_then(|at| usize::try_from(at).ok())
        else {
            continue;
        };
        let Some(word) = at.checked_add(8).and_then(|end| text.get_mut(at..end)) else {
            continue;
        };
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(word);
        word.copy_from_slice(&u64::from_le_bytes(bytes).wrapping_add(delta).to_le_bytes());
    }
}

/// The report's lines for `changes`: one per range of code, with every one
/// who changed it in order; `place` names an offset into `.text`.
///
/// # Examples
///
/// ```
/// use enshwick_loader::code_watch::{CodeChange, Stage, report};
///
/// let changes = [CodeChange { offset: 0x10, len: 5, by: "speed.dll".into(), stage: Stage::Load }];
/// assert_eq!(
///     report(&changes, |at| format!("game+{at:#x}")),
///     ["game code at game+0x10, 5 bytes: changed outside the API by speed.dll while it loaded"]
/// );
/// ```
#[must_use]
pub fn report(changes: &[CodeChange], place: impl Fn(usize) -> String) -> Vec<String> {
    let mut groups: Vec<(usize, usize, Vec<&CodeChange>)> = Vec::new();
    let mut sorted: Vec<&CodeChange> = changes.iter().collect();
    sorted.sort_by_key(|c| c.offset);
    for change in sorted {
        match groups.last_mut() {
            Some((_, end, members)) if change.offset < *end => {
                *end = (*end).max(change.offset + change.len);
                members.push(change);
            }
            _ => groups.push((change.offset, change.offset + change.len, vec![change])),
        }
    }
    groups
        .into_iter()
        .map(|(start, end, mut members)| {
            members.sort_by_key(|c| changes.iter().position(|o| core::ptr::eq(o, *c)));
            let by: Vec<String> = members
                .iter()
                .map(|change| {
                    let who = if change.by.is_empty() {
                        crate::texts::fill("watch.another-program", &[])
                    } else {
                        change.by.clone()
                    };
                    crate::texts::fill(
                        "watch.change",
                        &[
                            ("who", &who),
                            ("when", &crate::texts::fill(change.stage.key(), &[])),
                        ],
                    )
                })
                .collect();
            crate::texts::fill(
                "watch.code",
                &[
                    ("place", &place(start)),
                    ("bytes", &(end - start).to_string()),
                    ("changes", &by.join(&crate::texts::fill("watch.then", &[]))),
                ],
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(len: usize) -> Vec<u8> {
        (0..=250u8).cycle().take(len).collect()
    }

    #[test]
    fn each_change_goes_to_the_mod_that_made_it() {
        let mut now = code(3 * CHUNK);
        let mut watch = Watch::new(&now, &[]);
        now[100..105].copy_from_slice(&[0xE9; 5]);
        assert_eq!(watch.check(&now, "speed.dll", Stage::Load), 1);
        assert_eq!(watch.check(&now, "other.dll", Stage::Load), 0);
        now[2 * CHUNK + 7] ^= 0xFF;
        assert_eq!(watch.check(&now, "you.mod", Stage::Init), 1);
        assert_eq!(
            watch.changes(),
            [
                CodeChange {
                    offset: 100,
                    len: 5,
                    by: "speed.dll".into(),
                    stage: Stage::Load
                },
                CodeChange {
                    offset: 2 * CHUNK + 7,
                    len: 1,
                    by: "you.mod".into(),
                    stage: Stage::Init
                },
            ]
        );
    }

    #[test]
    fn absorbed_changes_are_never_put_down_to_a_mod() {
        let mut now = code(CHUNK);
        let mut watch = Watch::new(&now, &[]);
        now[10] = 0xCC;
        watch.absorb(&now);
        assert_eq!(watch.check(&now, "you.mod", Stage::Ready), 0);
        assert!(watch.into_changes().is_empty());
    }

    #[test]
    fn close_changes_join_and_far_ones_do_not() {
        let old = code(2 * CHUNK);
        let mut new = old.clone();
        new[10] ^= 1;
        new[20] ^= 1;
        new[CHUNK - 1] ^= 1;
        new[CHUNK] ^= 1;
        new[CHUNK + 100] ^= 1;
        assert_eq!(
            diff(&old, &new),
            [(10, 11), (CHUNK - 1, 2), (CHUNK + 100, 1)]
        );
    }

    #[test]
    fn changes_before_the_loader_come_from_the_image_on_disk() {
        let disk = code(CHUNK);
        let mut now = disk.clone();
        now[50] ^= 1;
        let watch = Watch::from_disk(disk.clone(), &now, &[]).expect("few changes");
        assert_eq!(
            watch.changes(),
            [CodeChange {
                offset: 50,
                len: 1,
                by: String::new(),
                stage: Stage::BeforeStart
            }]
        );
    }

    #[test]
    fn packed_code_on_disk_is_no_baseline() {
        let disk = code(PACKED_RANGES * 2 * JOIN_GAP + JOIN_GAP);
        let now: Vec<u8> = disk
            .iter()
            .enumerate()
            .map(|(i, b)| if i % (2 * JOIN_GAP) == 0 { !b } else { *b })
            .collect();
        assert!(Watch::from_disk(disk.clone(), &now, &[]).is_none());
        assert!(Watch::from_disk(disk.clone(), &disk[1..], &[]).is_none());
        let encrypted: Vec<u8> = disk.iter().map(|b| !b).collect();
        assert!(Watch::from_disk(disk, &encrypted, &[]).is_none());
    }

    #[test]
    fn the_loader_s_own_writes_are_never_put_down_to_anyone() {
        let disk = code(CHUNK);
        let mut now = disk.clone();
        now[100..114].copy_from_slice(&[0xFF; 14]);
        let entry = [(100, 14)];
        let mut watch = Watch::from_disk(disk.clone(), &now, &entry).expect("few changes");
        assert!(watch.changes().is_empty());
        now[100..114].copy_from_slice(&disk[100..114]);
        now[120] ^= 1;
        assert_eq!(watch.check(&now, "you.mod", Stage::Ready), 1);
        assert_eq!(
            watch.changes(),
            [CodeChange {
                offset: 120,
                len: 1,
                by: "you.mod".into(),
                stage: Stage::Ready
            }]
        );
    }

    #[test]
    fn a_change_around_an_ignored_range_keeps_both_ends() {
        assert_eq!(outside(0, 30, &[(10, 5)]), [(0, 10), (15, 15)]);
        assert_eq!(outside(10, 5, &[(0, 30)]), []);
        assert_eq!(outside(0, 4, &[(10, 5)]), [(0, 4)]);
    }

    #[test]
    fn relocations_outside_the_section_are_left_alone() {
        let mut text = vec![0u8; 16];
        relocate(&mut text, 0x1000, [0x0800, 0x100c, 0x1004], 1);
        assert_eq!(text[4], 1);
        assert_eq!(text[12..], [0, 0, 0, 0]);
    }

    #[test]
    fn mods_changing_the_same_code_are_named_in_order() {
        let changes = [
            CodeChange {
                offset: 0x40,
                len: 14,
                by: "b.dll".into(),
                stage: Stage::Load,
            },
            CodeChange {
                offset: 0x10,
                len: 5,
                by: String::new(),
                stage: Stage::BeforeStart,
            },
            CodeChange {
                offset: 0x44,
                len: 2,
                by: "you.mod".into(),
                stage: Stage::Init,
            },
        ];
        assert_eq!(
            report(&changes, |at| format!("game+{at:#x}")),
            [
                "game code at game+0x10, 5 bytes: changed outside the API by another program before enshwick started",
                "game code at game+0x40, 14 bytes: changed outside the API by b.dll while it loaded, then by you.mod in its init",
            ]
        );
    }
}
