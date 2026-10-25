# Messages between server and client

A join mod with a client part, such as one that draws something the server
decides, sends messages between its two halves through a channel.
[`load.channel::<M, _>(name, version, handler)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/net.rs)
opens one during `init`, on both sides, for messages of type `M`, any type
that serde serializes; `handler` runs on each message that comes in, with
who sent it, a `Peer`:

- on the server, `Peer::Player(key)`, the account key of the player whose
  client sent it, as [`Player::key`](world.md#players) gives it; the handler
  runs on the game thread, where world reads work;
- on a client, `Peer::Server`; the handler runs on the thread that takes the
  server's chat, so keep it short and hand work to the mod's own state.

```rust
use enshwick_loader_api::actions::{Action, Outcome};
use enshwick_loader_api::net::Peer;

#[derive(serde::Serialize, serde::Deserialize)]
enum Waypoint {
    Set([f64; 3]),
    Clear,
}

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let waypoints = load.channel::<Waypoint, _>("waypoints", 1, |from, message| {
        if let (Peer::Server, Waypoint::Set(at)) = (from, message) {
            enshwick_loader_api::info!("the server set a waypoint at {at:?}");
        }
    })?;
    let world = load.world()?;
    let actions = load.actions(enshwick_loader_api::i18n::Texts::default())?;
    actions.add(Action::new("mark", "Send my position to my client"), move |request| {
        let me = request.player;
        let Some(at) = world.position(me.entity) else {
            return Outcome::refused("No position");
        };
        match waypoints.send(Peer::Player(me.key), &Waypoint::Set(at)) {
            Ok(()) => Outcome::done("Sent"),
            Err(why) => Outcome::refused(format!("Not sent: {why}")),
        }
    })
}
```

`send(to, &message)` sends to one peer: a client sends to `Peer::Server`, the
server to `Peer::Player(key)`. `broadcast(&message)` sends from the server to
every client that runs Enshwick. `is_ready(peer)` tells whether a peer takes
messages now. Both send from any thread and fail with a `SendError`:

| Error | When |
| --- | --- |
| `NotReady` | the peer takes no messages: a client without Enshwick, a player who left, a server that has not answered yet |
| `TooLarge` | the message is over 4096 bytes encoded |
| `OverRate` | the side sent more than about twenty messages a second to that peer |
| `Encode` | the message does not encode |
| `Refused` | the loader refused the message for another reason |

## How messages travel

Messages ride in chat lines no player sees. A client with Enshwick that joins
a server advertising Enshwick, or a game whose host's Steam lobby names
Enshwick ([Games a client hosts](concepts/join-check.md#games-a-client-hosts)),
says hello in such a line; the server takes it
before the game posts it, answers, and from then on both sides send. A
vanilla client never says hello, so the server never sends it anything, and
the [join check](concepts/join-check.md) makes sure a client that said hello
runs the server's join mods. The server posts a frame line to that one
client, of a kind no player can send, and the client takes it before its chat
shows it. A client hosting a game is both server and client: its server part
talks to the clients, itself among them, and its client part to its own
server.

Channels are only for join mods, since both ends must run the mod; another
mod's `load.channel` turns it off. A message is encoded with `postcard`, so
both sides must agree on `M`: change the type, change the `version`, and a
peer on another version never hears the channel. Each side drops what goes
over a peer's rate and logs the drops.
