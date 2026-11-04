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
            Self::BeforeStart => "before Enshwick started",
            Self::Load => "while it loaded",
            Self::Skip => "while Enshwick unloaded the DLLs it skips",
            Self::Init => "in its init",
            Self::BeforeArming => "after every init, before Enshwick turned on its hooks",
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
        watch.note(now, &ranges, "", Stage::BeforeStart, |_, _| false);
        Some(watch)
    }

    /// Compares the code `now` with the copy and notes each new change at
    /// `stage`: put down to `by` when `made` says `by` made the change at that
    /// offset and length, else to another program, since another program's
    /// thread may write at any time; the number of changes noted.
    pub fn check(
        &mut self,
        now: &[u8],
        by: &str,
        stage: Stage,
        made: impl Fn(usize, usize) -> bool,
    ) -> usize {
        let before = self.changes.len();
        let ranges = diff(&self.copy, now);
        self.note(now, &ranges, by, stage, made);
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

    fn note(
        &mut self,
        now: &[u8],
        ranges: &[(usize, usize)],
        by: &str,
        stage: Stage,
        made: impl Fn(usize, usize) -> bool,
    ) {
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
                    by: if made(offset, len) {
                        by.to_owned()
                    } else {
                        String::new()
                    },
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

/// Jumps followed from a change towards the code of whoever made it.
const HOPS: usize = 3;
/// Bytes a jump may start before the first changed byte, when its opcode
/// equals the byte it replaced.
const LEAD: usize = 2;
/// The bytes [`branch`] looks at, at most.
pub const BRANCH_BYTES: usize = 16;

/// Where the instruction at the start of `code`, which lies at `address`,
/// sends control: `jmp` or `call` rel32, `jmp [rip+rel32]` to the address
/// `cell` reads in its cell, or `mov rax` or `mov r11` of an address and a
/// jump to that register; `None` for anything else.
///
/// # Examples
///
/// ```
/// use enshwick_loader::code_watch::branch;
///
/// assert_eq!(branch(&[0xE9, 0x10, 0, 0, 0], 0x1000, |_| None), Some(0x1015));
/// assert_eq!(branch(&[0x90], 0x1000, |_| None), None);
/// ```
#[must_use]
pub fn branch(code: &[u8], address: usize, cell: impl Fn(usize) -> Option<usize>) -> Option<usize> {
    let relative = |at: usize, len: usize| {
        let bytes: [u8; 4] = code.get(at..at + 4)?.try_into().ok()?;
        let delta = isize::try_from(i32::from_le_bytes(bytes)).ok()?;
        address.checked_add(len)?.checked_add_signed(delta)
    };
    let absolute = |tail: &[u8]| {
        (code.get(10..10 + tail.len())? == tail).then_some(())?;
        let bytes: [u8; 8] = code.get(2..10)?.try_into().ok()?;
        usize::try_from(u64::from_le_bytes(bytes)).ok()
    };
    match code {
        [0xE8 | 0xE9, ..] => relative(1, 5),
        [0xFF, 0x25, ..] => cell(relative(2, 6)?),
        [0x48, 0xB8, ..] => absolute(&[0xFF, 0xE0]),
        [0x49, 0xBB, ..] => absolute(&[0x41, 0xFF, 0xE3]),
        _ => None,
    }
}

/// Whether the change of `len` bytes at `offset` of `now`, the code mapped at
/// `text`, jumps into `image` (its first byte and end): directly, or through
/// up to [`HOPS`] more jumps in code `read` answers [`BRANCH_BYTES`] of.
///
/// # Examples
///
/// ```
/// use enshwick_loader::code_watch::reaches;
///
/// let mut now = vec![0x90; 32];
/// now[8..13].copy_from_slice(&[0xE9, 0x00, 0x10, 0, 0]);
/// assert!(reaches(&now, 0x1000, (8, 5), (0x2000, 0x3000), |_| None));
/// assert!(!reaches(&now, 0x1000, (8, 5), (0x4000, 0x5000), |_| None));
/// ```
#[must_use]
pub fn reaches(
    now: &[u8],
    text: usize,
    (offset, len): (usize, usize),
    image: (usize, usize),
    read: impl Fn(usize) -> Option<[u8; BRANCH_BYTES]>,
) -> bool {
    let inside = |at: usize| (image.0..image.1).contains(&at);
    let cell = |at: usize| {
        let bytes: [u8; 8] = read(at)?.get(..8)?.try_into().ok()?;
        usize::try_from(u64::from_le_bytes(bytes)).ok()
    };
    let follow = |mut to: usize| {
        for _ in 0..HOPS {
            if inside(to) {
                return true;
            }
            match read(to).and_then(|code| branch(&code, to, &cell)) {
                Some(next) => to = next,
                None => return false,
            }
        }
        inside(to)
    };
    (offset.saturating_sub(LEAD)..offset + len).any(|at| {
        now.get(at..)
            .zip(text.checked_add(at))
            .and_then(|(code, address)| branch(code, address, &cell))
            .is_some_and(&follow)
    })
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
        assert_eq!(watch.check(&now, "speed.dll", Stage::Load, |_, _| true), 1);
        assert_eq!(watch.check(&now, "other.dll", Stage::Load, |_, _| true), 0);
        now[2 * CHUNK + 7] ^= 0xFF;
        assert_eq!(watch.check(&now, "you.mod", Stage::Init, |_, _| true), 1);
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
    fn a_change_that_leads_nowhere_near_the_loading_mod_is_another_programs() {
        let mut now = code(CHUNK);
        let mut watch = Watch::new(&now, &[]);
        let text = 0x1000;
        let image = (0x9000, 0xa000);
        // Into the mod through a relay outside the game's code, as a hook
        // library writes it: `jmp rel32` to `jmp [rip+0]` and the address.
        let relay = 0x5000;
        now[100..105].copy_from_slice(&[0xE9, 0x97, 0x3F, 0, 0]);
        // A byte another program flipped at the same time.
        now[600] ^= 0xFF;
        let read = |at: usize| {
            let mut code = [0xCC; BRANCH_BYTES];
            if at == relay {
                code[..6].copy_from_slice(&[0xFF, 0x25, 0, 0, 0, 0]);
            } else if at == relay + 6 {
                code[..8].copy_from_slice(&0x9100u64.to_le_bytes());
            } else {
                return None;
            }
            Some(code)
        };
        assert!(reaches(&now, text, (100, 5), image, read));
        let made = |offset, len| reaches(&now, text, (offset, len), image, read);
        assert_eq!(watch.check(&now, "sockel.dll", Stage::Load, made), 2);
        let who: Vec<&str> = watch.changes().iter().map(CodeChange::who).collect();
        assert_eq!(who, ["sockel.dll", "another program"]);
    }

    #[test]
    fn each_jump_form_leads_where_it_says() {
        let at = 0x1000;
        assert_eq!(
            branch(&[0xE8, 0xFB, 0xFF, 0xFF, 0xFF], at, |_| None),
            Some(at)
        );
        assert_eq!(
            branch(&[0xFF, 0x25, 0, 0, 0, 0], at, |cell| Some(cell + 1)),
            Some(at + 7)
        );
        let mut mov = vec![0x48, 0xB8];
        mov.extend(0x7777u64.to_le_bytes());
        mov.extend([0xFF, 0xE0]);
        assert_eq!(branch(&mov, at, |_| None), Some(0x7777));
        mov[11] = 0xE1;
        assert_eq!(
            branch(&mov, at, |_| None),
            None,
            "jmp rcx is no jump to rax"
        );
        assert_eq!(branch(&[0xE9, 0, 0], at, |_| None), None, "cut short");
    }

    #[test]
    fn absorbed_changes_are_never_put_down_to_a_mod() {
        let mut now = code(CHUNK);
        let mut watch = Watch::new(&now, &[]);
        now[10] = 0xCC;
        watch.absorb(&now);
        assert_eq!(watch.check(&now, "you.mod", Stage::Ready, |_, _| true), 0);
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
        assert_eq!(watch.check(&now, "you.mod", Stage::Ready, |_, _| true), 1);
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
                "game code at game+0x10, 5 bytes: changed outside the API by another program before Enshwick started",
                "game code at game+0x40, 14 bytes: changed outside the API by b.dll while it loaded, then by you.mod in its init",
            ]
        );
    }
}
