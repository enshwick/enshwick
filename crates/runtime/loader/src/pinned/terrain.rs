//! Build table ids under `terrain.`.

use super::{Constant, Symbol};

pub(crate) const ADD: Symbol = Symbol::new("terrain.add");
pub(crate) const DIG: Symbol = Symbol::new("terrain.dig");
pub(crate) const EDITOR: Constant = Constant::new("terrain.editor");
pub(crate) const FLATTEN: Symbol = Symbol::new("terrain.flatten");
pub(crate) const REGISTRY: Constant = Constant::new("terrain.registry");
pub(crate) const REGISTRY_RESOURCE: Constant = Constant::new("terrain.registry_resource");
pub(crate) const VOXEL_WORLD: Constant = Constant::new("terrain.voxel_world");
