//! Mod channels: typed messages out and in, through the fake loader.

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use enshwick_loader_api::abi::{NetMessage, Status};
    use enshwick_loader_api::net::{Channel, Peer, SendError};
    use enshwick_loader_api::{Load, Mod, Off};

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    enum Sync {
        Marker { at: [i32; 3] },
        Text(String),
    }

    static CHANNEL: OnceLock<Channel<Sync>> = OnceLock::new();
    static HEARD: Mutex<Vec<(Peer, Sync)>> = Mutex::new(Vec::new());

    struct Talker;

    impl Mod for Talker {
        fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
            enshwick_loader_api::manifest!(id: "talker", side: BOTH, join: true)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let channel = load.channel::<Sync, _>("sync", 2, |from, message| {
                HEARD.lock().unwrap().push((from, message));
            })?;
            let _first = CHANNEL.set(channel);
            Ok(Self)
        }
    }

    enshwick_loader_api::declare_mod!(Talker);

    #[test]
    fn messages_encode_out_decode_in_and_wait_for_ready_peers() {
        let loader = enshwick_loader_api::test::FakeHost::new()
            .net_peer(Peer::Player(7))
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let channel = CHANNEL.get().unwrap();
        assert!(channel.is_ready(Peer::Player(7)));
        assert!(!channel.is_ready(Peer::Server));
        let marker = Sync::Marker { at: [1, -2, 3] };
        channel.send(Peer::Player(7), &marker).unwrap();
        assert_eq!(
            channel.send(Peer::Player(8), &marker),
            Err(SendError::NotReady)
        );
        assert!(!channel.is_ready(Peer::Player(0)));
        assert_eq!(
            channel.send(Peer::Player(0), &marker),
            Err(SendError::NotReady),
            "key 0 would reach every client"
        );
        assert_eq!(
            channel.broadcast(&Sync::Text("x".repeat(5000))),
            Err(SendError::TooLarge)
        );
        let sent = loader.net_sent();
        assert_eq!(sent.len(), 1);
        assert_eq!((sent[0].0.as_str(), sent[0].1), ("sync@2", 7));
        assert_ne!(sent[0].1, NetMessage::SERVER);

        assert!(loader.net_deliver("sync@2", Peer::Server, &Sync::Text("hi".into())));
        assert!(!loader.net_deliver("sync@1", Peer::Server, &Sync::Text("old".into())));
        assert_eq!(
            HEARD.lock().unwrap().as_slice(),
            [(Peer::Server, Sync::Text("hi".into()))]
        );
    }
}
