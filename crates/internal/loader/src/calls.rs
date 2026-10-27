//! Described services, calls. During init the loader keeps the calls mods
//! serve and the ones they ask for; once every mod is initialised it matches
//! each asked call with the served one by name and major, field by field and
//! in size. A call then runs the server's handler on the caller's thread.

use std::sync::{Mutex, OnceLock};

use enshwick::abi::{CallFn, Level, Status};

use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::points::Mods;
use crate::services::{Fields, Names};

/// A call as one side declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Declared {
    /// `<server id>:<call>`.
    pub(crate) name: String,
    /// The incompatible version.
    pub(crate) major: u32,
    /// The input's fields.
    pub(crate) input: Fields,
    /// The answer, one field.
    pub(crate) output: Fields,
    /// The input's size in bytes.
    pub(crate) input_size: u32,
    /// The answer's size in bytes.
    pub(crate) output_size: u32,
}

/// A served call.
#[derive(Debug)]
struct Served {
    owner: ModIndex,
    declared: Declared,
    callback: CallFn,
    user: usize,
}

/// A call a mod asked for.
#[derive(Debug)]
struct Wanted {
    owner: ModIndex,
    declared: Declared,
}

/// What mods served and asked for during init.
#[derive(Debug, Default)]
pub(crate) struct Registry {
    served: Vec<Served>,
    wanted: Vec<Wanted>,
}

/// The server an asked call reaches.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Server {
    /// The serving mod's index.
    pub(crate) owner: ModIndex,
    callback: CallFn,
    user: usize,
}

/// An asked call once sealed: who asked, and the server it reaches, or why
/// none.
#[derive(Debug)]
pub(crate) struct Route {
    /// The asking mod's index.
    pub(crate) caller: ModIndex,
    /// The call's name and major.
    pub(crate) name: String,
    /// The major it asked for.
    pub(crate) major: u32,
    /// The matched server, or why it stays unmatched.
    pub(crate) server: Result<Server, String>,
}

/// A served call, for the status summary.
#[derive(Debug)]
pub(crate) struct Offer {
    /// The serving mod's index.
    pub(crate) owner: ModIndex,
    /// The call's name.
    pub(crate) name: String,
    /// Its major.
    pub(crate) major: u32,
}

impl Registry {
    /// Serves a call named after its server, whose names are its id and then
    /// its former ids.
    ///
    /// # Errors
    ///
    /// Why it is refused: a name outside the server's names, or a name and
    /// major served twice.
    pub(crate) fn serve(
        &mut self,
        owner: ModIndex,
        names: &[&str],
        declared: Declared,
        callback: CallFn,
        user: usize,
    ) -> Result<(), String> {
        let name = &declared.name;
        if !crate::points::is_named_after(name, names) {
            let id = names.first().copied().unwrap_or_default();
            return Err(format!("call {name}: a call is named `{id}:<call>`"));
        }
        if self
            .served
            .iter()
            .any(|s| s.declared.name == *name && s.declared.major == declared.major)
        {
            return Err(format!("call {name} v{}: served twice", declared.major));
        }
        self.served.push(Served {
            owner,
            declared,
            callback,
            user,
        });
        Ok(())
    }

    /// Keeps an asked call for the seal and returns its id; the same call
    /// asked twice by one mod keeps its first id.
    ///
    /// # Errors
    ///
    /// Why it is refused: the mod asked for the call already with another
    /// declaration, or too many calls are asked for.
    pub(crate) fn find(&mut self, owner: ModIndex, declared: Declared) -> Result<u32, String> {
        let known = self.wanted.iter().position(|w| {
            w.owner == owner
                && w.declared.name == declared.name
                && w.declared.major == declared.major
        });
        let index = match known {
            Some(index)
                if self
                    .wanted
                    .get(index)
                    .is_some_and(|w| w.declared == declared) =>
            {
                index
            }
            Some(_) => {
                return Err(format!(
                    "call {} v{}: asked for twice, declared differently",
                    declared.name, declared.major
                ));
            }
            None => {
                self.wanted.push(Wanted { owner, declared });
                self.wanted.len() - 1
            }
        };
        u32::try_from(index).map_err(|_| "too many calls".to_owned())
    }

    /// Matches every asked call with its server: the routes by id, and the
    /// served calls.
    pub(crate) fn seal(&mut self, mods: &impl Mods) -> (Box<[Route]>, Vec<Offer>) {
        let routes = self
            .wanted
            .iter()
            .map(|wanted| Route {
                caller: wanted.owner,
                name: wanted.declared.name.clone(),
                major: wanted.declared.major,
                server: self.route(&wanted.declared, mods),
            })
            .collect();
        let offers = self
            .served
            .iter()
            .map(|s| Offer {
                owner: s.owner,
                name: s.declared.name.clone(),
                major: s.declared.major,
            })
            .collect();
        (routes, offers)
    }

    /// The server of `want`, or why none.
    fn route(&self, want: &Declared, mods: &impl Mods) -> Result<Server, String> {
        let named: Vec<&Served> = self
            .served
            .iter()
            .filter(|s| s.declared.name == want.name)
            .collect();
        let Some(served) = named.iter().find(|s| s.declared.major == want.major) else {
            return Err(unserved(want, &named, mods));
        };
        let (id, version) = mods.name(served.owner.get());
        if mods.off(served.owner.get()) {
            return Err(format!("{id} {version} serves it, but is off"));
        }
        let have = &served.declared;
        if want.input != have.input || want.input_size != have.input_size {
            return Err(format!(
                "the input differs here and at {id} {version}, in a field or in size; both mods must compile one declaration"
            ));
        }
        if want.output != have.output || want.output_size != have.output_size {
            return Err(format!(
                "the answer differs here and at {id} {version}, in its type or size; both mods must compile one declaration"
            ));
        }
        Ok(Server {
            owner: served.owner,
            callback: served.callback,
            user: served.user,
        })
    }
}

/// Why no server of `want`'s major runs; `named` are the calls of its name.
fn unserved(want: &Declared, named: &[&Served], mods: &impl Mods) -> String {
    if let Some(other) = named.first() {
        let (id, version) = mods.name(other.owner.get());
        let advice = if other.declared.major < want.major {
            format!("; update {id}")
        } else {
            "; update this mod".to_owned()
        };
        return format!(
            "{id} {version} serves v{}, this mod asks for v{}{advice}",
            other.declared.major, want.major
        );
    }
    let server = want
        .name
        .split_once(':')
        .map_or(want.name.as_str(), |(id, _)| id);
    match mods.find(server) {
        None => format!("{server} does not run"),
        Some(index) if mods.off(index) => {
            let (id, version) = mods.name(index);
            format!("{id} {version} is off")
        }
        Some(index) => {
            let (id, version) = mods.name(index);
            format!("{id} {version} serves no such call; update {id}")
        }
    }
}

/// The registry while mods initialise.
static REGISTRY: Mutex<Registry> = Mutex::new(Registry {
    served: Vec::new(),
    wanted: Vec::new(),
});
/// The routes once every mod is initialised, by id, read without a lock.
static SEALED: OnceLock<Box<[Route]>> = OnceLock::new();
/// The served calls once every mod is initialised.
static OFFERS: OnceLock<Vec<Offer>> = OnceLock::new();

/// The registry, for the entry points.
pub(crate) fn registry() -> std::sync::MutexGuard<'static, Registry> {
    REGISTRY.relock()
}

/// Whether the calls are sealed: serving and asking are over.
pub(crate) fn sealed() -> bool {
    SEALED.get().is_some()
}

/// Seals the calls and logs every asked call left unmatched.
pub(crate) fn seal(mods: &impl Mods) {
    let (routes, offers) = registry().seal(mods);
    for route in &*routes {
        if let Err(reason) = &route.server {
            let (id, _) = mods.name(route.caller.get());
            crate::log::write(
                Level::WARN,
                id,
                &format!(
                    "call {} v{}: unmatched, its calls answer nothing: {reason}",
                    route.name, route.major
                ),
            );
        }
    }
    if !offers.is_empty() {
        crate::log!(
            INFO,
            "{} calls served, {} asked for",
            offers.len(),
            routes.len()
        );
    }
    let _first = SEALED.set(routes);
    let _first = OFFERS.set(offers);
}

/// Runs the call `id`, which the mod at `caller` asked for.
pub(crate) fn run(
    caller: usize,
    id: u32,
    input: *const core::ffi::c_void,
    output: *mut core::ffi::c_void,
) -> Status {
    let Some(routes) = SEALED.get() else {
        return Status::MISSING;
    };
    let Some(route) = routes.get(id as usize) else {
        return Status::BAD_ARGS;
    };
    if route.caller != ModIndex::new(caller) {
        return Status::BAD_ARGS;
    }
    if input.is_null() || output.is_null() {
        return Status::BAD_ARGS;
    }
    let Ok(server) = &route.server else {
        return Status::MISSING;
    };
    if crate::host::is_off(server.owner) {
        return Status::REFUSED;
    }
    // SAFETY: the handler lives in the server's DLL, which stays loaded; the
    // seal matched the caller's input and answer to the server's field by
    // field and in size, the caller passes both as large as its declaration
    // says, and the safe layer catches the handler's panics.
    crate::profile::call(server.owner, crate::profile::Feature::Calls, || unsafe {
        (server.callback)(
            input,
            output,
            core::ptr::with_exposed_provenance_mut(server.user),
        )
    })
}

/// A line per served call and per unmatched asked call, for the status
/// summary.
pub(crate) fn lines(mods: &impl Names) -> Vec<String> {
    let routes = SEALED.get().map_or(&[][..], |r| &**r);
    let offers = OFFERS.get().map_or(&[][..], Vec::as_slice);
    lines_of(routes, offers, mods)
}

/// [`lines`] of `routes` and `offers`.
fn lines_of(routes: &[Route], offers: &[Offer], mods: &impl Names) -> Vec<String> {
    let mut lines = Vec::new();
    for offer in offers {
        let (id, version) = mods.name(offer.owner.get());
        let callers: Vec<&str> = routes
            .iter()
            .filter(|r| {
                r.server.as_ref().is_ok_and(|s| s.owner == offer.owner)
                    && r.name == offer.name
                    && r.major == offer.major
            })
            .map(|r| mods.name(r.caller.get()).0)
            .collect();
        lines.push(format!(
            "call {} v{} from {id} {version}: {}",
            offer.name,
            offer.major,
            if callers.is_empty() {
                "no callers".to_owned()
            } else {
                format!("called by {}", callers.join(", "))
            }
        ));
    }
    for route in routes {
        if let Err(reason) = &route.server {
            let (id, version) = mods.name(route.caller.get());
            lines.push(format!(
                "call {} v{} of {id} {version}: unmatched, {reason}",
                route.name, route.major
            ));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;

    use super::{Declared, Registry, lines_of};
    use crate::mod_index::ModIndex;
    use crate::points::Mods;
    use crate::services::Names;

    struct Running(Vec<(&'static str, &'static str, bool)>);

    impl Names for Running {
        fn name(&self, index: usize) -> (&str, &str) {
            self.0.get(index).map_or(("?", "?"), |m| (m.0, m.1))
        }

        fn off(&self, index: usize) -> bool {
            self.0.get(index).is_none_or(|m| m.2)
        }
    }

    impl Mods for Running {
        fn find(&self, id: &str) -> Option<usize> {
            self.0.iter().position(|m| m.0 == id)
        }
    }

    fn running() -> Running {
        Running(vec![
            ("you.trading-post", "1.0.0", false),
            ("you.haggling", "0.2.0", false),
            ("you.off", "0.1.0", true),
        ])
    }

    fn declared(name: &str, major: u32, input: &[(&str, u64)], size: u32) -> Declared {
        Declared {
            name: name.to_owned(),
            major,
            input: input.iter().map(|(n, h)| ((*n).to_owned(), *h)).collect(),
            output: vec![("output".to_owned(), 9)],
            input_size: size,
            output_size: 4,
        }
    }

    unsafe extern "system" fn answer(
        _input: *const core::ffi::c_void,
        _output: *mut core::ffi::c_void,
        _user: *mut core::ffi::c_void,
    ) -> Status {
        Status::OK
    }

    fn price(major: u32) -> Declared {
        declared("you.trading-post:price-of", major, &[("item", 1)], 4)
    }

    #[test]
    fn a_call_is_served_once_and_named_after_its_server() {
        let mut registry = Registry::default();
        let me = ModIndex::new(0);
        let names = ["you.trading-post"];
        assert!(registry.serve(me, &names, price(1), answer, 0).is_ok());
        let twice = registry.serve(me, &names, price(1), answer, 0).unwrap_err();
        assert!(twice.contains("served twice"), "{twice}");
        assert!(registry.serve(me, &names, price(2), answer, 0).is_ok());
        let other = declared("you.haggling:price-of", 1, &[], 0);
        let outside = registry.serve(me, &names, other, answer, 0).unwrap_err();
        assert!(
            outside.contains("named `you.trading-post:<call>`"),
            "{outside}"
        );
    }

    #[test]
    fn an_asked_call_keeps_its_id_and_its_declaration() {
        let mut registry = Registry::default();
        let me = ModIndex::new(1);
        assert_eq!(registry.find(me, price(1)), Ok(0));
        assert_eq!(registry.find(me, price(1)), Ok(0));
        let changed = declared("you.trading-post:price-of", 1, &[("item", 2)], 4);
        assert!(registry.find(me, changed).is_err());
        assert_eq!(registry.find(me, price(2)), Ok(1));
    }

    #[test]
    fn the_seal_matches_by_major_fields_and_size_and_names_the_rest() {
        let mods = running();
        let mut registry = Registry::default();
        let server = ModIndex::new(0);
        let caller = ModIndex::new(1);
        let names = ["you.trading-post"];
        registry.serve(server, &names, price(2), answer, 0).unwrap();
        let wanted = [
            price(2),
            price(1),
            price(3),
            declared("you.trading-post:price-of", 2, &[("item", 7)], 4),
            declared("you.trading-post:price-of", 2, &[("item", 1)], 8),
            declared("you.trading-post:rent", 1, &[], 0),
            declared("you.gone:rent", 1, &[], 0),
            declared("you.off:rent", 1, &[], 0),
        ];
        // One mod asks for a name and major once, so the variants of the
        // served call come from mods of their own.
        for (i, want) in wanted.into_iter().enumerate() {
            let asker = if i < 3 { caller } else { ModIndex::new(10 + i) };
            let _id = registry.find(asker, want);
        }
        let (routes, offers) = registry.seal(&mods);
        let reasons: Vec<String> = routes
            .iter()
            .map(|r| r.server.as_ref().err().cloned().unwrap_or_default())
            .collect();
        assert_eq!(reasons[0], "");
        assert!(reasons[1].contains("serves v2, this mod asks for v1; update this mod"));
        assert!(reasons[2].contains("serves v2, this mod asks for v3; update you.trading-post"));
        assert!(reasons[3].contains("the input differs"));
        assert!(reasons[4].contains("the input differs"));
        assert!(reasons[5].contains("serves no such call; update you.trading-post"));
        assert_eq!(reasons[6], "you.gone does not run");
        assert_eq!(reasons[7], "you.off 0.1.0 is off");
        let lines = lines_of(&routes, &offers, &mods);
        assert_eq!(
            lines[0],
            "call you.trading-post:price-of v2 from you.trading-post 1.0.0: called by you.haggling"
        );
        assert!(
            lines[1].starts_with(
                "call you.trading-post:price-of v1 of you.haggling 0.2.0: unmatched, "
            )
        );
    }

    #[test]
    fn a_server_that_is_off_serves_nothing() {
        let mods = running();
        let mut registry = Registry::default();
        registry
            .serve(
                ModIndex::new(2),
                &["you.off"],
                declared("you.off:rent", 1, &[], 0),
                answer,
                0,
            )
            .unwrap();
        let _id = registry.find(ModIndex::new(1), declared("you.off:rent", 1, &[], 0));
        let (routes, _) = registry.seal(&mods);
        let reason = routes[0].server.as_ref().unwrap_err();
        assert_eq!(reason, "you.off 0.1.0 serves it, but is off");
    }
}
