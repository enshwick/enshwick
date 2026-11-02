# Messages between server and client

A join mod with a client part, such as one that draws something the server
decides, sends messages between its two halves through a channel.

## Opening a channel

[`load.channel::<M, _>(name, version, handler)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/net.rs)
opens one during `init`, on both sides, for messages of type `M`, any type
that serde serializes; `handler` runs on each message that comes in, with
who sent it, a `Peer`:

- on the server, `Peer::Player(key)`, the account key of the player whose
  client sent it, as [`Player::key`](world.md#players) gives it; the handler
  runs on the game thread, where world reads work;
- on a client, `Peer::Server`; the handler runs on the thread that sends the
  client's chat, so keep it short and hand work to the mod's own state.

```rust
use enshwick::actions::{Action, Outcome};
use enshwick::net::Peer;

#[derive(serde::Serialize, serde::Deserialize)]
enum Waypoint {
    Set([f64; 3]),
    Clear,
}

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let waypoints = load.channel::<Waypoint, _>("waypoints", 1, |from, message| {
        if let (Peer::Server, Waypoint::Set(at)) = (from, message) {
            enshwick::info!("the server set a waypoint at {at:?}");
        }
    })?;
    let world = load.world()?;
    let actions = load.actions(enshwick::i18n::Texts::default())?;
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

[`examples/waypoints`](https://github.com/enshwick/enshwick/tree/main/examples/waypoints)
is this mod whole, with its client half, which draws the mark on the HUD,
and a fake host test for each half.

`send(to, &message)` sends to one peer: a client sends to `Peer::Server`, the
server to `Peer::Player(key)`. `broadcast(&message)` sends from the server to
every client that runs Enshwick. `is_ready(peer)` tells whether a peer takes
messages now. Both send from any thread and fail with a `SendError`:

| Error | When |
| --- | --- |
| `NotReady` | the peer takes no messages: a client without Enshwick, a player who left, a server that has not answered yet |
| `TooLarge` | the message is over 4096 bytes encoded |
| `OverRate` | the side sent more than about twenty messages a second to that peer, or its link to that peer holds too much |
| `Encode` | the message does not encode |
| `Refused` | the loader refused the message for another reason |

## How messages travel

Messages travel on Enshwick's own channel of the game's connection: a
message socket of the game's voice chat channel, which no game code uses.
A client with Enshwick opens it when it joins a server advertising
Enshwick, or a game whose host's Steam lobby names Enshwick
([Games a client hosts](concepts/join-check.md#games-a-client-hosts)), and
says hello on it; the server answers, and from then on both sides send. A
vanilla client never says hello, so the server never sends it anything, and
the [join check](concepts/join-check.md) makes sure a client that said hello
runs the server's join mods. A client hosting a game is both server and
client: its server part talks to the clients, itself among them, and its
client part to its own server, in process.

Channels are only for join mods, since both ends must run the mod; another
mod's `load.channel` turns it off. A message is encoded with `postcard`, so
both sides must agree on `M`: change the type, change the `version`, and a
peer on another version never hears the channel. Each side drops what goes
over a peer's rate and logs the drops.

Delivery is reliable and in order: each message arrives once, whole, after
the ones sent before it on the same link, or the peer counts as gone after
about 20 seconds without an answer. When a link holds too much, `send`
fails with `OverRate` and nothing is queued. A payload that starts like
executable code is refused both ways. Send important state again once
`is_ready` turns true, since a client that joins again starts a new link.
