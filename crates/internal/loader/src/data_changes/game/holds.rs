//! The hold a raised stack size sets on the side that keeps the world, and
//! the stack sizes kept for its saves.

use enshwick::abi::{Event, Level};
use enshwick::state::Hold;

use super::{CHANGES, loader_dir, on};
use crate::data_changes::{HoldStep, Kept, hold_reason, hold_step};
use crate::host::Loader;
use crate::locks::Relock;
use crate::mod_index::ModIndex;

/// On the side that keeps the world, when it loads or saves: keeps every
/// stack size in force above the game's, and holds the game while any is
/// kept, so a game the loader leaves vanilla stops before it cuts them.
fn keep_sizes(loader: &Loader) {
    let stacks = CHANGES.stacks.relock().clone().unwrap_or_default();
    let mut kept = CHANGES.kept.relock();
    if kept.is_none() {
        match Kept::load(&loader_dir(loader)) {
            Ok(read) => *kept = Some(read),
            // The file and its hold stay as they are; the next load or save
            // tries again.
            Err(why) => {
                crate::log!(
                    WARN,
                    "kept stack sizes do not read and stay as they are: {why}"
                );
                return;
            }
        }
    }
    let Some(kept) = kept.as_mut() else {
        return;
    };
    let mut grew = false;
    for (item, (vanilla, size)) in stacks {
        grew |= kept.record(item, vanilla, size);
    }
    let dir = loader_dir(loader);
    if grew && let Err(why) = kept.save(&dir) {
        crate::log!(WARN, "kept stack sizes not written: {why}");
    }
    let hold = Hold::new(&dir);
    let result = if kept.is_empty() {
        hold.clear()
    } else {
        hold.set(&hold_reason(kept))
    };
    if let Err(why) = result {
        crate::log!(WARN, "hold not kept: {why}");
    }
}

/// On the side that keeps the world: keeps the stack sizes in force when it
/// loads and saves, and when it loads sets the hold of every pack that
/// raised a stack size on that world, and releases that of every other pack
/// still on. A pack's hold of the whole data folder, from before holds knew
/// worlds, goes once the world is known. A mod with code keeps its own
/// hold.
pub(crate) fn world_event(loader: &Loader, event: Event) {
    if event == Event::WORLD_SAVED {
        keep_sizes(loader);
    }
    if event != Event::WORLD_LOADED {
        return;
    }
    keep_sizes(loader);
    let raised = CHANGES.raised.relock().clone().unwrap_or_default();
    let world = crate::saves::world_key(loader);
    for (index, record) in loader.mods.iter().enumerate() {
        let index = ModIndex::new(index);
        if !on(loader, index) {
            continue;
        }
        let dir = loader.data_dir.join(&record.id);
        let hold = match world {
            Some(world) => {
                if record.pack
                    && let Err(why) = Hold::new(&dir).clear()
                {
                    crate::log::write(Level::WARN, &record.id, &format!("hold not kept: {why}"));
                }
                Hold::for_world(&dir, world)
            }
            None => Hold::new(&dir),
        };
        let result = match hold_step(record.pack, raised.get(&index)) {
            HoldStep::Set(reason) => hold.set(&reason),
            HoldStep::Release => hold.clear(),
            HoldStep::Keep => Ok(()),
        };
        if let Err(why) = result {
            crate::log::write(Level::WARN, &record.id, &format!("hold not kept: {why}"));
        }
    }
}
