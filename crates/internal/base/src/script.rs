//! The script value model and the events a script subscribes to, which
//! `enshwick-core`'s `script` joins with how `Load` opens each part.

mod events;

#[doc(inline)]
pub use enshwick_values::script::*;
pub use events::{EventKind, GameEventKind, events, game_events};
