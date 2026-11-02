//! A mod built for an older ABI minor starts on the table of a loader of a
//! newer one, and on no table of another major; the fake reads and writes
//! its structs only as far as their headers say, as the loader does.

#[cfg(test)]
mod tests {
    use enshwick::abi::{
        ABI_MAJOR, ABI_MINOR, Headed, Header, HostApi, PlayerInfo, SpawnRequest, Status,
    };
    use enshwick::test::{FakeHost, Installed};

    struct Older;

    impl enshwick::Mod for Older {
        fn init(_: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Older,
        r#"
        id = "you.older-minor"
        side = "server"
        "#
    );

    /// The fake's table as a loader of ABI `major`.`minor` writes it, with a
    /// function appended that this build does not know.
    #[repr(C)]
    struct Later {
        api: HostApi,
        appended: usize,
    }

    /// `loader`'s table, restated as a loader of ABI `major`.`minor` that
    /// appended one function writes it, for the rest of the process.
    fn table(loader: &Installed, major: u16, minor: u16) -> &'static HostApi {
        // SAFETY: the fake's table is a static, aligned and never written.
        let mut api = unsafe { *loader.api() };
        api.header.abi_major = major;
        api.header.abi_minor = minor;
        api.header.size = u32::try_from(size_of::<Later>()).unwrap();
        &Box::leak(Box::new(Later { api, appended: 0 })).api
    }

    #[test]
    fn a_mod_of_an_older_minor_starts_on_a_newer_loader_and_not_on_another_major() {
        let loader = FakeHost::new().install();
        // SAFETY: `api` is a table of the fake's functions that lives for the
        // process, and the handle is the fake's.
        let init = |api: &'static HostApi| unsafe { enshwick_mod_init(api, loader.handle()) };
        assert_eq!(init(table(&loader, ABI_MAJOR + 1, 0)), Status::BAD_ARGS);
        assert!(Older::instance().is_none());
        assert_eq!(init(table(&loader, ABI_MAJOR, ABI_MINOR + 1)), Status::OK);
        assert!(Older::instance().is_some());
    }

    /// A spawn request as a mod sends it, with bytes after it that a reader
    /// may not touch.
    #[repr(C)]
    struct Sent {
        request: SpawnRequest,
        after: [u8; 16],
    }

    fn spawn_request() -> SpawnRequest {
        SpawnRequest {
            header: Header::of::<SpawnRequest>(),
            template: [0x3a; 16],
            position: [1, 2, 3],
            orientation: [0.0, 0.0, 0.0, 1.0],
            level: 12,
            reserved: 0,
            lifetime_ns: 0,
        }
    }

    #[test]
    fn a_mod_s_request_needs_only_what_every_writer_of_the_major_knows() {
        let loader = FakeHost::new().install();
        // SAFETY: the fake's table is a static, aligned and never written.
        let api = unsafe { &*loader.api() };
        let born = <SpawnRequest as Headed>::BORN;
        let mut sent = Sent {
            request: spawn_request(),
            after: [0xAB; 16],
        };
        sent.request.header.size = u32::try_from(born).unwrap();
        let mut entity = enshwick::Entity(0);
        let status = (api.spawn)(loader.handle(), &raw const sent.request, &raw mut entity);
        assert_eq!(status, Status::OK);
        let spawned = loader.spawns();
        assert_eq!(spawned.last().map(|(_, r)| r.level), Some(12));
        sent.request.header.size = u32::try_from(born - 1).unwrap();
        let status = (api.spawn)(loader.handle(), &raw const sent.request, &raw mut entity);
        assert_eq!(status, Status::BAD_ARGS, "short of what every writer knows");
        assert_eq!(loader.spawns().len(), spawned.len());
    }

    #[test]
    fn a_mod_s_shorter_list_entries_are_filled_only_as_far_as_they_reach() {
        let player = |slot: u32| PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot,
            entity: enshwick::Entity(100 + slot),
            key: enshwick::PlayerKey::new(u64::from(slot) + 1),
            permissions: 0,
            name_len: 0,
            name: [0; 64],
        };
        let loader = FakeHost::new()
            .player(player(0))
            .player(player(1))
            .install();
        // SAFETY: the fake's table is a static, aligned and never written.
        let api = unsafe { &*loader.api() };
        // An older mod's entries end before `name`, as if a later minor had
        // appended it.
        let stride = core::mem::offset_of!(PlayerInfo, name).next_multiple_of(8);
        let mut room = vec![0xABAB_ABAB_ABAB_ABABu64; (2 * stride + 16) / 8];
        for entry in 0..2 {
            let header = &mut room[entry * stride / 8];
            *header = (*header & !0xFFFF_FFFF) | u64::try_from(stride).unwrap();
        }
        let mut count = 0;
        let out = room.as_mut_ptr().cast::<PlayerInfo>();
        let status = (api.players)(loader.handle(), out, 2, &raw mut count);
        assert_eq!((status, count), (Status::OK, 2));
        let written: Vec<u8> = room.iter().flat_map(|word| word.to_le_bytes()).collect();
        let entity_at = core::mem::offset_of!(PlayerInfo, entity);
        for entry in 0..2 {
            let at = entry * stride + entity_at;
            let entity = u32::from_le_bytes(written[at..at + 4].try_into().unwrap());
            assert_eq!(entity, 100 + u32::try_from(entry).unwrap());
        }
        assert!(
            written[2 * stride..].iter().all(|&b| b == 0xAB),
            "written past the list"
        );
        let mut none: Vec<PlayerInfo> = Vec::new();
        let status = (api.players)(loader.handle(), none.as_mut_ptr(), 0, &raw mut count);
        assert_eq!(
            (status, count),
            (Status::OK, 2),
            "an empty list's dangling pointer"
        );
    }
}
