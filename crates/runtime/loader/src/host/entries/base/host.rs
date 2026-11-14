//! Host functions for the running game: components, templates, hosting, the
//! server's world, features, the status report and game events.

use enshwick_base::Relock;
use enshwick_core::abi::{EventTypeInfo, Header, Level, Side, Status, Str};
use enshwick_core::{Entity, abi as api_frame};

use crate::engine::world::Whose;
use crate::host::layer::guards::in_init;
use crate::host::layer::{Caller, HeadedListOut, Mod, Out, Text, entries};

/// Registers the calling mod's game event handler for the world `whose`.
fn game_event_of(
    whose: Whose,
    me: Mod,
    type_hash: u32,
    callback: api_frame::GameEventFn,
    user: *mut core::ffi::c_void,
) -> Result<(), crate::engine::registry::RegisterError> {
    crate::base::game_events::register(
        me.loader.side,
        whose,
        me.index,
        crate::engine::loader::in_init(me.index.get()),
        type_hash,
        callback,
        user,
    )
}

entries! {
    fn event_types(loader as Caller, [out, capacity, count] as HeadedListOut<EventTypeInfo>) -> Status {
        let types: Vec<EventTypeInfo> = crate::engine::catalogue::event_types(&loader.reflection)
            .filter_map(|ty| {
                Some(EventTypeInfo {
                    header: Header::of::<EventTypeInfo>(),
                    name: Str::new(ty.name()),
                    hash: crate::engine::world::type_hash(ty.va())?,
                    size: ty.size(),
                })
            })
            .collect();
        out.put(&types)
    }

    fn component(
        loader as Caller,
        entity: Entity,
        type_hash: u32,
        out as Out<*const core::ffi::c_void>,
    ) -> Status {
        out.put(crate::engine::world::component(loader, entity, type_hash)?)
    }

    fn entity_template(loader as Caller, entity: Entity, out as Out<[u8; 16]>) -> Status {
        out.put(crate::engine::world::template(loader, entity)?)
    }

    fn hosting() -> u8 {
        Ok(crate::engine::loader::loader().map_or(0, |loader| {
            u8::from(loader.side == Side::SERVER || crate::engine::world::hosting())
        }))
    }

    fn server_world() -> u64 {
        Ok(crate::engine::world::server_world())
    }

    fn feature(me as Mod, name as Text, off as Text) -> Status {
        if name.is_empty() {
            return Err(Status::BAD_ARGS.into());
        }
        let texts = me.record.texts.relock();
        let changed = crate::engine::features::report(
            &mut me.record.features.relock(),
            name,
            Some(off).filter(|off| !off.is_empty()),
            texts.as_ref(),
        );
        drop(texts);
        if let Some(line) = changed {
            let level = if off.is_empty() {
                Level::INFO
            } else {
                Level::WARN
            };
            crate::engine::log::write(level, &me.record.id, &line);
        }
        Ok(())
    }

    fn status(
        me as Mod,
        callback: api_frame::StatusFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        in_init(me.index)?;
        Ok(crate::base::status::register(me.index, callback, user))
    }

    fn client_game_event(
        me as Mod,
        type_hash: u32,
        callback: api_frame::GameEventFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        Ok(game_event_of(Whose::Client, me, type_hash, callback, user)?)
    }

    fn game_event(
        me as Mod,
        type_hash: u32,
        callback: api_frame::GameEventFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        Ok(game_event_of(Whose::Server, me, type_hash, callback, user)?)
    }
}
