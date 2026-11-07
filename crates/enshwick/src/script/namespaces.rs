//! The namespaces a script reaches: the parts of the API a runtime opens for a mod.

use std::sync::Arc;

use super::{Bound, Error, Function, InitFunction, IntoScript as _, Object, Signature, Ty, Value};

/// A part of the API as one namespace of a script, its type erased: what it
/// declares, and how a runtime opens it for a mod.
///
/// Parts that share a name are one namespace in a script, such as a
/// module's handle and its [`Load`](crate::Load) methods; the empty name is
/// the `enshwick` table itself.
///
/// # Examples
///
/// ```rust,standalone_crate
/// let names: Vec<&str> = enshwick::script::NAMESPACES.iter().map(|n| n.name()).collect();
/// assert!(names.contains(&"world"));
/// ```
#[derive(Debug)]
pub struct Namespace {
    name: &'static str,
    doc: &'static str,
    describe: fn() -> Vec<Declared>,
    open: fn(&mut crate::Load<'_>) -> Result<Opened, crate::Off>,
    init: &'static [&'static [InitFunction]],
    request: Option<Request>,
}

/// The start-time call that opens a namespace a script asks for, as a Rust
/// mod asks [`Load`](crate::Load) for the handle in `init`.
#[derive(Debug)]
struct Request {
    name: &'static str,
    signature: &'static Signature,
    open: fn(&mut crate::Load<'_>) -> Result<(Opened, Value), crate::Off>,
}

/// The signature of `T`'s getter: no parameters, the handle as an object.
struct Getter<T>(core::marker::PhantomData<T>);

impl<T: Object> Getter<T> {
    const SIGNATURE: &'static Signature = &Signature::new(&[], &Ty::Object(T::CLASS));
}

/// A function of a [`Namespace`] as definitions declare it.
///
/// # Examples
///
/// See [`Namespace::functions`].
#[derive(Clone, Copy, Debug)]
pub struct Declared {
    /// Its place in its namespace's [`Bound::functions`].
    index: usize,
    name: &'static str,
    doc: &'static str,
    signature: &'static Signature,
}

impl Declared {
    /// The function's name.
    ///
    /// # Examples
    ///
    /// See [`Namespace::functions`].
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Its doc comment up to the first section.
    ///
    /// # Examples
    ///
    /// See [`Namespace::functions`].
    #[must_use]
    pub const fn doc(&self) -> &'static str {
        self.doc
    }

    /// Its parameters and answer.
    ///
    /// # Examples
    ///
    /// See [`Namespace::functions`].
    #[must_use]
    pub const fn signature(&self) -> &'static Signature {
        self.signature
    }
}

/// The function behind an [`Opened`] namespace: the function's place in its
/// namespace, from [`Declared`], and its arguments.
type OpenedFn = dyn Fn(usize, Vec<Value>) -> Result<Value, Error> + Send + Sync;

/// A [`Namespace`] opened for one mod: calls its functions by index.
///
/// # Examples
///
/// See [`Namespace::open`].
#[derive(Clone)]
pub struct Opened(Arc<OpenedFn>);

impl core::fmt::Debug for Opened {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Opened")
    }
}

impl Opened {
    /// Calls `function`, one of [`Namespace::functions`] of the namespace
    /// this was opened from, with `args`.
    ///
    /// # Errors
    ///
    /// As [`Function::call`], and for a function past the end of this
    /// namespace; a function of another namespace is not detected.
    ///
    /// # Examples
    ///
    /// See [`Namespace::open`].
    pub fn call(&self, function: &Declared, args: Vec<Value>) -> Result<Value, Error> {
        (self.0)(function.index, args)
    }
}

impl Namespace {
    const fn of<T: Bound>() -> Self {
        Self {
            name: T::NAME,
            doc: T::DOC,
            describe: describe::<T>,
            open: open::<T>,
            init: &[],
            request: None,
        }
    }

    /// [`Namespace::of`] for a handle whose opening switches a loader
    /// feature on: a script opens it with `getter` at start, as a Rust mod
    /// calls the `Load` method of that name in `init`, so a mod that never
    /// asks pays nothing.
    const fn on_request<T: Bound + Object>(getter: &'static str) -> Self {
        Self {
            request: Some(Request {
                name: getter,
                signature: Getter::<T>::SIGNATURE,
                open: request::<T>,
            }),
            ..Self::of::<T>()
        }
    }

    /// The start-time function that opens this namespace, for a namespace a
    /// script asks for; `None` for one [`Namespace::open`] opens for every
    /// mod.
    ///
    /// # Examples
    ///
    /// ```rust,standalone_crate
    /// let getters: Vec<&str> = enshwick::script::NAMESPACES
    ///     .iter()
    ///     .filter_map(|n| Some(n.request_function()?.name()))
    ///     .collect();
    /// assert!(getters.contains(&"altars"));
    /// ```
    #[must_use]
    pub fn request_function(&self) -> Option<Declared> {
        self.request.as_ref().map(|request| Declared {
            index: 0,
            name: request.name,
            doc: self.doc,
            signature: request.signature,
        })
    }

    /// Opens a namespace a script asks for with
    /// [`Namespace::request_function`]: the namespace and its handle as an
    /// object; `None` for a namespace [`Namespace::open`] opens.
    ///
    /// # Errors
    ///
    /// [`Off`](crate::Off) when the game build lacks what it needs, or
    /// after `init`.
    ///
    /// # Examples
    ///
    /// See [`Namespace::request_function`].
    pub fn request(
        &self,
        load: &mut crate::Load<'_>,
    ) -> Option<Result<(Opened, Value), crate::Off>> {
        self.request.as_ref().map(|request| (request.open)(load))
    }

    /// The [`Load`](crate::Load) methods of `init` as the namespace `name`,
    /// callable while the mod starts.
    const fn starting(
        name: &'static str,
        doc: &'static str,
        init: &'static [&'static [InitFunction]],
    ) -> Self {
        Self {
            name,
            doc,
            describe: Vec::new,
            open: |_| Ok(Opened(Arc::new(|_, _| Err(Error::new("no such function"))))),
            init,
            request: None,
        }
    }

    /// Its functions a script calls only while the mod starts, as a Rust
    /// mod calls them in `init`.
    ///
    /// # Examples
    ///
    /// See [`InitFunction`].
    pub fn init_functions(&self) -> impl Iterator<Item = &'static InitFunction> {
        self.init.iter().flat_map(|part| part.iter())
    }

    /// The namespace's name in a script.
    ///
    /// # Examples
    ///
    /// See [`Namespace`].
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// What it is for, one sentence.
    ///
    /// # Examples
    ///
    /// See [`Namespace`].
    #[must_use]
    pub const fn doc(&self) -> &'static str {
        self.doc
    }

    /// Its functions, in the order [`Opened::call`] indexes them.
    ///
    /// # Examples
    ///
    /// ```rust,standalone_crate
    /// let world = enshwick::script::NAMESPACES.iter().find(|n| n.name() == "world");
    /// assert!(world.is_some_and(|w| w.functions().iter().any(|f| f.name() == "players")));
    /// ```
    #[must_use]
    pub fn functions(&self) -> Vec<Declared> {
        (self.describe)()
    }

    /// Opens the namespace for the mod `load` initialises.
    ///
    /// # Errors
    ///
    /// [`Off`](crate::Off) when the game build lacks what it needs.
    ///
    /// # Examples
    ///
    /// ```rust,standalone_crate
    /// use enshwick::script::{NAMESPACES, Value};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let log = NAMESPACES.iter().find(|n| n.name() == "log").ok_or(enshwick::Off::new("no log"))?;
    ///     let functions = log.functions();
    ///     let info = functions.iter().find(|f| f.name() == "info").ok_or(enshwick::Off::new("no info"))?;
    ///     log.open(load)?.call(info, vec![Value::String("hello".into())])?;
    ///     Ok(())
    /// }
    /// ```
    pub fn open(&self, load: &mut crate::Load<'_>) -> Result<Opened, crate::Off> {
        (self.open)(load)
    }
}

fn describe<T: Bound>() -> Vec<Declared> {
    declare(T::functions())
}

/// `functions` as a script declares them, each with its place in the list.
pub(super) fn declare<T: 'static>(
    functions: impl Iterator<Item = &'static Function<T>>,
) -> Vec<Declared> {
    functions
        .enumerate()
        .map(|(index, f)| Declared {
            index,
            name: f.name,
            doc: f.doc,
            signature: &f.signature,
        })
        .collect()
}

impl Declared {
    /// Its place in its list, which [`Opened::call`] and
    /// [`Handle::call`](super::Handle::call) index.
    pub(super) const fn index(&self) -> usize {
        self.index
    }
}

fn open<T: Bound>(load: &mut crate::Load<'_>) -> Result<Opened, crate::Off> {
    T::resolve(load).map(opened)
}

fn request<T: Bound + Object>(load: &mut crate::Load<'_>) -> Result<(Opened, Value), crate::Off> {
    let this = T::resolve(load)?;
    Ok((opened(this), this.into_value()))
}

fn opened<T: Bound>(this: T) -> Opened {
    Opened(Arc::new(move |index, args| {
        T::functions()
            .nth(index)
            .ok_or_else(|| Error::new(format!("{} has no such function", T::NAME)))?
            .call(&this, args)
    }))
}

/// Every namespace a script reaches, in the order a runtime opens them.
///
/// # Examples
///
/// See [`Namespace`].
pub const NAMESPACES: &[Namespace] = &[
    Namespace::of::<Log>(),
    #[cfg(feature = "unstable")]
    Namespace::of::<crate::world::World>(),
    #[cfg(feature = "unstable")]
    Namespace::starting(
        "world",
        "",
        &[
            crate::Load::SCRIPT_WORLD_RENAME,
            crate::Load::SCRIPT_WRITABLE_TYPE,
        ],
    ),
    Namespace::of::<crate::schedule::Scheduler>(),
    Namespace::of::<crate::inventory::Inventories>(),
    Namespace::of::<crate::resources::Resources>(),
    #[cfg(feature = "typed")]
    Namespace::starting(
        "typed",
        "The game's components, events and resources as the typed API generates them from its reflection.",
        &[crate::Load::SCRIPT_TYPED],
    ),
    Namespace::of::<crate::mods::Mods>(),
    #[cfg(feature = "unstable")]
    Namespace::of::<crate::actions::ActionFns>(),
    #[cfg(feature = "unstable")]
    Namespace::starting("actions", "", &[crate::Load::SCRIPT_ACTIONS]),
    #[cfg(feature = "unstable")]
    Namespace::of::<crate::controls::ControlFns>(),
    #[cfg(feature = "unstable")]
    Namespace::starting("controls", "", &[crate::Load::SCRIPT_CONTROLS]),
    #[cfg(feature = "unstable")]
    Namespace::of::<crate::console::ConsoleFns>(),
    Namespace::of::<crate::net::NetFns>(),
    Namespace::starting("net", "", &[crate::Load::SCRIPT_NET]),
    Namespace::starting("pacing", "", &[crate::Load::SCRIPT_PACING]),
    Namespace::of::<crate::ecs::EcsFns>(),
    #[cfg(feature = "unstable")]
    Namespace::starting("damage", "", &[crate::Load::SCRIPT_DAMAGE_FILTER]),
    Namespace::of::<crate::effects::Effects>(),
    Namespace::of::<crate::survivors::SurvivorFns>(),
    Namespace::starting(
        "survivors",
        "",
        &[
            crate::Load::SCRIPT_SURVIVORS,
            crate::Load::SCRIPT_SURVIVOR_HOOKS,
        ],
    ),
    #[cfg(feature = "unstable")]
    Namespace::starting("item_data", "", &[crate::Load::SCRIPT_ITEM_DATA]),
    #[cfg(feature = "unstable")]
    Namespace::starting("entity_data", "", &[crate::Load::SCRIPT_ENTITY_DATA]),
    Namespace::starting(
        "",
        "",
        &[
            crate::Load::SCRIPT_STATE,
            crate::Load::SCRIPT_TICKS,
            crate::Load::SCRIPT_DIRS,
        ],
    ),
    Namespace::of::<crate::state::StateFns>(),
    Namespace::of::<crate::files::FilesFns>(),
    Namespace::of::<crate::ui::Ui>(),
    Namespace::starting(
        "ui",
        "",
        &[
            crate::Load::SCRIPT_UI,
            #[cfg(feature = "unstable")]
            crate::Load::SCRIPT_UI_EXTENSION,
        ],
    ),
    Namespace::starting("vetoes", "", &[crate::Load::SCRIPT_VETOES]),
    Namespace::starting("chat", "", &[crate::Load::SCRIPT_CHAT]),
    #[cfg(feature = "unstable")]
    Namespace::of::<crate::interactions::Targets>(),
    Namespace::starting("interactions", "", &[crate::Load::SCRIPT_INTERACTIONS]),
    #[cfg(feature = "unstable")]
    Namespace::of::<crate::input::Input>(),
    #[cfg(feature = "unstable")]
    Namespace::starting("items", "", &[crate::Load::SCRIPT_ITEMS]),
    #[cfg(feature = "unstable")]
    Namespace::of::<crate::rewards::Rewards>(),
    #[cfg(feature = "unstable")]
    Namespace::starting("rewards", "", &[crate::Load::SCRIPT_REWARDS]),
    Namespace::starting(
        "progress",
        "",
        &[
            crate::Load::SCRIPT_PROGRESS,
            crate::Load::SCRIPT_PROGRESS_HOOKS,
        ],
    ),
    Namespace::starting("status", "", &[crate::Load::SCRIPT_STATUS]),
    Namespace::of::<crate::buffs::Buffs>(),
    Namespace::of::<crate::knowledge::Knowledge>(),
    Namespace::starting("knowledge", "", &[crate::Load::SCRIPT_KNOWLEDGE]),
    Namespace::of::<crate::game_settings::GameSettings>(),
    Namespace::of::<crate::rates::RateFns>(),
    #[cfg(feature = "unstable")]
    Namespace::starting("rates", "", &[crate::Load::SCRIPT_RATES]),
    Namespace::of::<crate::affixes::Affixes>(),
    Namespace::of::<crate::affixes::AffixFns>(),
    Namespace::starting(
        "affixes",
        "",
        &[
            crate::Load::SCRIPT_AFFIX_DEFS,
            #[cfg(feature = "unstable")]
            crate::Load::SCRIPT_AFFIX_ROLLS,
        ],
    ),
    Namespace::of::<crate::i18n::I18nFns>(),
    Namespace::starting("i18n", "", &[crate::Load::SCRIPT_I18N]),
    Namespace::of::<crate::weather::Weather>(),
    Namespace::starting(
        "weather",
        "",
        &[
            crate::Load::SCRIPT_WEATHER_FRONTS,
            #[cfg(feature = "typed")]
            crate::Load::SCRIPT_WEATHER_PHENOMENA,
        ],
    ),
    #[cfg(feature = "unstable")]
    Namespace::of::<crate::map::Map>(),
    #[cfg(feature = "unstable")]
    Namespace::starting("map", "", &[crate::Load::SCRIPT_MAP_EVENTS]),
    #[cfg(feature = "unstable")]
    Namespace::on_request::<crate::map::FogOfWar>("fog_of_war"),
    Namespace::of::<crate::daytime::Clock>(),
    Namespace::starting("daytime", "", &[crate::Load::SCRIPT_DAYTIME]),
    Namespace::starting(
        "sound",
        "",
        &[
            crate::Load::SCRIPT_SOUND,
            #[cfg(feature = "unstable")]
            crate::Load::SCRIPT_SOUND_ADD,
        ],
    ),
    Namespace::on_request::<crate::altars::Altars>("altars"),
    Namespace::starting(
        "altars",
        "",
        &[
            crate::Load::SCRIPT_ALTARS,
            crate::Load::SCRIPT_ALTAR_LIMIT,
            crate::Load::SCRIPT_ALTAR_LIMITS,
            crate::Load::SCRIPT_FLAME_ALTARS,
        ],
    ),
    Namespace::of::<crate::loot::LootFns>(),
    Namespace::starting(
        "loot",
        "",
        &[
            crate::Load::SCRIPT_LOOT_DROPS,
            crate::Load::SCRIPT_LOOT_PASS,
        ],
    ),
    Namespace::on_request::<crate::stats::Stats>("stats"),
    Namespace::starting("stats", "", &[crate::Load::SCRIPT_STATS]),
    Namespace::of::<crate::voxels::Voxels>(),
    Namespace::starting("voxels", "", &[crate::Load::SCRIPT_VOXEL_EDITS]),
    Namespace::of::<crate::terrain::Terrain>(),
    Namespace::of::<crate::content::Content>(),
    Namespace::starting(
        "content",
        "",
        &[
            crate::Load::SCRIPT_CONTENT_IDS,
            crate::Load::SCRIPT_CONTENT_ITEMS,
            crate::Load::SCRIPT_CONTENT_RECIPES,
            #[cfg(feature = "unstable")]
            crate::Load::SCRIPT_CONTENT_RECIPE_WITH,
            crate::Load::SCRIPT_CONTENT_WORKSHOPS,
            crate::Load::SCRIPT_CONTENT_TEMPLATES,
            crate::Load::SCRIPT_CONTENT_PROPS,
            #[cfg(feature = "typed")]
            crate::Load::SCRIPT_CONTENT_ENEMIES,
            #[cfg(feature = "typed")]
            crate::Load::SCRIPT_CONTENT_SPAWNS,
            #[cfg(feature = "typed")]
            crate::Load::SCRIPT_CONTENT_KNOWLEDGE,
            #[cfg(feature = "typed")]
            crate::Load::SCRIPT_CONTENT_JOURNAL,
            #[cfg(feature = "typed")]
            crate::Load::SCRIPT_CONTENT_SKILLS,
            #[cfg(feature = "typed")]
            crate::Load::SCRIPT_CONTENT_REMOVAL,
            crate::Load::SCRIPT_CONTENT_ICONS,
            crate::Load::SCRIPT_CONTENT_RENAMES,
            crate::Load::SCRIPT_CONTENT_CHANGE,
            crate::Load::SCRIPT_CONTENT_CHANGE_ITEMS,
            crate::Load::SCRIPT_CONTENT_CHANGE_RECIPES,
            crate::Load::SCRIPT_CONTENT_LIVE,
        ],
    ),
];

/// The mod's log as a script writes to it: one line per call, under the
/// mod's id.
///
/// # Examples
///
/// See [`Namespace::open`].
#[derive(Clone, Copy, Debug)]
pub struct Log(crate::Host);

#[crate::script(lines)]
impl Log {
    /// Writes `text` to the mod's log as information.
    #[script]
    pub fn info(&self, text: &str) {
        self.0.log(crate::abi::Level::INFO, text);
    }

    /// Writes `text` to the mod's log as a warning.
    #[script]
    pub fn warn(&self, text: &str) {
        self.0.log(crate::abi::Level::WARN, text);
    }

    /// Writes `text` to the mod's log as an error; the mod keeps running.
    #[script]
    pub fn error(&self, text: &str) {
        self.0.log(crate::abi::Level::ERROR, text);
    }

    /// The most detailed level the mod's log keeps, as
    /// [`Host::log_level`](crate::Host::log_level) answers it.
    #[script]
    #[must_use]
    pub fn level(&self) -> crate::abi::Level {
        self.0.log_level()
    }
}

impl Bound for Log {
    const NAME: &'static str = "log";
    const DOC: &'static str = "The mod's log file, one line per call.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_LINES];

    fn resolve(load: &mut crate::Load<'_>) -> Result<Self, crate::Off> {
        Ok(Self(load.host()))
    }
}
