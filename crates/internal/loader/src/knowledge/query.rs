//! The game's own answer to a query of its query database, for a player or
//! for the world, through the two thin wrappers every system of the game
//! calls around its query evaluator (server 26430253: player 0x140a2d5f0,
//! world 0x140a2d6b0; client 26430625: 0x140f0aa90, 0x140f0ab50). They
//! take the knowledge object and read its store and query state themselves.
//! The evaluator does not compute an Extern query on the call: it reads the
//! state the game's query pass last wrote, so the answer is the game's
//! as of that pass.

use enshwick_core::Entity;
use enshwick_core::abi::Status;

use super::{Layout, layout, record, store_of};
use crate::host::Loader;
use crate::memory::read;
use crate::pinned;

/// What a wrapper writes: whether it answered (0) and whether the query
/// holds (server 26430253); any other status (no database, a query not
/// evaluated yet, no player record) is no answer.
#[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
#[repr(C)]
struct Answer {
    status: u8,
    gap: [u8; 3],
    zero: u32,
    holds: u8,
    end: [u8; 3],
}

/// Which state of a player the player wrapper reads: the game's quest
/// payment passes `{2, 1}`, the player's own state (server 26430253), from
/// a writable stack slot as here.
#[repr(C)]
struct Mode {
    kind: u32,
    set: u8,
    end: [u8; 3],
}

impl Mode {
    /// The player's own state, as the quest payment reads it.
    const PERSONAL: Self = Self {
        kind: 2,
        set: 1,
        end: [0; 3],
    };
}

type PlayerFn = unsafe extern "system" fn(
    out: *mut Answer,
    knowledge: *const u8,
    query: *const u8,
    player: u32,
    mode: *const Mode,
) -> *mut Answer;
type WorldFn = unsafe extern "system" fn(
    out: *mut Answer,
    knowledge: *const u8,
    query: *const u8,
) -> *mut Answer;

/// The bytes the game's query data takes at most; the build's is 12.
const DATA: usize = 16;

/// The two wrappers and the game's query data of an Extern query that
/// holds, waiting for its id.
#[derive(Debug)]
pub(super) struct Calls {
    player: PlayerFn,
    world: WorldFn,
    data: [u8; DATA],
    id: usize,
}

/// The wrappers and the query data on this build, from its patterns and
/// reflection; `None` turns off only the game's answers.
pub(super) fn calls(loader: &Loader) -> Option<Calls> {
    let function = |name| {
        loader
            .address(name)
            .and_then(|address| usize::try_from(address).ok())
    };
    let (player, world) = (
        function(pinned::knowledge::QUERY_PLAYER)?,
        function(pinned::knowledge::QUERY_WORLD)?,
    );
    let reflection = &loader.reflection;
    let ty = "keen::KnowledgeQueryData";
    let field = |name: &str| {
        reflection
            .field_offset(ty, name)
            .ok()
            .and_then(|v| usize::try_from(v).ok())
    };
    let constant = |enumeration: &str, name: &str| {
        reflection
            .enum_value(enumeration, name)
            .ok()
            .and_then(|v| u8::try_from(v).ok())
    };
    let size = usize::try_from(reflection.type_info(ty).ok()?.size()).ok()?;
    let id = field("knowledgeOrQueryId")?;
    let compare = field("compareValue")?;
    let operator = field("compareOperator")?;
    let kind = field("type")?;
    if !fields_fit(size, id, compare, operator, kind) {
        return None;
    }
    let mut data = [0; DATA];
    data[compare..compare + 4].copy_from_slice(&1u32.to_le_bytes());
    data[operator] = constant("keen::CompareOperator", "Equals")?;
    data[kind] = constant("keen::KnowledgeQueryDataType", "Extern")?;
    Some(Calls {
        // SAFETY: the pattern `knowledge.query_player` finds the game's player
        // wrapper of its query evaluator, of the `PlayerFn` signature on
        // server 26430253 and client 26430625.
        player: unsafe { core::mem::transmute::<usize, PlayerFn>(player) },
        // SAFETY: the pattern `knowledge.query_world` finds the game's world
        // wrapper of its query evaluator, of the `WorldFn` signature on the
        // same builds.
        world: unsafe { core::mem::transmute::<usize, WorldFn>(world) },
        data,
        id,
    })
}

/// Whether the query data's fields lie inside its `size` bytes, at most
/// [`DATA`], and apart from each other: the u32 id and compare value at
/// `id` and `compare`, the u8 operator and kind at `operator` and `kind`,
/// so writing the id never changes what the query asks.
fn fields_fit(size: usize, id: usize, compare: usize, operator: usize, kind: usize) -> bool {
    let spans = [(id, 4), (compare, 4), (operator, 1), (kind, 1)];
    size <= DATA
        && spans.iter().all(|&(at, len)| at + len <= size)
        && spans.iter().enumerate().all(|(i, &(a, a_len))| {
            spans[i + 1..]
                .iter()
                .all(|&(b, b_len)| a + a_len <= b || b + b_len <= a)
        })
}

/// Whether the game query `query` holds for the character `entity`, or for
/// the world for `Entity(0)`, as the game answers it now.
pub(crate) fn holds(loader: &Loader, entity: Entity, query: u32) -> Result<bool, Status> {
    let layout: &Layout = layout(loader).ok_or(Status::MISSING)?;
    let calls = layout.query.as_ref().ok_or(Status::MISSING)?;
    let (game, state) = store_of(layout, crate::world::server()?)?;
    // SAFETY: `game` is the server's game object `store_of` found non-null on
    // the game thread; its knowledge object pointer, non-null by the same
    // call, sits at the build table's `knowledge.object` offset.
    let object: usize = unsafe { read(game, layout.object) };
    let knowledge = core::ptr::with_exposed_provenance(object);
    let mut data = calls.data;
    data[calls.id..calls.id + 4].copy_from_slice(&query.to_le_bytes());
    if entity == Entity(0) {
        answered(|answer| {
            // SAFETY: `knowledge` is the live knowledge object on the game thread
            // at a quiet moment (`world::server`); `answer` and `data`, the game's
            // query data of the build's size, live on this stack for the call.
            unsafe { (calls.world)(answer, knowledge, data.as_ptr()) };
        })
    } else {
        // The evaluator indexes the player records with no bounds check.
        record(layout, state, entity)?;
        let mut mode = Mode::PERSONAL;
        answered(|answer| {
            // SAFETY: `knowledge` is the live knowledge object on the game
            // thread at a quiet moment (`world::server`); `answer`, `data`
            // (the game's query data of the build's size) and `mode` live
            // writable on this stack for the call, `mode` as the game's own
            // callers pass it; `record` checked the character's record
            // exists and is active, which the evaluator indexes unchecked.
            unsafe { (calls.player)(answer, knowledge, data.as_ptr(), entity.0, &raw mut mode) };
        })
    }
}

/// Whether the query holds by what the wrapper `call` writes; a wrapper
/// that returns without writing has not answered.
fn answered(call: impl FnOnce(*mut Answer)) -> Result<bool, Status> {
    let mut answer = Answer {
        status: u8::MAX,
        ..bytemuck::Zeroable::zeroed()
    };
    call(&raw mut answer);
    if answer.status != 0 {
        return Err(Status::MISSING);
    }
    Ok(answer.holds != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrote(status: u8, holds: u8) -> impl FnOnce(*mut Answer) {
        move |out| {
            // SAFETY: `answered` passes its own live answer.
            unsafe {
                out.write(Answer {
                    status,
                    holds,
                    ..bytemuck::Zeroable::zeroed()
                });
            }
        }
    }

    #[test]
    fn a_wrapper_that_writes_nothing_reads_as_missing() {
        assert_eq!(answered(|_| {}), Err(Status::MISSING));
    }

    #[test]
    fn a_wrapper_s_answer_is_read_only_at_status_zero() {
        assert_eq!(answered(wrote(0, 1)), Ok(true));
        assert_eq!(answered(wrote(0, 0)), Ok(false));
        assert_eq!(answered(wrote(1, 1)), Err(Status::MISSING));
    }

    #[test]
    fn query_data_fields_must_fit_and_stay_apart() {
        assert!(fields_fit(12, 0, 4, 8, 9), "the build's layout");
        assert!(
            !fields_fit(12, 0, 2, 8, 9),
            "the id runs into the compare value"
        );
        assert!(!fields_fit(12, 0, 4, 2, 9), "the operator inside the id");
        assert!(!fields_fit(12, 0, 4, 8, 12), "the kind past the end");
        assert!(!fields_fit(12, 0, 4, 8, 8), "operator and kind on one byte");
        assert!(!fields_fit(DATA + 4, 0, 4, 8, 9), "more than the buffer");
    }
}
