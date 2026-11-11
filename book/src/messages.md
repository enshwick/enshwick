<!--
type: how-to
description: A mod of side both sends messages between its server and client parts through a channel, such as for something the client draws and the server decides.
-->

# Messages between server and client

A mod of side `both` with a client part sends messages between its two
halves through a channel. An example is a mod that draws something the
server decides.

## Opening a channel

[`load.channel::<M, _>(name, version, handler)`](https://github.com/enshwick/enshwick/blob/main/crates/internal/core/src/load/net.rs)
opens one during `init`, on both sides. It carries messages of type `M`, any
type that serde serializes. `handler` runs on each message that comes in,
with who sent it, a `Peer`:

- on the server, `Peer::Player(key)`, the account key of the player whose
  client sent it, as [`Player::key`](world.md#players) gives it. The handler
  runs on the game thread, where world reads work;
- on a client, `Peer::Server`. The handler runs on the thread that sends the
  client's chat. So keep it short and hand work to the mod's own state.

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

A Luau mod opens a channel with `enshwick.net.channel` while it starts, and
its messages are Luau values; here each player who joins gets their
position:

```luau
local waypoints = enshwick.net.channel("waypoints", 1, function(from, message)
    if from == "Server" and message.Set then
        enshwick.log.info(`the server set a waypoint at {message.Set}`)
    end
end)

enshwick.on("PlayerJoined", function(joined)
    local at = enshwick.world.position(joined.entity)
    if at then
        waypoints:send({ Player = joined.key }, { Set = at })
    end
end)
```

[`examples/waypoints`](https://github.com/enshwick/enshwick/tree/main/examples/waypoints)
is this mod whole, with its client half, which draws the mark on the HUD,
and a fake host test for each half.

`send(to, &message)` sends to one peer. A client sends to `Peer::Server`, the
server to `Peer::Player(key)`. `broadcast(&message)` sends from the server to
every client that runs Enshwick. `is_ready(peer)` tells whether a peer takes
messages now. Both send from any thread and fail with a `SendError`:

| Error | When |
| --- | --- |
| `NotReady` | the peer takes no messages: a client without Enshwick, a player who left, a server that has not answered yet |
| `TooLarge` | the message is over 8192 bytes encoded |
| `OverRate` | the side sent more than about twenty messages a second to that peer, or its link to that peer holds too much |
| `LooksLikeCode` | the encoded message starts like executable code, which a channel never carries |
| `Encode` | the message does not encode |
| `Refused` | the loader refused the message for another reason |

## How messages travel

Messages travel on Enshwick's own channel of the game's connection: a
message socket of the game's voice chat channel, which no game code uses.
A client with Enshwick opens it when it joins a server advertising
Enshwick. It also opens it when it joins a game whose host's Steam lobby
names Enshwick
([Games a client hosts](concepts/join-check.md#games-a-client-hosts)). The
client says hello on it. The server answers, and from then on both sides
send. A vanilla client never says hello, so the server never sends it
anything. The [join check](concepts/join-check.md) makes sure a client that
said hello runs the server's mods of side `both`. A client hosting a game is
both server and client. Its server part talks to the clients, itself among
them. Its client part talks to its own server, in process.

Channels are only for mods of side `both`, since both ends must run the mod;
a server mod's `load.channel` turns it off. A message is encoded with
`postcard`, so both sides must agree on `M`. When the type changes, change
the `version`. A peer on another version never hears the channel. Each side drops what goes
over a peer's rate and logs the drops.

Delivery is reliable and in order. Each message arrives once, whole, after
the ones sent before it on the same link. Otherwise the peer counts as gone after
about 20 seconds without an answer. When a link holds too much, `send`
fails with `OverRate` and nothing is queued. A payload that starts like
executable code is refused both ways. Send important state again once
`is_ready` turns true, since a client that joins again starts a new link.
