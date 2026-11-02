//! The checks before a transaction or its undo writes: extent, rights, columns, bricks and props.

use enshwick::abi::{Status, TransactionInfo};
use rustc_hash::FxHashSet;

use super::{Brick, Bricks, Change, Done, Outcome, Pass, Prop, Settings, Voxels, Work, hash};

/// `Ok` while the prop the transaction placed as `entity` is still there
/// as it placed it and holds no state, its start state as it first was;
/// the reason and the kinds of state otherwise.
pub(super) fn still_placed(
    world: &mut impl Voxels,
    entity: u32,
    prop: &Prop,
    settings: &Settings,
) -> Result<(), (Status, u32)> {
    const START: u32 = TransactionInfo::HOLDS_LOGIC_GRAPH_TRIGGER | TransactionInfo::HOLDS_GROWTH;
    match world.read_prop(entity) {
        Ok((now, holds)) if now.same_place(prop) => {
            let holds = if holds & START != 0 && settings_kept(world, entity, settings) {
                holds & !START
            } else {
                holds
            };
            if holds == 0 {
                Ok(())
            } else {
                Err((Status::REFUSED, holds))
            }
        }
        Ok(_) | Err(Status::NOT_A_PROP) => Err((Status::REFUSED, 0)),
        Err(reason) => Err((reason, 0)),
    }
}

/// Whether the start state of `entity` is still the one read the first
/// step it stood in the world.
fn settings_kept(world: &mut impl Voxels, entity: u32, settings: &Settings) -> bool {
    match settings {
        Settings::Read(Some(first)) => {
            matches!(world.start_state(entity), Ok(Some(now)) if now == *first)
        }
        Settings::Read(None) | Settings::Unread => false,
    }
}

/// `Ok` while the prop the transaction removed can come back as it was:
/// it held no state a respawn would lose, and its place is free.
pub(super) fn still_removed(
    world: &mut impl Voxels,
    prop: &Prop,
    holds: u32,
) -> Result<(), (Status, u32)> {
    if holds != 0 {
        return Err((Status::REFUSED, holds));
    }
    match world.prop_at(prop) {
        Ok(false) => Ok(()),
        Ok(true) => Err((Status::REFUSED, 0)),
        Err(reason) => Err((reason, 0)),
    }
}

impl Work {
    /// The checks of a transaction's first step, before it writes anything:
    /// inside the world, the props it removes, its actor's rights and its
    /// columns loaded; the outcome that stops it, or `None`.
    pub(super) fn first_check(
        &mut self,
        world: &mut impl Voxels,
        pass: &mut Pass,
    ) -> Option<Outcome> {
        let failed = |reason| Some(Outcome::Done(TransactionInfo::FAILED, reason));
        let inside = pass.extent.map(|extent| {
            self.ops
                .iter()
                .all(|op| op.bricks.is_none_or(|bricks| bricks.inside(extent)))
        });
        if inside != Some(true) {
            return failed(inside.map_or(Status::MISSING, |_| Status::OUTSIDE_WORLD));
        }
        let mut removed = Vec::with_capacity(self.ops.len());
        for op in &self.ops {
            let prop = match op.change {
                Change::RemoveProp {
                    entity,
                    discard_state,
                } => match world.read_prop(entity) {
                    Ok((prop, holds)) if holds == 0 || discard_state => Some(prop),
                    Ok((_, holds)) => {
                        self.holds = holds;
                        return failed(Status::REFUSED);
                    }
                    Err(reason) => return failed(reason),
                },
                _ => None,
            };
            removed.push(prop);
        }
        if let Some(actor) = self.actor {
            for (op, prop) in self.ops.iter().zip(&removed) {
                let Some((min, max)) = op.change.reach(prop.as_ref()) else {
                    return failed(Status::MISSING);
                };
                match world.rights(actor, min, max) {
                    Ok(0) => {}
                    Ok(refusal) => {
                        self.refusal = refusal;
                        return failed(Status::REFUSED);
                    }
                    Err(reason) => return failed(reason),
                }
            }
        }
        let mut columns = FxHashSet::default();
        let bricks = self.ops.iter().filter_map(|op| op.bricks);
        for brick in bricks.flat_map(Bricks::columns) {
            let [x, _, z] = brick.0;
            if columns.insert([x, z]) {
                pass.probes += 1;
                if let Err(reason) = world.loaded(brick) {
                    return failed(reason);
                }
            }
        }
        None
    }

    /// The checks of an undo before it writes anything: every brick holds
    /// what the transaction left, every prop it placed is there without
    /// state, every prop it removed can come back.
    pub(super) fn undo_check(&self, world: &mut impl Voxels) -> Result<(), (Status, u32)> {
        let bricks: FxHashSet<Brick> = self
            .done
            .iter()
            .flat_map(|done| done.bricks().iter().map(|(brick, _)| *brick))
            .collect();
        for brick in bricks {
            match (self.now.get(&brick), world.read(brick)) {
                (Some(left), Ok(now)) if hash(&now) == *left => {}
                (_, Err(reason)) => return Err((reason, 0)),
                _ => return Err((Status::REFUSED, 0)),
            }
        }
        for done in &self.done {
            match done {
                Done::Bricks(_) => {}
                Done::Placed {
                    entity,
                    prop,
                    settings,
                } => still_placed(world, *entity, prop, settings)?,
                Done::Removed { prop, holds } => still_removed(world, prop, *holds)?,
            }
        }
        Ok(())
    }

    /// Holds every prop it removes against players, for the step before
    /// it removes them; whether it holds any.
    pub(super) fn hold_removals(&mut self, world: &mut impl Voxels) -> Result<bool, Status> {
        let removals = self.ops.iter().filter_map(|op| match op.change {
            Change::RemoveProp { entity, .. } => Some(entity),
            _ => None,
        });
        for entity in removals {
            world.hold_prop(entity)?;
            self.held.push(entity);
        }
        Ok(!self.held.is_empty())
    }

    /// Removes the prop `entity` held since the step before, checking
    /// again that it holds no state unless `discard_state`.
    pub(super) fn remove(
        &mut self,
        world: &mut impl Voxels,
        entity: u32,
        discard_state: bool,
        actor: u32,
    ) -> Result<(), Status> {
        let (prop, holds) = world.read_prop(entity)?;
        if holds != 0 && !discard_state {
            self.holds = holds;
            return Err(Status::REFUSED);
        }
        world.still_held(entity)?;
        world.remove_prop(entity, actor)?;
        self.held.retain(|&held| held != entity);
        self.done.push(Done::Removed { prop, holds });
        Ok(())
    }

    /// Reads the start state of every prop it placed once the prop
    /// stands in the world, waiting a step at a time up to the wait limit;
    /// applied once read, or at the limit, with those unread kept unread.
    pub(super) fn read_settings(&mut self, world: &mut impl Voxels, pass: &Pass) -> Outcome {
        let mut waiting = false;
        for done in &mut self.done {
            if let Done::Placed {
                entity,
                settings: settings @ Settings::Unread,
                ..
            } = done
            {
                match world.start_state(*entity) {
                    Ok(bytes) => *settings = Settings::Read(bytes),
                    Err(Status::MISSING) => waiting = true,
                    Err(_) => {}
                }
            }
        }
        if waiting {
            self.waits += 1;
            if self.waits < pass.waits {
                return Outcome::Later;
            }
        }
        self.waits = 0;
        Outcome::Done(TransactionInfo::APPLIED, Status::OK)
    }

    /// Stops an undo for `reason`, with the kinds of state `holds` that
    /// stopped it; asking again checks afresh.
    pub(super) fn refuse_undo(&mut self, reason: Status, holds: u32) -> Outcome {
        self.undo_checked = false;
        self.waits = 0;
        self.holds = holds;
        Outcome::Done(TransactionInfo::UNDO_REFUSED, reason)
    }
}
