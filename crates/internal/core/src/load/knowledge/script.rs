//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::Load;
#[cfg(feature = "unstable")]
use crate::Off;
#[cfg(feature = "unstable")]
use crate::knowledge::KnowledgeRequest;
#[cfg(feature = "unstable")]
use enshwick_knowledge::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(knowledge)]
impl Load<'_> {
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
}
