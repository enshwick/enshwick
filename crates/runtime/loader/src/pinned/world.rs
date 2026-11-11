//! Build table ids under `world.`.

use super::{Constant, Symbol};

pub(crate) const ENTITIES: Constant = Constant::new("world.entities");
pub(crate) const SCENE_CHUNKS: Constant = Constant::new("world.scene_chunks");
pub(crate) const SCENE_CHUNKS_COUNT: Constant = Constant::new("world.scene_chunks.count");
pub(crate) const SCENE_CHUNKS_SCENE: Constant = Constant::new("world.scene_chunks.scene");
pub(crate) const STEP: Symbol = Symbol::new("world.step");
pub(crate) const TIME: Symbol = Symbol::new("world.time");
