# Examples

The whole mods the book follows, data packs and a Luau mod. The mods build and
test with the workspace, the packs are checked by `cargo enshwick`'s tests,
and none is released. The book's own examples cover the smaller cases.

| Folder                                                | Shows                                                       |
| ----------------------------------------------------- | ----------------------------------------------------------- |
| [trading-post](trading-post/)                         | publishing a service, sending an event, defining a point    |
| [haggling](haggling/)                                 | extending another mod's rule at its extension point         |
| [hello-c](hello-c/)                                   | a mod written in C against `enshwick.h`                     |
| [waypoints](waypoints/)                               | a join mod whose server sends a mark its client draws       |
| [you.material-stacks](you.material-stacks/)           | a data pack that raises the stack size of materials         |
| [you.recipe-cost-and-time](you.recipe-cost-and-time/) | a data pack that halves three recipes' cost and time        |
| [you.storage-slots](you.storage-slots/)               | a data pack that gives storage boxes more slots             |
| [you.welcome](you.welcome/)                           | a Luau mod that greets players and counts who is online     |

The book's chapter [Services between mods](../book/src/services.md) follows
the trading post, [Extending other mods](../book/src/extending.md) the
haggling, [Mods in C](../book/src/mods-in-c.md) the C mod,
[Messages between server and client](../book/src/messages.md) and
[The interface](../book/src/interface.md) the waypoints,
[A mod without Rust](../book/src/content/a-mod-without-rust.md) the data
packs, and [Luau mods](../book/src/luau.md) the Luau mod.
