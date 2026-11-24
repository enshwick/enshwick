//! The fake's buff entries: the buff types and what each holder holds.

use crate::test::table::base::args;
use crate::test::table::entries;
use crate::test::table::{get_headed, list, with};
use enshwick_core::abi::{ActiveBuff, BuffRequest, BuffTypeAdd, BuffTypeInfo, Header, Status};
use enshwick_core::buffs::BuffTypeId;

entries! {
    /// Adds a mod's buff type, as the loader: refused after init, for a mod
    /// clients do not run, for a GUID the mod did not declare the content id
    /// of, for a request it does not know, a `like` the
    /// vanilla collection lacks and an id the game or a mod before has; the
    /// first type serves the fake's one patch of the collection, before every
    /// mod's patch of it.
    pub(crate) fn buff_type_add(
        me as Mod,
        buff: *const BuffTypeAdd as args::Headed,
        id: *mut BuffTypeId as args::Out,
    ) -> Status {
        let Some(buff) = get_headed(buff) else {
            return Status::BAD_ARGS;
        };
        if id.is_null() {
            return Status::BAD_ARGS;
        }
        match served::add(me, &buff) {
            Ok(added) => {
                // SAFETY: `id` is non-null (checked above) and the safe layer
                // passes a writable id for the call.
                unsafe { id.write(added) };
                Status::OK
            }
            Err(status) => status,
        }
    }
}

/// The fake's buff patch of the collection, built as the loader builds its
/// own.
#[cfg(feature = "unstable")]
mod served {
    use enshwick_core::__fake::rules::buff_types::{Edit, Refusal, build, check};
    use enshwick_core::abi::{BuffTypeAdd, ModHandle, ResourceKey, Status};
    use enshwick_core::buffs::{BUFF_COLLECTION, BuffLayout, BuffTypeId};
    use enshwick_core::content::EngineFacts as _;
    use enshwick_core::content::Guid;

    use crate::test::table::base::registered::Registered;
    use crate::test::table::{State, with};
    use crate::test::{FakeHost, ServedResource};

    /// The layout and key of the fake's buff collection.
    fn collection(host: &FakeHost) -> Option<(BuffLayout, ResourceKey)> {
        let layout = BuffLayout::resolve(host).ok()?;
        let hash = host
            .hash(enshwick_core::__fake::ids::BUFF_COLLECTION)
            .ok()?;
        Some((layout, BUFF_COLLECTION.key_by_hash(hash)))
    }

    /// Checks and adds the type `buff` of `me` asks for: its game id.
    pub(super) fn add(me: ModHandle, buff: &BuffTypeAdd) -> Result<BuffTypeId, Status> {
        with(Err(Status::MISSING), |s| {
            // In the loader's order: the seal, the mod's side, the declared
            // id, the request.
            if s.base.closed() {
                return Err(Status::TOO_LATE);
            }
            if !crate::test::table::base::mods::joins(s, me)
                || !crate::test::table::content::declared(s, me, buff.guid)
            {
                return Err(Status::REFUSED);
            }
            let (layout, key) = collection(&s.host).ok_or(Status::UNAVAILABLE)?;
            let edit = Edit::of(buff).ok_or(Status::BAD_ARGS)?;
            let vanilla = s
                .host
                .content
                .resources
                .get(&(key.guid, key.type_hash, key.part))
                .ok_or(Status::UNAVAILABLE)?;
            let added: Vec<Edit> = s.world.buff_types_added.iter().map(|(_, e)| *e).collect();
            check(&layout, vanilla, &edit, &added).map_err(Refusal::status)?;
            if s.world.buff_types_added.is_empty() {
                serve(s, key);
            }
            s.world.buff_types_added.push((me, edit));
            Ok(edit.id())
        })
    }

    /// Serves the fake's patch of the collection `key`, before every mod's.
    fn serve(s: &mut State, key: ResourceKey) {
        let patch = Registered::unowned(
            patch as enshwick_core::abi::ResourcePatchFn,
            core::ptr::null_mut(),
        );
        let at = s
            .content
            .served
            .iter()
            .position(|served| served.0.key == key)
            .unwrap_or(s.content.served.len());
        s.content
            .served
            .insert(at, (ServedResource { key, base: key }, patch, None));
    }

    /// The types served now: those of mods still on, each naming its icon
    /// only while the fake shows served UI images and serves that image.
    fn serving(s: &State) -> Vec<Edit> {
        let shows = |icon: Guid| {
            s.host.content.icons_ready
                && s.content
                    .served
                    .iter()
                    .any(|(served, ..)| served.key.guid == icon)
        };
        s.world
            .buff_types_added
            .iter()
            .filter(|(owner, _)| !s.base.off.contains(owner))
            .map(|(_, edit)| match edit.icon() {
                Some(icon) if !shows(icon) => edit.without_icon(),
                _ => *edit,
            })
            .collect()
    }

    /// The patch: the collection with every mod's types, in a buffer the
    /// build lends; [`Status::MISSING`] when it does not read.
    unsafe extern "system" fn patch(
        base: *const u8,
        base_len: u32,
        _user: *mut core::ffi::c_void,
        out: *mut *mut u8,
        out_len: *mut u32,
    ) -> Status {
        if base.is_null() || out.is_null() || out_len.is_null() {
            return Status::BAD_ARGS;
        }
        // SAFETY: the fake's build passes `base_len` readable bytes at
        // `base`, unchanged for the call.
        let base = unsafe { core::slice::from_raw_parts(base, base_len as usize) };
        // As the loader: a build that asks before icons show is built again
        // once they show.
        if !with(false, |s| s.host.content.icons_ready) {
            crate::test::table::content::note_icons_read();
        }
        let Some((layout, types)) = with(None, |s| Some((collection(&s.host)?.0, serving(s))))
        else {
            return Status::MISSING;
        };
        let mut built = Vec::new();
        if build(&layout, base, &types, &mut built).is_none() {
            return Status::MISSING;
        }
        let Ok(len) = u32::try_from(built.len()) else {
            return Status::BAD_ARGS;
        };
        let mut lent = core::ptr::null_mut();
        let status = crate::test::table::resource_buffer(ModHandle(0), len, &raw mut lent);
        if !status.is_ok() {
            return status;
        }
        // SAFETY: the build lent `len` writable bytes at `lent`, apart from
        // `built`, which holds `len` bytes.
        unsafe { core::ptr::copy_nonoverlapping(built.as_ptr(), lent, built.len()) };
        // SAFETY: `out` is the build's out pointer, non-null by the check at
        // the top and writable for the call.
        unsafe { out.write(lent) };
        // SAFETY: `out_len` is the build's out length, non-null by the check
        // at the top and writable for the call.
        unsafe { out_len.write(len) };
        Status::OK
    }
}

/// Without the `unstable` feature the fake has no buff collection.
#[cfg(not(feature = "unstable"))]
mod served {
    use enshwick_core::abi::{BuffTypeAdd, ModHandle, Status};
    use enshwick_core::buffs::BuffTypeId;

    pub(super) const fn add(_owner: ModHandle, _buff: &BuffTypeAdd) -> Result<BuffTypeId, Status> {
        Err(Status::UNAVAILABLE)
    }
}

entries! {
    pub(crate) fn buff_types(
        me as Caller,
        [out: *mut BuffTypeInfo, entry_size: u32, capacity: u32, count: *mut u32] as args::SizedListOut,
    ) -> Status {
        let types = with(Vec::new(), |s| s.world.buff_types.clone());
        list(&types, out, entry_size, capacity, count)
    }

    pub(crate) fn buffs(
        me as Caller,
        entity: enshwick_core::Entity,
        [out: *mut ActiveBuff, entry_size: u32, capacity: u32, count: *mut u32] as args::SizedListOut,
    ) -> Status {
        match with(None::<Vec<ActiveBuff>>, |s| {
            s.world.buffs.get(&entity.0).cloned()
        }) {
            Some(on) => list(&on, out, entry_size, capacity, count),
            None => Status::MISSING,
        }
    }

    pub(crate) fn apply_buff(me as Caller, request: *const BuffRequest as args::Headed) -> Status {
        let Some(request) = get_headed(request) else {
            return Status::BAD_ARGS;
        };
        with(Status::MISSING, |s| {
            let Some(ty) = s
                .world
                .buff_types
                .iter()
                .find(|ty| ty.id == request.id)
                .copied()
            else {
                return Status::BAD_ARGS;
            };
            let Some(on) = s.world.buffs.get_mut(&request.entity.0) else {
                return Status::MISSING;
            };
            let duration = if request.duration_ns == 0 {
                ty.duration_ns
            } else {
                request.duration_ns
            };
            if let Some(buff) = on.iter_mut().find(|buff| buff.id == request.id) {
                buff.duration_ns = duration;
                buff.remaining_ns = duration;
            } else {
                on.push(ActiveBuff {
                    header: Header::of::<ActiveBuff>(),
                    id: request.id,
                    entity: enshwick_core::Entity(0x8000 + u32::try_from(on.len()).unwrap_or(0)),
                    slot: u32::from(ty.slot),
                    level: request.level,
                    duration_ns: duration,
                    remaining_ns: duration,
                });
            }
            Status::OK
        })
    }

    pub(crate) fn remove_buff(
        me as Caller,
        entity: enshwick_core::Entity,
        id: BuffTypeId,
        removed: *mut u8 as args::Out,
    ) -> Status {
        if removed.is_null() {
            return Status::BAD_ARGS;
        }
        with(Status::MISSING, |s| {
            let Some(on) = s.world.buffs.get_mut(&entity.0) else {
                return Status::MISSING;
            };
            let before = on.len();
            on.retain(|buff| buff.id != id);
            // SAFETY: `removed` is non-null, checked at the top; the safe layer
            // passes it pointing at a writable byte for the call.
            unsafe { removed.write(u8::from(on.len() != before)) };
            Status::OK
        })
    }
}
