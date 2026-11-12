### Added

- `world`: `World::set_level`, `set_experience` and `add_experience` write
  a player's level and experience in the server's next level step, as the
  game raises levels: a set level starts empty with what the game asks of
  it and raises no level-up event, given experience levels up as a kill's
  does, and skill points follow the level; Luau `enshwick.world` (#1036).
- `world`: `World::revive` stands a dead character up where it lies with
  the health asked, kept between 1 and its maximum; a living character is
  refused (#1036).
- `effects`: `Effects::drop_items` drops a stack of new items on the
  ground as one pickup any player can take, with a level and a rarity
  (`ItemDrop`); Luau `enshwick.effects.drop_items` (#1036).
- `abi`: `HostApi::progress_write` and `HostApi::drop_items`, ABI 11.24,
  with `DropRequest` and the `PROGRESS_*` values (#1036).
- `test`: `Installed::progress_writes` and `Installed::drops` list the
  writes and drops a mod asked the fake host for (#1036).
- `actions`: `Arg::item` and `Value::Item`. A window lists the game's
  vanilla items by the names the player's own game shows, with a search,
  and sends the picked item's GUID; a name several items share is followed
  by each one's debug name. A typed line takes a debug name, GUID, content
  id or shown name. The server refuses an item its game does not know before
  the handler runs. Luau `enshwick.actions.item` (#1033).
- `actions`: a window opens an optional argument's field with its default
  and runs while only optional fields are empty, and shows an action's
  group over it. The sheet carries both in new line kinds, which an older
  client skips (`Entry::group`, `ArgView::default`) (#1033).
- `content`: `Vanilla::items` lists the items with their name tags and
  reads each tag once; `vanilla::items` no longer reads the archive on each
  call, and an item registry that does not read yet is read again later
  (#1033).
- book: the ABI test coverage chapter lists how each `HostApi` entry is
  tested on the fake host, on a server copy and in game; a test fails when
  an entry is missing from it (#1035).
- `abi`: `HostApi::resource_buffer`, ABI 11.20, lends a resource patch
  bytes for its output that the loader owns: the game's first build keeps
  the last patch's output for the process, and the loader frees every other
  output. `ResourcePatchFn` still takes bytes of the patch's own, which stay
  for the process (#1006).
- `test`: `Installed::kept_outputs` counts the patch outputs the fake host
  got as a patch's own bytes rather than in a buffer it lent (#1006).
- `quests`: `Load::quests` reads every quest and lore page of the journal
  as the game built it, mods' included, with their steps, queries and
  rewards (`Quests::all`, `quest`, `pages`), and where a player or the
  world stands in each quest (`Quests::state`, `world_state`); Luau
  `enshwick.quests` (#1015).
- `abi`: `HostApi::game_query`, ABI 11.21, answers whether one of the
  game's queries holds for a player or the world, as the game reads it
  when it pays a quest (#1015).
- `content`: `KnowledgeTest::Equals`, `Below`, `HasFlag`, `Holds` and
  `Fails`, `KnowledgeQuery::not` and `KnowledgeQuery::game` with
  `GameQuery`, so a step, page or skill node tests a count exactly, below
  a count, any of a set of flags, or one of the game's queries;
  `KnowledgeTest::Unknown` marks a form enshwick does not read, and a
  quest step reads as unknown when its knowledge cannot be read (#1015).
- `test`: `Installed::set_game_queries` sets the fake host's answers to
  game queries (#1015).
- `actions`: `Actions::show` and `Actions::close` show, update and close a
  `Window` of lines, rows with buttons (`Row`) and whole actions on
  players' clients, where the loader draws it with the game's widgets, so
  a mod of side server needs only Enshwick on the client. The server
  renders it for each player in their language, only for players who may
  run every action it names, in hosted and single-player games as on a
  dedicated server. Each player keeps the window shown to them, so two
  players may hold different windows under one id; Luau `actions:show` and
  `actions:close` (#1022).
- `actions`: `Arg::text` and `Value::Text`, a line of free text of at most
  `MAX_TEXT` (256) bytes without control characters. A console or command
  line gives the last text argument the rest of the line and an earlier
  one a word or a quoted run; Luau `enshwick.actions.text` (#1022).
- `actions`: `Action::known` names up to `MAX_KNOWN` accounts a mod knows
  while they are offline, so player arguments and lists offer them and a
  typed name finds them; Luau `known` (#1022).
- `abi`: `HostApi::action_window` and `ActionCall::WINDOW`, ABI 11.22,
  show or close a mod's window on players' clients and ask the mod for it
  as each player sees it (#1022).
- `test`: `Installed::carried_windows` reads the windows a player's client
  holds after the mod's shows and closes (#1022).
- `actions`: `Action::group` names the group an action's line stands
  under in the help of a typed line, and `help <action>` answers an
  action's usage and the text of `Action::help`; Luau `group` (#1027).
- `actions`: `Arg::optional(default)` lets a typed line leave out the last
  arguments of an action, each taking its default. A text or a player
  before them takes the rest of the line up to the words that read as the
  optional arguments, so `give Ann Stone Axe` gives one and
  `give Ann Stone Axe 5` gives five; the help shows such an argument in
  square brackets, and a run from the interface that leaves values out at
  the end takes the defaults too; Luau `arg:optional(value)` (#1027).
- `actions`: `Action::with_self` offers the player who looks at an action
  among its players, first in a list and in a player argument's picker;
  Luau `with_self` (#1027).
- `actions`: `Actions::update` puts a window in place of the one of its id
  only on the clients that still show it and opens it for no one, and
  `Actions::on_closed` tells the mod when a client closed a window, let
  its oldest go for a ninth, or its player left, and when a window closed
  there because it rendered nothing for the player; the mod forgets such a
  window by itself; Luau
  `actions:update` and `actions:on_closed` (#1027).
- `abi`: ABI 11.23. `ActionCall::word` is the word that named the mod in a
  typed line, so its help and refusals name the command as typed;
  `ActionCall::WINDOW_CLOSED` tells a mod that a client closed its window
  or a player left; `HostApi::action_window` takes 2 as `close`
  (`WINDOW_UPDATE`, beside `WINDOW_SHOW` and `WINDOW_CLOSE`) for an update
  only where the window shows, which a loader before it answers with
  `Status::BAD_ARGS` (#1027).
- `content`: `vanilla::item_by_display_name` finds a vanilla item by the
  name the game shows, in any language it ships and whatever the case,
  with the names that look like it when none matches; `vanilla::items`
  lists every item with its debug name and the tag of its shown name, and
  `vanilla::recipes` every recipe with its debug name and the item it
  makes; Luau `enshwick.content.item_by_display_name`, `items` and
  `recipes`. The names come from the game's texts, which only a client's
  archive holds: the lookup answers `Unreadable` on a dedicated server
  (#1027).
- `test`: `FakeHost::later_mod_commands` adds a mod loaded after the one
  under test, `FakeHost::vanilla_item_names` gives vanilla items the tags
  of their names, and `Installed::close_window` and
  `Installed::player_left` close a window as a client does and make a
  player leave (#1027).
- `inventory`: `Inventories::give_what_fits` gives as many of a count of
  items as the inventory has room for and tells the caller how many were
  not given and the request's outcome, for a mod that gives its own
  quest's items on `QuestCompleted`;
  Luau `enshwick.inventory.give_what_fits` (#1028).
- `content`: `journal::lore_category` names the game's 13 lore categories
  by their cards' texts, for `NewJournalPage::category`; behind the
  `unstable` feature until a mod's page is seen on its card (#1028).
- `test`: `LORE_CATEGORY`, the category of the fake journal's one lore
  page; `FakeHost::inventory_requests`, the inventory preset with what a
  request needs to run (#1028).

### Changed

- **Breaking:** internals that were public only for the API's own crates
  are out of a mod's reach: `Host::install`, `Host::scope`,
  `schedule::Scheduler::get` and `run_due`, `ecs::ComponentType::read` and
  `game_events::CraftRecipeEvent::new`. They were hidden and never part of
  the API (#1047).
- A misspelt name gets its suggestions by one rule in a data pack's
  problems, a vanilla lookup and the loader's recipe file: names that start
  with what was typed, then names that hold it, then names within a third
  of its length in edits, at least one, ignoring case, five at most (#1047).
- The loader, `enshwick-gamedata` and the API take locks under one poison
  policy, `enshwick-base`'s; the loader writes a poisoned lock's place to
  its own log as before (#1047).
- The crates behind the facade, the macros among them, sit in
  `crates/internal/`, and the loader, its stub and the Luau runtime in
  `crates/runtime/`; package names stay (#1047).
- **Breaking:** the API's functions outside `Load` that took the mod's
  loader take `&impl Loading` or `&mut impl Loading`, which `Load`
  implements, so a mod passes its `Load` as before: `ecs::Ecs::resolve`,
  `ComponentType::resolve`, `Components::resolve`, `Worlds::resolve`,
  `Attributes::resolve`, `game_events::EventType::field` and
  `inherited_field`, `config::TableConfig::on_change` and
  `script::GameEventKind::subscribe`. `Loading` is new, sealed, at the root
  (#1046).
- The world domain (`world`, `map`, `altars`, `buffs`, `daytime`,
  `game_settings`, `terrain`, `voxels`, `weather`, `WorldError` and the
  typed handles of `typed`) is the internal crate `enshwick-world`, between
  `enshwick-content` and `enshwick-core`; every path stays (#1050).
- **Breaking:** the world domain's functions outside `Load` that took the
  mod's loader take `&impl Loading` or `&mut impl Loading`, so a mod passes
  its `Load` as before: `world::WritableType::resolve` and `field`, and
  `resolve`, `on` and `serve` of `typed::ComponentType`, `EventType` and
  `ResourceType` (#1050).
- The inventory domain (`inventory`, `items`, `item_data`, `entity_data`,
  `affixes`, `stats`, `damage`, `effects`, `survivors` and `loot`) is the
  internal crate `enshwick-inventory`, between `enshwick-world` and
  `enshwick-core`; every path stays (#1052).
- **Breaking:** `loot::Containers::resolve` takes `&mut impl Loading`, so a
  mod passes its `Load` as before (#1052).
- The ui domain (`ui` and `sound`) is the internal crate `enshwick-ui`,
  between `enshwick-actions` and `enshwick-core`, and the packs domain
  (`packs`) the internal crate `enshwick-packs` above it; `enshwick-core`
  keeps only `Load` and the script binding's namespace lists, and every
  path stays (#1055).
- The actions domain (`actions`, `chat`, `commands`, `console`,
  `controls`, `input`, `interactions` and `vetoes`) is the internal crate
  `enshwick-actions`, between `enshwick-knowledge` and `enshwick-core`;
  every path stays (#1054).
- The knowledge domain (`knowledge`, `progress`, `quests` and `rewards`) is
  the internal crate `enshwick-knowledge`, between `enshwick-inventory` and
  `enshwick-core`; every path stays (#1053).
- **Breaking:** `script::Bound::resolve` takes `&mut impl Loading` instead
  of `&mut Load`, so each domain's crate implements `Bound` beside its own
  types (#1053).
- The content domain (`content`, `rates`, and the shape samples of the
  generated `typed` items) is the internal crate
  `enshwick-content`, between `enshwick-base` and `enshwick-core`; every
  path stays, and the Luau definitions list `typed` after `content`
  (#1049).
- What every domain of the API shares is the internal crate
  `enshwick-base` below `enshwick-core`; every path stays (#1046).
- `content`: **breaking.** A resource key comes from its type:
  `types::ITEM_INFO.key(guid)` (`ResourceType::key`) replaces
  `guid.key(types::ITEM_INFO)` and `id.key(..)`, which are gone; a content
  id passes `id.guid()` (#1044).
- `script`: **breaking.** `Result<T, script::Error>` answers a callback for
  every `T` a script can pass (`FromScript`), not only game data; a return
  that is not a `T` is the error. `Namespace` is `Copy` (#1044).
- `actions`: **breaking.** `sheet::Entry` has the field `group` and
  `sheet::ArgView` the field `default`, so a struct literal of either names
  them or ends in `..Default::default()`. An optional item, like an
  optional text, must be an action's last argument, and takes the rest of a
  typed line (#1033).
- `commands`: **breaking.** A mod's command is named after its id, the
  part after the author: `you.admin-tools` gets `/admin-tools`.
  `Actions::commands` takes only the alias, in Rust and in Luau, and
  `HostApi::commands` keeps its `name` parameter, which the loader no
  longer reads. An alias goes to the first mod that asks for it; a later
  mod keeps its full name, and the log and the Mods screen name both mods
  once. An alias is never another mod's id or name, so a mod's full name
  always reaches it, and a name that is another mod's whole id stays that
  mod's. A mod built before ABI 11.23 whose alias is not shorter than the
  name in its id runs its commands by that name alone instead of failing
  its init. `CommandSet::alias` is an `Option`, and
  `FakeHost::other_mod_commands` takes the mod's id and alias (#1027).
- `actions`: the help of a typed line names the mod and the command as
  typed, then lists one line for each action the caller may run, under the
  groups the mod names. An unknown action is refused with the closest ids
  the caller may run and how to list them all (#1027).
- `commands`: the refusal of a command typed in chat goes into that
  player's chat instead of a notice, and a reply of more than 16 lines
  reaches the chat whole while replies to typed commands fill at most half
  of the lines waiting to be posted (#1027).
- `content`: the names `VanillaError::Unknown` and its like suggest are
  those that start with or hold what was asked, then those within a third
  of its length in edits (#1027).
- `content`: a builder of `Load::serve`, and the loader's own patches,
  build into a lent buffer, so `Host::served_resource`,
  `Host::served_value`, Luau `enshwick.resources.served_value` and each
  rebuild free what they build instead of keeping every patch's output for
  the process (#1006).
- `content`: a served resource whose build read another served key, such
  as an item `Load::rekey_item` re-keys, is built again whenever that key
  is rebuilt, so a re-keyed item follows a world's changed values of
  its new item; the loader keeps its base for it (#1006).
- `content`: one patch per resource serves all of a mod's knowledge
  entries, and one all of its quests, journal pages and quest changes, so
  100 entries and 50 quests build the knowledge in 4,464 bytes and 0.40 ms
  instead of 321,536 bytes and 85 ms, and the journal in 6,000 bytes and
  0.46 ms instead of 62,056 bytes and 20.5 ms; a change that fails is
  logged and left out, and the rest are made (#1015).
- `actions`: a console or command line reads a run in double quotes as
  one value (#1022).
- `packs`: a misspelt or unknown field in a pack's data is named with the
  fields its place takes and the one it is near, such as
  ``unknown field `cout`, expected `item` or `count`; near: count``, with
  its file, line and column, where a gift, a value, a scale or a
  dependency used to answer that the data matched no variant of an
  untagged enum (#1017).
- `rewards`: `Give::Items`, and an item gift of a pack's `rewards.toml`,
  gives as many items as fit and logs how many did not, or why none were
  given, where a full inventory got none (#1028).
- `content`: **Breaking.** `Load::add_journal_page` refuses a page whose
  `category` is not one of the game's own lore categories and names the
  categories of the running game: the journal has a card only for those
  and dropped any other page without a word. It needs the `unstable`
  feature until a mod's page is seen on its card (#1028).
- The book's new chapter "Performance" gives what Rust, C, C++ and Luau
  mods cost at run time, with each measured cost and its source; the Luau
  chapter says a script runs in an interpreter (#1034).
- The book says Enshwick supports the latest game version, and the alpha
  release notes name only the game version an alpha was checked against; the
  repository has a name policy, `NAME-POLICY.md` (#1034).
- The value types the API's layers share and the generated typed layer are
  now the internal crates `enshwick-values` and `enshwick-typed`, which
  `enshwick` re-exports under its own paths: a mod still depends on
  `enshwick` alone and names every item as before. In docs built with
  `docsrs`, the `unstable` badge no longer shows on the script impls of
  types that derive `Script` (#1038).
- The ABI types and the game memory reads and writes are now the internal
  crate `enshwick-abi`, which `enshwick` re-exports as `enshwick::abi` and
  `enshwick::raw`: a mod still depends on `enshwick` alone and names every
  item as before, and `enshwick.h` is unchanged (#1039).
- Every other module, from `Load` to the fake host, now lives in the
  internal crate `enshwick-core`; `enshwick` is a facade that re-exports
  each at its own path and keeps the crate docs, the macros' docs and the
  features. A mod still depends on `enshwick` alone and names every item as
  before (#1040).
- `packs::schemas`: a description in a pack file's JSON Schema leaves out the
  hidden lines of its examples, as the API docs do, so an editor no longer
  shows lines such as `# Ok::<(), PackError>(())` (#1040).
- The fake host now lives in the internal crate `enshwick-fake`, which
  `enshwick` re-exports as `enshwick::test` behind its `test` feature: a
  mod's tests still turn that feature on and name every item as before, and
  a build without it holds no test support (#1041).
- The loader, `enshwick-luau`, `enshwick-gamedata` and `cargo-enshwick`
  depend on the internal crates `enshwick-core` and `enshwick-abi` instead
  of `enshwick`, so a change to `enshwick` alone no longer rebuilds them;
  mods still depend on `enshwick` alone (#1043).

### Fixed

- Loader: a crash line names the module a fault lies in when no mod or the
  loader holds the address, and says `the game itself crashed` only when it
  searched the whole stack and found no mod's or loader's code, in a module's
  code; an overflowed stack, a deeper one or a fault in generated code says
  so instead of blaming the game; the next start's warning and the
  status summary open such a crash with `the game crashed in the last
  session`. Under Wine 11 every server stop writes one, after the save
  (#1029).
- `content`: a served read of a changed vanilla resource whose every patch
  fails reads it as the archive has it, instead of failing as missing
  (#1006).
- `HostApi::resource_served` keeps no bytes for a call that only asks the
  length, and answers no retry from bytes kept before a build began on that
  thread, so a later read there no longer gets stale bytes (#1006).
- `files`: `write_atomically` keeps the `.bak` when another writer replaces
  the file while the backup is linked, which Linux answers as not found,
  and waits up to about a second per step, not 50 ms, for a file that is held
  (#1025).
- `actions`: a run naming an account no one online or known has is
  answered "That player is no longer here." instead of with its account
  key (#1022).
- Mods screen: each mod is one row with its plain name as the title, its
  version and credits on one dimmed line, and a small Settings button at
  the row's right instead of a button across the window; a thin line parts
  the mods. A game build without the row frame stacks the row's parts
  (#1023).
- Mods screen: a mod's key bindings sit on its own settings page, after
  the world's settings, and say that they belong to the player on this
  computer; the Controls button and its page for every mod are gone. Back
  ends a key capture before it leaves the page (#1023).
- Mods screen: a text setting draws as an input box with its text at the
  left and an outline while it has the caret, and an integer setting's
  slider shows no decimals (#1023).
- Mods screen: what is off shows in red: a mod's off reason, each feature
  that is off, the loader's parts off and the features this game build
  lacks. A world store whose file does not read has its own line in the
  player's language, with the mod, the store, the file and the parser's
  words last (#1023).
- Chat: a line a client sends with a type other than typed text or an
  emote is dropped with a debug log line, instead of showing as an empty
  line under the player's name; emotes keep their look, pass the chat
  filters like typed text and are heard as `ChatKind::Emote` (#1023).
- Mods screen: a mod that is off or waiting says so in words under its
  name, not by colour alone; a key capture ends when a text setting takes
  the caret; key bindings page on a game build without the scroll area
  (#1023).
- Mods screen: a mod the loader did not start, such as one whose required
  mod is missing, shows as off with the reason in the player's language,
  and the main menu line and the chat line at world load count it as off
  (#1032).
- Mods screen: a mod's name is the row's title; one dimmed line under it
  holds the version and authors; the licence, homepage and extension points
  are on the mod's page. Each row has two small buttons at its right, the
  game's own: an info button that opens the mod's page and, under it, a gear
  for a mod with settings or key bindings here. A game build where the icons
  do not read shows the buttons as text (#1032).
- Interface: two dropdowns with the same label and number of options, such
  as two player arguments of one window, open alone instead of together
  (#1032).
- `content`: the docs of `QuestReward`, `NewQuest` and `Load::add_quest`
  no longer say the game pays a quest's items: it gives the experience and
  only draws the item (#1028).
- `loader`: a knowledge query the game's wrapper leaves unanswered reads as
  missing, where it read as false (#1028).
