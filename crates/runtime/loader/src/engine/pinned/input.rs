//! Build table ids under `input.`.

use super::{Constant, Symbol};

pub(crate) const BLOCKED: Constant = Constant::new("input.blocked");
pub(crate) const CAMERA_ROTATION: Constant = Constant::new("input.camera_rotation");
pub(crate) const CLIENT_BUILD: Symbol = Symbol::new("input.client_build");
pub(crate) const MOVEMENT_STICK: Constant = Constant::new("input.movement_stick");
pub(crate) const TARGET_SELECTION: Constant = Constant::new("input.target_selection");
pub(crate) const DIGITAL_INPUT: Constant = Constant::new("input.digital_input");
