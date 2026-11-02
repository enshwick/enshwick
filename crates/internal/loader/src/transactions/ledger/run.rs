//! A task's run in a step: its changes written in order, or its undo newest first.

use enshwick::abi::{Status, TransactionInfo};

use super::checks::{still_placed, still_removed};
use super::{
    BRICK_VOXELS, Brick, Change, Done, Image, Mode, Outcome, Pass, Settings, Task, Voxels, hash,
};

/// Writes each brick of `images` back with `change` laid over it; the first
/// write that fails, after which the bricks before it stay written.
fn write_over(
    world: &mut impl Voxels,
    images: &[(Brick, Image)],
    mut change: impl FnMut(Brick, &mut [u16; BRICK_VOXELS]),
) -> Option<Status> {
    images
        .iter()
        .try_for_each(|(brick, before)| {
            let mut image = **before;
            change(*brick, &mut image);
            world.write(*brick, &image)
        })
        .err()
}

impl Task {
    /// Runs the task against the world for what is left of the pass.
    pub(crate) fn run(&mut self, world: &mut impl Voxels, pass: &mut Pass) -> Outcome {
        let outcome = match self.mode {
            Mode::Apply => self.apply(world, pass),
            Mode::Undo => self.take_back(world, pass),
        };
        if matches!(outcome, Outcome::Done(..)) {
            for entity in self.work.held.drain(..) {
                world.release_prop(entity);
            }
        }
        outcome
    }

    fn apply(&mut self, world: &mut impl Voxels, pass: &mut Pass) -> Outcome {
        let work = &mut self.work;
        let failed = |reason| Outcome::Done(TransactionInfo::FAILED, reason);
        if !work.started {
            work.started = true;
            if let Some(outcome) = work.first_check(world, pass) {
                return outcome;
            }
            match work.hold_removals(world) {
                Ok(false) => {}
                Ok(true) => return Outcome::Later,
                Err(reason) => return failed(reason),
            }
        }
        while let Some(op) = work.ops.get(work.done.len()) {
            let cost = op.cost();
            if pass.spent > 0 && pass.spent + cost > pass.budget {
                return Outcome::Again;
            }
            let actor = work.actor.unwrap_or(0);
            match &op.change {
                Change::PlaceProp(prop) => match world.spawn_prop(prop, actor) {
                    Ok(entity) => work.done.push(Done::Placed {
                        entity,
                        prop: *prop,
                        settings: Settings::Unread,
                    }),
                    Err(reason) => return failed(reason),
                },
                &Change::RemoveProp {
                    entity,
                    discard_state,
                } => {
                    if let Err(reason) = work.remove(world, entity, discard_state, actor) {
                        return failed(reason);
                    }
                }
                change => {
                    let Some(bricks) = op.bricks else {
                        return failed(Status::MISSING);
                    };
                    let images = bricks
                        .iter()
                        .map(|brick| world.read(brick).map(|image| (brick, image)))
                        .collect::<Result<Vec<_>, _>>();
                    let images = match images {
                        Ok(images) => images,
                        Err(reason) => return failed(reason),
                    };
                    let mut lost = match change {
                        Change::Terrain(edit) => match world.edit(edit) {
                            Ok(()) => None,
                            Err(reason) => return failed(reason),
                        },
                        Change::Block(block) => match world.place(block, actor) {
                            Ok(()) => None,
                            Err(reason) => return failed(reason),
                        },
                        Change::Voxels(paste) => write_over(world, &images, |brick, image| {
                            paste.overlay(brick, image);
                        }),
                        Change::Erase(erase) => {
                            let mut cleared = 0;
                            let lost = write_over(world, &images, |brick, image| {
                                cleared += erase.overlay(brick, image);
                            });
                            if lost.is_none() && cleared > 0 {
                                world.erased(erase, actor);
                            }
                            lost
                        }
                        Change::PlaceProp(_) | Change::RemoveProp { .. } => None,
                    };
                    for (brick, _) in &images {
                        match world.read(*brick) {
                            Ok(now) => {
                                work.now.insert(*brick, hash(&now));
                            }
                            Err(reason) => {
                                work.now.remove(brick);
                                lost.get_or_insert(reason);
                            }
                        }
                    }
                    work.done.push(Done::Bricks(images));
                    if let Some(reason) = lost {
                        pass.spent += cost;
                        return failed(reason);
                    }
                }
            }
            pass.spent += cost;
        }
        work.read_settings(world, pass)
    }

    fn take_back(&mut self, world: &mut impl Voxels, pass: &mut Pass) -> Outcome {
        let work = &mut self.work;
        if !work.undo_checked {
            if let Err((reason, holds)) = work.undo_check(world) {
                return work.refuse_undo(reason, holds);
            }
            work.undo_checked = true;
            let placed: Vec<u32> = work
                .done
                .iter()
                .filter_map(|done| match done {
                    Done::Placed { entity, .. } => Some(*entity),
                    _ => None,
                })
                .collect();
            for entity in placed {
                if let Err(reason) = world.hold_prop(entity) {
                    return work.refuse_undo(reason, 0);
                }
                work.held.push(entity);
            }
            if !work.held.is_empty() {
                return Outcome::Later;
            }
        }
        while let Some(done) = work.done.last_mut() {
            if pass.spent >= pass.budget {
                return Outcome::Again;
            }
            match done {
                Done::Bricks(bricks) => {
                    let Some((brick, image)) = bricks.last() else {
                        work.done.pop();
                        continue;
                    };
                    let brick = *brick;
                    let expected = work.now.get(&brick).copied();
                    let result = world.read(brick).and_then(|now| {
                        if Some(hash(&now)) == expected {
                            world.write(brick, image)
                        } else {
                            Err(Status::REFUSED)
                        }
                    });
                    match result {
                        Ok(()) => {
                            work.now.insert(brick, hash(image));
                            bricks.pop();
                            pass.spent += 1;
                            work.waits = 0;
                        }
                        Err(Status::NOT_LOADED) => {
                            work.waits += 1;
                            if work.waits >= pass.waits {
                                return work.refuse_undo(Status::NOT_LOADED, 0);
                            }
                            return Outcome::Later;
                        }
                        Err(reason) => return work.refuse_undo(reason, 0),
                    }
                }
                Done::Placed {
                    entity,
                    prop,
                    settings,
                } => {
                    let entity = *entity;
                    if let Err((reason, holds)) = still_placed(world, entity, prop, settings) {
                        return work.refuse_undo(reason, holds);
                    }
                    if let Err(reason) = world.still_held(entity) {
                        return work.refuse_undo(reason, 0);
                    }
                    if let Err(reason) = world.remove_prop(entity, work.actor.unwrap_or(0)) {
                        return work.refuse_undo(reason, 0);
                    }
                    work.held.retain(|&held| held != entity);
                    work.done.pop();
                    pass.spent += 1;
                }
                &mut Done::Removed { prop, holds } => {
                    if let Err((reason, holds)) = still_removed(world, &prop, holds) {
                        return work.refuse_undo(reason, holds);
                    }
                    if let Err(reason) = world.spawn_prop(&prop, work.actor.unwrap_or(0)) {
                        return work.refuse_undo(reason, 0);
                    }
                    work.done.pop();
                    pass.spent += 1;
                }
            }
        }
        work.now.clear();
        work.holds = 0;
        Outcome::Done(TransactionInfo::UNDONE, Status::OK)
    }
}
