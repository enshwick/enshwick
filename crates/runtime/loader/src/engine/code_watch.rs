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

    /// When, in words fit for [`who`](Self::who); `None` for another
    /// program's change in a mod's own stage, which would read as that
    /// program's own load, init or ready.
    #[must_use]
    pub fn when(&self) -> Option<Stage> {
        match self.stage {
            Stage::Load | Stage::Init | Stage::Ready if self.by.is_empty() => None,
            stage => Some(stage),
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

/// The ranges where `now` differs from `disk`, the relocated code of the
/// image on disk, as [`diff`] finds them, without the `ignored` ranges the
/// loader writes itself.
///
/// `None` when the lengths differ or the ranges are so many that the file's
/// code is packed or encrypted rather than changed, and so no copy of the
/// code that runs.
///
/// # Examples
///
/// ```
/// use enshwick_loader::code_watch::shipped;
///
/// assert_eq!(shipped(&[0x90; 200], &[0x90; 200], &[]), Some(vec![]));
/// assert_eq!(shipped(&[0x90; 200], &[0xCC; 200], &[]), None);
/// ```
#[must_use]
pub fn shipped(disk: &[u8], now: &[u8], ignored: &[(usize, usize)]) -> Option<Vec<(usize, usize)>> {
    if disk.len() != now.len() {
        return None;
    }
    let ranges = diff(disk, now);
    let changed: usize = ranges.iter().map(|&(_, len)| len).sum();
    (ranges.len() <= PACKED_RANGES && changed <= now.len() / PACKED_SHARE).then(|| {
        ranges
            .into_iter()
            .flat_map(|(offset, len)| outside(offset, len, ignored))
            .collect()
    })
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
        let ranges = shipped(&disk, now, ignored)?;
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
            let parts: Vec<Vec<(usize, usize)>> = outside(offset, len, &self.ignored)
                .into_iter()
                .filter_map(|(at, n)| {
                    let (mut start, mut end) = (at, at + n);
                    while start < end && self.copy[start] == now[start] {
                        start += 1;
                    }
                    while end > start && self.copy[end - 1] == now[end - 1] {
                        end -= 1;
                    }
                    (start < end).then(|| pieces(&self.copy, now, start, end))
                })
                .collect();
            self.copy[offset..offset + len].copy_from_slice(&now[offset..offset + len]);
            for part in parts {
                let first = self.changes.len();
                for (offset, len) in part {
                    let by = if made(offset, len) { by } else { "" };
                    match self.changes[first..].last_mut() {
                        Some(last) if last.by == by => last.len = offset + len - last.offset,
                        _ => self.changes.push(CodeChange {
                            offset,
                            len,
                            by: by.to_owned(),
                            stage,
                        }),
                    }
                }
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
pub const LEAD: usize = 2;
/// The bytes [`branch`] looks at, at most.
pub const BRANCH_BYTES: usize = 16;

/// Where the jump at the start of `code`, which lies at `address`, sends
/// control; `None` when it is no jump.
///
/// The jumps are `jmp` or `call` rel32, `jmp [rip+rel32]` to the address
/// `cell` reads in its cell, and `mov rax` or `mov r11` of an address then
/// a jump to that register.
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
    let len = jump_len(code)?;
    let relative = |at: usize, next: usize| {
        let bytes: [u8; 4] = code.get(at..at + 4)?.try_into().ok()?;
        let delta = isize::try_from(i32::from_le_bytes(bytes)).ok()?;
        address.checked_add(next)?.checked_add_signed(delta)
    };
    match code {
        [0xE8 | 0xE9, ..] => relative(1, len),
        [0xFF, 0x25, ..] => cell(relative(2, 6)?),
        _ => {
            let bytes: [u8; 8] = code.get(2..10)?.try_into().ok()?;
            usize::try_from(u64::from_le_bytes(bytes)).ok()
        }
    }
}

/// How many bytes the jump at the start of `code` takes, of the forms
/// [`branch`] follows; `None` when it is no jump or is cut short.
fn jump_len(code: &[u8]) -> Option<usize> {
    let ends = |tail: &[u8], len: usize| (code.get(10..len)? == tail).then_some(len);
    let len = match code {
        [0xE8 | 0xE9, ..] => 5,
        // `jmp [rip+0]` with its address after it, as hook libraries write it.
        [0xFF, 0x25, 0, 0, 0, 0, ..] if code.len() >= 14 => 14,
        [0xFF, 0x25, ..] => 6,
        [0x48, 0xB8, ..] => ends(&[0xFF, 0xE0], 12)?,
        [0x49, 0xBB, ..] => ends(&[0x41, 0xFF, 0xE3], 13)?,
        _ => return None,
    };
    (code.len() >= len).then_some(len)
}

/// The pieces of the changed bytes from `start` to `end`, the first and last
/// of them changed, each its own change: from a run's first changed byte to
/// the end of a jump that starts within [`LEAD`] bytes of it, where
/// [`reaches`] looks, else to before more than [`LEAD`] unchanged bytes; a
/// run without a jump that close to the piece before joins it. So a
/// second hook close to another is put down to whoever made it, and the
/// search stays near the start of each piece.
fn pieces(old: &[u8], now: &[u8], start: usize, end: usize) -> Vec<(usize, usize)> {
    let changed = |at: usize| old[at] != now[at];
    let mut pieces = Vec::new();
    let mut from = 0;
    let mut at = start;
    while at < end {
        let jump = (at.saturating_sub(LEAD).max(from)..=(at + LEAD).min(end - 1))
            .find_map(|first| Some(first + jump_len(&now[first..])?));
        let stop = jump.map_or_else(
            || {
                let mut last = at;
                let mut next = at + 1;
                while next < end && next - last <= LEAD {
                    if changed(next) {
                        last = next;
                    }
                    next += 1;
                }
                last + 1
            },
            |stop| stop.clamp(at + 1, end),
        );
        match pieces.last_mut() {
            // Bytes a jump pads or a run continues join it.
            Some((first, len)) if jump.is_none() && at - from <= LEAD => *len = stop - *first,
            _ => pieces.push((at, stop - at)),
        }
        from = stop;
        at = (stop..end).find(|&next| changed(next)).unwrap_or(end);
    }
    pieces
}

/// Whether a changed piece of code jumps into `image`, its first byte and end.
///
/// The piece is `len` bytes at `offset` of `now`, the code mapped at `text`;
/// its jump starts within [`LEAD`] bytes of `offset` and lands in `image`
/// directly, or through up to [`HOPS`] more jumps in the code `read` answers,
/// up to [`BRANCH_BYTES`] bytes at a jump.
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
    piece: (usize, usize),
    image: (usize, usize),
    read: impl Fn(usize) -> Option<Vec<u8>>,
) -> bool {
    lands(now, text, piece, read, |at| {
        (image.0..image.1).contains(&at)
    })
    .is_some()
}

/// Where a changed piece of code's jump leads: the first address on its way
/// that `stop` takes, the jump's own target or one up to [`HOPS`] more
/// jumps on, as [`reaches`] follows them; `None` when the way stops short.
///
/// # Examples
///
/// ```
/// use enshwick_loader::code_watch::lands;
///
/// let mut now = vec![0x90; 32];
/// now[8..13].copy_from_slice(&[0xE9, 0x00, 0x10, 0, 0]);
/// let past = |at: usize| at >= 0x2000;
/// assert_eq!(lands(&now, 0x1000, (8, 5), |_| None, past), Some(0x200D));
/// assert_eq!(lands(&now, 0x1000, (8, 5), |_| None, |_| false), None);
/// ```
#[must_use]
pub fn lands(
    now: &[u8],
    text: usize,
    (offset, len): (usize, usize),
    read: impl Fn(usize) -> Option<Vec<u8>>,
    stop: impl Fn(usize) -> bool,
) -> Option<usize> {
    let cell = |at: usize| {
        let bytes: [u8; 8] = read(at)?.get(..8)?.try_into().ok()?;
        usize::try_from(u64::from_le_bytes(bytes)).ok()
    };
    let follow = |mut to: usize| {
        for _ in 0..HOPS {
            if stop(to) {
                return Some(to);
            }
            to = read(to).and_then(|code| branch(&code, to, cell))?;
        }
        stop(to).then_some(to)
    };
    (offset.saturating_sub(LEAD)..(offset + LEAD + 1).min(offset + len)).find_map(|at| {
        now.get(at..)
            .zip(text.checked_add(at))
            .and_then(|(code, address)| branch(code, address, cell))
            .and_then(follow)
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
                        crate::engine::texts::fill("watch.another-program", &[])
                    } else {
                        change.by.clone()
                    };
                    match change.when() {
                        Some(stage) => crate::engine::texts::fill(
                            "watch.change",
                            &[
                                ("who", &who),
                                ("when", &crate::engine::texts::fill(stage.key(), &[])),
                            ],
                        ),
                        None => who,
                    }
                })
                .collect();
            crate::engine::texts::fill(
                "watch.code",
                &[
                    ("place", &place(start)),
                    ("bytes", &(end - start).to_string()),
                    (
                        "changes",
                        &by.join(&crate::engine::texts::fill("watch.then", &[])),
                    ),
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
            Some(code.to_vec())
        };
        assert!(reaches(&now, text, (100, 5), image, read));
        let made = |offset, len| reaches(&now, text, (offset, len), image, read);
        assert_eq!(watch.check(&now, "sockel.dll", Stage::Load, made), 2);
        let who: Vec<&str> = watch.changes().iter().map(CodeChange::who).collect();
        assert_eq!(who, ["sockel.dll", "another program"]);
    }

    /// `jmp rel32` at `offset` of the code mapped at `text`, to `to`.
    fn jump_at(now: &mut [u8], text: usize, offset: usize, to: usize) {
        let rel = i32::try_from(to.cast_signed() - (text + offset + 5).cast_signed()).unwrap();
        now[offset] = 0xE9;
        now[offset + 1..offset + 5].copy_from_slice(&rel.to_le_bytes());
    }

    #[test]
    fn two_hooks_closer_than_the_join_gap_go_each_to_its_maker() {
        let mut now = code(CHUNK);
        let mut watch = Watch::new(&now, &[]);
        let (text, image) = (0x1000, (0x9000, 0xa000));
        jump_at(&mut now, text, 100, 0x5000);
        jump_at(&mut now, text, 110, 0x9100);
        let made = |offset, len| reaches(&now, text, (offset, len), image, |_| None);
        assert_eq!(watch.check(&now, "sockel.dll", Stage::Load, made), 2);
        let found: Vec<(usize, usize, &str)> = watch
            .changes()
            .iter()
            .map(|c| (c.offset, c.len, c.who()))
            .collect();
        assert_eq!(found, [(100, 5, "another program"), (110, 5, "sockel.dll")]);
    }

    #[test]
    fn a_jump_deep_inside_a_large_unrelated_change_is_not_followed() {
        let mut now = code(CHUNK);
        let mut watch = Watch::new(&now, &[]);
        let (text, image) = (0x1000, (0x9000, 0xa000));
        now[100..1100].fill(0x90);
        jump_at(&mut now, text, 600, 0x9100);
        let reads = core::cell::Cell::new(0);
        let read = |_| {
            reads.set(reads.get() + 1);
            None
        };
        let made = |offset, len| reaches(&now, text, (offset, len), image, read);
        assert_eq!(watch.check(&now, "sockel.dll", Stage::Load, made), 1);
        assert_eq!(watch.changes()[0].who(), "another program");
        assert_eq!(
            (watch.changes()[0].offset, watch.changes()[0].len),
            (100, 1000)
        );
        assert_eq!(reads.get(), 0, "no jump near the start to follow");
    }

    #[test]
    fn a_hook_and_the_bytes_it_pads_stay_one_change() {
        let mut now = code(CHUNK);
        let mut watch = Watch::new(&now, &[]);
        let (text, image) = (0x1000, (0x9000, 0xa000));
        jump_at(&mut now, text, 100, 0x9100);
        now[105..108].fill(0xCC);
        let made = |offset, len| reaches(&now, text, (offset, len), image, |_| None);
        assert_eq!(watch.check(&now, "sockel.dll", Stage::Load, made), 1);
        assert_eq!(
            watch.changes(),
            [CodeChange {
                offset: 100,
                len: 8,
                by: "sockel.dll".into(),
                stage: Stage::Load
            }]
        );
    }

    #[test]
    fn a_piece_runs_to_its_jump_s_end_or_to_a_gap_wider_than_the_lead() {
        let old = vec![0u8; 128];
        let mut now = old.clone();
        // Two bytes two apart join; one seven further on is its own piece.
        now[10] = 1;
        now[12] = 1;
        now[19] = 1;
        assert_eq!(pieces(&old, &now, 10, 20), [(10, 3), (19, 1)]);
        // A jump and the bytes it pads are one piece, up to the jump's end.
        jump_at(&mut now, 0, 40, 0x50);
        now[45..48].fill(0xCC);
        assert_eq!(pieces(&old, &now, 40, 48), [(40, 8)]);
        // A jump that starts within the lead of a run takes the run.
        now[60] = 0x90;
        jump_at(&mut now, 0, 61, 0x50);
        assert_eq!(pieces(&old, &now, 60, 66), [(60, 6)]);
        // A jump past the lead starts its own piece, as does a second jump.
        now[80] = 0x90;
        jump_at(&mut now, 0, 84, 0x50);
        jump_at(&mut now, 0, 89, 0x50);
        assert_eq!(pieces(&old, &now, 80, 94), [(80, 1), (84, 5), (89, 5)]);
    }

    #[test]
    fn a_far_jump_with_its_address_inline_is_one_piece_whatever_the_address() {
        let old = vec![0u8; 64];
        let mut now = old.clone();
        // `jmp [rip+0]` and the address after it, whose bytes hold a `jmp`
        // opcode.
        now[10..16].copy_from_slice(&[0xFF, 0x25, 0, 0, 0, 0]);
        now[16..24].copy_from_slice(&0x7FFE_E900_9100u64.to_le_bytes());
        assert_eq!(pieces(&old, &now, 10, 24), [(10, 14)]);
        let read = |at: usize| (at == 16).then(|| now[16..24].to_vec());
        assert_eq!(
            branch(&now[10..], 10, |cell| {
                let bytes: [u8; 8] = read(cell)?.try_into().ok()?;
                usize::try_from(u64::from_le_bytes(bytes)).ok()
            }),
            Some(0x7FFE_E900_9100)
        );
    }

    #[test]
    fn close_hooks_of_one_maker_stay_one_change() {
        let mut now = code(CHUNK);
        let mut watch = Watch::new(&now, &[]);
        let (text, image) = (0x1000, (0x9000, 0xa000));
        jump_at(&mut now, text, 100, 0x9100);
        jump_at(&mut now, text, 110, 0x9200);
        let made = |offset, len| reaches(&now, text, (offset, len), image, |_| None);
        assert_eq!(watch.check(&now, "sockel.dll", Stage::Load, made), 1);
        let found: Vec<(usize, usize, &str)> = watch
            .changes()
            .iter()
            .map(|c| (c.offset, c.len, c.who()))
            .collect();
        assert_eq!(found, [(100, 15, "sockel.dll")]);
    }

    #[test]
    fn a_jump_counts_only_within_the_lead_and_the_piece() {
        let (text, image) = (0x1000, (0x9000, 0xa000));
        let into = |at: usize| {
            let mut now = vec![0x90; 128];
            jump_at(&mut now, text, at, 0x9100);
            now
        };
        let reached = |at, piece| reaches(&into(at), text, piece, image, |_| None);
        assert!(reached(102, (100, 8)), "two bytes in");
        assert!(!reached(103, (100, 8)), "three bytes in");
        assert!(reached(98, (100, 8)), "two bytes before");
        assert!(reached(101, (100, 2)));
        assert!(!reached(102, (100, 2)), "past a two-byte piece");
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
        let mut r11 = vec![0x49, 0xBB];
        r11.extend(0x8888u64.to_le_bytes());
        r11.extend([0x41, 0xFF, 0xE3]);
        assert_eq!(branch(&r11, at, |_| None), Some(0x8888));
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
    fn bytes_the_stub_holds_are_no_difference_from_the_shipped_code() {
        let disk = code(CHUNK);
        let mut now = disk.clone();
        now[100..114].fill(0xFF);
        now[300] ^= 1;
        assert_eq!(shipped(&disk, &now, &[(100, 14)]), Some(vec![(300, 1)]));
        assert_eq!(shipped(&disk, &now, &[]), Some(vec![(100, 14), (300, 1)]));
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

    #[test]
    fn another_programs_change_never_borrows_the_running_mods_stage() {
        let change = |stage| CodeChange {
            offset: 0x10,
            len: 5,
            by: String::new(),
            stage,
        };
        for stage in [Stage::Load, Stage::Init, Stage::Ready] {
            assert_eq!(change(stage).when(), None, "{stage:?}");
        }
        assert_eq!(
            report(&[change(Stage::Init)], |at| format!("game+{at:#x}")),
            ["game code at game+0x10, 5 bytes: changed outside the API by another program"]
        );
        assert_eq!(
            change(Stage::BeforeArming).when(),
            Some(Stage::BeforeArming)
        );
        let by_mod = CodeChange {
            by: "you.mod".into(),
            ..change(Stage::Init)
        };
        assert_eq!(by_mod.when(), Some(Stage::Init));
    }
}
