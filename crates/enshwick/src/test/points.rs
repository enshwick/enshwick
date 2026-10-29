//! The fake loader's extension points: the points the mod defines, with the
//! links a test attaches as other mods would, and the links the mod
//! attaches to other mods' points, which a test runs as their owner would.

use core::ffi::c_void;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::abi::{
    ModHandle, PointFn, PointHandle, PointLink, PointSpec, PointVerdict, Status, Str,
};
use crate::points::{Decision, LinkPlace, Point, Verdict, order};

use super::Installed;
use super::table::{Registration, put, view, with};

/// A point as declared: name, input and answer layouts, whether links
/// change the answer and whether they may refuse.
#[derive(Clone)]
struct Declared {
    name: String,
    input: Vec<(String, u64)>,
    output: Vec<(String, u64)>,
    mode: crate::abi::PointMode,
    refuse: bool,
    input_size: u32,
    output_size: u32,
}

impl Declared {
    fn of<P: Point>() -> Self {
        let fields = |layout: &[crate::abi::LayoutField]| {
            layout
                .iter()
                // SAFETY: the declaration's names are static strings.
                .filter_map(|f| unsafe { f.name.as_str() }.map(|n| (n.to_owned(), f.hash)))
                .collect()
        };
        Self {
            name: P::NAME.to_owned(),
            input: fields(P::LAYOUT),
            output: fields(P::OUTPUT),
            mode: P::MODE.to_abi(),
            refuse: P::REFUSE,
            input_size: u32::try_from(size_of::<P>()).unwrap_or(u32::MAX),
            output_size: u32::try_from(size_of::<P::Output>()).unwrap_or(u32::MAX),
        }
    }

    /// Whether a link declared as `link` serves this point.
    fn serves(&self, link: &Self) -> bool {
        self.name == link.name
            && crate::services::compare(&view(&link.input), &view(&self.input)).is_ok()
            && self.output == link.output
            && (self.mode, self.refuse) == (link.mode, link.refuse)
            && link.input_size <= self.input_size
            && link.output_size == self.output_size
    }
}

/// A link: its handler and pointer, and whether it is still on.
#[derive(Clone, Copy)]
struct Link {
    callback: PointFn,
    user: usize,
    on: bool,
}

/// A point the mod defined, with the links tests attached.
struct Defined {
    declared: Declared,
    live: &'static AtomicU32,
    links: Vec<Link>,
}

/// A link the mod attached to another mod's point; every one has the
/// mod's id, so `id` is empty.
struct Attached {
    declared: Declared,
    id: String,
    priority: i32,
    before: Vec<String>,
    after: Vec<String>,
    link: Link,
}

/// The fake's points.
#[derive(Default)]
pub(crate) struct FakePoints {
    defined: Vec<Defined>,
    attached: Vec<Attached>,
}

/// The declaration a spec carries; `None` for a bad one.
fn declared(spec: *const PointSpec) -> Option<Declared> {
    // SAFETY: the safe layer passes a valid spec.
    let spec = unsafe { spec.as_ref() }?;
    // SAFETY: the safe layer passes the spec's name borrowed and unchanged
    // for the call.
    let name = unsafe { spec.name.as_str() }?.to_owned();
    // SAFETY: the safe layer passes the input layout of a declared type, its
    // fields and their names borrowed and unchanged for the call.
    let input = unsafe { crate::services::read_layout(&spec.input) }?;
    // SAFETY: the safe layer passes the output layout of a declared type, its
    // fields and their names borrowed and unchanged for the call.
    let output = unsafe { crate::services::read_layout(&spec.output) }?;
    Some(Declared {
        name,
        input,
        output,
        mode: spec.mode,
        refuse: spec.flags.contains(crate::abi::PointFlags::REFUSE),
        input_size: spec.input.size,
        output_size: spec.output.size,
    })
}

/// The ids a link names, as owned strings.
fn ids(raw: *const Str, count: u32) -> Vec<String> {
    if raw.is_null() || count == 0 {
        return Vec::new();
    }
    // SAFETY: the safe layer passes `count` strings, borrowed for the call.
    let ids = unsafe { core::slice::from_raw_parts(raw, count as usize) };
    ids.iter()
        // SAFETY: each id is one of the `count` strings the safe layer passes
        // borrowed and unchanged for the call.
        .filter_map(|id| unsafe { id.as_str() }.map(str::to_owned))
        .collect()
}

pub(crate) extern "system" fn define_point(
    _me: ModHandle,
    spec: *const PointSpec,
    out: *mut PointHandle,
) -> Status {
    if with(false, |s| s.registration == Registration::Closed) {
        return Status::TOO_LATE;
    }
    let Some(declared) = declared(spec) else {
        return Status::BAD_ARGS;
    };
    let defined = with(Err(Status::MISSING), |s| {
        let named_after_the_mod = s
            .manifest
            .as_ref()
            .is_none_or(|manifest| crate::rules::is_named_after(&declared.name, &manifest.names()));
        let points = &mut s.points;
        let defined_twice = points
            .defined
            .iter()
            .any(|d| d.declared.name == declared.name);
        if !named_after_the_mod || defined_twice {
            return Err(Status::REFUSED);
        }
        let live: &'static AtomicU32 = Box::leak(Box::new(AtomicU32::new(0)));
        points.defined.push(Defined {
            declared,
            live,
            links: Vec::new(),
        });
        let id = u32::try_from(points.defined.len() - 1).map_err(|_| Status::REFUSED)?;
        Ok((id, live))
    });
    match defined {
        Ok((id, live)) => put(
            out,
            PointHandle {
                header: crate::abi::Header::of::<PointHandle>(),
                id,
                live,
            },
        ),
        Err(status) => status,
    }
}

pub(crate) extern "system" fn attach_point(
    _me: ModHandle,
    link: *const PointLink,
    callback: PointFn,
    user: *mut c_void,
) -> Status {
    if with(false, |s| s.registration == Registration::Closed) {
        return Status::TOO_LATE;
    }
    // SAFETY: the safe layer passes a valid link.
    let Some(link) = (unsafe { link.as_ref() }) else {
        return Status::BAD_ARGS;
    };
    let Some(declared) = declared(link.point) else {
        return Status::BAD_ARGS;
    };
    let attached = Attached {
        declared,
        id: String::new(),
        priority: link.priority,
        before: ids(link.before, link.before_count),
        after: ids(link.after, link.after_count),
        link: Link {
            callback,
            user: user.expose_provenance(),
            on: true,
        },
    };
    with(Status::MISSING, |s| {
        let attached_already = s
            .points
            .attached
            .iter()
            .any(|a| a.declared.name == attached.declared.name);
        if attached_already {
            return Status::REFUSED;
        }
        s.points.attached.push(attached);
        Status::OK
    })
}

/// Runs `links` in turn over `value`; the positions of links that panicked
/// come back with the verdict.
fn run(
    links: &[Link],
    changes: bool,
    refuse: bool,
    input: *const c_void,
    value: *mut c_void,
) -> (PointVerdict, Vec<usize>) {
    let mut verdict = PointVerdict::PASS;
    let mut panicked = Vec::new();
    for (i, link) in links.iter().enumerate().filter(|(_, l)| l.on) {
        // SAFETY: the layouts match, input and value live for the call, and
        // `user` is the pointer the link was attached with.
        let answer = unsafe {
            (link.callback)(
                input,
                value,
                core::ptr::with_exposed_provenance_mut(link.user),
            )
        };
        match answer {
            PointVerdict::SET if changes => verdict = PointVerdict::SET,
            PointVerdict::REFUSE if changes && refuse => return (PointVerdict::REFUSE, panicked),
            PointVerdict::PANICKED => panicked.push(i),
            _ => {}
        }
    }
    (verdict, panicked)
}

pub(crate) extern "system" fn run_point(
    _me: ModHandle,
    point: u32,
    input: *const c_void,
    value: *mut c_void,
    verdict: *mut PointVerdict,
) -> Status {
    let Some((links, changes, refuse)) = with(None, |s| {
        s.points.defined.get(point as usize).map(|d| {
            (
                d.links.clone(),
                d.declared.mode != crate::abi::PointMode::OBSERVE,
                d.declared.refuse,
            )
        })
    }) else {
        return Status::BAD_ARGS;
    };
    let (answer, panicked) = run(&links, changes, refuse, input, value);
    if !panicked.is_empty() {
        with((), |s| {
            if let Some(defined) = s.points.defined.get_mut(point as usize) {
                for &i in &panicked {
                    if let Some(link) = defined.links.get_mut(i) {
                        link.on = false;
                        defined.live.fetch_sub(1, Ordering::Relaxed);
                    }
                }
            }
        });
    }
    put(verdict, answer)
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// Attaches `link` to the mod's point `P`, as another mod would; it runs
    /// after the links attached before it. Returns false when the mod
    /// defines no point that serves `P`. A panicking link turns off, as
    /// under the loader.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::points::Verdict;
    ///
    /// #[enshwick::point(name = "you.my-mod:limit", mode = Override, output = u32)]
    /// #[derive(Clone, Copy, Debug)]
    /// pub struct Limit {
    ///     pub level: u32,
    /// }
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(!loader.extend_point(|_: &Limit, _: &u32| Verdict::Set(3)));
    /// ```
    #[must_use = "false means the mod defines no such point"]
    pub fn extend_point<P: Point, F>(&self, link: F) -> bool
    where
        F: Fn(&P, &P::Output) -> Verdict<P::Output> + Send + Sync + 'static,
    {
        let wanted = Declared::of::<P>();
        let user = Box::into_raw(Box::new(link));
        let attached = with(false, |s| {
            let Some(defined) = s
                .points
                .defined
                .iter_mut()
                .find(|d| d.declared.serves(&wanted))
            else {
                return false;
            };
            defined.links.push(Link {
                callback: crate::points::run_link::<P, F>,
                user: user.expose_provenance(),
                on: true,
            });
            defined.live.fetch_add(1, Ordering::Relaxed);
            true
        });
        if !attached {
            // SAFETY: `user` came from `Box::into_raw` just before, and the
            // closure returned `false` before pushing a link, so no link holds
            // it and this is its only release.
            drop(unsafe { Box::from_raw(user) });
        }
        attached
    }

    /// The points the mod defined, by name, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.defined_points().is_empty());
    /// ```
    #[must_use]
    pub fn defined_points(&self) -> Vec<String> {
        with(Vec::new(), |s| {
            s.points
                .defined
                .iter()
                .map(|d| d.declared.name.clone())
                .collect()
        })
    }

    /// The points the mod attached links to, by name, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.attached_points().is_empty());
    /// ```
    #[must_use]
    pub fn attached_points(&self) -> Vec<String> {
        with(Vec::new(), |s| {
            s.points
                .attached
                .iter()
                .map(|a| a.declared.name.clone())
                .collect()
        })
    }

    /// Asks the point `P` as its owner would, with `answer` as the owner's
    /// own, running the mod's links that serve `P` in the loader's order;
    /// [`Decision::Kept`] when none does.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::points::Decision;
    ///
    /// #[enshwick::point(name = "you.my-mod:limit", mode = Override, output = u32)]
    /// #[derive(Clone, Copy, Debug)]
    /// pub struct Limit {
    ///     pub level: u32,
    /// }
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.ask_point(&Limit { level: 1 }, 4), Decision::Kept(4));
    /// ```
    #[must_use]
    pub fn ask_point<P: Point>(&self, input: &P, answer: P::Output) -> Decision<P::Output> {
        let owner = Declared::of::<P>();
        let (links, positions) = with((Vec::new(), Vec::new()), |s| {
            let serving: Vec<(usize, &Attached)> = s
                .points
                .attached
                .iter()
                .enumerate()
                .filter(|(_, a)| owner.serves(&a.declared))
                .collect();
            let before: Vec<Vec<&str>> = serving
                .iter()
                .map(|(_, a)| a.before.iter().map(String::as_str).collect())
                .collect();
            let after: Vec<Vec<&str>> = serving
                .iter()
                .map(|(_, a)| a.after.iter().map(String::as_str).collect())
                .collect();
            let places: Vec<LinkPlace<'_>> = serving
                .iter()
                .zip(before.iter().zip(&after))
                .map(|((_, a), (before, after))| LinkPlace {
                    id: &a.id,
                    priority: a.priority,
                    before,
                    after,
                })
                .collect();
            let (running, _) = order(&places);
            let links: Vec<Link> = running.iter().map(|&i| serving[i].1.link).collect();
            let positions: Vec<usize> = running.iter().map(|&i| serving[i].0).collect();
            (links, positions)
        });
        let mut value = answer;
        let (verdict, panicked) = run(
            &links,
            owner.mode != crate::abi::PointMode::OBSERVE,
            owner.refuse,
            core::ptr::from_ref(input).cast(),
            (&raw mut value).cast(),
        );
        with((), |s| {
            for i in panicked {
                if let Some(attached) = positions.get(i).and_then(|&p| s.points.attached.get_mut(p))
                {
                    attached.link.on = false;
                }
            }
        });
        match verdict {
            PointVerdict::SET => Decision::Changed(value),
            PointVerdict::REFUSE => Decision::Refused,
            _ => Decision::Kept(answer),
        }
    }
}
