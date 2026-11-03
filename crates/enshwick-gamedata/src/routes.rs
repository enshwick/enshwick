//! Engine routes: table entries found in the game's own code through what
//! the engine names (its ECS systems and its debug strings), the same way on
//! every build, so a new build needs no table row for them.
//!
//! The routes are data, `anchors/routes.toml`, one `[[route]]` per entry and
//! side. A route may name another route's id, an ECS system or a table
//! entry wherever it names a function; each is found when first listed, so
//! the order of the rows does not matter. Each finds its code one of these
//! ways:
//!
//! | Keys | Finds |
//! | --- | --- |
//! | `system` | the code of the listed ECS system, reflected or a record without component lists |
//! | `called_by` | the one function every listed function calls directly, less the helpers a quarter or more of all systems call |
//! | `calls` | the one function that calls every listed function directly |
//! | `loads` | the one function whose code loads every listed string |
//! | `system` or `in`, with `shape` | the one run of instructions of the [shape](shape) in the listed function's code |
//!
//! `take` says what the entry is: `start` (the default; the function, or
//! the shape's first instruction), `end` (the address after the shape),
//! `address` (the address the shape's `*` takes), `value` (the number it
//! takes) or `cell` (the `u32` the game keeps at the address it takes); the
//! last two are constants, the rest symbols. A taken constant needs a shape
//! of two steps or one with an exact operand or an `{entry}`. `shift` turns
//! a taken constant `v` into `1 << v`, and `plus` then adds to it. `sides`
//! limits a route to some sides; it defaults to every side.
//!
//! A shape's start checks the code of the symbol it finds
//! ([`Routed::Symbol`]) when it has two steps or more over eight bytes, and so
//! does `check`, a shape the code at any symbol the route finds must start
//! with, a `jmp` step going on at its target: a symbol that does not finds
//! nothing.
//! Every other symbol is [`Routed::Reached`], and one in the code must start
//! a function. A route that finds another symbol than
//! `known` holds stands for nothing to the routes that name it.
//!
//! A route whose id starts with `~` or has no `.` is a waypoint: a function
//! other routes pass through that no table pins, never an entry itself.
//!
//! A route never guesses: a name that is not found, a set of functions that
//! is not exactly one, or a shape that matches nowhere or twice leaves its
//! entry not found, for the table to supply.

mod check;
mod code;
mod find;
pub(crate) mod shape;
#[cfg(test)]
mod tests;

use rustc_hash::FxHashSet;

use crate::reflect::{Image, Reflection};
use crate::{Resolved, Side, TableError};
pub use check::{check_holds, make_check, shape_checks};
use code::Code;
use shape::{Names, Step};

/// What an engine route found.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::Routed;
///
/// assert_ne!(Routed::Symbol(0x10), Routed::Reached(0x10));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Routed {
    /// A symbol whose code the route's shape matched, relative to the image
    /// base: the match checked the code there.
    Symbol(u64),
    /// A symbol the route reached without matching its code, relative to the
    /// image base: the start of a function, an address the shape took, or the
    /// address after the shape. Nothing checked the code there.
    Reached(u64),
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
    System(String),
    CalledBy(Vec<String>),
    Calls(Vec<String>),
    Loads(Vec<String>),
    Shape { within: String, steps: Vec<Step> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Route {
    id: String,
    sides: Vec<Side>,
    how: How,
    take: Take,
    /// The shape the code at a found symbol must start with.
    check: Vec<Step>,
    /// Whether a taken constant `v` becomes `1 << v`.
    shift: bool,
    /// What is added to a taken constant.
    plus: i64,
}

impl Route {
    /// Whether the route finds an entry rather than a waypoint.
    fn is_entry(&self) -> bool {
        !self.id.starts_with('~') && self.id.contains('.')
    }

    /// Whether the route's match checks the code at the symbol it finds:
    /// the start of its shape.
    fn checks_code(&self) -> bool {
        !self.check.is_empty()
            || self.take == Take::Start
                && matches!(&self.how, How::Shape { steps, .. } if steps.len() >= 2)
    }
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
    calls: Option<Vec<String>>,
    loads: Option<Vec<String>>,
    system: Option<String>,
    #[serde(rename = "in")]
    within: Option<String>,
    shape: Option<Vec<String>>,
    take: Option<Take>,
    check: Option<Vec<String>>,
    shift: Option<bool>,
    plus: Option<i64>,
}

impl RouteRow {
    fn route(self, names: &Names) -> Result<Route, TableError> {
        let id = self.id;
        let bad = |why: &str| TableError::Toml(format!("{id}: {why}"));
        let listed = |list: Option<Vec<String>>| list.filter(|l| !l.is_empty());
        let how = match (
            listed(self.called_by),
            listed(self.calls),
            listed(self.loads),
            self.system,
            self.within,
            self.shape,
        ) {
            (Some(names), None, None, None, None, None) => How::CalledBy(names),
            (None, Some(names), None, None, None, None) => How::Calls(names),
            (None, None, Some(strings), None, None, None) => How::Loads(strings),
            (None, None, None, Some(system), None, None) => How::System(system),
            (None, None, None, Some(within), None, Some(shape))
            | (None, None, None, None, Some(within), Some(shape))
                if !shape.is_empty() =>
            {
                How::Shape {
                    within,
                    steps: shape
                        .iter()
                        .map(|step| Step::parse(step, names).map_err(|e| bad(&e.to_string())))
                        .collect::<Result<_, _>>()?,
                }
            }
            _ => {
                return Err(bad(
                    "a route is `called_by`, `calls`, `loads`, `system`, or `system` or `in` with `shape`, each not empty",
                ));
            }
        };
        let take = self.take.unwrap_or(Take::Start);
        let steps: &[Step] = match &how {
            How::Shape { steps, .. } => steps,
            _ => &[],
        };
        let takes: usize = steps.iter().map(Step::takes).sum();
        let shaped = !steps.is_empty();
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
        if matches!(take, Take::Value | Take::Cell)
            && steps.len() < 2
            && !steps.iter().any(Step::is_exact)
        {
            return Err(bad(
                "a taken constant needs a shape of two steps, an exact operand or an `{entry}`",
            ));
        }
        let check: Vec<Step> = self
            .check
            .unwrap_or_default()
            .iter()
            .map(|step| Step::parse(step, names).map_err(|e| bad(&e.to_string())))
            .collect::<Result<_, _>>()?;
        if !check.is_empty()
            && (matches!(take, Take::Value | Take::Cell) || check.iter().any(|s| s.takes() > 0))
        {
            return Err(bad("`check` is a shape without `*` for a symbol"));
        }
        if (self.shift.is_some() || self.plus.is_some())
            && !matches!(take, Take::Value | Take::Cell)
        {
            return Err(bad("`shift` and `plus` change a `value` or `cell` take"));
        }
        Ok(Route {
            sides: self
                .sides
                .unwrap_or_else(|| vec![Side::Server, Side::Client]),
            id,
            how,
            take,
            check,
            shift: self.shift.unwrap_or(false),
            plus: self.plus.unwrap_or(0),
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
    /// Sorted by id.
    routes: Vec<Route>,
}

impl Routes {
    /// Reads routes from their TOML.
    ///
    /// # Errors
    ///
    /// [`TableError::Toml`] when the text is no routes file, two routes
    /// share an id on one side, or a route names no way or a wrong one.
    ///
    /// # Examples
    ///
    /// See [`Routes`].
    pub fn parse(text: &str) -> Result<Self, TableError> {
        let file: RoutesFile = toml::from_str(text).map_err(|e| TableError::Toml(e.to_string()))?;
        let names = Names::new();
        let mut routes: Vec<Route> = file
            .route
            .into_iter()
            .map(|row| row.route(&names))
            .collect::<Result<_, _>>()?;
        routes.sort_by(|a, b| a.id.cmp(&b.id));
        let mut seen = FxHashSet::default();
        for route in &routes {
            if let Some(side) = route
                .sides
                .iter()
                .find(|&&s| !seen.insert((route.id.as_str(), s)))
            {
                return Err(TableError::Toml(format!(
                    "{}: two routes on the {side:?} side",
                    route.id
                )));
            }
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

    /// The id of every entry a route finds, once each, sorted; waypoints are
    /// left out.
    ///
    /// # Examples
    ///
    /// See [`Routes`].
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        let mut last = None;
        self.routes
            .iter()
            .filter(|r| r.is_entry())
            .map(|r| r.id.as_str())
            .filter(move |&id| last.replace(id) != Some(id))
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
        let from = self.routes.partition_point(|r| r.id.as_str() < id);
        self.routes[from..]
            .iter()
            .take_while(|r| r.id == id)
            .any(|r| r.is_entry() && r.sides.contains(&side))
    }

    /// Whether the route of `side` for symbol `id` checks the code it finds,
    /// as its `<id>.bytes` would: its shape starts there.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_gamedata::{Routes, Side};
    ///
    /// let routes = Routes::embedded()?;
    /// assert!(routes.checks("ui.slider", Side::Client));
    /// assert!(!routes.checks("ui.slider", Side::Server));
    /// # Ok::<(), enshwick_gamedata::TableError>(())
    /// ```
    #[must_use]
    pub fn checks(&self, id: &str, side: Side) -> bool {
        let from = self.routes.partition_point(|r| r.id.as_str() < id);
        self.routes[from..]
            .iter()
            .take_while(|r| r.id == id)
            .any(|r| r.is_entry() && r.sides.contains(&side) && r.checks_code())
    }

    /// What the route of `side` for each entry finds in `image`, by id,
    /// `None` where it finds nothing; `known` gives the functions and
    /// entries a route names that no route of `side` finds.
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
    /// let routes = Routes::embedded()?;
    /// let found = routes.find(&image, &reflection, Side::Server, &table.resolve());
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
        let threads = std::thread::available_parallelism().map_or(1, usize::from);
        self.find_on(threads, image, reflection, side, known)
    }

    /// [`Routes::find`] on at most `threads` threads, for measuring the
    /// start on fewer cores.
    #[doc(hidden)]
    #[must_use]
    pub fn find_on<'r>(
        &'r self,
        threads: usize,
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
        let entries = || {
            sided
                .iter()
                .copied()
                .enumerate()
                .filter(|(_, r)| r.is_entry())
        };
        let strings = find::strings(image, &sided, threads);
        let wanted: FxHashSet<u64> = strings.values().flatten().copied().collect();
        let Some(code) = Code::new(image, &wanted, threads) else {
            return entries().map(|(_, r)| (r.id.as_str(), None)).collect();
        };
        let mut finder = find::Finder::new(&code, reflection, known, &sided, strings);
        entries()
            .map(|(i, r)| (r.id.as_str(), finder.resolve(i)))
            .collect()
    }
}
