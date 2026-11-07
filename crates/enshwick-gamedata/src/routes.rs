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
//! | `callers`, with `shape` | among the functions that call the listed function directly, the one whose code the shape matches; a call in a chained fragment counts for its function |
//! | `vtable`, with `slot` | the function the 64-bit pointer at the listed vtable + 8 · `slot` points at, which must start a function |
//! | `field` | a constant: the sum of the reflected offsets of each `<type>.<field>` listed, each field holding the next type |
//! | `size` | a constant: the reflected size of the type |
//!
//! `take` says what a shaped entry is: `start` (the default; the function,
//! or the shape's first instruction), `end` (the address after the shape),
//! `address` (the address the shape's `*` takes), `value` (the number it
//! takes), `cell` (the `u32` the game keeps at the address it takes) or
//! `spelled` (the route's `value`, which a step of the shape spells in its
//! own encoding: memory without a displacement spells 0, else an index
//! scale or an immediate equal to it; a `nop` spells nothing) or `scale`
//! (the factor the shape's instructions multiply the register its first
//! one reads by, such as a record size held only in `lea r9,[rax+rax*4];
//! shl r9,5`); the last four are constants, the rest symbols. A taken
//! constant needs a shape of two steps or one with an exact operand or an
//! `{entry}`. `shift` turns
//! a taken constant `v` into `1 << v`, and `plus` then adds to it. `sides`
//! limits a route to some sides; it defaults to every side.
//!
//! `range = [least, greatest]` and `align` bound a constant a route takes
//! or reflects: one outside them is [`Routed::Outside`] and no entry, so a
//! shape that matches a changed instruction gives nothing rather than a
//! wrong value. Every constant entry states its range, as a property of
//! the value: from half the least to twice the greatest the kept builds
//! give, at least 16, and exactly the value for a name hash the code holds;
//! a 64-bit key that changes between builds states `unbounded = true`.
//! An address a shape takes outside `.text` counts only inside `.data` or
//! `.rdata`. `type` names the reflected type the last step of a `field`
//! route holds, which that field must still hold.
//!
//! `nth` lets a `system` or `in` shape match more than once and takes its
//! `nth` run, from 1, in address order, when there are exactly `of` runs;
//! with `after`, a shape that must match once in the function, it counts
//! only the runs in the same part of the code that start after that match,
//! such as the ninth of thirteen identical copies.
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
pub use shape::ShapeNames;
use shape::{ShapeError, Step};

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
    /// A constant outside the range or alignment its route states: the
    /// route finds no entry, and the value says what the code gave.
    Outside(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum Take {
    Start,
    End,
    Address,
    Value,
    Cell,
    Spelled,
    Scale,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum How {
    System(String),
    CalledBy(Vec<String>),
    Calls(Vec<String>),
    Loads(Vec<String>),
    Shape {
        within: String,
        steps: Vec<Step>,
    },
    /// The one function among the direct callers of `callee` whose code the
    /// shape matches.
    Callers {
        callee: String,
        steps: Vec<Step>,
    },
    /// The function a slot of a vtable points at: the 64-bit pointer at
    /// `vtable` + 8 · `slot`.
    Slot {
        vtable: String,
        slot: u32,
    },
    /// The sum of reflected field offsets, each a type and its field.
    Field(Vec<(String, String)>),
    /// A reflected type's size.
    Size(String),
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
    /// The constant a `spelled` take finds.
    value: u64,
    /// The shape that must match once before an ordinal's runs are counted.
    after: Vec<Step>,
    /// Which run of the shape a route takes, from 1, and how many runs
    /// there must be, where it takes one of several rather than the only
    /// one.
    nth: Option<(u32, u32)>,
    /// The least and the greatest value a constant may have, both included.
    range: Option<(u64, u64)>,
    /// What a constant must be a multiple of; 1 for any.
    align: u64,
    /// The reflected type the last step of a `field` route must hold.
    holds: Option<String>,
}

impl Route {
    /// Whether the route finds an entry rather than a waypoint.
    fn is_entry(&self) -> bool {
        !self.id.starts_with('~') && self.id.contains('.')
    }

    /// Whether the route finds a constant: one it takes from code, or a
    /// reflected offset or size.
    #[cfg(test)]
    fn finds_constant(&self) -> bool {
        matches!(self.take, Take::Value | Take::Cell | Take::Scale)
            || matches!(self.how, How::Field(_) | How::Size(_))
    }

    /// `found`, or [`Routed::Outside`] for a constant outside the route's
    /// range or alignment.
    fn bounded(&self, found: Routed) -> Routed {
        match found {
            Routed::Constant(v)
                if self.range.is_some_and(|(lo, hi)| !(lo..=hi).contains(&v))
                    || !v.is_multiple_of(self.align) =>
            {
                Routed::Outside(v)
            }
            found => found,
        }
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
    value: Option<u64>,
    field: Option<Vec<String>>,
    size: Option<String>,
    callers: Option<String>,
    vtable: Option<String>,
    slot: Option<u32>,
    after: Option<Vec<String>>,
    nth: Option<u32>,
    of: Option<u32>,
    range: Option<[u64; 2]>,
    unbounded: Option<bool>,
    align: Option<u64>,
    #[serde(rename = "type")]
    holds: Option<String>,
}

impl RouteRow {
    fn route(mut self, names: &ShapeNames) -> Result<Route, TableError> {
        let how = self.how(names)?;
        let id = self.id;
        let bad = |why: &str| TableError::Toml(format!("{id}: {why}"));
        let reflected = matches!(how, How::Field(_) | How::Size(_));
        if reflected
            && (self.take.is_some()
                || self.check.is_some()
                || self.shift.is_some()
                || self.plus.is_some())
        {
            return Err(bad(
                "a `field` or `size` route is the reflected number itself, without `take`, `check`, `shift` or `plus`",
            ));
        }
        let take = self.take.unwrap_or(Take::Start);
        let steps: &[Step] = match &how {
            How::Shape { steps, .. } | How::Callers { steps, .. } => steps,
            _ => &[],
        };
        let shaped = !steps.is_empty();
        let takes: usize = steps.iter().map(Step::takes).sum();
        match take {
            _ if reflected => {}
            Take::Start if takes == 0 => {}
            Take::End | Take::Spelled | Take::Scale if shaped && takes == 0 => {}
            Take::Address | Take::Value | Take::Cell if shaped && takes == 1 => {}
            _ => {
                return Err(bad(
                    "`end`, `spelled` and `scale` take a shape without `*`, `address`, `value` and `cell` a shape with one `*`, and `start` no `*`",
                ));
            }
        }
        let constant = matches!(take, Take::Value | Take::Cell | Take::Spelled | Take::Scale);
        if constant && steps.len() < 2 && !steps.iter().any(Step::is_exact) {
            return Err(bad(
                "a taken constant needs a shape of two steps, an exact operand or an `{entry}`",
            ));
        }
        let value = match (take, self.value) {
            (Take::Spelled, Some(value)) if steps.iter().any(|s| s.spells(value)) => value,
            (Take::Spelled, _) => {
                return Err(bad(
                    "a `spelled` take names its `value`, and a step of the shape spells it: memory without a displacement for 0, else an index scale or an immediate equal to it",
                ));
            }
            (_, None) => 0,
            (_, Some(_)) => return Err(bad("only a `spelled` take names a `value`")),
        };
        let check = parse_steps(self.check, names).map_err(|e| bad(&e.to_string()))?;
        if !check.is_empty() && (constant || check.iter().any(|s| s.takes() > 0)) {
            return Err(bad("`check` is a shape without `*` for a symbol"));
        }
        if (self.shift.is_some() || self.plus.is_some())
            && !matches!(take, Take::Value | Take::Cell)
        {
            return Err(bad("`shift` and `plus` change a `value` or `cell` take"));
        }
        let after = parse_steps(self.after, names).map_err(|e| bad(&e.to_string()))?;
        let ordinal = match (self.nth, self.of) {
            (Some(nth), Some(of)) if matches!(how, How::Shape { .. }) && nth > 0 && nth <= of => {
                Some((nth, of))
            }
            _ => None,
        };
        let asked = self.nth.is_some() || self.of.is_some() || !after.is_empty();
        if asked && ordinal.is_none() || after.iter().any(|s| s.takes() > 0) {
            return Err(bad(
                "`nth` counts from 1 the `of` runs of a `system` or `in` shape, after the one match of `after`, a shape without `*`, when it is given",
            ));
        }
        let range = bounds(
            stated_range(self.range, self.unbounded).map_err(bad)?,
            self.align,
            constant && take != Take::Spelled || reflected,
        )
        .map_err(bad)?;
        if self.holds.is_some() && !matches!(how, How::Field(_)) {
            return Err(bad(
                "`type` names what the last step of a `field` route holds",
            ));
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
            value,
            after,
            nth: ordinal,
            range,
            align: self.align.unwrap_or(1),
            holds: self.holds,
        })
    }

    /// The one way the row names, taken out of it.
    fn how(&mut self, names: &ShapeNames) -> Result<How, TableError> {
        let bad = |why: &str| TableError::Toml(format!("{}: {why}", self.id));
        let listed = |list: Option<Vec<String>>| list.filter(|l| !l.is_empty());
        let ways = [
            self.called_by.is_some(),
            self.calls.is_some(),
            self.loads.is_some(),
            self.system.is_some(),
            self.within.is_some(),
            self.field.is_some(),
            self.size.is_some(),
            self.callers.is_some(),
            self.vtable.is_some(),
        ];
        if self.shape.as_ref().is_some_and(Vec::is_empty) {
            return Err(bad("a `shape` is not empty"));
        }
        let shape = self.shape.take();
        let shaped = shape.is_some();
        let slot = self.slot.take();
        if let Some(vtable) = self.vtable.take() {
            return match slot {
                Some(slot) if !shaped && ways.iter().filter(|&&w| w).count() == 1 => {
                    Ok(How::Slot { vtable, slot })
                }
                _ => Err(bad(
                    "a `vtable` route names its `slot` and no `shape` or other way",
                )),
            };
        }
        if slot.is_some() {
            return Err(bad("`slot` goes with `vtable`"));
        }
        let within_given = self.within.is_some();
        let how = match (
            listed(self.called_by.take()),
            listed(self.calls.take()),
            listed(self.loads.take()),
            self.system.take().or(self.within.take()),
            listed(self.field.take()),
            self.size.take(),
            self.callers.take(),
        ) {
            _ if ways.iter().filter(|&&w| w).count() != 1 => None,
            (Some(names), ..) if !shaped => Some(How::CalledBy(names)),
            (_, Some(names), ..) if !shaped => Some(How::Calls(names)),
            (_, _, Some(strings), ..) if !shaped => Some(How::Loads(strings)),
            (_, _, _, Some(within), ..) if shaped => Some(How::Shape {
                within,
                steps: parse_steps(shape, names).map_err(|e| bad(&e.to_string()))?,
            }),
            (_, _, _, Some(system), ..) if !within_given => Some(How::System(system)),
            (.., Some(callee)) if shaped => Some(How::Callers {
                callee,
                steps: parse_steps(shape, names).map_err(|e| bad(&e.to_string()))?,
            }),
            (.., Some(path), None, None) if !shaped => Some(How::Field(
                path.iter()
                    .map(|step| {
                        step.rsplit_once('.')
                            .map(|(ty, field)| (ty.to_owned(), field.to_owned()))
                            .ok_or_else(|| bad("a `field` step is `<type>.<field>`"))
                    })
                    .collect::<Result<_, _>>()?,
            )),
            (.., Some(ty), None) if !shaped => Some(How::Size(ty)),
            _ => None,
        };
        how.ok_or_else(|| {
            bad("a route is one of `called_by`, `calls`, `loads`, `system`, `field` or `size`, `vtable` with `slot`, or `system`, `in` or `callers` with `shape`, each not empty")
        })
    }
}

/// A row's `range`, or every value for `unbounded = true`: TOML integers
/// stop at `i64::MAX`, so a 64-bit key states it in words.
fn stated_range(
    range: Option<[u64; 2]>,
    unbounded: Option<bool>,
) -> Result<Option<[u64; 2]>, &'static str> {
    match (range, unbounded) {
        (Some(_), Some(true)) => Err("a row states `range` or `unbounded`, not both"),
        (None, Some(true)) => Ok(Some([0, u64::MAX])),
        (range, _) => Ok(range),
    }
}

/// A row's `range` as least and greatest, after checking it and `align`
/// bound a constant route (`constant`) and read as bounds.
fn bounds(
    range: Option<[u64; 2]>,
    align: Option<u64>,
    constant: bool,
) -> Result<Option<(u64, u64)>, &'static str> {
    if (range.is_some() || align.is_some()) && !constant {
        return Err(
            "`range` and `align` bound a `value`, `cell` or `scale` take or a `field` or `size` route",
        );
    }
    match range {
        Some([lo, hi]) if lo > hi => Err("a `range` is `[least, greatest]`"),
        _ if align == Some(0) => Err("an `align` is 1 or more"),
        range => Ok(range.map(|[lo, hi]| (lo, hi))),
    }
}

/// The range a constant route's row states for the least and greatest
/// values the kept builds give, as the routes module docs set it out: a
/// name hash the code holds, the same everywhere, exactly; a 64-bit value
/// that changes, unbounded; else half the least to twice the greatest, at
/// least 16.
#[cfg(test)]
fn proposed_range(least: u64, greatest: u64) -> (u64, u64) {
    match (least, greatest) {
        (_, g) if g >= 1 << 32 && least != g => (0, u64::MAX),
        (l, g) if l == g && is_name_hash(l) => (l, g),
        (l, g) => (l / 2, g.saturating_mul(2).max(16)),
    }
}

/// Whether a value too big for an offset or count may be a 32-bit name
/// hash: the kept builds' struct offsets reach `0x4667f0`, their hashes
/// start at `0x4723f62`, so an offset of the big UI objects keeps room to
/// move.
#[cfg(test)]
const fn is_name_hash(value: u64) -> bool {
    value >= 1 << 24 && value < 1 << 32
}

/// The steps of a shape's text, none for none.
fn parse_steps(texts: Option<Vec<String>>, names: &ShapeNames) -> Result<Vec<Step>, ShapeError> {
    texts
        .unwrap_or_default()
        .iter()
        .map(|step| Step::parse(step, names))
        .collect()
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
        let names = ShapeNames::new();
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
