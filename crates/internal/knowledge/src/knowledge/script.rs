//! What only a script's filters get of knowledge: the input filter that
//! answers nil to keep the verdict.

use super::KnowledgeRequest;
use crate::Off;

/// `Load`'s script-only methods of this module on the base handle, which
/// core's forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::on_knowledge_input_script`.
    #[doc(hidden)]
    fn on_knowledge_input_script<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&KnowledgeRequest) -> Option<bool> + Send + Sync + 'static;
}

impl LoadMethods for crate::BaseLoad {
    fn on_knowledge_input_script<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&KnowledgeRequest) -> Option<bool> + Send + Sync + 'static,
    {
        use super::LoadMethods as _;
        self.on_knowledge_input(move |request| filter(request).unwrap_or(request.allowed))
    }
}
