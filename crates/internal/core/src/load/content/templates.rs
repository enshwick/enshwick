//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::resources::{Registry, Type, Value, Variant};
use crate::{Load, Off};
use enshwick_content::__load::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

#[enshwick_macros::forward(base)]
#[crate::script(content_templates)]
impl Load<'_> {
    /// Adds an entity template: a copy of the vanilla template
    /// [`NewTemplate::like`] under the mod's content id, its family, model
    /// and proxy model kept, with the changes applied in order, and lists
    /// it in the game's template collection. On the server and the client
    /// alike, so a mod that adds one is a both mod.
    ///
    /// When the mod is removed, the loader serves the vanilla template under
    /// the copy's GUID, so saved objects and creatures load as the vanilla
    /// one.
    ///
    /// # Errors
    ///
    /// [`Off`] when `like` is no vanilla template, a change's component type
    /// is not reflected or does not fit (see [`ComponentChange`]), or the
    /// loader refuses the id (see [`Load::declare`]).
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::{Guid, id};
    /// use enshwick::content::templates::{ComponentChange, NewTemplate};
    ///
    /// const WOLF: Guid = Guid::parse("b7a4f1c0-4c1e-4f39-9d5b-2a0f3c6d8e11");
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let calm = load.add_template(NewTemplate {
    ///         changes: vec![ComponentChange::remove_named("ecs::Aggro")],
    ///         ..NewTemplate::new(id!("calm-wolf"), WOLF)
    ///     })?;
    ///     enshwick::info!("calm wolves are {}", calm.guid());
    ///     Ok(())
    /// }
    /// ```
    #[script]
    pub fn add_template(&mut self, new: NewTemplate) -> Result<TemplateId, Off>;

    /// Changes the components of the vanilla template `template` in memory,
    /// the changes applied in order; mods that change one template change it
    /// in load order, each on the one before's result. Nothing is written to
    /// a save or the archive.
    ///
    /// # Errors
    ///
    /// [`Off`] when `template` is no vanilla template, or a change's
    /// component type is not reflected or does not fit (see
    /// [`ComponentChange`]).
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    /// use enshwick::content::templates::ComponentChange;
    ///
    /// const WOLF: Guid = Guid::parse("b7a4f1c0-4c1e-4f39-9d5b-2a0f3c6d8e11");
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.change_template(WOLF, vec![ComponentChange::remove_named("ecs::Aggro")])
    /// }
    /// ```
    #[script]
    pub fn change_template(
        &mut self,
        template: Guid,
        changes: Vec<ComponentChange>,
    ) -> Result<(), Off>;
}
