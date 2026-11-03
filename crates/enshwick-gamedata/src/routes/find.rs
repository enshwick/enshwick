//! How the routes of one side find their entries in one image: each route
//! runs when first asked for, through the functions it names.

use iced_x86::Instruction;
use rustc_hash::{FxHashMap, FxHashSet};

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
        let found = self.run(self.routes[i]);
        self.states[i] = State::Done(found);
        found
    }

    fn run(&mut self, route: &'f Route) -> Option<Routed> {
        let rva = match &route.how {
            How::System(name) => self.system(name)?,
            How::CalledBy(names) => self.called_by(names)?,
            How::Calls(names) => self.calls(names)?,
            How::Loads(strings) => self.loading(strings)?,
            How::Shape { within, steps } => return self.shaped(within, steps, route),
        };
        Some(Routed::Symbol(u64::from(rva)))
    }

    /// The RVA `name` stands for: what the route of that id finds, else the
    /// ECS system of that name, else the table's symbol.
    fn address(&mut self, name: &str) -> Option<u32> {
        if let Ok(i) = self.routes.binary_search_by(|r| r.id.as_str().cmp(name))
            && let Some(Routed::Symbol(rva)) = self.resolve(i)
        {
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

    fn shaped(&mut self, within: &str, steps: &'f [Step], route: &Route) -> Option<Routed> {
        let base = self.code.base();
        let function = self.address(within)?;
        let mut entries: Vec<(&str, u64)> = Vec::new();
        for id in steps.iter().flat_map(Step::entries) {
            let rva = self.address(id)?;
            entries.push((id, base + u64::from(rva)));
        }
        let entry = |id: &str| entries.iter().find(|(e, _)| *e == id).map(|&(_, va)| va);
        let code = self.code;
        let parts = self
            .decoded
            .entry(function)
            .or_insert_with(|| code.parts_of(function).map(|p| code.decode(p)).collect());
        let mut found = parts
            .iter()
            .flat_map(|part| shape::matches(steps, part, &entry));
        let one = found.next()?;
        if found.next().is_some() {
            return None;
        }
        let symbol = |va: u64| va.checked_sub(base).map(Routed::Symbol);
        let constant = |v: u64| {
            let v = if route.shift {
                1u64.checked_shl(u32::try_from(v).ok()?)?
            } else {
                v
            };
            v.checked_add_signed(route.plus).map(Routed::Constant)
        };
        match route.take {
            Take::Start => symbol(one.start),
            Take::End => symbol(one.end),
            Take::Address => symbol(one.taken?),
            Take::Value => constant(one.taken?),
            Take::Cell => constant(u64::from(code.image().u32(one.taken?)?)),
            Take::Fixed => Some(Routed::Constant(route.fixed)),
        }
    }
}

/// The only item of `items`.
fn one(mut items: impl Iterator<Item = u32>) -> Option<u32> {
    let first = items.next()?;
    items.next().is_none().then_some(first)
}

/// The `.rdata` addresses of each string any of `routes` names, by string.
pub(super) fn strings(image: &Image<'_>, routes: &[&Route]) -> FxHashMap<String, Vec<u64>> {
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
        .zip(code::strings(image, &texts))
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
/// record of a name or an entry counts, and none a reflected system has.
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
        if reflection.system(spelled).is_ok() || found.contains_key(spelled) || !taken.insert(entry)
        {
            continue;
        }
        if let Some(rva) = entry.checked_sub(base).and_then(|r| u32::try_from(r).ok()) {
            found.insert(spelled.to_owned(), rva);
        }
    }
    found
}
