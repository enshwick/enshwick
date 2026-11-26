//! Every raw call that adds content by GUID refuses a GUID the calling mod
//! did not declare: a mod passing another mod's GUID is refused, while that
//! mod's own call with it succeeds; a changed key, the game's content and
//! the old GUIDs of a mod's own renames need no declaration. A mod that
//! declares a GUID another renamed from earlier keeps it, and that rename
//! goes.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{
        ContentRename, HostApi, ModHandle, ResourceKey, Side, Status, TerrainMaterialAdd,
    };
    use enshwick::content::{Guid, Id};
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Load, Mod, Off};

    /// A mod loaded before Bells, which passes Bells' GUID as its own.
    const EARLIER: ModHandle = ModHandle(77);
    /// Bells' content id, which only Bells declares.
    const BELL: Id = Id::new("you.bells:bell");
    /// A GUID Bells' content had before.
    const OLD_BELL: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
    /// A GUID the earlier mod's content had before.
    const OLD_GONG: Guid = Guid::parse("1c0d9e5a-7b2f-4e3a-8d61-5f4a2b9c7e10");
    /// A GUID no mod declared.
    const LOOSE: Guid = Guid([5; 16]);
    /// A resource of the game's own.
    const GAME: ResourceKey = ResourceKey {
        guid: Guid([3; 16]),
        type_hash: 7,
        part: 0,
    };
    /// The game's sand, as a terrain id.
    const SAND: u8 = 9;

    /// A raw call that adds content by GUID.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Call {
        ServeResource,
        TerrainMaterialAdd,
        CraftingProp,
        RenameContent,
    }

    const CALLS: [Call; 4] = [
        Call::ServeResource,
        Call::TerrainMaterialAdd,
        Call::CraftingProp,
        Call::RenameContent,
    ];

    /// What Bells' own calls answered, in [`CALLS`] order.
    static OWN: Mutex<Vec<Status>> = Mutex::new(Vec::new());
    /// What the earlier mod's calls with Bells' GUID answered, in [`CALLS`]
    /// order.
    static EARLIER_ANSWERS: Mutex<Vec<Status>> = Mutex::new(Vec::new());
    /// What the earlier mod's renames from Bells' GUID and in a cycle
    /// answered.
    static REFUSED_RENAMES: Mutex<Vec<Status>> = Mutex::new(Vec::new());
    /// What the earlier mod's chain of renames through Bells' GUID and its
    /// serve under it answered before Bells declared it, then that serve
    /// again and a serve under the chain's start after.
    static CLAIMED: Mutex<Vec<Status>> = Mutex::new(Vec::new());
    /// What the earlier mod's calls that need no declaration answered: a
    /// changed game key, a new key from a game key, a rename onto a game
    /// GUID, and a key under the old GUID of that rename.
    static EXEMPT: Mutex<Vec<Status>> = Mutex::new(Vec::new());

    /// A patch that builds nothing, so the game keeps its base.
    unsafe extern "system" fn unbuilt(
        _base: *const u8,
        _base_len: u32,
        _user: *mut core::ffi::c_void,
        _out: *mut *mut u8,
        _out_len: *mut u32,
    ) -> Status {
        Status::REFUSED
    }

    fn serve(api: &HostApi, me: ModHandle, key: ResourceKey, base: ResourceKey) -> Status {
        (api.serve_resource)(
            me,
            &raw const key,
            &raw const base,
            Some(unbuilt),
            core::ptr::null_mut(),
        )
    }

    fn rename(api: &HostApi, me: ModHandle, list: &[(Guid, Guid)]) -> Status {
        let renames: Vec<_> = list
            .iter()
            .map(|&(from, to)| ContentRename { from, to })
            .collect();
        let count = u32::try_from(renames.len()).unwrap();
        (api.rename_content)(me, renames.as_ptr(), count)
    }

    /// Makes `call` as mod `me` with Bells' GUID.
    fn make(api: &HostApi, me: ModHandle, call: Call) -> Status {
        let guid = BELL.guid();
        match call {
            Call::ServeResource => serve(api, me, ResourceKey { guid, ..GAME }, GAME),
            Call::TerrainMaterialAdd => {
                let material = enshwick::abi::headed!(TerrainMaterialAdd {
                    guid,
                    item: Guid::NIL,
                    health: 0,
                    like: SAND,
                    fallback: SAND,
                    changes: 0,
                    hardness: 0,
                    flags: 0,
                    reserved: [0; 3],
                });
                let mut id = 0u8;
                (api.terrain_material_add)(me, &raw const material, &raw mut id)
            }
            Call::CraftingProp => {
                let mut index = 0u8;
                (api.crafting_prop)(me, &raw const guid, &raw mut index)
            }
            Call::RenameContent => rename(api, me, &[(OLD_BELL, guid)]),
        }
    }

    struct Bells;

    impl Mod for Bells {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.declare(BELL)?;
            let host = enshwick::host().ok_or_else(|| Off::new("no host"))?;
            *OWN.lock().unwrap() = CALLS
                .iter()
                .map(|&call| make(host.api(), host.handle(), call))
                .collect();
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Bells,
        r#"
        id = "you.bells"
        side = "both"
        "#
    );

    /// Bells' init, then the earlier mod's calls, as a C mod would make them
    /// before the mods' registrations close.
    unsafe extern "system" fn earlier_after(api: *const HostApi, me: ModHandle) -> Status {
        // SAFETY: the init export of this file's `declare_mod!(Bells)`, with
        // the arguments the fake passed.
        let status = unsafe { enshwick_mod_init(api, me) };
        // SAFETY: the fake passes its table, alive for the call.
        let api = unsafe { &*api };
        earlier_calls(api);
        *REFUSED_RENAMES.lock().unwrap() = vec![
            rename(api, EARLIER, &[(BELL.guid(), GAME.guid)]),
            rename(api, EARLIER, &[(OLD_GONG, LOOSE), (LOOSE, OLD_GONG)]),
        ];
        let new_from_game = ResourceKey {
            guid: Guid([4; 16]),
            ..GAME
        };
        let old_gong = ResourceKey {
            guid: OLD_GONG,
            ..GAME
        };
        *EXEMPT.lock().unwrap() = vec![
            serve(api, EARLIER, GAME, GAME),
            serve(api, EARLIER, GAME, new_from_game),
            rename(api, EARLIER, &[(OLD_GONG, GAME.guid)]),
            serve(api, EARLIER, old_gong, GAME),
        ];
        status
    }

    /// The earlier mod's calls with Bells' GUID, then Bells' init, as a mod
    /// loaded first makes them before Bells declares its id.
    unsafe extern "system" fn earlier_before(api: *const HostApi, me: ModHandle) -> Status {
        // SAFETY: the fake passes its table, alive for the call.
        earlier_calls(unsafe { &*api });
        // SAFETY: the init export of this file's `declare_mod!(Bells)`, with
        // the arguments the fake passed.
        unsafe { enshwick_mod_init(api, me) }
    }

    /// Bells' key, as the earlier mod serves it from another base.
    const BELL_KEY: ResourceKey = ResourceKey {
        guid: BELL.guid(),
        ..GAME
    };

    /// The earlier mod renames its old GUID through Bells' onto the game's
    /// and serves Bells' key from another base, both before Bells declares
    /// its id; then Bells' init, then that serve again and one under the
    /// earlier mod's old GUID, which its chain still renames.
    unsafe extern "system" fn renamer_before(api: *const HostApi, me: ModHandle) -> Status {
        // SAFETY: the fake passes its table, alive for the call.
        let table = unsafe { &*api };
        let other_base = ResourceKey {
            guid: Guid([4; 16]),
            ..GAME
        };
        let chain = [(OLD_GONG, BELL.guid()), (BELL.guid(), GAME.guid)];
        let mut answers = vec![
            rename(table, EARLIER, &chain),
            serve(table, EARLIER, BELL_KEY, other_base),
        ];
        // SAFETY: the init export of this file's `declare_mod!(Bells)`, with
        // the arguments the fake passed.
        let status = unsafe { enshwick_mod_init(api, me) };
        answers.push(serve(table, EARLIER, BELL_KEY, other_base));
        let old_gong = ResourceKey {
            guid: OLD_GONG,
            ..GAME
        };
        answers.push(serve(table, EARLIER, old_gong, other_base));
        *CLAIMED.lock().unwrap() = answers;
        status
    }

    fn earlier_calls(api: &HostApi) {
        *EARLIER_ANSWERS.lock().unwrap() =
            CALLS.iter().map(|&call| make(api, EARLIER, call)).collect();
    }

    /// Starts Bells with `init` running its init and the earlier mod's calls.
    fn start(init: unsafe extern "system" fn(*const HostApi, ModHandle) -> Status) -> Installed {
        let loader = FakeHost::new()
            .props()
            .resource(GAME, vec![1])
            .pack(EARLIER, Side::BOTH)
            .install();
        // SAFETY: the query and ready exports of this file's
        // `declare_mod!(Bells)`, with an init that runs Bells' first.
        let status = unsafe { loader.start(enshwick_mod_query, init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    /// Bells' answer and the earlier mod's for `call` with Bells' GUID.
    fn answers(call: Call) -> (Status, Status) {
        let _loader = start(earlier_after);
        let at = CALLS.iter().position(|&c| c == call).unwrap();
        (OWN.lock().unwrap()[at], EARLIER_ANSWERS.lock().unwrap()[at])
    }

    #[test]
    fn serve_resource_refuses_a_new_key_under_a_guid_another_mod_declared() {
        assert_eq!(answers(Call::ServeResource), (Status::OK, Status::REFUSED));
    }

    #[test]
    fn terrain_material_add_refuses_a_guid_another_mod_declared() {
        assert_eq!(
            answers(Call::TerrainMaterialAdd),
            (Status::OK, Status::REFUSED)
        );
    }

    #[test]
    fn crafting_prop_refuses_a_guid_another_mod_declared() {
        assert_eq!(answers(Call::CraftingProp), (Status::OK, Status::REFUSED));
    }

    #[test]
    fn rename_content_refuses_a_new_guid_another_mod_declared() {
        assert_eq!(answers(Call::RenameContent), (Status::OK, Status::REFUSED));
    }

    #[test]
    fn every_call_refuses_bells_guid_to_a_mod_that_calls_before_bells_declares_it() {
        let _loader = start(earlier_before);
        assert_eq!(*EARLIER_ANSWERS.lock().unwrap(), [Status::REFUSED; 4]);
        assert_eq!(*OWN.lock().unwrap(), [Status::OK; 4]);
    }

    #[test]
    fn a_rename_from_another_mod_s_guid_or_in_a_cycle_is_refused() {
        let loader = start(earlier_after);
        assert_eq!(*REFUSED_RENAMES.lock().unwrap(), [Status::REFUSED; 2]);
        assert_eq!(*OWN.lock().unwrap(), [Status::OK; 4]);
        assert_eq!(
            loader.content_renames(),
            [(OLD_BELL, BELL.guid())],
            "Bells keeps its own rename"
        );
    }

    #[test]
    fn a_mod_declaring_its_id_after_another_renamed_from_it_keeps_it_and_the_rename_goes() {
        let loader = start(renamer_before);
        assert_eq!(
            *CLAIMED.lock().unwrap(),
            [Status::OK, Status::OK, Status::REFUSED, Status::OK]
        );
        assert_eq!(*OWN.lock().unwrap(), [Status::OK; 4]);
        let bases: Vec<ResourceKey> = loader
            .resources()
            .iter()
            .filter(|served| served.key == BELL_KEY)
            .map(|served| served.base)
            .collect();
        assert_eq!(bases, [GAME], "only Bells serves its key");
    }

    #[test]
    fn a_late_call_is_too_late_before_its_guid_is_looked_at() {
        let loader = start(earlier_after);
        // SAFETY: the fake's table is a static, aligned and never written.
        let api = unsafe { &*loader.api() };
        for call in [Call::ServeResource, Call::TerrainMaterialAdd] {
            assert_eq!(make(api, EARLIER, call), Status::TOO_LATE, "{call:?}");
        }
    }

    #[test]
    fn a_changed_key_the_game_s_content_and_an_old_guid_of_the_mod_s_renames_need_no_declaration() {
        let _loader = start(earlier_after);
        assert_eq!(*EXEMPT.lock().unwrap(), [Status::OK; 4]);
    }
}
