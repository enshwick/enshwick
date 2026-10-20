//! A hook shared by a hosting client's two worlds tells them apart.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick_api::abi::{Side, Status};
    use enshwick_api::ecs::Worlds;
    use enshwick_api::{Load, Mod, Off};

    static WORLDS: OnceLock<Worlds> = OnceLock::new();

    struct Shared;

    impl Mod for Shared {
        fn manifest() -> &'static enshwick_api::abi::Manifest {
            enshwick_api::manifest!(id: "shared", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = WORLDS.set(Worlds::resolve(load)?);
            Ok(Self)
        }
    }

    enshwick_api::declare_mod!(Shared);

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
        let loader = enshwick_test::FakeHost::new()
            .side(Side::CLIENT)
            .hosting(true)
            .server_world(SERVER_WORLD)
            .constant("ecs.context_entity_world", 0x30)
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let worlds = WORLDS.get().expect("resolved at init");
        assert!(worlds.hosting());

        let (_server_descriptor, mut server) = context(SERVER_WORLD);
        let (_client_descriptor, mut client) = context(0x1000);
        // SAFETY: each context points at a live descriptor.
        assert!(unsafe { worlds.is_server(server.as_mut_ptr()) });
        // SAFETY: as above.
        assert!(!unsafe { worlds.is_server(client.as_mut_ptr()) });
        // SAFETY: null is refused without a read.
        assert!(!unsafe { worlds.is_server(core::ptr::null_mut()) });
    }
}
