//! How the routes of one side find their entries in one image: each route
//! runs when first asked for, through the functions it names.

use iced_x86::Instruction;
use rustc_hash::{FxHashMap, FxHashSet};

use super::check;
use super::code::{self, Code};
use super::shape::{self, Step};
use super::{How, Route, Routed, Take};
use crate::Resolved;
use crate::reflect::{Image, Reflection, System};

/// A quarter or more of all systems calling a function makes it a helper,
/// such as the ECS query calls nearly every system makes.
const HELPER_SHARE: usize = 4;
/// An ECS system record: a name pointer and length, then the entry, at
/// these offsets of a run of pointers in `.rdata`.
const RECORD_NAME_LEN: usize = 8;
const RECORD_ENTRY: usize = 0x10;
const RECORD_SIZE: usize = 0x18;
/// The lengths a system record's name may have.
const RECORD_NAME: core::ops::RangeInclusive<u64> = 4..=80;

#[derive(Clone, Copy)]
enum State {
    Open,
    Running,
    Done(Option<Routed>),
}

pub(super) struct Finder<'f, 'i, 'a> {
    code: &'f Code<'i, 'a>,
    reflection: &'f Reflection<'f>,
    known: &'f Resolved,
    /// The routes of the side, sorted by id.
    routes: &'f [&'f Route],
    states: Vec<State>,
    loaders: FxHashMap<String, FxHashSet<u32>>,
    /// How many systems there are and how many call each function, once a
    /// route needs it.
    helpers: Option<(usize, FxHashMap<u32, usize>)>,
    /// The ECS system records the reflection skips, once a route needs them.
    records: Option<FxHashMap<String, u32>>,
    decoded: FxHashMap<u32, Vec<Vec<Instruction>>>,
}

impl<'f, 'i, 'a> Finder<'f, 'i, 'a> {
    pub(super) fn new(
        code: &'f Code<'i, 'a>,
        reflection: &'f Reflection<'f>,
        known: &'f Resolved,
        routes: &'f [&'f Route],
        strings: FxHashMap<String, Vec<u64>>,
    ) -> Self {
        let loaders = strings
            .into_iter()
            .map(|(text, at)| (text, at.iter().flat_map(|&va| code.loaders(va)).collect()))
            .collect();
        Self {
            code,
            reflection,
            known,
            routes,
            states: vec![State::Open; routes.len()],
            loaders,
            helpers: None,
            records: None,
            decoded: FxHashMap::default(),
        }
    }

    /// What route `i` finds; a route that comes back to itself finds nothing.
    pub(super) fn resolve(&mut self, i: usize) -> Option<Routed> {
        match self.states[i] {
            State::Done(found) => return found,
            State::Running => return None,
            State::Open => {}
        }
        self.states[i] = State::Running;
        let route = self.routes[i];
        let found = self
            .run(route)
            .and_then(|found| self.checked(route, found))
            .map(|found| route.bounded(found));
        self.states[i] = State::Done(found);
        found
    }

    /// `found` once the code at a symbol starts with the route's `check`,
    /// which makes it a checked [`Routed::Symbol`]; nothing when it does not.
    fn checked(&self, route: &Route, found: Routed) -> Option<Routed> {
        match found {
            Routed::Symbol(rva) | Routed::Reached(rva) if !route.check.is_empty() => {
                check::runs(&route.check, self.code.image(), rva).then_some(Routed::Symbol(rva))
            }
            found => Some(found),
        }
    }

    fn run(&mut self, route: &'f Route) -> Option<Routed> {
        let rva = match &route.how {
            How::System(name) => self.system(name)?,
            How::CalledBy(names) => self.called_by(names)?,
            How::Calls(names) => self.calls(names)?,
            How::Loads(strings) => self.loading(strings)?,
            How::Slot { vtable, slot } => self.slot(vtable, *slot)?,
            How::Shape { within, steps } => return self.shaped(within, steps, route),
            How::Callers { callee, steps } => return self.called_from(callee, steps, route),
            How::Field(path) => {
                return self
                    .reflection
                    .field_path(path, route.holds.as_deref())
                    .map(Routed::Constant);
            }
            How::Size(ty) => {
                let info = self.reflection.type_info(ty).ok()?;
                return Some(Routed::Constant(u64::from(info.size())));
            }
        };
        self.code
            .is_function(rva)
            .then_some(Routed::Reached(u64::from(rva)))
    }

    /// The RVA `name` stands for: what the route of that id finds, else the
    /// ECS system of that name, else the table's symbol. A route that finds
    /// another symbol than the table holds stands for nothing, so nothing
    /// builds on an entry the cross-check turns off.
    fn address(&mut self, name: &str) -> Option<u32> {
        if let Ok(i) = self.routes.binary_search_by(|r| r.id.as_str().cmp(name))
            && let Some(Routed::Symbol(rva) | Routed::Reached(rva)) = self.resolve(i)
        {
            if self.known.symbol(name).is_some_and(|pinned| pinned != rva) {
                return None;
            }
            return u32::try_from(rva).ok();
        }
        self.system(name)
            .or_else(|| u32::try_from(self.known.symbol(name)?).ok())
    }

    /// The begin RVA of the code of the system `name`.
    fn system(&mut self, name: &str) -> Option<u32> {
        let reflected = self
            .reflection
            .system(name)
            .ok()
            .and_then(|s| u32::try_from(s.entry().checked_sub(self.code.base())?).ok())
            .filter(|&rva| rva != 0);
        if reflected.is_some() {
            return reflected;
        }
        let (code, reflection) = (self.code, self.reflection);
        self.records
            .get_or_insert_with(|| records(code, reflection))
            .get(name)
            .copied()
    }

    fn called_by(&mut self, names: &[String]) -> Option<u32> {
        let mut common: Option<FxHashSet<u32>> = None;
        for name in names {
            let callees = self.code.callees(self.address(name)?);
            common = Some(match common {
                None => callees,
                Some(mut set) => {
                    set.retain(|c| callees.contains(c));
                    set
                }
            });
        }
        let (code, reflection) = (self.code, self.reflection);
        let (total, counts) = self
            .helpers
            .get_or_insert_with(|| callers(code, reflection));
        let total = *total;
        one(common?
            .into_iter()
            .filter(|c| counts.get(c).is_none_or(|&n| n * HELPER_SHARE < total)))
    }

    fn calls(&mut self, names: &[String]) -> Option<u32> {
        let mut common: Option<FxHashSet<u32>> = None;
        for name in names {
            let callers = self.code.callers(self.address(name)?);
            common = Some(match common {
                None => callers,
                Some(mut set) => {
                    set.retain(|c| callers.contains(c));
                    set
                }
            });
        }
        one(common?.into_iter())
    }

    fn loading(&self, strings: &[String]) -> Option<u32> {
        let mut sets = strings.iter().map(|s| self.loaders.get(s));
        let mut common = sets.next()??.clone();
        for set in sets {
            let set = set?;
            common.retain(|f| set.contains(f));
        }
        one(common.into_iter())
    }

    /// The RVA the 64-bit pointer in slot `slot` of the vtable `vtable`
    /// holds, whether the image is mapped (relocated against its base) or a
    /// file (against the base it was linked for).
    fn slot(&mut self, vtable: &str, slot: u32) -> Option<u32> {
        let table = self.address(vtable)?;
        let base = self.code.base();
        let at = base
            .checked_add(u64::from(table))?
            .checked_add(u64::from(slot).checked_mul(8)?)?;
        let target = self.code.image().u64(at)?;
        u32::try_from(target.checked_sub(base)?).ok()
    }

    fn shaped(&mut self, within: &str, steps: &'f [Step], route: &Route) -> Option<Routed> {
        let function = self.address(within)?;
        let one = match route.nth {
            Some(nth) => self.nth_match(function, &route.after, steps, nth)?,
            None => self.only_match([function], steps)?.1,
        };
        self.take(route, steps, one)
    }

    /// The `nth` run of `steps`, from 1, in address order: in the part of
    /// `function`'s code where `after` matches once, after that match, or,
    /// with no `after`, anywhere in its code; none unless there are `of`
    /// runs, so a run added or dropped never shifts the count silently.
    fn nth_match(
        &mut self,
        function: u32,
        after: &[Step],
        steps: &[Step],
        (nth, of): (u32, u32),
    ) -> Option<shape::Match> {
        let entries = self.entries(after.iter().chain(steps))?;
        let entry = |id: &str| entries.iter().find(|(e, _)| *e == id).map(|&(_, va)| va);
        let parts = self.parts(function);
        let mut runs: Vec<shape::Match> = if after.is_empty() {
            parts
                .iter()
                .flat_map(|part| shape::matches(steps, part, &entry))
                .collect()
        } else {
            let mut anchors = parts
                .iter()
                .enumerate()
                .flat_map(|(i, part)| shape::matches(after, part, &entry).map(move |m| (i, m)));
            let (part, anchor) = anchors.next()?;
            if anchors.next().is_some() {
                return None;
            }
            shape::matches(steps, &parts[part], &entry)
                .filter(|run| run.start >= anchor.end)
                .collect()
        };
        if runs.len() != usize::try_from(of).ok()? {
            return None;
        }
        runs.sort_unstable_by_key(|run| run.start);
        runs.into_iter()
            .nth(usize::try_from(nth).ok()?.checked_sub(1)?)
    }

    /// The decoded parts of `function`'s code, decoded once.
    fn parts(&mut self, function: u32) -> &[Vec<Instruction>] {
        let code = self.code;
        self.decoded
            .entry(function)
            .or_insert_with(|| code.parts_of(function).map(|p| code.decode(p)).collect())
    }

    /// The address of each entry `steps` name, by id; none when one is not
    /// found.
    fn entries<'s>(
        &mut self,
        steps: impl Iterator<Item = &'s Step>,
    ) -> Option<Vec<(&'s str, u64)>> {
        let base = self.code.base();
        steps
            .flat_map(Step::entries)
            .map(|id| Some((id, base + u64::from(self.address(id)?))))
            .collect()
    }

    /// Among the functions that call `callee` directly, the one whose code
    /// the shape matches, as [`Self::shaped`] takes it; `start` is that
    /// function.
    fn called_from(&mut self, callee: &str, steps: &'f [Step], route: &Route) -> Option<Routed> {
        let callee = self.address(callee)?;
        let mut callers: Vec<u32> = self.code.callers(callee).into_iter().collect();
        callers.sort_unstable();
        let (function, one) = self.only_match(callers, steps)?;
        match route.take {
            Take::Start => self
                .code
                .is_function(function)
                .then_some(Routed::Reached(u64::from(function))),
            _ => self.take(route, steps, one),
        }
    }

    /// The one run of `steps` in the code of all `functions`, with the
    /// function it lies in; none for none or more than one.
    fn only_match(
        &mut self,
        functions: impl IntoIterator<Item = u32>,
        steps: &'f [Step],
    ) -> Option<(u32, shape::Match)> {
        let entries = self.entries(steps.iter())?;
        let entry = |id: &str| entries.iter().find(|(e, _)| *e == id).map(|&(_, va)| va);
        let mut found = None;
        for function in functions {
            let parts = self.parts(function);
            for one in parts
                .iter()
                .flat_map(|part| shape::matches(steps, part, &entry))
            {
                if found.replace((function, one)).is_some() {
                    return None;
                }
            }
        }
        found
    }

    /// What a route takes from its shape's one match.
    fn take(&self, route: &Route, steps: &[Step], one: shape::Match) -> Option<Routed> {
        let code = self.code;
        let base = code.base();
        let rva = |va: u64| va.checked_sub(base);
        // An address taken in the code must start a function there, and
        // one outside it must lie in the image's data.
        let taken_address = |va: u64| {
            let at = rva(va)?;
            let held = if code.in_text(va) {
                u32::try_from(at).is_ok_and(|f| code.is_function(f))
            } else {
                code.image().in_data(va)
            };
            held.then_some(Routed::Reached(at))
        };
        let constant = |v: u64| {
            let v = if route.shift {
                1u64.checked_shl(u32::try_from(v).ok()?)?
            } else {
                v
            };
            v.checked_add_signed(route.plus).map(Routed::Constant)
        };
        match route.take {
            Take::Start if check::match_checks(steps.len(), one.start, one.end) => {
                rva(one.start).map(Routed::Symbol)
            }
            Take::Start => rva(one.start).map(Routed::Reached),
            Take::End => rva(one.end).map(Routed::Reached),
            Take::Address => taken_address(one.taken?),
            Take::Value => constant(one.taken?),
            Take::Cell => constant(u64::from(code.image().u32(one.taken?)?)),
            Take::Spelled => Some(Routed::Constant(route.value)),
            Take::Scale => {
                let run = (
                    u32::try_from(rva(one.start)?).ok()?,
                    u32::try_from(rva(one.end)?).ok()?,
                );
                shape::scale(&code.decode(run)).map(Routed::Constant)
            }
        }
    }
}

/// The only item of `items`.
fn one(mut items: impl Iterator<Item = u32>) -> Option<u32> {
    let first = items.next()?;
    items.next().is_none().then_some(first)
}

/// The `.rdata` addresses of each string any of `routes` names, by string.
pub(super) fn strings(
    image: &Image<'_>,
    routes: &[&Route],
    threads: usize,
) -> FxHashMap<String, Vec<u64>> {
    let texts: Vec<&str> = routes
        .iter()
        .filter_map(|r| match &r.how {
            How::Loads(strings) => Some(strings.iter().map(String::as_str)),
            _ => None,
        })
        .flatten()
        .collect();
    if texts.is_empty() {
        return FxHashMap::default();
    }
    texts
        .iter()
        .zip(code::strings(image, &texts, threads))
        .map(|(text, addresses)| ((*text).to_owned(), addresses))
        .collect()
}

/// How many systems there are, and how many of them call each function.
fn callers(code: &Code<'_, '_>, reflection: &Reflection<'_>) -> (usize, FxHashMap<u32, usize>) {
    let entries: FxHashSet<u32> = reflection
        .systems()
        .filter_map(|s| u32::try_from(s.entry().checked_sub(code.base())?).ok())
        .filter(|&rva| rva != 0)
        .collect();
    let mut counts: FxHashMap<u32, usize> = FxHashMap::default();
    for &entry in &entries {
        for callee in code.callees(entry) {
            *counts.entry(callee).or_default() += 1;
        }
    }
    (entries.len(), counts)
}

/// The ECS system records without component lists, which the reflection
/// skips, by name: in `.rdata`, a pointer to the name, its length and the
/// entry in the code, the name lowercase ASCII and NUL-terminated. The first
/// record of an entry counts, none a reflected system has, and none of a name
/// two records give different entries.
fn records(code: &Code<'_, '_>, reflection: &Reflection<'_>) -> FxHashMap<String, u32> {
    let image = code.image();
    let base = code.base();
    let mut found = FxHashMap::default();
    let (Some(rdata), Some(text)) = (image.section(".rdata"), image.section(".text")) else {
        return found;
    };
    let (rdata, text) = (image.range(rdata), image.range(text));
    let Some(bytes) = image.read(rdata.start, rdata.end - rdata.start) else {
        return found;
    };
    let mut taken: FxHashSet<u64> = reflection.systems().map(System::entry).collect();
    let mut seen: FxHashMap<&str, u64> = FxHashMap::default();
    let mut ambiguous: FxHashSet<&str> = FxHashSet::default();
    let word = |at: usize| {
        bytes
            .get(at..at + 8)
            .map(|w| u64::from_le_bytes(w.try_into().unwrap_or([0; 8])))
    };
    for at in (0..bytes.len().saturating_sub(RECORD_SIZE - 1)).step_by(8) {
        let (Some(name), Some(len), Some(entry)) = (
            word(at),
            word(at + RECORD_NAME_LEN),
            word(at + RECORD_ENTRY),
        ) else {
            continue;
        };
        if !RECORD_NAME.contains(&len) || !rdata.contains(&name) || !text.contains(&entry) {
            continue;
        }
        let (Ok(from), Ok(len)) = (usize::try_from(name - rdata.start), usize::try_from(len))
        else {
            continue;
        };
        let Some(spelled) = bytes.get(from..from + len + 1) else {
            continue;
        };
        let (spelled, nul) = spelled.split_at(len);
        if nul != [0]
            || !spelled
                .iter()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
        {
            continue;
        }
        let Ok(spelled) = core::str::from_utf8(spelled) else {
            continue;
        };
        if reflection.system(spelled).is_ok() {
            continue;
        }
        if *seen.entry(spelled).or_insert(entry) != entry {
            ambiguous.insert(spelled);
        }
        if found.contains_key(spelled) || !taken.insert(entry) {
            continue;
        }
        if let Some(rva) = entry.checked_sub(base).and_then(|r| u32::try_from(r).ok()) {
            found.insert(spelled.to_owned(), rva);
        }
    }
    found.retain(|name, _| !ambiguous.contains(name.as_str()));
    found
}
