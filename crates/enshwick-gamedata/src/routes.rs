//! Engine routes: table entries found in the game's own code through what
//! the engine names (its reflected ECS systems and its debug strings), the
//! same way on every build, so a new build needs no table row for them.
//!
//! The routes are data, `anchors/routes.toml`, one `[[route]]` per entry,
//! in the order they resolve; a route may name an entry an earlier route
//! found. Each finds its code one of three ways:
//!
//! | Keys | Finds |
//! | --- | --- |
//! | `called_by` | the one function every named system calls directly, less the helpers a quarter or more of all systems call |
//! | `loads` | the one function whose code loads every named string |
//! | `system`, `shape` | the first run of instructions of the [shape](shape) in the named system's code |
//!
//! `take` says what the entry is: `start` (the default; the function, or
//! the shape's first instruction), `end` (the address after the shape),
//! `address` (the address the shape's `*` takes), `value` (the number it
//! takes) or `cell` (the `u32` the game keeps at the address it takes);
//! the last two are constants, the rest symbols. `sides` limits a route to
//! some sides; it defaults to every side.

mod code;
pub(crate) mod shape;
#[cfg(test)]
mod tests;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::reflect::{Image, Reflection};
use crate::{Resolved, Side, TableError};
use code::Code;
use shape::Step;

/// A quarter or more of all systems calling a function makes it a helper,
/// such as the ECS query calls nearly every system makes.
const HELPER_SHARE: usize = 4;

/// What an engine route found.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::Routed;
///
/// assert_ne!(Routed::Symbol(0x10), Routed::Constant(0x10));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Routed {
    /// A symbol, relative to the image base.
    Symbol(u64),
    /// A constant.
    Constant(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum Take {
    Start,
    End,
    Address,
    Value,
    Cell,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum How {
    CalledBy(Vec<String>),
    Loads(Vec<String>),
    Shape { system: String, steps: Vec<Step> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Route {
    id: String,
    sides: Vec<Side>,
    how: How,
    take: Take,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RoutesFile {
    #[serde(default)]
    route: Vec<RouteRow>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RouteRow {
    id: String,
    sides: Option<Vec<Side>>,
    called_by: Option<Vec<String>>,
    loads: Option<Vec<String>>,
    system: Option<String>,
    shape: Option<Vec<String>>,
    take: Option<Take>,
}

impl RouteRow {
    fn route(self) -> Result<Route, TableError> {
        let id = self.id;
        let bad = |why: &str| TableError::Toml(format!("{id}: {why}"));
        let how = match (self.called_by, self.loads, self.system, self.shape) {
            (Some(systems), None, None, None) if !systems.is_empty() => How::CalledBy(systems),
            (None, Some(strings), None, None) if !strings.is_empty() => How::Loads(strings),
            (None, None, Some(system), Some(shape)) if !shape.is_empty() => How::Shape {
                system,
                steps: shape
                    .iter()
                    .map(|step| Step::parse(step).map_err(|e| bad(&e.to_string())))
                    .collect::<Result<_, _>>()?,
            },
            _ => {
                return Err(bad(
                    "a route is `called_by`, `loads`, or `system` with `shape`, each not empty",
                ));
            }
        };
        let take = self.take.unwrap_or(Take::Start);
        let takes = match &how {
            How::Shape { steps, .. } => steps.iter().map(Step::takes).sum(),
            _ => 0,
        };
        let shaped = matches!(how, How::Shape { .. });
        match take {
            Take::Start if takes == 0 => {}
            Take::End if shaped && takes == 0 => {}
            Take::Address | Take::Value | Take::Cell if shaped && takes == 1 => {}
            _ => {
                return Err(bad(
                    "`end` takes a shape, `address`, `value` and `cell` a shape with one `*`, and `start` no `*`",
                ));
            }
        }
        Ok(Route {
            sides: self
                .sides
                .unwrap_or_else(|| vec![Side::Server, Side::Client]),
            id,
            how,
            take,
        })
    }
}

/// The engine routes, read from their data.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::Routes;
///
/// let routes = Routes::parse(
///     r#"
///     [[route]]
///     id = "ui.slider"
///     loads = ["doSettingSlider()"]
///     sides = ["client"]
///     "#,
/// )?;
/// assert_eq!(routes.ids().collect::<Vec<_>>(), ["ui.slider"]);
/// # Ok::<(), enshwick_gamedata::TableError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Routes {
    routes: Vec<Route>,
}

impl Routes {
    /// Reads routes from their TOML.
    ///
    /// # Errors
    ///
    /// [`TableError::Toml`] when the text is no routes file, two routes
    /// share an id, or a route names no way or a wrong one.
    ///
    /// # Examples
    ///
    /// See [`Routes`].
    pub fn parse(text: &str) -> Result<Self, TableError> {
        let file: RoutesFile = toml::from_str(text).map_err(|e| TableError::Toml(e.to_string()))?;
        let routes: Vec<Route> = file
            .route
            .into_iter()
            .map(RouteRow::route)
            .collect::<Result<_, _>>()?;
        let mut seen = FxHashSet::default();
        if let Some(twice) = routes.iter().find(|r| !seen.insert(r.id.as_str())) {
            return Err(TableError::Toml(format!("{}: two routes", twice.id)));
        }
        Ok(Self { routes })
    }

    /// The routes the loader embeds.
    ///
    /// # Errors
    ///
    /// As [`Routes::parse`]; a test reads them on every build of the crate.
    ///
    /// # Examples
    ///
    /// ```
    /// let routes = enshwick_gamedata::Routes::embedded()?;
    /// assert!(routes.ids().any(|id| id == "knowledge.push"));
    /// # Ok::<(), enshwick_gamedata::TableError>(())
    /// ```
    pub fn embedded() -> Result<Self, TableError> {
        Self::parse(include_str!("../anchors/routes.toml"))
    }

    /// The id of every route, in order.
    ///
    /// # Examples
    ///
    /// See [`Routes`].
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.routes.iter().map(|r| r.id.as_str())
    }

    /// Whether a route of `side` finds the entry `id`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_gamedata::{Routes, Side};
    ///
    /// let routes = Routes::embedded()?;
    /// assert!(routes.routes("ui.slider", Side::Client));
    /// assert!(!routes.routes("ui.slider", Side::Server));
    /// # Ok::<(), enshwick_gamedata::TableError>(())
    /// ```
    #[must_use]
    pub fn routes(&self, id: &str, side: Side) -> bool {
        self.routes
            .iter()
            .any(|r| r.id == id && r.sides.contains(&side))
    }

    /// What each route of `side` finds in `image`, in route order, `None`
    /// where it finds nothing or more than one answer; `known` gives the
    /// entries a shape names that no earlier route found.
    ///
    /// A route never guesses: a system or string that is missing, a set of
    /// callees or loading functions that is not exactly one, or a shape that
    /// does not match leaves its entry `None`, for the table to supply.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use enshwick_gamedata::reflect::{Image, Layout, Reflection};
    /// use enshwick_gamedata::{Routes, Side, Table};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let bytes = std::fs::read("enshrouded_server.exe")?;
    /// let image = Image::parse(&bytes, Layout::File, None)?;
    /// let reflection = Reflection::scan(&image);
    /// let table = Table::parse("name = \"t\"\nside = \"server\"\nguid = \"00000000000000000000000000000001\"\nage = 1")?;
    /// let found = Routes::embedded()?.find(&image, &reflection, Side::Server, &table.resolve());
    /// assert!(found.iter().any(|(id, routed)| *id == "knowledge.push" && routed.is_some()));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn find<'r>(
        &'r self,
        image: &Image<'_>,
        reflection: &Reflection<'_>,
        side: Side,
        known: &Resolved,
    ) -> Vec<(&'r str, Option<Routed>)> {
        let sided: Vec<&Route> = self
            .routes
            .iter()
            .filter(|r| r.sides.contains(&side))
            .collect();
        let Some(code) = Code::new(image) else {
            return sided.iter().map(|r| (r.id.as_str(), None)).collect();
        };
        let mut finder = Finder {
            code: &code,
            reflection,
            helpers: None,
            loaders: loaders(&code, &sided),
            found: FxHashMap::default(),
            known,
        };
        sided
            .iter()
            .map(|route| {
                let routed = finder.find(route);
                if let Some(Routed::Symbol(rva)) = routed {
                    finder.found.insert(route.id.as_str(), rva);
                }
                (route.id.as_str(), routed)
            })
            .collect()
    }
}

/// The functions that load each string any of `routes` names, by string.
fn loaders(code: &Code<'_, '_>, routes: &[&Route]) -> FxHashMap<String, FxHashSet<u32>> {
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
    let addresses: Vec<(usize, u64)> = code
        .strings(&texts)
        .into_iter()
        .enumerate()
        .flat_map(|(t, found)| found.into_iter().map(move |va| (t, va)))
        .collect();
    let targets: Vec<u64> = addresses.iter().map(|&(_, va)| va).collect();
    let mut by_text: FxHashMap<String, FxHashSet<u32>> = texts
        .iter()
        .map(|t| ((*t).to_owned(), FxHashSet::default()))
        .collect();
    for ((t, _), functions) in addresses.iter().zip(code.loaders(&targets)) {
        if let Some(set) = by_text.get_mut(texts[*t]) {
            set.extend(functions);
        }
    }
    by_text
}

/// How many systems there are, and how many of them call each function.
fn callers(code: &Code<'_, '_>, reflection: &Reflection<'_>) -> (usize, FxHashMap<u64, usize>) {
    let entries: FxHashSet<u32> = reflection
        .systems()
        .filter_map(|s| u32::try_from(s.entry().checked_sub(code.base())?).ok())
        .filter(|&rva| rva != 0)
        .collect();
    let mut counts: FxHashMap<u64, usize> = FxHashMap::default();
    for &entry in &entries {
        for callee in code.callees(entry) {
            *counts.entry(callee).or_default() += 1;
        }
    }
    (entries.len(), counts)
}

struct Finder<'f, 'i, 'a, 'r> {
    code: &'f Code<'i, 'a>,
    reflection: &'f Reflection<'f>,
    /// How many systems call each function, once a route needs it.
    helpers: Option<(usize, FxHashMap<u64, usize>)>,
    loaders: FxHashMap<String, FxHashSet<u32>>,
    found: FxHashMap<&'r str, u64>,
    known: &'f Resolved,
}

impl Finder<'_, '_, '_, '_> {
    fn find(&mut self, route: &Route) -> Option<Routed> {
        match &route.how {
            How::CalledBy(systems) => self.called_by(systems).map(Routed::Symbol),
            How::Loads(strings) => self.loading(strings).map(Routed::Symbol),
            How::Shape { system, steps } => self.shaped(system, steps, route.take),
        }
    }

    /// The begin RVA of the function the system `name` runs.
    fn system(&self, name: &str) -> Option<u32> {
        let entry = self.reflection.system(name).ok()?.entry();
        u32::try_from(entry.checked_sub(self.code.base())?)
            .ok()
            .filter(|&rva| rva != 0)
    }

    fn called_by(&mut self, systems: &[String]) -> Option<u64> {
        let mut common: Option<FxHashSet<u64>> = None;
        for name in systems {
            let callees = self.code.callees(self.system(name)?);
            common = Some(match common {
                None => callees,
                Some(mut set) => {
                    set.retain(|c| callees.contains(c));
                    set
                }
            });
        }
        if self.helpers.is_none() {
            self.helpers = Some(callers(self.code, self.reflection));
        }
        let (total, counts) = self.helpers.as_ref()?;
        let total = *total;
        let mut left = common?
            .into_iter()
            .filter(|c| counts.get(c).copied().unwrap_or(0) * HELPER_SHARE < total);
        let one = left.next()?;
        left.next().is_none().then_some(one)
    }

    fn loading(&self, strings: &[String]) -> Option<u64> {
        let mut sets = strings.iter().map(|s| self.loaders.get(s));
        let mut common = sets.next()??.clone();
        for set in sets {
            let set = set?;
            common.retain(|f| set.contains(f));
        }
        let mut left = common.into_iter();
        let one = left.next()?;
        left.next().is_none().then_some(u64::from(one))
    }

    fn shaped(&self, system: &str, steps: &[Step], take: Take) -> Option<Routed> {
        let base = self.code.base();
        let entry = |id: &str| {
            self.found
                .get(id)
                .copied()
                .or_else(|| self.known.symbol(id))
                .map(|rva| base + rva)
        };
        let found = self
            .code
            .parts_of(self.system(system)?)
            .find_map(|part| shape::find(steps, &self.code.decode(part), &entry))?;
        match take {
            Take::Start => Some(Routed::Symbol(found.start - base)),
            Take::End => Some(Routed::Symbol(found.end - base)),
            Take::Address => Some(Routed::Symbol(found.taken?.checked_sub(base)?)),
            Take::Value => Some(Routed::Constant(found.taken?)),
            Take::Cell => Some(Routed::Constant(u64::from(
                self.code.image().u32(found.taken?)?,
            ))),
        }
    }
}
