# Examples

Mods and data packs that each show one part of the API. The mods build and
test with the workspace, the packs are checked by `cargo enshwick`'s tests,
and none is released.

| Folder                                                | Shows                                                       |
| ----------------------------------------------------- | ----------------------------------------------------------- |
| [trading-post-api](trading-post-api/)                 | a service table and a mod event, declared once for two mods |
| [trading-post](trading-post/)                         | publishing the service, sending the event, defining a point |
| [market-stalls](market-stalls/)                       | using the service and subscribing to the event              |
| [haggling](haggling/)                                 | extending another mod's rule at its extension point         |
| [hello-c](hello-c/)                                   | a mod written in C against `enshwick.h`                     |
| [hot-reload](hot-reload/)                             | a mod a development server reloads while it runs            |
| [you.material-stacks](you.material-stacks/)           | a data pack that raises the stack size of materials         |
| [you.recipe-cost-and-time](you.recipe-cost-and-time/) | a data pack that halves three recipes' cost and time        |
| [you.storage-slots](you.storage-slots/)               | a data pack that gives storage boxes more slots             |

The book's chapter [Services between mods](../book/src/services.md) walks
through the trading post and the market stalls,
[Extending other mods](../book/src/extending.md) through the haggling, and
[Testing](../book/src/testing.md#reloading-a-mod-while-the-game-runs) the
reload, and [A mod without Rust](../book/src/content/a-mod-without-rust.md)
through making a data pack.
