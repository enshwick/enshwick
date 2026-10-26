//! Extension points: rules one mod lets other mods observe, advise or
//! override.
//!
//! The mod that owns a rule declares it once with
//! [`#[point]`](macro@crate::point): a `repr(C)` input, the type of the
//! answer, a name `<owner id>:<point>` and a [`Mode`]. The owner defines it
//! with [`Load::define_point`] and asks it with [`OwnedPoint::decide`],
//! passing its own answer; other mods attach links with
//! [`Load::extend_point`] or [`Load::observe_point`], ordered by
//! [`Order`]. The loader checks every link's declaration against the
//! owner's, field by field, once every mod is initialised, orders the links
//! the same way at every start, turns a panicking link off and lets
//! `mods/enshwick/mods.toml` switch one off. With no live link, asking a
//! point is one atomic load.
//!
//! The points of Enshwick's own mods are declared here, so a mod can extend
//! them with this crate alone: [`safes`], [`survivors_per_altar`],
//! [`flame_altar_count`] and [`area_loot`].

use core::ffi::c_void;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::abi::{LayoutField, PointFlags, PointMode, PointVerdict, Status, Str};
use crate::load::{hand_over, ok};
use crate::{Host, Load, Off};

/// How links act on a point's answer, as the owning mod chooses.
///
/// # Examples
///
/// ```
/// use enshwick::points::Mode;
///
/// assert!(Mode::Override.changes());
/// assert!(!Mode::Observe.changes());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Mode {
    /// Links see the answer and change nothing.
    Observe,
    /// Links may change the answer; the owner may ignore the change.
    Advise,
    /// Links may change the answer, and the owner takes it.
    Override,
}

impl Mode {
    /// Whether links may change the answer.
    ///
    /// # Examples
    ///
    /// See [`Mode`].
    #[must_use]
    pub const fn changes(self) -> bool {
        !matches!(self, Self::Observe)
    }

    /// The mode as the ABI carries it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::PointMode;
    /// use enshwick::points::Mode;
    ///
    /// assert_eq!(Mode::Advise.to_abi(), PointMode::ADVISE);
    /// ```
    #[must_use]
    pub const fn to_abi(self) -> PointMode {
        match self {
            Self::Observe => PointMode::OBSERVE,
            Self::Advise => PointMode::ADVISE,
            Self::Override => PointMode::OVERRIDE,
        }
    }
}

/// The input of an extension point, with its name, mode and answer type.
///
/// # Safety
///
/// Implemented by [`#[point]`](macro@crate::point) only: [`LAYOUT`] must
/// list every field of the `repr(C)` input in order, and [`OUTPUT`] the
/// answer type, each with its size and alignment in the hash.
///
/// [`LAYOUT`]: Point::LAYOUT
/// [`OUTPUT`]: Point::OUTPUT
///
/// # Examples
///
/// ```
/// use enshwick::points::{Mode, Point};
///
/// #[enshwick::point(name = "trading-post:price", mode = Override, output = u32)]
/// #[derive(Clone, Copy, Debug)]
/// pub struct Price {
///     pub item: u32,
/// }
///
/// assert_eq!(Price::NAME, "trading-post:price");
/// assert_eq!(Price::MODE, Mode::Override);
/// assert!(!Price::REFUSE);
/// ```
pub unsafe trait Point: Copy + 'static {
    /// The point's name, `<owner id>:<point>`.
    const NAME: &'static str;
    /// How links act on the answer.
    const MODE: Mode;
    /// Whether a link may refuse, which stops the chain.
    const REFUSE: bool;
    /// The input's fields, in order.
    const LAYOUT: &'static [LayoutField];
    /// The answer's type, as one field.
    const OUTPUT: &'static [LayoutField];
    /// The point's schema as `cargo enshwick point` reads it from the
    /// owner's manifest: its `[[point]]` table with the layout hashes, then
    /// the `[[type]]` tables of its input and answer.
    const SCHEMA: &'static str;
    /// The answer.
    type Output: Copy + 'static;
}

/// What a link answers.
///
/// # Examples
///
/// ```
/// use enshwick::points::Verdict;
///
/// let raise = |limit: &u32| Verdict::Set(limit * 2);
/// assert_eq!(raise(&3), Verdict::Set(6));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Verdict<O> {
    /// The answer stays as the links before left it.
    Pass,
    /// The answer becomes this; ignored on an observed point.
    Set(O),
    /// The chain stops and the answer is a refusal, on a point that allows
    /// it; elsewhere the loader takes it as [`Verdict::Pass`].
    Refuse,
}

/// What the links made of the owner's answer.
///
/// # Examples
///
/// ```
/// use enshwick::points::Decision;
///
/// assert_eq!(Decision::Changed(4).value_or(0), 4);
/// assert_eq!(Decision::<u32>::Refused.value_or(0), 0);
/// assert!(Decision::<u32>::Refused.is_refused());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Decision<O> {
    /// No link changed the answer.
    Kept(O),
    /// Links changed the answer to this.
    Changed(O),
    /// A link refused.
    Refused,
}

impl<O: Copy> Decision<O> {
    /// The answer, or `refused` when a link refused.
    ///
    /// # Examples
    ///
    /// See [`Decision`].
    #[must_use]
    pub const fn value_or(self, refused: O) -> O {
        match self {
            Self::Kept(value) | Self::Changed(value) => value,
            Self::Refused => refused,
        }
    }

    /// Whether a link refused.
    ///
    /// # Examples
    ///
    /// See [`Decision`].
    #[must_use]
    pub const fn is_refused(self) -> bool {
        matches!(self, Self::Refused)
    }
}

/// Where a link stands in its point's chain: links run in turn, each seeing
/// the answer the ones before left.
///
/// `after` a mod sees, and may change, that mod's answer; `before` a mod
/// leaves it the last word. Among links the ids leave free, the lower
/// priority runs first, then the lower mod id. Ids of mods that do not run
/// are ignored.
///
/// # Examples
///
/// ```
/// use enshwick::points::Order;
///
/// let order = Order::new().after("safe-viewers").priority(10);
/// assert_eq!(order.priority_value(), 10);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Order {
    before: Vec<&'static str>,
    after: Vec<&'static str>,
    priority: i32,
}

impl Order {
    /// No constraint, priority 0.
    ///
    /// # Examples
    ///
    /// See [`Order`].
    #[must_use]
    pub const fn new() -> Self {
        Self {
            before: Vec::new(),
            after: Vec::new(),
            priority: 0,
        }
    }

    /// Runs before the link of mod `id`.
    ///
    /// # Examples
    ///
    /// ```
    /// let order = enshwick::points::Order::new().before("safe-viewers");
    /// # let _ = order;
    /// ```
    #[must_use]
    pub fn before(mut self, id: &'static str) -> Self {
        self.before.push(id);
        self
    }

    /// Runs after the link of mod `id`.
    ///
    /// # Examples
    ///
    /// ```
    /// let order = enshwick::points::Order::new().after("safe-viewers");
    /// # let _ = order;
    /// ```
    #[must_use]
    pub fn after(mut self, id: &'static str) -> Self {
        self.after.push(id);
        self
    }

    /// Sets the priority; lower runs earlier.
    ///
    /// # Examples
    ///
    /// See [`Order`].
    #[must_use]
    pub const fn priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// The priority.
    ///
    /// # Examples
    ///
    /// See [`Order`].
    #[must_use]
    pub const fn priority_value(&self) -> i32 {
        self.priority
    }
}

/// A point this mod defined, from [`Load::define_point`]: asks the links
/// from any thread.
///
/// # Examples
///
/// ```
/// #[enshwick::point(name = "trading-post:price", mode = Override, output = u32)]
/// #[derive(Clone, Copy, Debug)]
/// pub struct Price {
///     pub item: u32,
/// }
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let price = load.define_point::<Price>()?;
///     let coins = price.decide(&Price { item: 7 }, 10).value_or(10);
///     enshwick::info!("item 7 costs {coins}");
///     Ok(())
/// }
/// ```
pub struct OwnedPoint<P> {
    host: Host,
    id: u32,
    live: &'static AtomicU32,
    _point: PhantomData<fn(&P)>,
}

impl<P> Clone for OwnedPoint<P> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<P> Copy for OwnedPoint<P> {}

impl<P> core::fmt::Debug for OwnedPoint<P> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("OwnedPoint")
            .field("id", &self.id)
            .field("live", &self.live.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl<P: Point> OwnedPoint<P> {
    /// Runs the live links over `answer`, this mod's own, on this thread.
    /// With no live link it returns [`Decision::Kept`] at once.
    ///
    /// # Examples
    ///
    /// See [`OwnedPoint`].
    #[inline]
    pub fn decide(&self, input: &P, answer: P::Output) -> Decision<P::Output> {
        if self.live.load(Ordering::Relaxed) == 0 {
            return Decision::Kept(answer);
        }
        self.run(input, answer)
    }

    /// Whether any link runs.
    ///
    /// # Examples
    ///
    /// ```
    /// #[enshwick::point(name = "trading-post:price", mode = Override, output = u32)]
    /// #[derive(Clone, Copy, Debug)]
    /// pub struct Price {
    ///     pub item: u32,
    /// }
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let price = load.define_point::<Price>()?;
    ///     assert!(!price.is_extended(), "links run once every mod is initialised");
    ///     Ok(())
    /// }
    /// ```
    #[must_use]
    pub fn is_extended(&self) -> bool {
        self.live.load(Ordering::Relaxed) != 0
    }

    #[inline(never)]
    fn run(&self, input: &P, answer: P::Output) -> Decision<P::Output> {
        let mut value = answer;
        let mut verdict = PointVerdict::PASS;
        let status = (self.host.api().run_point)(
            self.host.handle(),
            self.id,
            core::ptr::from_ref(input).cast(),
            (&raw mut value).cast(),
            &raw mut verdict,
        );
        if !status.is_ok() {
            return Decision::Kept(answer);
        }
        match verdict {
            PointVerdict::SET => Decision::Changed(value),
            PointVerdict::REFUSE => Decision::Refused,
            _ => Decision::Kept(answer),
        }
    }
}

/// Always 0: the count of a point the loader refused to define.
static NONE_LIVE: AtomicU32 = AtomicU32::new(0);

/// The ABI description of the point `P`.
pub(crate) fn spec<P: Point>() -> crate::abi::PointSpec {
    crate::abi::PointSpec {
        header: crate::abi::Header::of::<crate::abi::PointSpec>(),
        name: Str::new(P::NAME),
        mode: P::MODE.to_abi(),
        flags: if P::REFUSE {
            PointFlags::REFUSE
        } else {
            PointFlags::NONE
        },
        input: crate::services::layout(P::LAYOUT, size_of::<P>()),
        output: crate::services::layout(P::OUTPUT, size_of::<P::Output>()),
    }
}

impl Load<'_> {
    /// Defines the extension point `P` for other mods to extend, and returns
    /// the handle that asks it.
    ///
    /// `P` comes from [`#[point]`](macro@crate::point), named after this
    /// mod: `<id>:<point>`. Links run once every mod is initialised.
    ///
    /// # Errors
    ///
    /// [`Off`] when the name is outside this mod's id or defined twice, or
    /// when called after `init`.
    ///
    /// # Examples
    ///
    /// See [`OwnedPoint`].
    pub fn define_point<P: Point>(&mut self) -> Result<OwnedPoint<P>, Off> {
        let spec = spec::<P>();
        let mut handle = crate::abi::PointHandle::EMPTY;
        let status = (self.host().api().define_point)(
            self.host().handle(),
            &raw const spec,
            &raw mut handle,
        );
        // SAFETY: the loader's count lives for the process.
        let live = unsafe { handle.live.as_ref() }.unwrap_or(&NONE_LIVE);
        ok(
            status,
            OwnedPoint {
                host: self.host(),
                id: handle.id,
                live,
                _point: PhantomData,
            },
            || {
                if status == Status::REFUSED {
                    format!(
                        "point {}: named after another mod or defined twice",
                        P::NAME
                    )
                } else {
                    format!("point {}", P::NAME)
                }
            },
        )
    }

    /// Attaches `link` to another mod's extension point `P`, at `order` in
    /// its chain. A panic in `link` turns this link off, not the mod.
    ///
    /// The loader matches the link once every mod is initialised: with no
    /// running mod defining `P`, the link stays inert, which is no error;
    /// with a definition whose input lacks or changed a field of `P`, or
    /// whose answer or mode differs, it logs why and the link stays inert.
    /// Declare the owner in the manifest with `recommends`, or `requires`
    /// when this mod is pointless without it.
    ///
    /// # Errors
    ///
    /// [`Off`] when called after `init`, or when this mod links `P` already.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::points::{Order, Verdict};
    ///
    /// #[enshwick::point(name = "trading-post:price", mode = Override, output = u32)]
    /// #[derive(Clone, Copy, Debug)]
    /// pub struct Price {
    ///     pub item: u32,
    /// }
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.extend_point(&Order::new(), |price: &Price, coins: &u32| {
    ///         if price.item == 7 { Verdict::Set(coins / 2) } else { Verdict::Pass }
    ///     })
    /// }
    /// ```
    pub fn extend_point<P: Point>(
        &mut self,
        order: &Order,
        link: impl Fn(&P, &P::Output) -> Verdict<P::Output> + Send + Sync + 'static,
    ) -> Result<(), Off> {
        attach(self.host(), order, link)
    }

    /// Attaches `link` to another mod's extension point `P` to see each
    /// answer as the links before left it, changing nothing; otherwise as
    /// [`Load::extend_point`].
    ///
    /// # Errors
    ///
    /// [`Off`] when called after `init`, or when this mod links `P` already.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::points::Order;
    ///
    /// #[enshwick::point(name = "trading-post:price", mode = Observe, output = u32)]
    /// #[derive(Clone, Copy, Debug)]
    /// pub struct Price {
    ///     pub item: u32,
    /// }
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.observe_point(&Order::new(), |price: &Price, coins: &u32| {
    ///         enshwick::debug!("item {} costs {coins}", price.item);
    ///     })
    /// }
    /// ```
    pub fn observe_point<P: Point>(
        &mut self,
        order: &Order,
        link: impl Fn(&P, &P::Output) + Send + Sync + 'static,
    ) -> Result<(), Off> {
        attach(self.host(), order, move |input: &P, value: &P::Output| {
            link(input, value);
            Verdict::Pass
        })
    }
}

/// Hands `link` to the loader for the point `P`.
fn attach<P: Point, F: Fn(&P, &P::Output) -> Verdict<P::Output> + Send + Sync + 'static>(
    host: Host,
    order: &Order,
    link: F,
) -> Result<(), Off> {
    let spec = spec::<P>();
    let before: Vec<Str> = order.before.iter().map(|id| Str::new(id)).collect();
    let after: Vec<Str> = order.after.iter().map(|id| Str::new(id)).collect();
    let count = |ids: &[Str]| u32::try_from(ids.len()).unwrap_or(0);
    let raw = crate::abi::PointLink {
        header: crate::abi::Header::of::<crate::abi::PointLink>(),
        point: &raw const spec,
        before: before.as_ptr(),
        after: after.as_ptr(),
        before_count: count(&before),
        after_count: count(&after),
        priority: order.priority,
    };
    hand_over(
        link,
        |user| (host.api().attach_point)(host.handle(), &raw const raw, run_link::<P, F>, user),
        &format!("point {}", P::NAME),
    )
}

/// Runs a link; `user` is the link boxed by [`attach`], alive for the
/// process. A panic answers [`PointVerdict::PANICKED`] and leaves the mod on.
pub(crate) unsafe extern "system" fn run_link<
    P: Point,
    F: Fn(&P, &P::Output) -> Verdict<P::Output>,
>(
    input: *const c_void,
    value: *mut c_void,
    user: *mut c_void,
) -> PointVerdict {
    // SAFETY: `user` is the leaked `Box<F>` this function was attached with.
    let link = unsafe { &*user.cast::<F>() };
    // SAFETY: the loader checked the owner's input starts with every field
    // of `P`, and the input lives for the call.
    let Some(input) = (unsafe { input.cast::<P>().as_ref() }) else {
        return PointVerdict::PASS;
    };
    // SAFETY: the loader checked the owner's answer is `P::Output`, and the
    // value lives for the call, written by nothing else meanwhile.
    let Some(value) = (unsafe { value.cast::<P::Output>().as_mut() }) else {
        return PointVerdict::PASS;
    };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| link(input, value))) {
        Ok(Verdict::Set(new)) if P::MODE.changes() => {
            *value = new;
            PointVerdict::SET
        }
        Ok(Verdict::Refuse) if P::REFUSE && P::MODE.changes() => PointVerdict::REFUSE,
        Ok(_) => PointVerdict::PASS,
        Err(_) => PointVerdict::PANICKED,
    }
}

/// Orders the links of one point: `before` and `after` name other links'
/// mods, then the lower priority and the lower id run first. A constraint
/// that would close a cycle is dropped; its link is named in the second
/// list.
///
/// The loader and the fake call it, so both order alike; mods have no use
/// for it.
///
/// # Examples
///
/// ```
/// use enshwick::points::{LinkPlace, order};
///
/// let links = [
///     LinkPlace { id: "b", priority: 0, before: &[], after: &[] },
///     LinkPlace { id: "a", priority: 0, before: &[], after: &["b"] },
///     LinkPlace { id: "c", priority: -1, before: &[], after: &[] },
/// ];
/// let (running, dropped) = order(&links);
/// assert_eq!(running, [2, 0, 1]);
/// assert!(dropped.is_empty());
/// ```
#[doc(hidden)]
#[must_use]
pub fn order(links: &[LinkPlace<'_>]) -> (Vec<usize>, Vec<usize>) {
    let n = links.len();
    let index_of = |id: &str| links.iter().position(|l| l.id == id);
    // edges[a] holds every b that must run after a.
    let mut edges: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, link) in links.iter().enumerate() {
        for &later in link.before {
            if let Some(j) = index_of(later).filter(|&j| j != i) {
                edges[i].push(j);
            }
        }
        for &earlier in link.after {
            if let Some(j) = index_of(earlier).filter(|&j| j != i) {
                edges[j].push(i);
            }
        }
    }
    let key = |i: usize| (links[i].priority, links[i].id, i);
    let mut dropped = Vec::new();
    let mut placed = vec![false; n];
    let mut running = Vec::with_capacity(n);
    while running.len() < n {
        let mut waiting = vec![0usize; n];
        for (a, after) in edges.iter().enumerate() {
            if placed[a] {
                continue;
            }
            for &b in after {
                waiting[b] += 1;
            }
        }
        let free = (0..n)
            .filter(|&i| !placed[i] && waiting[i] == 0)
            .min_by_key(|&i| key(i));
        if let Some(next) = free {
            placed[next] = true;
            running.push(next);
            continue;
        }
        // A cycle: among the links of a cycle that nothing outside it holds
        // back, the first by key runs next, and only the constraints of its
        // own cycle are dropped. A link merely waiting behind a cycle keeps
        // its constraints.
        let reach = reachable(&edges, &placed);
        let Some(next) = (0..n)
            .filter(|&i| {
                !placed[i]
                    && reach[i][i]
                    && (0..n).all(|q| placed[q] || !reach[q][i] || reach[i][q])
            })
            .min_by_key(|&i| key(i))
        else {
            break;
        };
        for (a, after) in edges.iter_mut().enumerate() {
            if !placed[a] {
                after.retain(|&b| b != next);
            }
        }
        dropped.push(next);
        placed[next] = true;
        running.push(next);
    }
    (running, dropped)
}

/// `reach[a][b]`: whether `b` must run after `a`, through one constraint or
/// more between links not placed yet.
fn reachable(edges: &[Vec<usize>], placed: &[bool]) -> Vec<Vec<bool>> {
    let n = edges.len();
    let mut reach = vec![vec![false; n]; n];
    for (start, row) in reach.iter_mut().enumerate() {
        if placed[start] {
            continue;
        }
        let mut stack = vec![start];
        while let Some(a) = stack.pop() {
            for &b in &edges[a] {
                if !placed[b] && !row[b] {
                    row[b] = true;
                    stack.push(b);
                }
            }
        }
    }
    reach
}

/// A link's place, as [`order`] reads it.
///
/// # Examples
///
/// See [`order`].
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkPlace<'a> {
    /// The linking mod's id.
    pub id: &'a str,
    /// Lower runs earlier among free links.
    pub priority: i32,
    /// Ids of mods whose links run after this one.
    pub before: &'a [&'a str],
    /// Ids of mods whose links run before this one.
    pub after: &'a [&'a str],
}

pub mod area_loot;
pub mod flame_altar_count;
pub mod safes;
pub mod survivors_per_altar;

#[cfg(test)]
mod tests {
    use super::{LinkPlace, order};

    fn place<'a>(id: &'a str, priority: i32, after: &'a [&'a str]) -> LinkPlace<'a> {
        LinkPlace {
            id,
            priority,
            before: &[],
            after,
        }
    }

    #[test]
    fn a_cycle_drops_only_its_own_constraints() {
        // `c` and `d` wait for each other; `x` waits for `c` and has the
        // lowest priority, but its own constraint closes no cycle.
        let links = [
            place("c", 0, &["d"]),
            place("d", 0, &["c"]),
            place("x", -5, &["c"]),
        ];
        let (running, dropped) = order(&links);
        assert_eq!(running, [0, 2, 1]);
        assert_eq!(dropped, [0]);
    }

    #[test]
    fn a_cycle_behind_another_waits_for_it() {
        // `a` and `b` close one cycle, `c` and `d` another that also waits
        // for `b`; the first cycle breaks first, whatever the priorities.
        let links = [
            place("a", 5, &["b"]),
            place("b", 5, &["a"]),
            place("c", -5, &["d", "b"]),
            place("d", -5, &["c"]),
        ];
        let (running, dropped) = order(&links);
        assert_eq!(running, [0, 1, 2, 3]);
        assert_eq!(dropped, [0, 2]);
    }
}
