//! Build table ids under `input.`.

use super::{Constant, Symbol};

pub(crate) const BINDINGS: Constant = Constant::new("input.bindings");
pub(crate) const BINDING_FIRST: Constant = Constant::new("input.binding_first");
pub(crate) const BINDING_MASK: Constant = Constant::new("input.binding_mask");
pub(crate) const BINDING_MODIFIERS: Constant = Constant::new("input.binding_modifiers");
pub(crate) const BINDING_SECOND: Constant = Constant::new("input.binding_second");
pub(crate) const BINDING_SIZE: Constant = Constant::new("input.binding_size");
pub(crate) const BLOCKED: Constant = Constant::new("input.blocked");
pub(crate) const CAMERA_ROTATION: Constant = Constant::new("input.camera_rotation");
pub(crate) const CLIENT_BUILD: Symbol = Symbol::new("input.client_build");
pub(crate) const MODIFIER_COUNT: Constant = Constant::new("input.modifier_count");
pub(crate) const MODIFIER_ITEMS: Constant = Constant::new("input.modifier_items");
pub(crate) const MODIFIER_SIZE: Constant = Constant::new("input.modifier_size");
pub(crate) const MOVEMENT_STICK: Constant = Constant::new("input.movement_stick");
pub(crate) const TARGET_SELECTION: Constant = Constant::new("input.target_selection");
pub(crate) const DIGITAL_INPUT: Constant = Constant::new("input.digital_input");
