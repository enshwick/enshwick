# Examples

Mods that each show one part of the API. They build and test with the
workspace and are never released.

| Folder                                | Shows                                                       |
| ------------------------------------- | ----------------------------------------------------------- |
| [trading-post-api](trading-post-api/) | a service table and a mod event, declared once for two mods |
| [trading-post](trading-post/)         | publishing the service and sending the event                |
| [market-stalls](market-stalls/)       | using the service and subscribing to the event              |
| [hot-reload](hot-reload/)             | a mod a development server reloads while it runs            |

The book's chapter [Services between mods](../book/src/services.md) walks
through the trading post and the market stalls, and
[Testing](../book/src/testing.md#reloading-a-mod-while-the-game-runs) the
reload.
