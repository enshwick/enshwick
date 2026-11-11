//! What a mod does while the loader initialises it: [`Load`], whose base
//! methods forward to the base handle it holds.

pub(crate) use enshwick_base::__off_from as off_from;
use enshwick_base::BaseLoad;
pub(crate) use enshwick_base::load::{CHECK_WHY, hand_over, off_status, ok};
#[doc(inline)]
pub use enshwick_values::Off;

use crate::host::Host;

/// A mod: what the loader initialises when it loads the DLL.
///
/// Implement it on the mod's state type and export it with
/// [`declare_mod!`](crate::declare_mod), which reads what the mod is from its
/// `Cargo.toml`.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// struct Quiet;
///
/// impl enshwick::Mod for Quiet {
///     fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
///         Ok(Self)
///     }
/// }
/// ```
pub trait Mod: Sized + Send + Sync + 'static {
    /// Registers hooks, services and settings. Nothing takes effect until every
    /// mod is initialised and the loader arms the hooks.
    ///
    /// # Errors
    ///
    /// [`Off`] when the mod cannot run on this game or with these settings.
    fn init(load: &mut Load<'_>) -> Result<Self, Off>;

    /// Runs once the hooks are armed; start background work here.
    fn ready(&'static self, _host: Host) {}
}

/// The loader during a mod's initialisation: lookups, hooks, settings and
/// services.
///
/// # Examples
///
/// See [`Mod`].
#[derive(Debug)]
pub struct Load<'a> {
    base: BaseLoad,
    _init: core::marker::PhantomData<&'a ()>,
}

impl Load<'_> {
    pub(crate) fn new(host: Host) -> Self {
        Self::of(host, false, None)
    }

    /// A `Load` for a mod that takes part in the join check when `joins`,
    /// with the texts `declare_mod!` embedded.
    pub(crate) fn of(host: Host, joins: bool, embedded_texts: Option<&'static str>) -> Self {
        Self {
            base: BaseLoad::of(host, joins, embedded_texts),
            _init: core::marker::PhantomData,
        }
    }

    /// The base handle, for what a domain registers through it.
    pub(crate) fn base(&mut self) -> &mut BaseLoad {
        &mut self.base
    }

    /// The mod's `src/texts.toml`, as `declare_mod!` embedded it.
    pub(crate) fn embedded_texts(&self) -> Option<&'static str> {
        self.base.embedded_texts()
    }

    /// The state of type `T` a domain keeps while the mod initialises, empty
    /// until the domain fills it.
    #[cfg_attr(
        not(any(feature = "unstable", feature = "typed")),
        expect(dead_code, reason = "only unstable and typed domains keep state")
    )]
    pub(crate) fn slot<T: enshwick_base::load::Slot>(&mut self) -> &mut Option<T> {
        self.base.slot()
    }

    /// Whether the mod runs on both sides, so it takes part in the join
    /// check, as its manifest says.
    pub(crate) fn joins(&self) -> bool {
        self.base.joins()
    }

    fn api(&self) -> &'static crate::abi::HostApi {
        self.base.api()
    }
}

// `Load`'s methods of the base, by the base module they forward to.
mod build;
mod code;
mod config;
mod events;
mod game_events;
mod i18n;
mod net;
mod pacing;
mod points;
mod resources;
mod services;
mod settings;
mod state;
mod status;
mod time;

impl enshwick_base::load::sealed::Sealed for Load<'_> {}

impl crate::Loading for Load<'_> {
    fn base(&self) -> &BaseLoad {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseLoad {
        &mut self.base
    }
}

impl crate::facts::EngineFacts for Load<'_> {
    fn offset(&self, id: &str) -> Result<usize, Off> {
        self.host().offset(id)
    }

    fn size(&self, id: &str) -> Result<usize, Off> {
        self.host().size(id)
    }

    fn hash(&self, id: &str) -> Result<u32, Off> {
        self.host().hash(id)
    }
}
