//! Build table ids under `sound.`.

use super::{Constant, Symbol};

pub(crate) const CLIENT_LINK: Constant = Constant::new("sound.client_link");
pub(crate) const CLIENT_UI_SCENE: Constant = Constant::new("sound.client_ui_scene");
pub(crate) const GAMEUI_LINK: Constant = Constant::new("sound.gameui_link");
pub(crate) const GAMEUI_UI_SCENE: Constant = Constant::new("sound.gameui_ui_scene");
pub(crate) const IS_PLAYING: Symbol = Symbol::new("sound.is_playing");
pub(crate) const SET_POSITION: Symbol = Symbol::new("sound.set_position");
pub(crate) const SET_VOLUME: Symbol = Symbol::new("sound.set_volume");
pub(crate) const START: Symbol = Symbol::new("sound.start");
pub(crate) const STOP: Symbol = Symbol::new("sound.stop");
pub(crate) const UI_OBJECT: Constant = Constant::new("sound.ui_object");
pub(crate) const UI_SCENE: Constant = Constant::new("sound.ui_scene");
pub(crate) const UI_SYSTEM: Constant = Constant::new("sound.ui_system");
