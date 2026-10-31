//! Extension points. During init the loader keeps the points mods define and
//! the links they attach; once every mod is initialised it matches each link
//! with its point field by field, orders the links the same way at every
//! start, and sets the count of live links the owner reads before it asks.
//! Asking a point runs its live links in turn on the asking thread; a link
//! that panics is turned off, and the point runs on without it.

use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick::abi::{Level, PointFn, PointMode, PointVerdict, Status};
use enshwick::points::{LinkPlace, order};
use enshwick::services::{LayoutError, compare};

use crate::locks::Relock;
use crate::mod_index::ModIndex;
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
    /// The bytes of the input the declaration spans.
    pub(crate) input_size: u32,
    /// The answer's size in bytes.
    pub(crate) output_size: u32,
}

/// A defined point.
#[derive(Debug)]
struct Definition {
    owner: ModIndex,
    declared: Declared,
    live: &'static AtomicU32,
}

/// A link waiting for the seal.
#[derive(Debug)]
pub(crate) struct Pending {
    /// The linking mod's index.
    pub(crate) owner: ModIndex,
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
    pub(crate) owner: ModIndex,
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
    pub(crate) owner: ModIndex,
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
    pub(crate) owner: ModIndex,
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
    /// Defines a point named after its definer, the mod `owner_id`, and
    /// returns its id and the count of its live links.
    ///
    /// # Errors
    ///
    /// Why it is refused: a name outside the definer's namespace, or defined
    /// twice.
    pub(crate) fn define(
        &mut self,
        owner: ModIndex,
        owner_id: &str,
        declared: Declared,
    ) -> Result<(u32, &'static AtomicU32), String> {
        let name = &declared.name;
        if !enshwick::__private::rules::is_named_after(name, owner_id) {
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
            if mods.off(link.owner.get()) {
                continue;
            }
            let point = link.declared.name.clone();
            let find = |name: &str| {
                self.defined
                    .iter()
                    .enumerate()
                    .find(|(_, d)| d.declared.name == name)
            };
            let found = find(&point).or_else(|| renamed(&point, mods).and_then(|p| find(&p)));
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
            let (linker, _) = mods.name(link.owner.get());
            if link_off(linker, &point) || link_off(linker, &definition.declared.name) {
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
                id: mods.name(l.owner.get()).0,
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

/// The point `point` under its owner's id now, when the owner had the id
/// the point names before a rename: `old:what` as `new:what`.
fn renamed(point: &str, mods: &impl Mods) -> Option<String> {
    let (owner, what) = point.split_once(':')?;
    let (id, _) = mods.name(mods.find(owner)?);
    (id != owner).then(|| format!("{id}:{what}"))
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
    let (id, version) = mods.name(definition.owner.get());
    if mods.off(definition.owner.get()) {
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
    if want.output_size != have.output_size || want.input_size > have.input_size {
        return Some((
            format!(
                "the input or the answer is larger here than at {id} {version}, or the answer's size differs; both mods must compile one declaration"
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
        off: impl Fn(ModIndex) -> bool,
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
            let answer = call(link.owner.get(), &|| unsafe {
                (link.callback)(
                    input,
                    value,
                    core::ptr::with_exposed_provenance_mut(link.user),
                )
            });
            match answer {
                PointVerdict::SET if changing => {
                    link.changes.fetch_add(1, Ordering::Relaxed);
                    if last_changer != usize::MAX && last_changer != link.owner.get() {
                        link.undid.fetch_add(1, Ordering::Relaxed);
                        link.undid_whom.store(last_changer, Ordering::Relaxed);
                    }
                    last_changer = link.owner.get();
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
    fn arm(&self, off: impl Fn(ModIndex) -> bool) {
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
    REGISTRY.relock()
}

/// Whether the points are sealed: defining and attaching are over.
pub(crate) fn sealed() -> bool {
    SEALED.get().is_some()
}

/// Seals the points, logs every link left inert and arms the rest.
pub(crate) fn seal(mods: &impl Mods) {
    let (points, inert, cycles) = registry().seal(mods, crate::modlist::is_link_off);
    for note in &inert {
        let (id, _) = mods.name(note.owner.get());
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
        let (id, _) = mods.name(note.owner.get());
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
        point.arm(|owner| mods.off(owner.get()));
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
                    .any(|n| n.point == *point && mods.name(n.owner.get()).0 == id)
        })
        .map(|(id, point)| (id.as_str(), point.as_str()))
}

/// Counts each point's live links again, once a mod turned off: its links
/// no longer run, and an owner reads 0 when none does.
pub(crate) fn rearm(off: impl Fn(ModIndex) -> bool) {
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
    if point.owner != ModIndex::new(asker) {
        return Err(Status::REFUSED);
    }
    Ok(point.run(
        input,
        value,
        crate::host::is_off,
        |owner, link| {
            crate::profile::call(ModIndex::new(owner), crate::profile::Feature::Points, link)
        },
        |point, link| {
            if let Some(loader) = crate::host::loader() {
                let (id, _) = Names::name(loader, link.owner.get());
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
        let (id, version) = mods.name(point.owner.get());
        let chain: Vec<String> = point
            .links
            .iter()
            .map(|l| {
                let (linker, _) = mods.name(l.owner.get());
                let mut text = format!("{linker} (priority {}", l.priority);
                if mods.off(l.owner.get()) {
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
        let (id, version) = mods.name(note.owner.get());
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
        let running = || point.links.iter().filter(|l| !mods.off(l.owner.get()));
        let mut owners: Vec<ModIndex> = running().map(|l| l.owner).collect();
        owners.sort_unstable();
        owners.dedup();
        if owners.len() < 2 {
            continue;
        }
        let order: Vec<&str> = running().map(|l| mods.name(l.owner.get()).0).collect();
        let mut parts = vec![crate::texts::fill(
            "points.conflict",
            &[
                ("point", &point.declared.name),
                (
                    "mods",
                    &order.join(&crate::texts::fill("conflicts.then", &[])),
                ),
            ],
        )];
        for link in &*point.links {
            let undid = link.undid.load(Ordering::Relaxed);
            if undid > 0 {
                let whom = link.undid_whom.load(Ordering::Relaxed);
                parts.push(crate::texts::fill(
                    "points.undid",
                    &[
                        ("mod", mods.name(link.owner.get()).0),
                        ("whom", mods.name(whom).0),
                        ("count", &undid.to_string()),
                    ],
                ));
            }
        }
        lines.push(parts.join("; "));
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
        .filter(|p| p.owner == ModIndex::new(index))
        .map(|p| {
            let running: Vec<&str> = p
                .links
                .iter()
                .filter(|l| l.is_on() && !mods.off(l.owner.get()))
                .map(|l| mods.name(l.owner.get()).0)
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
        .filter(|(_, l)| l.owner == ModIndex::new(index))
        .map(|(p, l)| {
            if l.is_on() && !mods.off(index) {
                p.declared.name.clone()
            } else {
                crate::texts::fill("points.off", &[("point", &p.declared.name)])
            }
        })
        .collect();
    extends.extend(
        inert
            .iter()
            .filter(|n| n.owner == ModIndex::new(index))
            .map(|n| crate::texts::fill("points.off", &[("point", &n.point)])),
    );
    (defines, extends)
}

#[cfg(test)]
mod tests;
