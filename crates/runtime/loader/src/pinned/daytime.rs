//! Build table ids under `daytime.`.

use super::{Constant, Symbol};

pub(crate) const ANCHOR: Constant = Constant::new("daytime.anchor");
pub(crate) const CLOCK: Constant = Constant::new("daytime.clock");
pub(crate) const DAY_END: Constant = Constant::new("daytime.day_end");
pub(crate) const DAY_LENGTH: Constant = Constant::new("daytime.day_length");
pub(crate) const DAY_START: Constant = Constant::new("daytime.day_start");
pub(crate) const GAME_TIME: Constant = Constant::new("daytime.game_time");
pub(crate) const IS_DAY: Constant = Constant::new("daytime.is_day");
pub(crate) const NIGHT_LENGTH: Constant = Constant::new("daytime.night_length");
pub(crate) const OBJECT: Constant = Constant::new("daytime.object");
pub(crate) const SET_SPEED: Symbol = Symbol::new("daytime.set_speed");
pub(crate) const SET_TIME: Symbol = Symbol::new("daytime.set_time");
pub(crate) const SPEED: Constant = Constant::new("daytime.speed");
