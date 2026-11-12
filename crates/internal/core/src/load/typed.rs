//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::{Load, Off};
use enshwick_world::__internal::typed_script::{ComponentType, EventType, ResourceType};
use enshwick_world::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(typed)]
impl Load<'_> {
    /// The generated component type `id`, its path in the typed API joined
    /// by dots, such as `ecs.Health`, on the running build, its shape
    /// checked.
    ///
    /// # Errors
    ///
    /// [`Off`] for an id of no generated component, and when the build
    /// lacks the type, its shape differs from the generated struct, or a
    /// field's data lies where a read does not reach, such as a designer
    /// data kind's, naming the field and the build.
    #[script(name = "component_type")]
    pub(crate) fn typed_component_type(&mut self, id: &str) -> Result<ComponentType, Off>;

    /// The generated game event type `id`, its path in the typed API
    /// joined by dots, such as `ecs.EntityDiedEvent`, on the running build,
    /// its shape checked.
    ///
    /// # Errors
    ///
    /// [`Off`] for an id of no generated event, and when the build lacks
    /// the type or its shape differs from the generated struct, naming the
    /// build.
    #[script(name = "event_type")]
    pub(crate) fn typed_event_type(&mut self, id: &str) -> Result<EventType, Off>;

    /// The generated resource type `id`, its path in the typed API joined
    /// by dots, such as `ItemCollectionResource`, on the running build, its
    /// shape checked.
    ///
    /// # Errors
    ///
    /// [`Off`] for an id of no generated resource, and when the build lacks
    /// the type or its shape differs from the generated struct, naming the
    /// build.
    #[script(name = "resource_type")]
    pub(crate) fn typed_resource_type(&mut self, id: &str) -> Result<ResourceType, Off>;
}
