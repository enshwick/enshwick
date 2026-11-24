//! The fake's base entries: the build, hooks, mods, the host, messages,
//! pacing and services.

use enshwick_core::abi::{
    ConfigCheckFn, Event, EventFn, GameEventFn, Level, LoaderOff, MidHookFn, ModEventFn, ModHandle,
    NetMessageFn, StatusFn,
};

use crate::test::base::{calls::FakeCalls, hooks::LinkedHook, host::FakeService};
use crate::test::base::{points::FakePoints, ticks::FakeTicks};
use crate::test::table::Registration;
use registered::Registered;

pub(crate) mod args;
pub(crate) mod build;
pub(crate) mod caller;
#[cfg(test)]
mod headed_tests;
pub(crate) mod hooks;
pub(crate) mod host;
pub(crate) mod io;
pub(crate) mod mods;
pub(crate) mod net;
pub(crate) mod pacing;
pub(crate) mod registered;
pub(crate) mod services;

/// A defined mod event: name, layout, the size of its data and the mod that
/// defined it.
pub(crate) type DefinedEvent = (String, Vec<(String, u64)>, u32, ModHandle);

/// A mod event handler: name, layout, the bytes it reads and the handler.
pub(crate) type ModEventSubscription = (String, Vec<(String, u64)>, u32, Registered<ModEventFn>);

/// What the installed fake's base answers and has recorded.
#[derive(Default)]
pub(crate) struct BaseState {
    /// The install's own folder of [`FakeHost::mod_file`](crate::test::FakeHost::mod_file)
    /// files, removed when the install drops.
    pub(crate) mod_files: Option<crate::test::base::settings::OwnFolder>,
    pub(crate) logs: Vec<(Level, String)>,
    pub(crate) faults: Vec<String>,
    /// The handle each log line and fault came under, in order.
    pub(crate) sent: Vec<(ModHandle, Level, String)>,
    pub(crate) hooks: Vec<LinkedHook>,
    /// The slots of the linked hooks: one slot joins one chain.
    pub(crate) hook_slots: Vec<usize>,
    /// The mod as its manifest names it, once its init ran.
    pub(crate) manifest: Option<mods::ManifestFacts>,
    /// Whether the mod may still ask for what only init may.
    pub(crate) registration: Registration,
    /// Mid-function hooks: address, handler and the mod's pointer.
    pub(crate) mid_hooks: Vec<(u64, Registered<MidHookFn>)>,
    /// Loader event handlers, by event.
    pub(crate) subscriptions: Vec<(Event, Registered<EventFn>)>,
    /// Game event handlers, by type hash (0 for every type).
    pub(crate) game_events: Vec<(u32, Registered<GameEventFn>)>,
    /// Game event handlers of the client's own world, by type hash (0 for
    /// every type); none on a dedicated server.
    pub(crate) client_game_events: Vec<(u32, Registered<GameEventFn>)>,
    pub(crate) code_patches: Vec<(u64, Vec<u8>, Vec<u8>)>,
    pub(crate) published: Vec<FakeService>,
    /// Mod events the mod defined: name and layout, by id.
    pub(crate) defined_events: Vec<DefinedEvent>,
    /// Mod event handlers: name, layout, the bytes read and the handler.
    pub(crate) mod_event_subscriptions: Vec<ModEventSubscription>,
    /// The names of the mod events the mod sent, in order.
    pub(crate) sent_events: Vec<(String, Vec<u8>)>,
    pub(crate) status_sources: Vec<Registered<StatusFn>>,
    /// The mod's features as it last reported them: name and, when off,
    /// the reason.
    pub(crate) features: Vec<(String, Option<String>)>,
    /// Channels the mod opened: full name, id, handler and pointer.
    pub(crate) channels: Vec<(String, u32, Registered<NetMessageFn>)>,
    /// Messages the mod sent: channel id, raw peer and bytes.
    pub(crate) net_sent: Vec<(u32, enshwick_core::PlayerKey, Vec<u8>)>,
    /// The schemas of the mod's settings files, by name, as TOML.
    pub(crate) config_schemas: std::collections::BTreeMap<String, String>,
    /// The loader's parts and features off, as the test turned them off.
    pub(crate) loader_off: Vec<LoaderOff>,
    /// The mod's checks of new settings texts, by file.
    pub(crate) config_checks: std::collections::BTreeMap<String, Registered<ConfigCheckFn>>,
    /// The mod's settings writes in flight, one per nested write, each
    /// `true` while it runs the mod's check, as the loader refuses them.
    pub(crate) config_writes: Vec<bool>,
    /// The texts the mod handed the loader last, as TOML.
    pub(crate) published_texts: Option<String>,
    /// Extension points the mod defined and links it attached.
    pub(crate) points: FakePoints,
    /// Calls the mod serves and asks for.
    pub(crate) calls: FakeCalls,
    /// The fake steps and the mod's periodic tick handlers.
    pub(crate) ticks: FakeTicks,
    /// What the mod asked of the engine's pacing places.
    pub(crate) pacing: pacing::Pacing,
    /// The settings texts handed out, alive while the fake is installed.
    settings: Vec<Box<str>>,
    /// The handles turned off, as the loader turns a mod off: its init or
    /// ready failed, or it faulted. None of their callbacks runs again.
    pub(crate) off: Vec<ModHandle>,
}

impl BaseState {
    /// Whether the mod's init has returned, so what only init may ask for
    /// comes too late, as the loader answers.
    pub(crate) fn closed(&self) -> bool {
        self.registration == Registration::Closed
    }
}
