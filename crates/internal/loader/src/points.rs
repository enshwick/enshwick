//! Extension points. During init the loader keeps the points mods define and
//! the links they attach; once every mod is initialised it matches each link
//! with its point field by field, orders the links the same way at every
//! start, and sets the count of live links the owner reads before it asks.
//! Asking a point runs its live links in turn on the asking thread; a link
//! that panics is turned off, and the point runs on without it.

use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use enshwick::abi::{Level, PointFn, PointMode, PointVerdict, Status};
use enshwick::points::{LinkPlace, order};
use enshwick::services::{LayoutError, compare};

use crate::services::{Fields, Names};

/// A point as one side declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Declared {
    /// `<owner id>:<point>`.
    pub(crate) name: String,
    /// The input's fields.
    pub(crate) input: Fields,
    /// The answer, one field.
    pub(crate) output: Fields,
    /// How links act on the answer.
    pub(crate) mode: PointMode,
    /// Whether a link may refuse.
    pub(crate) refuse: bool,
}

/// A defined point.
#[derive(Debug)]
struct Definition {
    owner: usize,
    declared: Declared,
    live: &'static AtomicU32,
}

/// A link waiting for the seal.
#[derive(Debug)]
pub(crate) struct Pending {
    /// The linking mod's index.
    pub(crate) owner: usize,
    /// The point as the linking mod declares it.
    pub(crate) declared: Declared,
    /// Lower runs earlier among free links.
    pub(crate) priority: i32,
    /// Ids of mods whose links run after this one.
    pub(crate) before: Vec<String>,
    /// Ids of mods whose links run before this one.
    pub(crate) after: Vec<String>,
    /// The link, inside the mod's DLL, which stays loaded.
    pub(crate) callback: PointFn,
    /// The mod's pointer for the link, as an address.
    pub(crate) user: usize,
}

/// What mods defined and attached during init.
#[derive(Debug, Default)]
pub(crate) struct Registry {
    defined: Vec<Definition>,
    pending: Vec<Pending>,
}

/// One live link of a sealed point.
#[derive(Debug)]
pub(crate) struct Link {
    /// The linking mod's index.
    pub(crate) owner: usize,
    callback: PointFn,
    user: usize,
    /// Its priority, for the report.
    pub(crate) priority: i32,
    /// False once it panicked.
    on: AtomicBool,
    /// How often it changed the answer.
    changes: AtomicU64,
    /// How often it refused.
    refusals: AtomicU64,
    /// How often it changed an answer another mod's link had changed in
    /// the same call.
    undid: AtomicU64,
    /// The mod whose change it changed last.
    undid_whom: AtomicUsize,
}

impl Link {
    /// Whether it still runs.
    pub(crate) fn is_on(&self) -> bool {
        self.on.load(Ordering::Relaxed)
    }
}

/// A sealed point: its owner, its links in running order, and the links
/// left inert.
#[derive(Debug)]
pub(crate) struct Sealed {
    /// The defining mod's index, the one mod that asks it.
    pub(crate) owner: usize,
    /// The point as its owner declares it.
    pub(crate) declared: Declared,
    live: &'static AtomicU32,
    /// The matched links, in running order.
    pub(crate) links: Box<[Link]>,
}

/// A link left inert, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Inert {
    /// The linking mod's index.
    pub(crate) owner: usize,
    /// The point it named.
    pub(crate) point: String,
    /// Why it stays inert.
    pub(crate) reason: String,
    /// How loud the log says it.
    pub(crate) level: Level,
}

/// The mods the seal asks about.
pub(crate) trait Mods: Names {
    /// The index of the running or off mod `id`.
    fn find(&self, id: &str) -> Option<usize>;
}

impl Registry {
    /// Defines a point named after its definer `owner_id`, and returns its
    /// id and the count of its live links.
    ///
    /// # Errors
    ///
    /// Why it is refused: a name outside the definer's id, or defined twice.
    pub(crate) fn define(
        &mut self,
        owner: usize,
        owner_id: &str,
        declared: Declared,
    ) -> Result<(u32, &'static AtomicU32), String> {
        let name = &declared.name;
        if name
            .strip_prefix(owner_id)
            .and_then(|rest| rest.strip_prefix(':'))
            .is_none_or(str::is_empty)
        {
            return Err(format!(
                "point {name}: a point is named `{owner_id}:<point>`"
            ));
        }
        if self.defined.iter().any(|d| d.declared.name == *name) {
            return Err(format!("point {name}: defined twice"));
        }
        let id = u32::try_from(self.defined.len()).map_err(|_| "too many points".to_owned())?;
        let live: &'static AtomicU32 = Box::leak(Box::new(AtomicU32::new(0)));
        self.defined.push(Definition {
            owner,
            declared,
            live,
        });
        Ok((id, live))
    }

    /// Keeps a link for the seal.
    ///
    /// # Errors
    ///
    /// Why it is refused: the mod linked the point already.
    pub(crate) fn attach(&mut self, link: Pending) -> Result<(), String> {
        if self
            .pending
            .iter()
            .any(|p| p.owner == link.owner && p.declared.name == link.declared.name)
        {
            return Err(format!(
                "point {}: a mod links a point once",
                link.declared.name
            ));
        }
        self.pending.push(link);
        Ok(())
    }

    /// Matches every link with its point and orders each point's links:
    /// the sealed points by id, the links left inert, and the links whose
    /// order closes a cycle, which run by their priority. `link_off` says
    /// whether the mod list turns off a mod's link on a point.
    pub(crate) fn seal(
        &mut self,
        mods: &impl Mods,
        link_off: impl Fn(&str, &str) -> bool,
    ) -> (Box<[Sealed]>, Vec<Inert>, Vec<Inert>) {
        let mut matched: Vec<Vec<Pending>> = self.defined.iter().map(|_| Vec::new()).collect();
        let mut inert = Vec::new();
        let mut cycles = Vec::new();
        for link in std::mem::take(&mut self.pending) {
            if mods.off(link.owner) {
                continue;
            }
            let point = link.declared.name.clone();
            let found = self
                .defined
                .iter()
                .enumerate()
                .find(|(_, d)| d.declared.name == point);
            let Some((id, definition)) = found else {
                inert.push(missing(&link, mods));
                continue;
            };
            if let Some((reason, level)) = refusal(&link, definition, mods) {
                inert.push(Inert {
                    owner: link.owner,
                    point,
                    reason,
                    level,
                });
                continue;
            }
            let (linker, _) = mods.name(link.owner);
            if link_off(linker, &point) {
                inert.push(Inert {
                    owner: link.owner,
                    point,
                    reason: format!("off in {}", crate::modlist::LIST_PATH),
                    level: Level::INFO,
                });
                continue;
            }
            if let Some(links) = matched.get_mut(id) {
                links.push(link);
            }
        }
        let sealed = self
            .defined
            .iter()
            .zip(matched)
            .map(|(definition, links)| Sealed {
                owner: definition.owner,
                declared: definition.declared.clone(),
                live: definition.live,
                links: chain(&definition.declared.name, links, mods, &mut cycles),
            })
            .collect();
        (sealed, inert, cycles)
    }
}

/// The links of the point `point` in running order; links whose order
/// closes a cycle go to `cycles` as well.
fn chain(
    point: &str,
    links: Vec<Pending>,
    mods: &impl Mods,
    cycles: &mut Vec<Inert>,
) -> Box<[Link]> {
    let (running, dropped) = {
        let before: Vec<Vec<&str>> = links
            .iter()
            .map(|l| l.before.iter().map(String::as_str).collect())
            .collect();
        let after: Vec<Vec<&str>> = links
            .iter()
            .map(|l| l.after.iter().map(String::as_str).collect())
            .collect();
        let places: Vec<LinkPlace<'_>> = links
            .iter()
            .zip(before.iter().zip(&after))
            .map(|(l, (before, after))| LinkPlace {
                id: mods.name(l.owner).0,
                priority: l.priority,
                before,
                after,
            })
            .collect();
        order(&places)
    };
    for &i in &dropped {
        if let Some(link) = links.get(i) {
            cycles.push(Inert {
                owner: link.owner,
                point: point.to_owned(),
                reason: "its order closes a cycle; it runs by its priority".to_owned(),
                level: Level::WARN,
            });
        }
    }
    let mut links: Vec<Option<Pending>> = links.into_iter().map(Some).collect();
    running
        .iter()
        .filter_map(|&i| links.get_mut(i).and_then(Option::take))
        .map(|l| Link {
            owner: l.owner,
            callback: l.callback,
            user: l.user,
            priority: l.priority,
            on: AtomicBool::new(true),
            changes: AtomicU64::new(0),
            refusals: AtomicU64::new(0),
            undid: AtomicU64::new(0),
            undid_whom: AtomicUsize::new(usize::MAX),
        })
        .collect()
}

/// Why a link whose point no running mod defines stays inert.
fn missing(link: &Pending, mods: &impl Mods) -> Inert {
    let point = &link.declared.name;
    let owner_id = point.split_once(':').map_or(point.as_str(), |(id, _)| id);
    let (reason, level) = match mods.find(owner_id) {
        None => (format!("{owner_id} does not run"), Level::INFO),
        Some(owner) if mods.off(owner) => {
            let (id, version) = mods.name(owner);
            (format!("{id} {version} is off"), Level::INFO)
        }
        Some(owner) => {
            let (id, version) = mods.name(owner);
            (
                format!("{id} {version} defines no such point; update {id}"),
                Level::WARN,
            )
        }
    };
    Inert {
        owner: link.owner,
        point: point.clone(),
        reason,
        level,
    }
}

/// Why `link` does not serve `definition`, if it does not.
fn refusal(link: &Pending, definition: &Definition, mods: &impl Mods) -> Option<(String, Level)> {
    let (id, version) = mods.name(definition.owner);
    if mods.off(definition.owner) {
        return Some((format!("{id} {version} is off"), Level::INFO));
    }
    let (want, have) = (&link.declared, &definition.declared);
    if (want.mode, want.refuse) != (have.mode, have.refuse) {
        return Some((
            format!(
                "{id} {version} declares it {}, this mod {}; both mods must compile one declaration",
                describe(have.mode, have.refuse),
                describe(want.mode, want.refuse)
            ),
            Level::WARN,
        ));
    }
    if want.output != have.output {
        return Some((
            format!(
                "the answer's type, or a field, variant or value inside it, differs here and at {id} {version}; both mods must compile one declaration"
            ),
            Level::WARN,
        ));
    }
    compare(&view(&want.input), &view(&have.input))
        .err()
        .map(|mismatch| {
            let advice = match mismatch {
                LayoutError::Newer { .. } => format!("; update {id}"),
                _ => "; both mods must compile one declaration".to_owned(),
            };
            let mismatch = mismatch.to_string().replace("publisher", "owner");
            (format!("{id} {version}: {mismatch}{advice}"), Level::WARN)
        })
}

/// A mode and the refusal flag, in words.
pub(crate) fn describe(mode: PointMode, refuse: bool) -> String {
    let mode = match mode {
        PointMode::OBSERVE => "observed",
        PointMode::ADVISE => "advised",
        _ => "overridden",
    };
    if refuse {
        format!("{mode}, refusable")
    } else {
        mode.to_owned()
    }
}

/// A layout as borrowed `(name, hash)` pairs.
fn view(fields: &[(String, u64)]) -> Vec<(&str, u64)> {
    fields
        .iter()
        .map(|(name, hash)| (name.as_str(), *hash))
        .collect()
}

impl Sealed {
    /// Runs the live links in turn over `value`, each through `call`, and
    /// answers the chain's verdict. `off` says whether a mod is off; a link
    /// that panics is turned off and its change is lost.
    pub(crate) fn run(
        &self,
        input: *const core::ffi::c_void,
        value: *mut core::ffi::c_void,
        off: impl Fn(usize) -> bool,
        call: impl Fn(usize, &dyn Fn() -> PointVerdict) -> PointVerdict,
        panicked: impl Fn(&Self, &Link),
    ) -> PointVerdict {
        let changing = self.declared.mode != PointMode::OBSERVE;
        let mut verdict = PointVerdict::PASS;
        let mut last_changer = usize::MAX;
        for link in &*self.links {
            if !link.is_on() || off(link.owner) {
                continue;
            }
            // SAFETY: the link lives in the mod's DLL, which stays loaded;
            // the seal checked the owner's input starts with the link's and
            // the answers are one type; input and value live for the call.
            let answer = call(link.owner, &|| unsafe {
                (link.callback)(
                    input,
                    value,
                    core::ptr::with_exposed_provenance_mut(link.user),
                )
            });
            match answer {
                PointVerdict::SET if changing => {
                    link.changes.fetch_add(1, Ordering::Relaxed);
                    if last_changer != usize::MAX && last_changer != link.owner {
                        link.undid.fetch_add(1, Ordering::Relaxed);
                        link.undid_whom.store(last_changer, Ordering::Relaxed);
                    }
                    last_changer = link.owner;
                    verdict = PointVerdict::SET;
                }
                PointVerdict::REFUSE if changing && self.declared.refuse => {
                    link.refusals.fetch_add(1, Ordering::Relaxed);
                    return PointVerdict::REFUSE;
                }
                PointVerdict::PANICKED if link.on.swap(false, Ordering::Relaxed) => {
                    self.arm(&off);
                    panicked(self, link);
                }
                _ => {}
            }
        }
        verdict
    }

    /// Sets the count the owner reads to the links that run: on, of a mod
    /// that is not `off`.
    fn arm(&self, off: impl Fn(usize) -> bool) {
        let count = self
            .links
            .iter()
            .filter(|l| l.is_on() && !off(l.owner))
            .count();
        self.live
            .store(u32::try_from(count).unwrap_or(u32::MAX), Ordering::Relaxed);
    }
}

/// The registry while mods initialise.
static REGISTRY: Mutex<Registry> = Mutex::new(Registry {
    defined: Vec::new(),
    pending: Vec::new(),
});
/// The points once every mod is initialised, by id, read without a lock.
static SEALED: OnceLock<Box<[Sealed]>> = OnceLock::new();
/// The links left inert, for the status and the Mods screen.
static INERT: OnceLock<Vec<Inert>> = OnceLock::new();

/// The registry, for the entry points.
pub(crate) fn registry() -> std::sync::MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Whether the points are sealed: defining and attaching are over.
pub(crate) fn sealed() -> bool {
    SEALED.get().is_some()
}

/// Seals the points, logs every link left inert and arms the rest.
pub(crate) fn seal(mods: &impl Mods) {
    let (points, inert, cycles) = registry().seal(mods, crate::modlist::is_link_off);
    for note in &inert {
        let (id, _) = mods.name(note.owner);
        crate::log::write(
            note.level,
            id,
            &format!(
                "point {}: the link stays inert: {}",
                note.point, note.reason
            ),
        );
    }
    for (id, point) in unmatched_links_off(crate::modlist::links_off(), &inert, mods) {
        crate::log!(
            WARN,
            "{}: links_off of {id} names {point}, which {id} does not link; nothing turned off",
            crate::modlist::LIST_PATH
        );
    }
    for note in &cycles {
        let (id, _) = mods.name(note.owner);
        crate::log::write(
            note.level,
            id,
            &format!("point {}: {}", note.point, note.reason),
        );
    }
    let links: usize = points.iter().map(|p| p.links.len()).sum();
    if !points.is_empty() {
        crate::log!(
            INFO,
            "{} extension points defined, {links} links attached",
            points.len()
        );
    }
    for point in &*points {
        point.arm(|owner| mods.off(owner));
    }
    let _first = SEALED.set(points);
    let _first = INERT.set(inert);
}

/// The entries of the mod list's `links_off` that turned no link off: the
/// mod runs but links no such point.
fn unmatched_links_off<'a>(
    entries: &'a [(String, String)],
    inert: &[Inert],
    mods: &impl Mods,
) -> impl Iterator<Item = (&'a str, &'a str)> {
    entries
        .iter()
        .filter(move |(id, point)| {
            mods.find(id).is_some_and(|index| !mods.off(index))
                && !inert
                    .iter()
                    .any(|n| n.point == *point && mods.name(n.owner).0 == id)
        })
        .map(|(id, point)| (id.as_str(), point.as_str()))
}

/// Counts each point's live links again, once a mod turned off: its links
/// no longer run, and an owner reads 0 when none does.
pub(crate) fn rearm(off: impl Fn(usize) -> bool) {
    for point in SEALED.get().map_or(&[][..], |points| &**points) {
        point.arm(&off);
    }
}

/// Runs the point `id`, which the mod at `asker` defined, over `value`.
pub(crate) fn run(
    asker: usize,
    id: u32,
    input: *const core::ffi::c_void,
    value: *mut core::ffi::c_void,
) -> Result<PointVerdict, Status> {
    let Some(points) = SEALED.get() else {
        return Ok(PointVerdict::PASS);
    };
    let point = points.get(id as usize).ok_or(Status::BAD_ARGS)?;
    if point.owner != asker {
        return Err(Status::REFUSED);
    }
    Ok(point.run(
        input,
        value,
        crate::host::is_off,
        |owner, link| crate::profile::call(owner, crate::profile::Feature::Points, link),
        |point, link| {
            if let Some(loader) = crate::host::loader() {
                let (id, _) = Names::name(loader, link.owner);
                crate::log::write(
                    Level::WARN,
                    id,
                    &format!(
                        "its link on {} panicked and is off; {} runs without it",
                        point.declared.name, point.declared.name
                    ),
                );
            }
        },
    ))
}

/// The sealed points, empty before the seal.
fn points() -> &'static [Sealed] {
    SEALED.get().map_or(&[], |points| points)
}

/// The links left inert, empty before the seal.
fn inert() -> &'static [Inert] {
    INERT.get().map_or(&[], Vec::as_slice)
}

/// A line per point and per inert link, for the status summary.
pub(crate) fn lines(mods: &impl Names) -> Vec<String> {
    let mut lines = Vec::new();
    for point in points() {
        let (id, version) = mods.name(point.owner);
        let chain: Vec<String> = point
            .links
            .iter()
            .map(|l| {
                let (linker, _) = mods.name(l.owner);
                let mut text = format!("{linker} (priority {}", l.priority);
                if mods.off(l.owner) {
                    text.push_str(", off: the mod is off");
                } else if !l.is_on() {
                    text.push_str(", off: it panicked");
                }
                let changes = l.changes.load(Ordering::Relaxed);
                let refusals = l.refusals.load(Ordering::Relaxed);
                if changes > 0 {
                    let _w = write!(text, ", {changes} changes");
                }
                if refusals > 0 {
                    let _w = write!(text, ", {refusals} refusals");
                }
                text.push(')');
                text
            })
            .collect();
        lines.push(format!(
            "point {} ({}) from {id} {version}: {}",
            point.declared.name,
            describe(point.declared.mode, point.declared.refuse),
            if chain.is_empty() {
                "no links".to_owned()
            } else {
                chain.join(" then ")
            }
        ));
    }
    for note in inert() {
        let (id, version) = mods.name(note.owner);
        lines.push(format!(
            "link of {id} {version} on {}: inert, {}",
            note.point, note.reason
        ));
    }
    lines
}

/// Where several mods change one point, for the conflict report: who, in
/// which order, and how often a link changed what another had set.
pub(crate) fn conflicts(mods: &impl Names) -> Vec<String> {
    conflict_lines(points(), mods)
}

/// [`conflicts`] of `points`.
fn conflict_lines(points: &[Sealed], mods: &impl Names) -> Vec<String> {
    let mut lines = Vec::new();
    for point in points {
        if point.declared.mode == PointMode::OBSERVE {
            continue;
        }
        let running = || point.links.iter().filter(|l| !mods.off(l.owner));
        let mut owners: Vec<usize> = running().map(|l| l.owner).collect();
        owners.sort_unstable();
        owners.dedup();
        if owners.len() < 2 {
            continue;
        }
        let order: Vec<&str> = running().map(|l| mods.name(l.owner).0).collect();
        let mut line = format!(
            "point {}: {}, each may change the answer",
            point.declared.name,
            order.join(" then ")
        );
        for link in &*point.links {
            let undid = link.undid.load(Ordering::Relaxed);
            if undid > 0 {
                let whom = link.undid_whom.load(Ordering::Relaxed);
                let times = match undid {
                    1 => "once".to_owned(),
                    2 => "twice".to_owned(),
                    n => format!("{n} times"),
                };
                let _w = write!(
                    line,
                    "; {} changed the answer {} had set, {times}",
                    mods.name(link.owner).0,
                    mods.name(whom).0
                );
            }
        }
        lines.push(line);
    }
    lines
}

/// What the Mods screen says of mod `index`: the points it defines, with
/// their links, and the points it links, with the ones that do not run.
pub(crate) fn screen(index: usize, mods: &impl Names) -> (Vec<String>, Vec<String>) {
    screen_lines(points(), inert(), index, mods)
}

/// [`screen`] of `points` and `inert`.
fn screen_lines(
    points: &[Sealed],
    inert: &[Inert],
    index: usize,
    mods: &impl Names,
) -> (Vec<String>, Vec<String>) {
    let defines = points
        .iter()
        .filter(|p| p.owner == index)
        .map(|p| {
            let running: Vec<&str> = p
                .links
                .iter()
                .filter(|l| l.is_on() && !mods.off(l.owner))
                .map(|l| mods.name(l.owner).0)
                .collect();
            if running.is_empty() {
                p.declared.name.clone()
            } else {
                format!("{} ({})", p.declared.name, running.join(", "))
            }
        })
        .collect();
    let mut extends: Vec<String> = points
        .iter()
        .flat_map(|p| p.links.iter().map(move |l| (p, l)))
        .filter(|(_, l)| l.owner == index)
        .map(|(p, l)| {
            if l.is_on() && !mods.off(index) {
                p.declared.name.clone()
            } else {
                format!("{} (off)", p.declared.name)
            }
        })
        .collect();
    extends.extend(
        inert
            .iter()
            .filter(|n| n.owner == index)
            .map(|n| format!("{} (off)", n.point)),
    );
    (defines, extends)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use enshwick::abi::{Level, PointMode, PointVerdict};

    use super::{Declared, Mods, Pending, Registry, Sealed};
    use crate::services::Names;

    struct Loaded(Vec<(&'static str, &'static str, bool)>);

    impl Names for Loaded {
        fn name(&self, index: usize) -> (&str, &str) {
            self.0.get(index).map_or(("?", "?"), |m| (m.0, m.1))
        }
        fn off(&self, index: usize) -> bool {
            self.0.get(index).is_some_and(|m| m.2)
        }
    }

    impl Mods for Loaded {
        fn find(&self, id: &str) -> Option<usize> {
            self.0.iter().position(|m| m.0 == id)
        }
    }

    fn loaded() -> Loaded {
        Loaded(vec![
            ("safes", "0.3.0", false),
            ("safe-viewers", "0.1.0", false),
            ("safe-rules", "0.1.0", false),
            ("safe-locks", "0.1.0", false),
        ])
    }

    fn fields(names: &[&str]) -> Vec<(String, u64)> {
        names
            .iter()
            .zip(1..)
            .map(|(n, h)| ((*n).to_owned(), h))
            .collect()
    }

    fn access() -> Declared {
        Declared {
            name: "safes:access".to_owned(),
            input: fields(&["player", "owner"]),
            output: fields(&["output"]),
            mode: PointMode::OVERRIDE,
            refuse: true,
        }
    }

    /// The link answers what `user` encodes: the verdict in the low byte,
    /// the value it sets above it.
    unsafe extern "system" fn scripted(
        _input: *const core::ffi::c_void,
        value: *mut core::ffi::c_void,
        user: *mut core::ffi::c_void,
    ) -> PointVerdict {
        let script = user.addr();
        let verdict = PointVerdict(u32::try_from(script & 0xff).unwrap_or(0));
        if verdict == PointVerdict::SET {
            // SAFETY: the tests pass a `u32` value.
            unsafe { *value.cast::<u32>() = u32::try_from(script >> 8).unwrap_or(0) };
        }
        verdict
    }

    fn link(owner: usize, script: usize) -> Pending {
        Pending {
            owner,
            declared: access(),
            priority: 0,
            before: Vec::new(),
            after: Vec::new(),
            callback: scripted,
            user: script,
        }
    }

    fn set(value: usize) -> usize {
        (value << 8) | PointVerdict::SET.0 as usize
    }

    fn seal(registry: &mut Registry) -> (Box<[Sealed]>, Vec<super::Inert>) {
        let (points, inert, cycles) = registry.seal(&loaded(), |_, _| false);
        assert!(cycles.is_empty(), "{cycles:?}");
        (points, inert)
    }

    fn ask(point: &Sealed, value: u32) -> (PointVerdict, u32) {
        let mut value = value;
        let verdict = point.run(
            core::ptr::null(),
            (&raw mut value).cast(),
            |_| false,
            |_, link| link(),
            |_, _| {},
        );
        (verdict, value)
    }

    fn defined(registry: &mut Registry) -> &'static AtomicU32 {
        registry.define(0, "safes", access()).unwrap().1
    }

    #[test]
    fn points_are_named_after_their_owner_once() {
        let mut registry = Registry::default();
        assert!(registry.define(0, "safes", access()).is_ok());
        assert!(registry.define(0, "safes", access()).is_err(), "twice");
        let foreign = Declared {
            name: "area-loot:eligible".to_owned(),
            ..access()
        };
        assert!(registry.define(0, "safes", foreign).is_err());
        let empty = Declared {
            name: "safes:".to_owned(),
            ..access()
        };
        assert!(registry.define(0, "safes", empty).is_err());
        assert!(registry.attach(link(1, 0)).is_ok());
        assert!(registry.attach(link(1, 0)).is_err(), "a mod links once");
    }

    #[test]
    fn links_run_in_their_asked_order_and_the_last_change_wins() {
        let mut registry = Registry::default();
        let live = defined(&mut registry);
        let mut first = link(3, set(30));
        first.before = vec!["safe-viewers".to_owned()];
        let mut last = link(2, set(20));
        last.after = vec!["safe-viewers".to_owned()];
        last.priority = -5;
        assert!(registry.attach(last).is_ok());
        assert!(registry.attach(link(1, set(10))).is_ok());
        assert!(registry.attach(first).is_ok());
        let (points, inert) = seal(&mut registry);
        assert!(inert.is_empty(), "{inert:?}");
        let order: Vec<usize> = points[0].links.iter().map(|l| l.owner).collect();
        assert_eq!(order, [3, 1, 2]);
        points[0].arm(|_| false);
        assert_eq!(live.load(Ordering::Relaxed), 3);
        assert_eq!(ask(&points[0], 1), (PointVerdict::SET, 20));
        assert_eq!(points[0].links[2].undid.load(Ordering::Relaxed), 1);
        assert_eq!(points[0].links[2].undid_whom.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn priority_then_id_order_free_links() {
        let mut registry = Registry::default();
        defined(&mut registry);
        let mut early = link(3, 0);
        early.priority = -1;
        assert!(registry.attach(link(2, 0)).is_ok());
        assert!(registry.attach(link(1, 0)).is_ok());
        assert!(registry.attach(early).is_ok());
        let (points, _) = seal(&mut registry);
        let order: Vec<usize> = points[0].links.iter().map(|l| l.owner).collect();
        // safe-locks first by priority, then safe-rules < safe-viewers by id.
        assert_eq!(order, [3, 2, 1]);
    }

    #[test]
    fn a_refusal_stops_the_chain() {
        let mut registry = Registry::default();
        defined(&mut registry);
        let mut refuses = link(1, PointVerdict::REFUSE.0 as usize);
        refuses.priority = -1;
        assert!(registry.attach(refuses).is_ok());
        assert!(registry.attach(link(2, set(9))).is_ok());
        let (points, _) = seal(&mut registry);
        assert_eq!(ask(&points[0], 1), (PointVerdict::REFUSE, 1));
        assert_eq!(points[0].links[1].changes.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_refusal_where_the_point_allows_none_is_a_pass() {
        let mut registry = Registry::default();
        let plain = Declared {
            refuse: false,
            ..access()
        };
        assert!(registry.define(0, "safes", plain.clone()).is_ok());
        let mut refuses = link(1, PointVerdict::REFUSE.0 as usize);
        refuses.declared = plain.clone();
        let mut sets = link(2, set(4));
        sets.declared = plain;
        assert!(registry.attach(refuses).is_ok());
        assert!(registry.attach(sets).is_ok());
        let (points, _) = seal(&mut registry);
        assert_eq!(ask(&points[0], 1), (PointVerdict::SET, 4));
    }

    #[test]
    fn an_observed_point_keeps_its_answer() {
        let mut registry = Registry::default();
        let observed = Declared {
            mode: PointMode::OBSERVE,
            refuse: false,
            ..access()
        };
        assert!(registry.define(0, "safes", observed.clone()).is_ok());
        let mut watcher = link(1, PointVerdict::REFUSE.0 as usize);
        watcher.declared = observed;
        assert!(registry.attach(watcher).is_ok());
        let (points, _) = seal(&mut registry);
        assert_eq!(ask(&points[0], 7), (PointVerdict::PASS, 7));
    }

    #[test]
    fn a_panicking_link_turns_off_and_the_point_runs_without_it() {
        let mut registry = Registry::default();
        let live = defined(&mut registry);
        let mut panics = link(1, PointVerdict::PANICKED.0 as usize);
        panics.priority = -1;
        assert!(registry.attach(panics).is_ok());
        assert!(registry.attach(link(2, set(5))).is_ok());
        let (points, _) = seal(&mut registry);
        points[0].arm(|_| false);
        let told = std::cell::Cell::new(0);
        let mut value = 1u32;
        let verdict = points[0].run(
            core::ptr::null(),
            (&raw mut value).cast(),
            |_| false,
            |_, link| link(),
            |_, link| told.set(told.get() + link.owner),
        );
        assert_eq!((verdict, value), (PointVerdict::SET, 5));
        assert_eq!(told.get(), 1, "told once, of safe-viewers");
        assert!(!points[0].links[0].is_on());
        assert_eq!(live.load(Ordering::Relaxed), 1);
        assert_eq!(ask(&points[0], 1), (PointVerdict::SET, 5));
        assert_eq!(told.get(), 1, "an off link is not asked again");
    }

    #[test]
    fn a_linking_mod_that_turns_off_leaves_the_count() {
        let mut registry = Registry::default();
        let live = defined(&mut registry);
        assert!(registry.attach(link(1, set(5))).is_ok());
        assert!(registry.attach(link(2, set(6))).is_ok());
        let (points, _) = seal(&mut registry);
        points[0].arm(|_| false);
        assert_eq!(live.load(Ordering::Relaxed), 2);
        points[0].arm(|owner| owner == 2);
        assert_eq!(live.load(Ordering::Relaxed), 1);
        points[0].arm(|owner| owner != 0);
        assert_eq!(
            live.load(Ordering::Relaxed),
            0,
            "no link runs, the owner reads 0"
        );
    }

    #[test]
    fn a_link_the_list_turns_off_stays_inert() {
        let mut registry = Registry::default();
        let live = defined(&mut registry);
        assert!(registry.attach(link(1, set(5))).is_ok());
        assert!(registry.attach(link(2, set(6))).is_ok());
        let (points, inert, _) = registry.seal(&loaded(), |id, point| {
            id == "safe-viewers" && point == "safes:access"
        });
        points[0].arm(|_| false);
        assert_eq!(live.load(Ordering::Relaxed), 1);
        assert_eq!(points[0].links.len(), 1);
        assert_eq!(inert.len(), 1);
        assert_eq!(inert[0].owner, 1);
        assert!(inert[0].reason.contains("mods.toml"), "{}", inert[0].reason);
        assert_eq!(ask(&points[0], 1), (PointVerdict::SET, 6));
    }

    #[test]
    fn a_layout_mismatch_leaves_the_link_inert_with_the_field() {
        let mut registry = Registry::default();
        defined(&mut registry);
        let mut newer = link(1, 0);
        newer.declared.input = fields(&["player", "owner", "safe"]);
        let mut changed = link(2, 0);
        changed.declared.input = vec![("player".to_owned(), 1), ("owner".to_owned(), 99)];
        let mut other_answer = link(3, 0);
        other_answer.declared.output = vec![("output".to_owned(), 7)];
        assert!(registry.attach(newer).is_ok());
        assert!(registry.attach(changed).is_ok());
        assert!(registry.attach(other_answer).is_ok());
        let (points, inert) = seal(&mut registry);
        assert!(points[0].links.is_empty());
        let reasons: Vec<&str> = inert.iter().map(|n| n.reason.as_str()).collect();
        assert!(
            reasons[0].contains("`safe`") && reasons[0].ends_with("update safes"),
            "{}",
            reasons[0]
        );
        assert!(reasons[1].contains("field 1"), "{}", reasons[1]);
        assert!(reasons[2].contains("answer"), "{}", reasons[2]);
        assert!(inert.iter().all(|n| n.level == Level::WARN));
    }

    #[test]
    fn another_mode_leaves_the_link_inert() {
        let mut registry = Registry::default();
        defined(&mut registry);
        let mut advises = link(1, 0);
        advises.declared.mode = PointMode::ADVISE;
        assert!(registry.attach(advises).is_ok());
        let (points, inert) = seal(&mut registry);
        assert!(points[0].links.is_empty());
        assert!(
            inert[0].reason.contains("advised, refusable"),
            "{}",
            inert[0].reason
        );
    }

    #[test]
    fn a_missing_owner_leaves_the_link_inert_without_a_warning() {
        let mut registry = Registry::default();
        let mut gone = link(1, 0);
        gone.declared.name = "area-loot:eligible".to_owned();
        let mut old = link(2, 0);
        old.declared.name = "safes:open".to_owned();
        assert!(registry.attach(gone).is_ok());
        assert!(registry.attach(old).is_ok());
        let (points, inert) = seal(&mut registry);
        assert!(points.is_empty());
        assert_eq!(inert[0].reason, "area-loot does not run");
        assert_eq!(inert[0].level, Level::INFO);
        assert!(inert[1].reason.contains("defines no such point"));
        assert_eq!(inert[1].level, Level::WARN);
        let mut off = loaded();
        off.0[0].2 = true;
        let mut registry = Registry::default();
        assert!(registry.define(0, "safes", access()).is_ok());
        assert!(registry.attach(link(1, 0)).is_ok());
        let (points, inert, _) = registry.seal(&off, |_, _| false);
        assert!(points[0].links.is_empty());
        assert_eq!(inert[0].reason, "safes 0.3.0 is off");
    }

    #[test]
    fn the_report_names_mods_changing_one_point_and_who_undid_whom() {
        let mut registry = Registry::default();
        defined(&mut registry);
        assert!(registry.attach(link(2, set(1))).is_ok());
        assert!(registry.attach(link(1, set(0))).is_ok());
        let mut gone = link(3, 0);
        gone.declared.name = "area-loot:eligible".to_owned();
        assert!(registry.attach(gone).is_ok());
        let (points, inert) = seal(&mut registry);
        let _ = ask(&points[0], 1);
        assert_eq!(
            super::conflict_lines(&points, &loaded()),
            [
                "point safes:access: safe-rules then safe-viewers, each may change the answer; safe-viewers changed the answer safe-rules had set, once"
            ]
        );
        let (defines, extends) = super::screen_lines(&points, &inert, 0, &loaded());
        assert_eq!(defines, ["safes:access (safe-rules, safe-viewers)"]);
        assert!(extends.is_empty());
        let (_, extends) = super::screen_lines(&points, &inert, 3, &loaded());
        assert_eq!(extends, ["area-loot:eligible (off)"]);
        let mut off = loaded();
        off.0[1].2 = true;
        assert!(super::conflict_lines(&points, &off).is_empty());
        let (defines, _) = super::screen_lines(&points, &inert, 0, &off);
        assert_eq!(defines, ["safes:access (safe-rules)"]);
        let (_, extends) = super::screen_lines(&points, &inert, 1, &off);
        assert_eq!(extends, ["safes:access (off)"]);
    }

    #[test]
    fn a_links_off_entry_that_matches_no_link_is_named() {
        let mut registry = Registry::default();
        defined(&mut registry);
        assert!(registry.attach(link(1, 0)).is_ok());
        let (_, inert, _) = registry.seal(&loaded(), |id, point| {
            id == "safe-viewers" && point == "safes:access"
        });
        let entries = [
            ("safe-viewers", "safes:access"),
            ("safe-viewers", "safes:gate"),
            ("safes", "safes:access"),
            ("not-installed", "safes:access"),
        ]
        .map(|(id, point)| (id.to_owned(), point.to_owned()));
        let unmatched: Vec<_> = super::unmatched_links_off(&entries, &inert, &loaded()).collect();
        assert_eq!(
            unmatched,
            [("safe-viewers", "safes:gate"), ("safes", "safes:access")]
        );
    }

    #[test]
    fn a_cycle_is_broken_by_priority_and_named() {
        let mut registry = Registry::default();
        defined(&mut registry);
        let mut a = link(1, 0);
        a.after = vec!["safe-rules".to_owned()];
        let mut b = link(2, 0);
        b.after = vec!["safe-viewers".to_owned()];
        assert!(registry.attach(a).is_ok());
        assert!(registry.attach(b).is_ok());
        let (points, inert, cycles) = registry.seal(&loaded(), |_, _| false);
        assert_eq!(points[0].links.len(), 2);
        assert!(inert.is_empty());
        assert_eq!(cycles.len(), 1);
        assert!(cycles[0].reason.contains("cycle"));
    }
}
