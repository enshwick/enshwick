//! A hook shared by a hosting client's two worlds tells them apart.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{Side, Status};
    use enshwick::ecs::Worlds;
    use enshwick::{Load, Mod, Off};

    static WORLDS: OnceLock<Worlds> = OnceLock::new();

    struct Shared;

    impl Mod for Shared {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.shared", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = WORLDS.set(Worlds::resolve(load)?);
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Shared);

    /// A system context whose descriptor names `world` at 0x30.
    fn context(world: u64) -> (Box<[u64; 8]>, Box<[u64; 1]>) {
        let mut descriptor = Box::new([0u64; 8]);
        descriptor[6] = world;
        let ctx = Box::new([descriptor.as_ptr() as u64]);
        (descriptor, ctx)
    }

    #[test]
    fn only_the_built_in_servers_contexts_are_the_servers() {
        const SERVER_WORLD: u64 = 0x2000;
        let loader = enshwick::test::FakeHost::new()
            .side(Side::CLIENT)
            .hosting(true)
            .server_world(SERVER_WORLD)
            .constant("ecs.context_entity_world", 0x30)
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Shared)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let worlds = WORLDS.get().expect("resolved at init");
        assert!(worlds.hosting());

        let (_server_descriptor, mut server) = context(SERVER_WORLD);
        let (_client_descriptor, mut client) = context(0x1000);
        // SAFETY: `server` is a context whose first word points at the live
        // `_server_descriptor`.
        assert!(unsafe { worlds.is_server(server.as_mut_ptr()) });
        // SAFETY: `client` is a context whose first word points at the live
        // `_client_descriptor`.
        assert!(!unsafe { worlds.is_server(client.as_mut_ptr()) });
        // SAFETY: `is_server` accepts a null context and reads nothing.
        assert!(!unsafe { worlds.is_server(core::ptr::null_mut()) });
    }
}
