//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::Off;
use crate::WorldError;
#[cfg(feature = "unstable")]
use crate::abi::KnowledgeInput;
use crate::abi::{Header, KnowledgeInfo, Status};
use crate::{Entity, PlayerKey};
use crate::{Host, Load};
#[cfg(feature = "unstable")]
use core::ffi::c_void;
use enshwick_knowledge::__load::*;
use enshwick_world::__internal::world_status;

#[enshwick_macros::forward(base)]
#[crate::script(knowledge)]
impl Load<'_> {
    /// Runs `filter` on the knowledge each client asks the server for in
    /// its input, after the loader's guard; its answer, `true` to let the
    /// request through, replaces the verdict so far, so a mod can refuse
    /// more than the guard or allow what it refuses. A filter's refusal
    /// stands whatever the guard's mode; a guard that only watches lets
    /// through what it would refuse, so `filter` sees it allowed, with the
    /// reason in `KnowledgeRequest::refusal`. A refused request changes
    /// nothing and is not asked again. It runs on the server's worker
    /// threads, rarely: only when a client asks.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// [`Off`] after init or on a build without the input site.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::knowledge::Refusal;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     const TUTORIAL_COUNTER: enshwick::knowledge::KnowledgeId = enshwick::knowledge::KnowledgeId::new(0x51);
    ///     load.on_knowledge_input(|request| {
    ///         request.allowed
    ///             || (request.id == TUTORIAL_COUNTER && request.refusal == Some(Refusal::Counter))
    ///     })
    /// }
    /// ```
    #[cfg(feature = "unstable")]
    pub fn on_knowledge_input<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&KnowledgeRequest) -> bool + Send + Sync + 'static;

    /// Runs `filter` on each player's knowledge input, as
    /// [`Load::on_knowledge_input`] does; its answer replaces the verdict so
    /// far, and nil or an error keeps it.
    ///
    /// # Errors
    ///
    /// As [`Load::on_knowledge_input`].
    #[cfg(feature = "unstable")]
    #[script(name = "on_knowledge_input")]
    pub(crate) fn on_knowledge_input_script<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&KnowledgeRequest) -> Option<bool> + Send + Sync + 'static;

    /// The [`Knowledge`] of the server's world; on a client, the built-in
    /// server's while it hosts a game.
    ///
    /// # Examples
    ///
    /// See the [module](crate::knowledge).
    #[must_use]
    pub fn knowledge(&mut self) -> Knowledge;
}
