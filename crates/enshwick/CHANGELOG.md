# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change.

## [Unreleased]

### Fixed

- `actions`: a server sends a joining player only the controls of a server
  mod whose action that player may run, by the action's `allow` or
  `admins`, and sends them again when the server changes the player's
  permissions; before, every player saw every such key (#988).
- The loader's release check on a stable loader reads the repository's tags
  instead of the newest twenty releases, so a newer stable release is found
  however many releases follow it (#988).
- The loader's Ukrainian texts are tested for apostrophes the game font does
  not draw (#988).

## [1.0.0-alpha.13] - 2026-11-07

### Added

- `sound`: `SoundContainerId` and `ui::Frame::play` play any sound container
  the client's archive holds as an interface sound and refuse a GUID it
  lacks; `Load::change_sound` and `SoundChange` change a vanilla container's
  volume, pitch, randomness, distances, looping and priority within bounds,
  in load order on the client; unstable `Load::add_sound` and `NewSound` add
  a 16-bit PCM WAV at 48000 Hz from the mod's folder as a streamed
  uncompressed sound and a container that plays it; `sounds.toml` in data
  packs; Luau `frame:play`, `enshwick.sound.change` and `enshwick.sound.add`.
  `UiWidget::PLAY` and `UiWidget::sound`, ABI 11.14 (#977).
- `interactions`: unstable `Load::interaction_targets` and `Targets::target`
  read the entity a player's prompt offers in the server's world; Luau
  `enshwick.interactions.target` (#977).
- `input`: unstable `Input::local_held` reads the gameplay controls a
  client's own player holds; Luau `enshwick.input.local_held`.
  `HostApi::local_held`, ABI 11.14 (#977).
- `ui`: unstable `Frame::prompt` and `Prompt` read the title and verb of
  the player's interaction prompt; `Frame::screen_host` and
  `ScreenChanged::host` name the client-world entity the open screen
  belongs to; Luau `frame:prompt()`, `frame:screen_host()` and
  `change.host`. `UiFrame::prompt_title`, `prompt_verb` and `screen_host`,
  ABI 11.14 (#977).
- `map`: unstable `Load::on_fog_uncovered` and `FogOfWarUncovered` tell a
  client mod at most once a second where and how much fog of war its player
  uncovered; Luau `enshwick.map.on_fog_uncovered`. `FogInfo::uncovered` and
  its window fields, ABI 11.14 (#977).
- `content`: Luau `enshwick.content.item_id` takes a GUID, such as
  `enshwick.content.item` gives for a vanilla item, as well as a content id,
  and `enshwick.content.recipe_id` gives a recipe's id the same way, as
  `ItemId::from_guid` and `RecipeId::from_guid` do in Rust (#986).
- `altars`: `Load::limit_altar_level(kind, max_level)`, unstable, gives a
  Flame Altar kind a maximum level below the game's own: a craft filter of
  the mod refuses, on the server, the vanilla altar upgrade recipes that
  start at that level or above at the kind's altars, so the altar keeps its
  level and the player the items; the Flame's own upgrades stay allowed;
  Luau `enshwick.altars.limit_altar_level`. The fake host's
  `upgrade_recipes` puts upgrade recipes of given workshops and levels in its
  recipe registry (#978).
- `events`: `TerrainPageLoaded` and `TerrainPageUnloaded` tell a mod, after
  the step, when the dedicated server loads a page of the finest voxels
  into memory or drops it, 128 m a side on server 26430253, with or
  without players near; a mod that keeps edits outside the bases writes
  them again when a page loads. Luau: `enshwick.on("TerrainPageLoaded")`.
  `Event::TERRAIN_PAGE_LOADED`, `Event::TERRAIN_PAGE_UNLOADED` and
  `TerrainPageData`, ABI 11.15 (#984).
- `content`: `Load::rekey_item(old, new)` and `enshwick.content.rekey_item`
  take an item the mod added with `add_item` before the call as `new`: `old`
  is served with that item's data, cloned from its base item with its name,
  tooltip lines and icon; an item with a meter re-keys only an item with an
  entity of its own (#972).
- `test`: `FakeHost::pack_log_level(pack, level)` sets one data pack's log
  level (#972).
- `actions`: `Actions::control(id, key)` puts a declared action without
  arguments on a key in one call: a control of the same name that runs the
  action at each press, labelled with the action's label in each player's
  language, offered to joining clients by a mod of side server; Luau
  `actions:control(id, key)`. The fake host's `controls()` lists a label in
  its language from the texts the mod handed over, as the Controls page
  does (#970).
- `voxels`: `Load::on_voxels_changed` hears every write of the server's
  world voxels after the step, a player's digging and building, the game's
  own and the terrain edits and transactions of mods, as a `VoxelEdit`: its
  `EditSource` (`Game`, `ThisMod` or `OtherMod`), the box the game wrote
  and each `VoxelChange`,
  the voxel's place before and after. A mod keeps edits the save drops by
  storing them and writing them back with `write_voxels`, and skips its
  own replay. Luau reaches it as `enshwick.voxels.on_voxels_changed`.
  `Event::VOXELS_CHANGED`, `VoxelEditData` and `VoxelChange`, ABI 11.12.
  The fake host raises edits with `voxels_changed`, and a transaction's
  voxel writes and their undo as the mod's own, brick by brick (#963).
- `Control::runs(action)` (unstable): a control runs the mod's own action on
  the server, for the player, each time it fires on a client; the server
  checks who may run it and shows the reply as a notice. The server sends
  the controls of a mod of side `server` that run an action to each client
  with Enshwick that joins: the Controls page lists them under the mod's
  name in the player's language, the player may change their keys in
  `mod-controls.toml`, and a press runs the action on the server, so a
  server-only mod offers its actions in every game, hosted and single player
  included. Luau: the control's `runs` field. `ControlSpec::action`,
  ABI 11.11 (#964).
- `input` (unstable): `Load::input` and `Input::held(player)` read the game
  actions a player holds this step in the server's world, also for a player
  without Enshwick, as an `InputMask` (`contains(GameAction)`, `iter`,
  `bits`); the running build's reflection names each bit. Luau:
  `enshwick.input.held(entity)`, the list of the held actions' game names
  (#957).
- `Frame::open_screen` says which of the game's screens (`HudScreenType`)
  the player has open in a frame, and `Load::on_screen_changed` hears each
  change as a `ScreenChanged`, both client only; `HudScreenType::ALL` and
  `from_name`. Luau: `frame:open_screen()` and
  `enshwick.ui.on_screen_changed`. `UiFrame::hud_screen`, ABI 11.10 (#957).
- A build filter refuses with a reason the builder reads:
  `BuildVerdict::RefuseBecause` (`BuildVerdict::refuse_because`), a key of
  the mod's texts or plain words. A client running the mod shows it under
  the build cursor; the server sends it once as a notice, at most once a
  second per player. `Load::on_build` takes any answer that turns into a
  `BuildVerdict`, `bool` included; a Luau filter returns a string to refuse
  with that reason. `BuildCheck::reason`, ABI 11.10 (#957).
- Player actions name the object they act on: `Actions::run_at` with an
  `ActionTarget` (the server's entity, a position, or both), which the
  handler reads in `Ask::target` and `Request::target` once the server
  checked it: a named entity must exist there and gives its own position,
  and the object must stand within 12 metres of the player; Luau `actions:run_at`, `ask:target()` and `request:target()`.
  `HostApi::run_action_at` and `ActionCall::target`, ABI 11.10 (#957).
- `Load::on_waypoint` hears a player's waypoint set, moved or cleared
  (`WaypointChanged`), and `Load::on_ping` each ping (`MapPinged`), on the
  server, a place outside the map's frame left out; Luau
  `enshwick.map.on_waypoint` and `on_ping` (#957).
- `content::knowledge`, `content::journal`, `content::skills` and
  `content::removal` (behind `typed`), in Rust and Luau: mods add knowledge
  entries of the world or of each player with skill points and experience;
  quests of steps on knowledge with rewards, lore pages, changes of vanilla
  quests and a player's quest progression; skill nodes copied from vanilla
  ones within the room a player's save keeps, and perks; and take vanilla
  items, workshops and loot tables out of play, or serve a vanilla item with
  another's data under its own id, while every held stack survives (#960).
- The example `you.trail`, a Luau mod with a quest, a lore page and a skill
  node (#960).
- Luau mods reach the typed API (`typed`): `enshwick.typed.component_type`,
  `event_type` and `resource_type` open a generated component, event or
  resource by its id, such as `ecs.Health`, at start, with
  `ComponentType:read` and `write`, `EventType:on`, and `ResourceType:key`,
  `game_value` and `serve`. A value is a table by the generated item's Rust
  names, an enum constant its variant's name or `{ Other = <name or number> }`,
  read and written through the generated Rust item. Every generated struct
  and enum carries its script description (#940).
- The Mods screen says when a newer Enshwick release exists: one dimmed
  line with its version and the address of its release page. The loader
  asks GitHub once per start on a thread of its own, with a 5-second limit
  per step; it downloads nothing, a dedicated server only logs it, and a
  prerelease counts only while the running loader is one. `release_check =
  false` in `enshwick.toml` turns the check off (#943).
- `Load::add_weather_phenomenon` (behind `typed`) adds a copy of one of the
  game's weather phenomena under the mod's content id, with its own
  probability, duration, radius, strength and effect (`NewPhenomenon`,
  `PhenomenonChange`), in its original's weather layer or another, and
  `Load::change_weather_phenomenon` changes the game's own; mods apply in
  load order. Clients draw weather from their own copy, so such a mod runs
  on clients too. Luau reaches both through `enshwick.weather` (#936).
- `Front::phenomenon` names the phenomenon of a weather front, and
  `Load::on_front` tells a mod when a front starts and ends
  (`FrontChanged`), also as `enshwick.weather.on_front`.
  `WeatherFront::phenomenon`, ABI 11.6 (#936).
- `chat`: `Load::on_chat` filters what players type on the server, after
  the chat mute: a filter keeps, drops or rewrites a line (`ChatVerdict`,
  an empty rewrite drops it), filters chain in load order, and a filter that panics turns off alone.
  `Load::on_chat_said` hears every line that reaches the chat as a
  `ChatLine`, a player's lines at most eight a second. Luau reaches them as
  `enshwick.chat.on_chat` and `enshwick.chat.on_chat_said`.
  `HostApi::chat_filter`, `Event::CHAT_SAID`, `ChatLine` and `ChatRewrite`,
  ABI 11.6. The fake host runs a player's line through the chat with `say`
  (#936).
- `WorldSaving` is raised when a save of the world begins, before
  `WorldSaved`; `World::save_info` reads the save on disk as a `SaveInfo`
  (its `WorldGuid`, time, size and backups); `World::hold_saves` holds the
  world's saves while any running mod holds them, until the game a client
  hosts ends; `World::save` is refused meanwhile. Luau reaches them through `enshwick.world`.
  `Event::WORLD_SAVING`, `HostApi::save_info` and `hold_saves`, and
  `SaveInfo`, ABI 11.6. The fake host sets the save with `set_save_info`
  and reports holds with `saves_held` (#936).
- `Spawn::do_not_save` spawns an entity the world's save leaves out, as the
  game's own passing animals. `SpawnRequest::flags` and
  `SpawnRequest::DO_NOT_SAVE`, ABI 11.6 (#936).
- `World::set_saved` keeps a live entity a mod spawned or placed out of the
  world's saves, or puts it back, through the game's own `DoNotSave`
  commands, from the next step on; vanilla entities are refused. Luau
  reaches it as `enshwick.world.set_saved`. `HostApi::set_saved`, ABI 11.9;
  the fake host reports the entities left out with `unsaved` (#961).
- `items::Kind::Consumed`: every item filter hears a use once all of them
  allowed it, the place to count uses or reward one. `ItemAction::CONSUMED`,
  ABI 11.7; the fake host's `item_action` follows an allowed use with it;
  a pack binds it with the `consumed` key of an item's `on` table (#945).
- Luau mods reach player actions, controls and the interface with the Rust
  names. `enshwick.actions.actions(texts)` returns `Actions`, whose `add`
  takes an action as a table of the builder's fields, with `allow`, `at` and
  `per_player` as functions, and whose `run` asks the server from a client.
  A handler gets the `Request` as an object and answers
  `enshwick.actions.done` or `refused`. `enshwick.ui` has `on_frame`,
  `on_hud`, `extend_screen`, `dialog`, `set_markers`, `clear_markers` and
  `map_marker_types`. A frame and a HUD are objects lent to the callback, the
  same one each frame. Widgets, dialogs and markers are tables, and
  `toggle`, `slider` and `dropdown` take their value and return it (#947).
- `script::Class::lent` makes the class of an object a hook lends a script's
  function for one call, so a runtime hands the function the same script
  object on every call. A Luau script that keeps it past the call gets an
  error from it, never the next call's value (#947).
- `content::templates`: `Load::add_template` copies one of the game's entity
  templates under the mod's content id, and `Load::change_template` changes
  one of the game's own, by `ComponentChange`s that set, edit or remove a
  component (by its generated type with `typed`), in memory and in load
  order. The loader lists the templates mods serve to the game's template
  registry on the server and the client, so a copy spawns and places like
  the game's own, and lists a removed mod's copies as the vanilla template
  they were made from, so a world's placed copies still load. Workshops and
  item meters are templates made this way.
  Luau reaches both through `enshwick.content` (#952).
- `content::props`: `Load::add_prop` adds a prop type, an item that places a
  copy of a vanilla prop's template, with a copy of its recipe for the build
  menu, and optionally a crafting prop that the named recipes require. The
  loader gives each crafting prop the next bit of the game's 64-bit crafting
  prop mask, which all mods share, and refuses one past the last.
  `HostApi::crafting_prop`, ABI 11.8 (#952).
- `content::enemies` (behind `typed`): `Load::add_enemy` copies a vanilla
  enemy with its own level, health, damage, faction, arsenal and boss mark;
  `Load::add_arsenal` and `Load::change_arsenal` make and change arsenals
  from the game's attacks and behaviours. `vanilla::enemy` and
  `vanilla::template` name the game's enemies and templates; a Rust mod's
  first such call after `init` logs a warning, as it reads every
  template's name on the calling thread (#952).
- `content::spawns` (behind `typed`): `Load::add_spawn_entry` adds a
  template to the game's random spawners, by spawner or wherever a template
  spawns already, and `Load::change_spawner` changes their counts and
  timeouts (#952).
- `altars::kinds`: `Load::add_flame_altar` adds a kind of Flame Altar, a
  prop copied from the vanilla altar whose altars take zones of their own
  per level through the altar zone filter chain. `Altar::template` names an
  altar's kind (#952).

### Changed

- `typed`: `enshwick.typed` values cross into Luau and back through one walk
  over a static shape of each generated type instead of compiled conversions
  per type, taking and refusing what the Rust item does; the release loader
  shrinks from 29.8 MB to 15.9 MB. The rows of `typed::NAMED` hold the shape
  and give `check` and `to_script`; the script types are `typed::SCRIPT_TYPES`
  (#986).
- `typed`: a component type with a field whose data lies outside the
  component, such as a `BlobArray`, or of a kind this API does not know,
  does not resolve for live reads and writes; the refusal names the field
  (#986).
- `actions`: `Load::actions` hands its texts to the loader once, beneath the
  texts the mod hands itself, before or after; a runs-control's label reads
  from them in each player's language with the usual fallback, and a key both
  give reads as the mod handed it (#978).
- Every book chapter reads in sentences of at most 25 words with at most
  two code spans, 20 words on average. Long runs of names became lists and
  tables, and every fact and code example stayed (#976).
- The crate root and the book say a mod depends on an alpha's tag from
  git, as `cargo enshwick new` writes it, since alphas are not on crates.io
  (#979).
- The crate's description, README and root docs, and the book's introduction,
  describe Enshwick as the modding framework and API for Enshrouded, with its
  mod loader (#968).
- The book leads from the smallest mod to the hardest parts. It opens with
  Hello, world, the smallest data pack, Luau script and Rust mod, and the
  tutorial Your first real mod. Everyday tasks, game systems, mods working
  together and reaching into the game follow, then shipping, concepts and
  reference. Every chapter keeps its address. Settings, Hooks and Luau mods
  read with shorter sentences and one topic per paragraph (#967).
- The entity calls tell a call this process cannot make apart from an
  entity that is gone: `World::destroy`, `World::set_saved`,
  `Effects::teleport` and the component writes (`World::update`, `set` and
  the stat setters) fail with `WorldError::Missing` where no server runs, as
  on a client in another player's game, or on a build without their
  commands, and keep `WorldError::Gone` for an entity not in the world.
  `WorldError::Missing` reads "not available here, or on this loader or game
  build". The fake host answers the same, and `FakeHost::without_entity_commands`
  plays a build without the commands. `Status::UNAVAILABLE`, ABI 11.13
  (#969).
- `ui::HudScreenType`, its `ALL` and the screen names the fake host checks
  extensions against come from the game's reflection: `cargo enshwick
  generate` writes them from the game's `HudScreenType`, so a new build's
  screens need no hand change (#969).
- A save hold no longer outlasts a shutdown: as a dedicated server or a
  hosted game shuts down, every `World::hold_saves` hold lets go and the
  final save runs, and the log names each mod that held saves and for how
  long. A mod that holds saves inside its `ShutdownStarted` handler refuses
  the final save; a hold asked for after that handler is not taken. Once
  saves stay held for 10 minutes, the log and a notice to each administrator
  online warn, again every 10 minutes. The fake loader's
  `Installed::shut_down` shuts down the same way (#965).
- **Breaking:** item meters and workshops are made through
  `content::templates`, so `content::items::MeterLayout`,
  `clone_meter_template`, `list_meter_template`,
  `content::workshops::clone_template` and `list_template` are gone,
  `TEMPLATE_COLLECTION` moved to `content::templates`, and `WorkshopLayout`
  lost its `crafting`, `interaction` and `client` fields (#952).
- **Breaking:** the fake loader's `Installed::build` returns
  `Result<(), Option<String>>`, the refusing filter's reason if any, in
  place of a `bool`; a Luau build filter's answer is typed `any`, and a value
  other than nil, a boolean or a string refuses (#957).
- `abi::ActionTarget::read` reads the object a run names as the loaders
  take it; `run_action_at` refuses a target whose header is too short with
  `BAD_ARGS`, where it ran the action without the object (#957).
- The ABI is 11.10: mods built for 11.9 and older load as before (#957).
- With developer tools on, the loader logs once when the map's marker list
  has no room for a mod's markers, with how many it left out, where it dropped them silently
  (#957).
- The loader finds every game build the same way, a new one too: by the
  anchors of `enshwick-gamedata` carried by their code patterns, then the
  engine routes. It reads no per-build table at run time, and an entry
  whose route finds nothing and that no anchor holds counts as missing.
  The start line, the status report and every feature-off line name the
  build by its side, Steam build, game version and executable.
  `build_patterns = false` keeps vanilla a build that no compatibility
  report of the release covers, and `BuildInfo::table` is `EXACT` on a
  build where nothing is missing, `PATTERNS` otherwise (#951).
- **Breaking:** a Luau mod declares a control with
  `enshwick.controls.control(control, on)`, as `Load::control` does, in
  place of `controls.declare`: the control is a table of the builder's
  fields, a key its text, such as `"Shift+E"`, and the handler gets the
  `Fired` fields as a table. `enshwick.controls.parse` and `from_bits` make
  a key, and `bits`, `code`, `modifiers` and `is_mouse` read one.
  `controls::Controls` is gone, and the namespace is `controls::ControlFns`
  (#947).
- A Luau damage filter keeps one damage object from hit to hit, so a hit no
  longer copies the damage into a new object or makes a new Luau object.
  That cuts a hit from 9 allocations to 7 (#947).
- The fake host refuses subscriptions to events the loader never raises, as
  the loader does (#936).
- A game build without its own table where the engine routes and the
  newest table's patterns find every entry runs without a warning: the log
  says so at info, with no E1005 and no player message; E1005 stays for a
  build where entries are missing. Mods still see such a build's table as
  carried by patterns (#946).
- Bases past 16 needs the client's create, destroy, size and base id list
  only on a client, so a server table needs no rows for them, and sizes a
  create an engine route found by the image's function table (#946).
- The join check, the join dialog hold, the event loop's waits and the
  workshop limit lift work on a symbol an engine route found and checked
  when no table pins it: the route's match stands for the pinned bytes,
  the division's span ends with its function, and the stop block ends at
  its jump to the next recipe or its padding (#946). A wait is hooked only
  where the code from the division multiplies by the time left and keeps
  it up to the wait, so a moved register leaves the wait unhooked instead
  of giving a wrong time left (#946).
- **Breaking:** `script::Field::ty` is no longer `const`: a field gives its
  type when asked, so a game type may hold itself (#940).
- A Luau mod's `u64` or `i64`, such as a key, takes a whole number up to
  2^53 as well as its decimal digits (#940).
- A control fires when its key goes down with at least its modifiers held,
  so a control on `G` fires while the player sprints with Shift; of the
  controls on one key, only those with the most of the held modifiers fire,
  so `Shift+G` still beats `G`, and `Shift+G` and `Ctrl+G` both fire with
  Shift and Ctrl held. A held combination that cannot be bound, such as
  Alt+F4, fires nothing. A control stays down until the key that brought it
  down goes up, and a modifier pressed or let go meanwhile fires nothing.
  The fake host's `keys` holds the modifiers of all the keys it is given
  together, as a keyboard does (#942).
- **Breaking:** the Mods screen shows no player actions: a mod offers them
  through its own controls and panels, and the console runs them as
  before. The client no longer asks the server for action sheets, and the
  reply to a run reaches the player as one notice with all its lines, or as
  a chat line where the client takes no notices (#950).
- `Weather::set` with a `lasting` over `MAX_RESTORED_REMAINING`, 30
  minutes, logs a warning: a save keeps a front only while it has that much
  left, so a longer front lasts only until the world next loads (#952).

### Fixed

- `log`: a data pack's log check reads its own level from an atomic the pack
  keeps and asks the loader again when the loader's settings change (#983).
- `test`: the fake host runs every callback a mod or data pack registered as
  its owner, as the loader does; `Installed::set_pack_log_level` sets a
  pack's level in the fake (#983).
- `abi`: `ChunkData` and `Event::CHUNK_LOADED` and `CHUNK_UNLOADED` say they
  name chunks of the world's entities, never of its terrain's voxels (#977).
- `events`: `ChunkLoaded` and `ChunkUnloaded` say they follow the chunks of
  the world's entities, which load only near players; the terrain's voxels
  load and unload without them and raise no event (#978).
- The loader calls no `init` of a mod already turned off before its turn,
  as `ready` skips it (#979).
- A data pack acts as itself everywhere the loader's copy of the API runs
  it: a thread the API spawns for it (the world store's write), its log
  level from `enshwick.toml`, each `info_once!` and `warn_every!` site, and
  under `FakeHost` its event, tick, game event, mod event, frame and HUD
  callbacks. A callback of the loader or a mod DLL that runs inside a pack's
  call no longer runs as the pack (#972).
- The terrain docs said the save keeps every terrain edit; it keeps one
  only inside a base, a Flame Altar's building zone (#963).
- A recipe rule a mod sets during init is kept until the loader resolves the
  recipe layout, where the loader refused it (#960).
- `Load::export_dir` and `enshwick.export_dir` name the folder
  `export/<id>/` by the mod id, which the loader lists for the mod's folder
  in `mods/`, where before they took the folder's name; a folder the loader
  lists no mod in has no export folder (#969).
- A world's saved weather fronts keep their phenomenon when the mods that
  add phenomena change: the loader writes the phenomena's GUIDs beside the
  save and gives each restored front its phenomenon's new place, where
  before a front could load as another phenomenon. A save from before the
  loader kept them frees only the fronts outside their layer's
  phenomena (#952).
- A mod item's meter lives: its template is registered with the game, where
  before the entity of a given item read as gone (#952).
- The workshop of `Load::add_workshop` places and spawns like the game's
  own: its template is registered with the game (#952).
- A data pack's code runs as the pack: its init, resource builders and
  callbacks log in its own log, read its own settings and turn the pack off
  when they panic or fail, where before they went out as the loader, a
  builder's warnings were lost and a panicking pack stayed on (#958).
- The book says what a pack's chest slots do: they reach every chest of that
  kind each time a world loads, chests placed before the pack too, and
  removing the pack loses no item, since the game gives a chest saved with
  more slots than its kind now has 128 slots (#958).
- A client joins a dedicated server again in the same game process: the
  loader closes its own socket of the last game before it opens one for
  the next, where before every join after the first was kicked for a
  missing hello (#953).
- A data pack names a vanilla item by its debug name in the running game,
  as `cargo enshwick pack check` allows; before, the loader read packs as if
  no game data were at hand and refused such a line (#953).
- `items = "backpacks"` (`Items::Backpacks`) picks the game's backpacks;
  before, it picked none (#953).
- `items::Kind::Consumed` (`ItemAction::CONSUMED`), and with it a use reward
  (`Action::Consume`, a pack's `consume` reward), comes once the game took
  the used item from its slot, within 5 seconds of the use, and never for a
  use the game refuses, such as a healing potion at full health, nor for
  one whose player does anything else before then, so a stack dragged or
  stashed out of its slot pays no reward. The fake
  host's `Installed::refused_use` plays such a use (#953).
- Text settings on the Mods screen take typing on client builds 26320373
  and 26430625: the loader finds the text field's kept-state key, which
  changes with each build, through an engine route (#950).
- A text the loader draws on the game's screens or a map marker label, such
  as an off reason with a Windows path, shows each backslash and the letter
  after it (#950).
- A dialog shows its title and message, and a notice its text, where the
  game turns a text tag into text: the loader answers its own tags in the
  game's text format too, not only in its text lookup (#950).
- A data pack, Luau scripts included, draws its windows, HUD and screen
  panels: their callbacks draw under the pack's own handle, which the
  loader takes, not the loader's (#950).
- A dropdown shows its options under its row and takes a pick; the game's
  own list opened under the loader's windows (#950).
- The host of a game opens no socket to its own player, which it reaches
  in process (#950).
- An action's handler, `allow`, `at` or `per_player` may add an action with
  `Actions::add`; before, the call deadlocked the thread (#947).
- A use reward (`Action::Consume`, a pack's `consume` reward) follows only
  a use every item filter allowed: a later filter's refusal no longer gives
  it (#945).
- Item filters and holds screen a player from the step after they join and
  from the first steps after a world loads, not only every 30 steps (#945).
- A craft reward pays each unit of a request to the player who asked,
  within two server steps of the request, instead of any request of the
  last 30 seconds (#945).
- A pack recipe whose `like` names a recipe instead of the item it makes is
  refused at its line by `cargo enshwick pack check` and at load; the line
  that turns a mod off for a served resource names the resource (#945).
- A crash report puts each hook a mod writes into the game's code down to
  that mod, also when two hooks lie close together or a jump ends just
  before an unreadable page (#945).
- A world a client hosts from Steam saves has its save information, GUID
  record and mod folders beside its save in Steam's `remote/` folder, as a
  local world does; before, they sat in the game folder and every autosave
  logged a warning (#955). Such a world starts from its mods' defaults until
  its `<stem>-mod-settings` and `<stem>-mod-data` folders are moved from the
  game folder to `remote/`. When Steam does not name that folder, the world
  has no mod folders rather than shared ones in the game folder.

## [1.0.0-alpha.12] - 2026-11-05

### Added

- `Load::extend_screen` adds a panel at an `Edge` of one of the game's
  screens, a `ui::HudScreenType` such as `StorageChest`, filled with the
  widgets of `Frame` once per frame while the player has that screen open.
  The loader draws it in a window of its own above the game's screen and
  never changes the screen's layout. `HostApi::ui_extension` and
  `UiExtension`, ABI 11.4. The fake host draws a screen's panels with
  `draw_screen`. The example mod `you.chest-panel` adds one to the storage
  chest screen (#933).
- Luau links an interaction filter with
  `enshwick.interactions.on_interaction`, whose `Interaction` is a table
  and whose kind an `InteractionKind` name (#933).
- `items` (unstable) lets a mod hear each player's use, trash, equip,
  unequip, pickup and placement of an item in the server's world, with
  `Load::on_item` for one item and kind and `Load::on_item_action` for all,
  and refuse a use, trash, equip or unequip the game's own way.
  `HostApi::item_filter` and `ItemAction`, ABI 11.5. Luau reaches them as
  `items.on_item` and `items.on_item_action`, and `content.item_id` gives
  the item id of a content id. An item's `on` table in `items.toml` binds
  its actions to global functions of the pack's `main.luau`. The fake host
  hands actions to the filters with `item_action` (#935).
- `rewards` (unstable) declares rewards for actions, with `Load::add_reward`:
  a craft, a level, a join, a loot, a quest, an item's use or pickup gives
  items, experience, a knowledge entry or a buff, every time, once per
  player, daily or once per world, at a chance; who got a limited reward is
  kept in the world's data. `Load::on_reward` holds a grant back and
  `Rewards` grants and gives from code. A pack declares them in
  `rewards.toml` (#935).
- A drop's chance in `loot.toml`, and a reward's chance, item count and
  experience in `rewards.toml`, may name a setting of `pack.toml`, which
  `Load::add_pack` declares as the mod's settings; each world's value takes
  effect as its settings arrive (#935).

- `controls` lets a mod declare controls: a name, a label, a default and a
  second `Key` (a scan code with exact modifiers, or a mouse button), the
  phases it fires on and a hold time, with `Load::control`. The client
  fires them only while its player is in the world with no window open, a
  building tool in hand included, and never takes the key from the game; a
  control linked to a `GameAction` fires on the server, after the step, for
  the player whose action fired, also without Enshwick on their client.
  Players rebind them on the Mods screen's
  Controls page, which captures the next key and warns when two controls
  share one, and the keys they change live in `mod-controls.toml` in the
  game's folder. `Actions::run` sends the server a player action from a
  control. `HostApi::control` and `run_action`, ABI 11.3. Luau declares
  them with `controls.declare`. The fake host reads keys on a client with
  `keys`, looks at a player's game actions with `game_actions` and takes the
  player's keys with `FakeHost::bind` (#930).
- `typed`, behind the `typed` feature, which turns on `unstable`: a typed
  item for every component, event and resource the game reflects (648, 128
  and 211 on server build 479) and every type they reach, generated from the
  game's reflection and read and written by field name through the
  `resources` codec. `ComponentType`, `EventType` and `ResourceType` resolve
  an item on the running build and check its shape, every nested array and
  optional included, so a type a game update changed turns off alone with
  the build named; they read and write a component, handle an event, read a
  game resource and serve a changed one in load order. The book's chapter on
  the game's own types shows each, and a concepts chapter explains what each
  kind of game update does (#931).
- Luau reaches the Rust API: each public module is a table of `enshwick`
  with the Rust names, a call a Rust mod makes on `Load` at init runs while
  `main.luau` starts, a Rust handle is a Luau object with its methods, and a
  hook or filter takes a function that returns the changed value. Altars,
  stats and the fog of war open only when a script asks at start, as a Rust
  mod does in `init`. A test
  holds every public function to its Luau mirror or a row of the book's
  table of what differs. `#[derive(Script)]` takes `#[script(name = "...")]`
  for a type whose name another module's type has (#928).
- `Load::export_dir` names `export/<folder>/` in the game folder, after the
  mod's folder in `mods/`, where a mod
  writes files for tools outside the game, and a script writes them with
  `enshwick.files.write_atomically`; `Relock::try_relock` takes a lock no one
  holds, also after a panic (#928).
- `resources` reads any game resource as typed values and writes it back:
  `Registry::game` holds every reflected type of the running build (names,
  hashes, layout, fields, constants, attributes), `Registry::decode` and
  `encode` turn a resource's bytes into a `Value` tree and back, keeping the
  bytes of what did not change, and `Host::resource_keys`, `content_hashes`,
  `game_content`, `game_value`, `serve_content` and `game_version` list and
  read the archive. `Load::serve_new` serves a new resource from a value
  alone. `crc32`, `crc64`, `Guid::NIL` and the content-hash form of a GUID
  join `fnv1a32`. `HostApi::resource_keys`, `content_hashes`, `types` and
  `game_version`, ABI 11.2 (#925).
- Luau reaches all of it as `enshwick.resources`, with values as plain
  tables, an absent optional in a list as `resources.none()`, and bytes as
  buffers;
  `enshwick.mods` names the running mods, the side, whether the game hosts
  the server (`hosting`) and the files of the mod's own folder; a patch that
  fails is logged and serves its base. `icons::encode_png` writes PNG
  (#925).
- `altars::set_base_cap` caps the bases of the world that runs below the
  mod's `Load::request_base_limits`, so a mod asks for its most at init and
  sets each world's cap from its settings; each call replaces the mod's own
  cap, the lowest of every mod holds, below 16 is refused, and
  `altars::base_limit` reads the capped bases. Behind `unstable`;
  `HostApi::set_base_cap`, ABI 11.1 (#923).
- Every loader error code and every question of "The game won't start" has
  its text in Ukrainian too, so the documentation site has its error pages at
  `/uk/errors/<code>/` (#927).

### Changed

- The loader ships as alphas, GitHub prereleases tagged `v1.0.0-alpha.N`
  whose notes list the supported game builds and what changed, in place of
  the rolling `dev` prerelease, which no longer follows `main` (#937).
- A Luau mod's runtime lives for the mod's life, so a function it hands the
  API, such as a resource patch, runs after start; `HostApi::serve_resource`
  says that several mods' patches on one key chain in load order (#925).

- **Breaking:** disabled recipes live only in each world's
  `<stem>-mod-settings/enshwick/recipes.toml`, written from the template as
  the world first loads; `mod-configs/enshwick.recipes.toml` is gone, the
  loader no longer makes `mod-configs/` and logs once at start that an old
  one is not read, and the console's `enshwick recipes disable|enable`
  answers that a world must run while none does. The console's `entity` and
  `placed` answers, the craft refusal log and container slot changes name
  the game's items and recipes as `enshrouded:<debug name>` with their
  number (#923).

- **Breaking:** the loader keeps few files of its own. Every log is in
  `logs/`, one file per source and run, `logs/<source>_<date>_<time>.log`,
  the newest 10 of each source kept and a new file past 8 MiB; the lines
  from before the log opens, the stub's too, open the loader's file, and
  `logs/start.log` stays only when the loader did not start; a crash goes to
  the loader's file and `logs/enshwick-crash_<date>_<time>.log`. `all.log`,
  `status.log`, the periodic summaries and `report.md` are gone; the
  console's `enshwick status` writes the summary into the loader's log. The
  loader's settings are `enshwick.toml` in the game folder, written with
  every key commented out, with `log_level`, `log_levels`, `diagnostics`,
  `diagnostics_timing` and `build_patterns`; `status_minutes`, `actions_key`
  (F7 is fixed), `language` (the console answers in English), `allowlist`,
  `motd` and `knowledge_guard` (the guard always enforces) are gone.
  `require_enshwick` is a world rule in
  `<stem>-mod-settings/enshwick/enshwick.toml`, read as the world loads. The
  allowlist with `World::set_allowlisted` and `World::allowlisted`
  (`HostApi::allowlist_set`, `allowlist_has`), offline bans, the message of
  the day, the console's `kick`, `ban`, `unban`, `give`, `give-to`, `spawn`,
  `teleport` and `allowlist` tools and the command file are gone; the
  console reads its input only. `World::account_action` refuses a kick or a
  ban of an account not online, and chat mutes belong to the world, in
  `<stem>-mod-data/enshwick/muted.txt`, `Missing` while no world runs;
  ABI 11.0 (#921).
- **Breaking:** the game's names read as full ids where a person reads or
  writes them: `enshrouded:<debug name>` in recipe files, the console and
  logs, and `mods::GAME_ID`; a bare debug name in a recipe file is a
  problem that says how to write it (#921).
- **Breaking:** two settings types of one crate that write one file fail the
  build, naming the file and pointing at both types, and a group's type is
  declared `#[enshwick::config(group)]` (#921).
- **Breaking:** a mod's side is `server` or `both`: the `client` side, the
  `join` flag (`ManifestFlags::JOIN`, `ModEntry::join`, a pack's `join`) and
  the relations `recommends` and `replaces` (`Relation::{RECOMMENDS,
  REPLACES}`, `mods::replaced`, `HostApi::replaced`, `ModInfo::replaced_by`,
  the lock's `[[replaced]]`) are gone. Every running mod of side `both`
  counts in the join check, and only such a mod opens message channels. A
  client turns its mods the joined world does not run off for that world and
  back on before the next; a data pack stays off until the game restarts.
  After a refused join or a kick for its mods, the client lists every mod
  that differs in the main menu, by id with the needed and the installed
  version. Servers no longer advertise `enshwick.page.*` or `enshwick.name.*`,
  and `mods::join_missing` carries ids and versions alone. The join timers
  `join_hello_seconds` and `join_note_seconds` are gone, fixed at 30 and 10
  seconds; ABI 10.0 (#919).
- **Breaking:** settings exist only per world. Each settings type writes
  `<stem>-mod-settings/<mod>/<file>.toml`, `settings.toml` unless
  `#[enshwick::config(file = "name")]` names another; the loader writes it
  from the type as a world loads, adds the keys a newer type has and removes
  the ones it lacks, with one log line each and one `.bak`. A value that does
  not read, or sits outside its choices, takes its default and a range
  clamps, each with a log line; a `validate` refusal keeps the values in
  force; settings never turn a mod off. `Load::on_settings` hands a type's
  values over at init (the defaults) and as each world loads;
  `Load::on_settings_change` also follows changes while it runs. A client in
  another's world runs on every non-secret setting its server sends after
  the hello. `Load::config`, `apply_config`, `on_config_change`, `Stage`,
  `Scope`, `Field::{stage, scope, synced}`, `Config::{values, merge_values,
  synced, merge_synced}`, `config(default)`, `HostApi::sync`, the
  `enshwick.sync.*` rules, `mod-configs/<id>.toml`, `<id>.next.toml` and the
  text overrides `mod-configs/<id>.texts.toml` are gone. `HostApi::settings`,
  `config_schema` and `config_check` take the file; ABI 9.0 (#917).
- **Breaking:** a world's mod settings live in `<stem>-mod-settings/<mod>/`
  and the data mods write for it in `<stem>-mod-data/<mod>/`, beside its
  save, so copying the save folder moves a world with its mod data. The
  loader's world files (`attachments.bin`, `stats.bin`, `game-settings.toml`,
  `knowledge.toml`, `mods.lock`) are in `<stem>-mod-data/enshwick/` and the
  recipes a world turns off in `<stem>-mod-settings/enshwick/recipes.toml`.
  The loader records each world's GUID from its save in
  `<stem>-mod-data/enshwick/world.toml`; when a new world takes a deleted
  world's save name, both folders are renamed to `<name>.old-<date>-<time>`
  and the world starts from the mods' defaults. `clones.toml`,
  `kept-stacks.toml`, `last-crash.txt` and the `developer-tools` marker are
  in `mods/enshwick/`. `mod-data/` is no longer read, the loader logs one
  line when it is there, and moved worlds are no longer taken over: move a
  world's files by hand (#915).
- **Breaking:** `Load::data_dir` answers the mod's data folder in the world
  that runs, `None` without one, and `World::data_dir` does the same at run
  time; `Load::mod_dir` names the mod's folder in `mods/`, where an
  installation `Hold` now lives. `World::key` is the save's stem, such as
  `world0`. `Store`, `Hold::for_world`, `world_holds`, `held_worlds` and
  `WORLDS_DIR` are gone: a world hold is a `Hold` in the world's data folder.
  The ABI is 8.0, with `HostApi::mod_dir` (#915).
- **Breaking:** each mod lives in a folder of its own, `mods/<mod>/`, and runs
  while that folder is there. In each folder the one DLL with the Enshwick
  exports is the mod; the folder's other DLLs are the mod's own, and a second
  mod DLL there is skipped with both files named. `cargo enshwick install`
  writes `mods/<id>/<package>.dll`. A DLL directly in `mods/`, `mods/chain/`
  and `mods/enshwick/mods.toml` are no longer read, and the loader logs one
  line for each it finds: move each mod into a folder of its own. There is no
  mod list: a mod is turned off by moving its folder out of `mods/`, and a
  link on an extension point can no longer be turned off alone. The stub no
  longer loads other proxies from `mods/chain/` (#913).
- **Breaking:** `mods/enshwick/enshwick.lock` became each world's record,
  `mod-data/enshwick/worlds/<world>/mods.lock`, written at every load of that
  world with each mod's folder, and what changed since the world last loaded
  goes to `loader.log` (#913).
- **Breaking:** an exported `#[enshwick::service]` ships its schema, a
  `[[service]]` table with its name, major, doc and layout hashes and a
  `[[type]]` table of its fields, `extern "system"` functions written as
  `fn(<arguments>) -> <answer>`; `Service::SCHEMA` holds it. A field a
  schema cannot describe fails the build of an exported service. The schema
  format is 3: a mod built with an older release asks for a rebuild (#910).
- **Breaking:** `Installed::start` takes the mod's `enshwick_mod_query`
  first, as `start(enshwick_mod_query, enshwick_mod_init,
  enshwick_mod_ready)`, and reads the manifest before init as the loader
  does: a mod the loader would refuse first, such as a developer mod while
  developer tools are off, is refused without its init running, whatever
  wrote that init. Add `enshwick_mod_query` to every call (#910).
- **Breaking:** every export names its kind: `Manifest::exports`, a pack's
  `exports` and `FakeHost::exports` hold `<kind>:<local name>`, the kind one
  of `point`, `event`, `call`, `service`, `content` and `stat`
  (`schema_section::Kind::ALL`, hidden, which the loader and the fake share). The loader and the fake match kind and name, so
  an exported point opens no content, stat kind or event of the same name;
  content and stat kinds still export their parts. A call, point, service
  or event is exported by `export` on its declaration, as
  `#[enshwick::point(name = "price", export, ..)]`, and ships its schema
  only then; `exports` in `[package.metadata.enshwick]` takes
  `content:<name>` and `stat:<name>` alone. `export` with `of`, or on a full
  name of another mod's id outside a test, fails the build. A dependency's
  stat kind is named `deps::<alias>::stat("<name>")`, which `open_stat`
  checks as a stat; `deps::<alias>::id` checks a content export. Since this changes what
  `Manifest::exports` means, the ABI is 6.0: the loader refuses a mod built
  for 5.x with "rebuild it for Enshwick" and its version. Write each export
  with its kind, move declarations onto `export`, and rebuild every mod
  (#883).
- **Breaking:** `declare_mod!(Type)` takes no `points`, `events` or `calls`
  lists. Each exported `#[point]`, `#[event]` and `#[call]` writes its schema into the DLL's
  `.enshpt` section; the manifest collects them from there at the loader's
  first query, and `cargo enshwick deps` from the DLL file. An exported
  event a schema cannot describe fails the build, as a point or call does
  whether exported or not.
  `enshwick_mod_query` builds the manifest at its first call, returns null
  when that panics, and the manifest lives until `enshwick_mod_shutdown`,
  which frees it. Remove the lists (#868).
- `net::MAX_MESSAGE` is 8192 bytes, up from 4096, and `Channel::send` and
  `broadcast` encode into a stack buffer instead of allocating (#889).
- Channel messages travel on the loader's own channel of the game's
  connection instead of hidden chat lines: reliable and in order, and
  refused when they start like executable code. `SendError::OverRate` also
  means the link to the peer holds too much. A message that starts like
  executable code fails with the new `SendError::LooksLikeCode`, which
  `net::looks_like_code` tells beforehand, and the loader's other refusals
  with `SendError::Refused`. A client and server must both run this loader
  to talk (#885).
- **Breaking:** a mod with `developer: true` and `dev_reload` run only
  while the file `mods::DEVELOPER_TOOLS`, `mod-data/enshwick/developer-tools`,
  exists, on a dedicated server, a client and a game a client hosts alike:
  without it the mod is off before its `init`, with
  `mods::DEVELOPER_TOOL_OFF` in the log and on the Mods screen, and
  `dev_reload` stays off with a warning. The fake host has developer tools
  off by default and refuses a developer mod's `init` the same way; a
  developer mod's tests add `FakeHost::developer_tools(true)` (#893).
- **Breaking:** mod settings show only behind each mod's Settings button on
  the Mods screen: the player's own settings, then the world's. In a world
  the game runs, single player or hosted, those are that world's; in the
  main menu they are the settings new worlds start with, written to
  `mod-configs/<id>.toml`. Joined to another's server, the page opens the
  server's settings of the mod. The game's world settings show no mod rows
  any more. A dedicated server still keeps a world setting changed before
  its world loads in `mod-data/<id>/worlds/next.toml` for that world. Nothing
  changes for mod authors (#878). The screen that creates a world no
  longer offers mod settings; set them for new worlds in the main menu. In
  another's world the page shows the world settings the server sends,
  read-only, also on a server without Enshwick, and it follows a world that
  starts or stops while it is open. A world that loads takes a
  `next.toml` left from an older version once and logs it (#899).
- **Breaking:** the ABI version is a major and a minor, `abi::ABI_MAJOR`
  and `abi::ABI_MINOR`, starting at 5.0, instead of the layout hash
  `ABI_VERSION`. A field appended to a struct that starts with a `Header`, a
  function appended to `HostApi` or a new type is the next minor; any other
  layout change is a new major, and the layout tests fail without the bump
  the change needs. The loader loads a mod of its major built for its minor
  or an older one, and refuses another major or a newer minor with the
  versions and what to do, through `abi::check_abi` and its
  `CheckAbiError`. `Header` holds `abi_major` and `abi_minor` in place of
  `abi_version`, `enshwick_mod_abi` returns the major, `abi::OTHER_ABI` is
  gone, and the C header defines `ENSHWICK_ABI_MAJOR` and
  `ENSHWICK_ABI_MINOR`. Rebuild every mod once (#852).
- **Breaking:** `abi::Headed` has the constant `BORN`, the end of the fields
  every writer of the ABI major knows, which the loader requires of a mod's
  struct; `impl_headed!(T, born = N)` sets it once a minor appends to `T`.
  `abi::check_major` checks the value of `enshwick_mod_abi`, and
  `check_abi` takes the manifest header's `u16` major. `CheckAbiError`
  gains `BeforeMajors` for a DLL built before ABI majors, and its
  `OtherMajor` holds a `u16`. The ABI major moves only to the next one
  (#865).
- Item data on a leaving player's items waits for that character's next
  join and attaches again where the same item lies in the same slot,
  instead of going aside in the world's file; an item's stats apply again
  to the character that wears it (#782).
- **Breaking:** a mod does not build when a key of its texts lacks a text
  in a language of the new `Language::REQUIRED`, `en` and `uk`; add the
  missing Ukrainian texts (#838).
- **Breaking:** a mod's texts live in `src/texts.toml`, which
  `declare_mod!` embeds: `load.texts()` takes no argument and returns `Off`
  for a crate without the file, and `load.texts_from(toml)` hands over texts
  from a string, such as a data pack's. `enshwick::texts!()` writes
  `pub mod texts`, a constant per key such as `texts::SAFE_NAME` for
  `"safe.name"`, with `texts::value`, `texts::ALL` and `texts::TOML`. The
  build fails when a key has no `en` text, a table is no language code, or
  a translation fills in other values than its `en` text. Replace
  `load.texts(include_str!("texts.toml"))` with `load.texts()` (#827).
- **Breaking:** `Action::at` takes a closure that returns
  `Result<Subject, FindSubjectError>`; a `String` or `&str` reason turns into
  a `FindSubjectError` with `into`, so `Err(ask.say(..))` becomes
  `Err(ask.say(..).into())`. The hidden `packs::reload_loot` returns
  `loot::ReplaceDropsError` (#824).
- **Breaking:** `#[enshwick::config]` declares a settings struct: it derives
  serde's `Deserialize` and `Serialize` through the API's serde, refuses
  unknown keys and derives `Config`, so a mod needs no serde for its
  settings. `#[enshwick::config(default)]` adds serde's `default`;
  `validate` and `text` are the derive's struct attributes. `#[derive(Config)]`
  alone refuses a struct without `#[serde(deny_unknown_fields)]`. Replace
  `#[derive(serde::Deserialize, serde::Serialize, enshwick::Config)]` and
  `#[serde(deny_unknown_fields)]` with `#[enshwick::config]` (#795).
- **Breaking:** stat kinds and affixes take a content `Id`:
  `load.define_stat(id!("luck"), &def)`, `load.define_affix(id!("bear"), &def)`,
  and `load.open_stat(deps::lockers::id("luck")?)` for a dependency's kind.
  The `stats::builtin` kinds are `Id` constants. The text that names a kind
  or an affix is `<name>.name` in its owner's texts, and `stats::name_key`
  gives it: `StatDef::additive`, `multiplicative`, `flag` and `AffixDef::new`
  lose their text key, and `StatDef` loses `text`. `StatInfo` gains `text`, and
  `StatDef::from_data` is safe. Opening a kind of a loaded mod that does not
  export it returns `Off`, with the owner's exports in the log, and so does
  an id whose `<name>.name` is longer than a content id's name may be. The
  fake host refuses the same, the exports for a mod `FakeHost::exports`
  names (#797).

- **Breaking:** a mod's content is its own until its `exports` list the
  name, or the name it is a part of: `exports = ["locker"]` lets other mods
  name `locker` and `locker.recipe`. A required dependency's
  `deps::<alias>::id` is no longer `const` and returns `Result<Id, Off>`,
  `Off` for a name the dependency does not export; an optional one's returns
  `None` for it. `Load::add_pack` returns `Off` for another mod's content id
  a pack names outside that mod's exports. The loader logs the id with what
  the mod exports. `HostApi` gains `use_content`, which changes
  `ABI_VERSION`; rebuild every mod. The fake loader refuses the same for a
  mod `FakeHost::exports` names (#798).

- **Breaking:** `pack.toml` names its dependencies as
  `[package.metadata.enshwick]` does: `requires`, `after`, `recommends` and
  `breaks` are tables keyed by alias, each a mod id or `{ id, version }`, and
  `replaces` lists the mods the pack takes the place of. `PackManifest::requires`
  holds `PackDependency` values, and `PackManifest::dependencies` lists every
  table with its `Relation`. An alias is `[a-z][a-z0-9_]*`, unique across the
  tables, and neither `vanilla` nor a Luau keyword; `after` takes no version.
  Write `"you.lockers" = ">=0.1"` as
  `lockers = { id = "you.lockers", version = ">=0.1" }` (#784).

- A pack's files name a required mod's content by the alias `pack.toml`
  gives it, `lockers:locker` for `you.lockers:locker`, as `deps::lockers::id`
  does in a Rust mod; the alias of a mod the pack does not require is a
  problem at its line. A full content id still works (#785).

- A Luau mod's `enshwick.define_event` takes a local name, `greeted` for
  `<id>:greeted`, and `enshwick.deps` gives each mod of `requires`, `after`
  and `recommends` in `pack.toml` by its alias, with its `id` and
  `running()` (#788).

- **Breaking:** a mod's calls, points, services and events are its own until
  `exports` in `[package.metadata.enshwick]`, or in a pack's `pack.toml`,
  lists their local names. The loader refuses another mod everything else of
  a loaded mod: `Load::caller`, `extend_point`, `observe_point` and
  `on_mod_event` return `Off`, `service` returns `Off` and
  `optional_service` returns `None`, with the reason in the log. A point,
  event or call listed in `declare_mod!` and missing from `exports` fails the
  build. `Manifest` gains `exports` and `export_count`, which changes
  `ABI_VERSION`; rebuild every mod. The fake loader refuses the same for a
  mod `FakeHost::exports` names (#783).

- **Breaking:** `Load::caller`, `Load::extend_point` and `Load::observe_point`
  return `Off` when the mod requires the call's server or the point's owner
  and it does not serve or define it as declared; a required mod initialises
  first, so the loader knows at once and logs why. Before, the call answered
  `CallError::Unmatched` and the link stayed inert. The fake loader refuses
  the same: serve a required mod's call with `Installed::serve_call`, and
  define its point with the new `Installed::define_point`, before the mod
  starts (#777).

### Added

- Build tables for client 26320373 and dedicated server 26319703, the game
  update of 2026-11-02, so neither carries the previous table by patterns
  (#914).
- Build tables for client 26430625 and dedicated server 26430253, game
  version 1287369 of 2026-11-04 (#932).
- The Mods screen has an Interface demo button while developer tools are
  on in that game, so the demo opens without the console (#897).
- `World::set_muted` and `World::muted` mute a player in chat and lift the
  mute, on a dedicated server or in a game a client hosts. The server drops
  every line a muted player types before another player reads it and tells
  that player with a notice at most every 30 seconds; lines mods send pass.
  The loader keeps the mutes in `mod-data/enshwick/muted.txt` and never
  mutes an administrator online or the host. `HostApi` gains
  `chat_mute_set` and `chat_mute_has` as ABI 5.1; the fake keeps the list
  (`Installed::muted`) (#881).
- `enshwick::deps!()` makes `src/deps/<alias>.rs`, which
  `cargo enshwick deps` writes, the public module `deps::<alias>` when the
  file exists (#803).
- The built-in stat kinds have names in the loader's texts, in every
  language the game offers: the `<kind>.name` key of each kind's
  `StatInfo::text`, such as "Max Health" for `health_max.name` (#831).
  The names are the game's own texts in each language, so lightning reads
  "Shock" as the game's tooltips do (#851).
- `Relock`, `RelockRw` and `RelockWait` take a `Mutex`, an `RwLock` and a
  `Condvar` wait under the API's poison policy: the state a panicking thread
  left is kept, the poison cleared and the place logged, so a mod needs no
  lock helper of its own (#821).
- `FakeHost::items`, `recipes`, `workshops`, `crafting_menu` and `ui_images`
  set every engine fact one area's layouts resolve, and `FakeHost` answers
  `EngineFacts`, so a test lays out its game data by `ItemLayout` and the
  other layouts rather than by offsets of its own. `ImageLayout::resolve`
  takes any `EngineFacts`, as the other layouts do (#806).

- `FakeHost::world` sets the current transform and used item types `World`
  needs and where a transform keeps its position, orientation and scale;
  `FakeHost::inventory` sets what `Inventories` reads slots, durability and
  meters by. `test::facts` names each fact a test lays its blobs out by, and
  `FakeHost::without_fact` drops one, so a world or inventory test copies no
  type hash or offset of its own (#809).

- `FakeHost::game_events` sets the hit, death and equipment change events,
  the hit history and experience credit a death reads its killer from, and
  damage bits, an attack category and an equipment slot numbered unlike the
  API's; `FakeHost::map_waypoints` sets each player's waypoint;
  `FakeHost::effect_events` sets the NPC command, threat and ping events
  `Effects` raises; `FakeHost::loot` sets the components loot from destroyed
  things checks. `test::facts` names their facts, and the loot slot's, so a
  test of these areas lays out and reads its events by them (#871).

- `FakeHost::vanilla_items` lists items in the game's item registry by
  their debug names, so a mod's test finds them with
  `content::vanilla::item` as in the game (#880).

- `FakeHost::meters` sets what `MeterLayout` resolves, and
  `FakeHost::container_kinds` the markers `ContainerKinds` tells holders
  apart by, which `test::facts` names, so a mod's test of item meters or
  containers copies no type hash or offset of its own (#877).

- `content::vanilla::item`, `recipe`, `loot_table`, `station` and `workshop`
  name the running game's content by its debug names, so a mod's code holds
  no vanilla GUID; `deps!` offers them as `deps::vanilla`, and no dependency
  takes the alias `vanilla` (#780).

- `deps!()` writes a module `deps` with a module per alias of `requires`,
  `after` and `recommends`: `deps::lockers::ID` and `deps::lockers::id("locker")`,
  the dependency's content id, a constant for a required dependency and an
  `Option` for an optional one, `None` while it does not run. `#[call]`,
  `#[point]`, `#[service]` and `#[event]` take a local name, the mod's own,
  or one of a dependency with `of = <alias>`. `Id::of` and `Id::try_of` build
  an id from its two sides in a constant, and `mods::is_running` says whether
  a mod runs (#773).

- `content::id!("locker")` names the mod's own content by its local name:
  a constant `Id` whose mod side, `<mod>:`, comes from the
  `[package.metadata.enshwick]` table at build time, so its GUID stays a
  constant. `Id::part("name")` and `Id::try_part` derive the id
  `<mod>:locker.name` from it, in a constant too (#766).

- `Ui::map_marker_types` lists the client's map marker types from the
  game's registry, each a `MapMarkerType` with its key, its
  `MarkerTypeSorting` group and the game's flags, and `Marker::marker_type`
  shows a mod's marker with one of them (`HostApi::map_marker_types`,
  `MapMarkerTypeInfo`; `UiMarker` takes the type in place of its reserved
  field); the fake host serves a registry with
  `FakeHost::map_marker_types` (#728).

- `developer: true` in `manifest!` (`ManifestFlags::DEVELOPER`) marks a mod
  as a developer tool. While one runs, or the file
  `mod-data/enshwick/developer-tools` exists, every player reads an
  upper-case warning in chat at join, on the Mods screen and in the main
  menu's line. The loader setting `motd` is a message of the day players
  read in chat at join (#726).

- `Load::apply_config` reads the settings, hands them to a closure at once
  and after every change, and returns them (#721).

- A part of the loader that a panic turns off, such as the HUD items, a
  window or map markers, shows on the Mods screen and in the status summary
  with its reason; each window and menu entry turns off alone. A player who
  joins a server gets one chat line naming what is off there (#683).

- A panic in a mod logs a line at `error` with its message, place and
  thread, and the reason of a mod a panic turns off carries the panic's
  message; a panicking `init` turns the mod off with that reason (#682).

- A world whose save moved to another folder keeps its mods' data: the
  loader moves the folders of the world it came from to its new key, or
  copies them while the old save is still there (#677).

- A world store whose file does not read shows on the Mods screen as the
  feature `world store <name>` off, with the reason (#675).

- `WorldStore::shut_reason` says why the running world's file did not read
  while the store stays shut, and the loader's status summary shows it
  under the mod (#668).

- `PackManifest::enshwick`: a pack's `pack.toml` may name the Enshwick
  releases it is made for, such as `enshwick = "=1.0.0-alpha.12"`; a loader
  of another release skips the pack and logs both versions (#661).

- `files`: `read_if_present` and `read_to_string_if_present` tell a
  missing file from one that does not read, and `write_atomically` writes a
  file whole, flushed to disk, keeping the old one as `backup_path`.
  `Store`, `WorldStore` and `Hold` write through it, so a store keeps
  `<name>.toml.bak` beside its file (#658).

### Changed

- **Breaking:** `NewWorkshop` takes one `id`, the station item, and derives
  the rest from it: the template `<id>.template`, the workshop
  `<id>.workshop`, the recipe `<id>.recipe` and the tags `<id>.name` and
  `<id>.description`, each also a method. `named` and `described` choose
  whether the station takes its own name and description, `recipe_cost`
  adds the station's recipe, and `NewWorkshop::new(id, like)` fills the
  rest. `StationRecipe` and the fields `item`, `template`, `workshop`,
  `name`, `description` and `station_recipe` are gone. A data pack's
  workshop names its texts by the same tags (#768).

- **Breaking:** `Id::name` is gone: an `Id` writes its full id through
  `Display`, and two ids are equal when their GUIDs are. `Id::new` takes
  an id in full, for tests and tools, as do `BuffTypeId::named` and the
  other `named` game ids; a mod's own buff type is
  `BuffTypeId::from_guid(id!("warmth").guid())` (#766).

- **Breaking:** a mod names itself once, in the `[package.metadata.enshwick]`
  table of its `Cargo.toml`: `id`, `name`, `side`, `join`, `reload`,
  `developer`, `replaces`, the `builds` rows and its dependencies under
  aliases (`requires`, `after`, `recommends`, `breaks`, each a mod id or
  `{ id, version }`). `declare_mod!(Type)` reads the table when the mod
  compiles, takes the version, description, authors, licence and homepage
  from `[package]`, and lists points, events and calls after the type
  (`declare_mod!(Type, points: [..])`). `manifest!` and `Mod::manifest` are
  gone; a key, side, id or alias the table does not take fails the build
  with its name. `test::declare_mod!(Type, "<toml>")` gives a test mod its
  table inline, so one test binary declares several mods (#759).

- `stats::Origin::Mod` names its mod by the new `mods::ModIndex`, its place
  in load order, which `ModIndex::entry` and `ModEntry::index` resolve
  (#719).

- The fake host refuses the hooks, mid-function hooks, code patches,
  points and rates the loader refuses, with the loader's status: a mod's
  test now fails where the game would. Both take the rules from one place.

- The ABI layout file lists each field's type, and a function pointer's
  signature, so a changed signature is another `ABI_VERSION`. Every mod
  needs a rebuild (#681).

- `Stats::set_item`, `Affixes::roll`, `Load::add_loot_affixes`,
  `Load::add_craft_affixes`, `AffixLoot` and `AffixCraft` need the
  `unstable` feature: they keep their values as item data, which sets aside
  a value on an item no load finds after a restart (#673).

- `item_data` and `entity_data` need the `unstable` feature: after a
  restart, a value on an item no load finds stays aside. `ItemInstance` moved
  to `inventory`, and `stats::SetError` names the error of
  `Stats::set_item` (#665).

- `abi::ABI_VERSION`, a hash of the committed ABI layout, replaces
  `ABI_MAJOR` and `ABI_MINOR`; `Header` carries it as `abi_version`, and the
  C header defines `ENSHWICK_ABI_VERSION`. The loader, a mod's host check and
  `cargo enshwick` load only mods built for exactly this version and name the
  release to rebuild any other for (`abi::OTHER_ABI`). `cargo enshwick new`
  pins `enshwick` with `=`. Every mod needs a rebuild (#657).

### Fixed

- A damage filter sees shroud damage on the dedicated server build
  26319703: the loader hooks the shroud site one instruction earlier, where
  no branch of its function lands inside the hook (#922).
- Hosting a world no longer crashes about 5 seconds after the character
  loads: the own channel asks the session for its packet size before it
  makes a socket, and a session whose packets hold no message keeps the
  channel off with one warning, without a socket or a retry every 5 seconds,
  until the game ends (#932).
- A crash in game code the loader called, with no mod's code on the stack,
  reads `in the game, called from enshwick` with the frame and its offset
  in the loader, not that Enshwick was not involved (#932).
- Game code changed while a mod loaded, ran `init` or ran `ready` is put
  down to that mod only when the change jumps into its DLL, directly or
  through up to three more jumps, also for a DLL skipped as no mod of this
  ABI; any other change reads `by another program`, without the mod's
  stage, since another program's thread may write at any time (#932).
- A changed client no longer passes the chat mute or forges an emote or
  notice line by sending a chat type other than typed text: the server tells
  a player's line by where the game posts it and posts it as typed text,
  with a log line naming the account (#919).
- A dedicated server where no mod subscribes to `WorldLoaded`,
  `ShutdownStarted` or data that needs them now loads each world's settings
  and record and raises `WorldLoaded` again (#916).
- Hooks that read the caller frame are armed on a client whose own start
  uses most TLS slots: the loader takes its slots first, and gives back the
  ones it took when it cannot have all of them (#914).
- A mid-function hook is refused when the loader cannot move its first bytes
  cleanly: a branch of its own function lands inside them, or the encoder
  would split one of them into several. Relays and trampolines now carry
  unwind data, so a stack walk or exception passes a frame stopped in one; a
  function whose unwind data cannot be read is still hooked, without it, and
  the loader logs why (#872, #873).
- An item data value cleared off the game thread, or while its slot did not
  read, stays cleared: the value a joining player's item still waited for
  in the world's file no longer comes back at the next look (#911).
- A mod whose binary holds declarations that do not read, such as a
  dependency's built with another enshwick, or damaged ones, gives the
  loader its manifest without schemas or exports and stays off with the
  reason: the log, the status report and the Mods screen name the mod and
  say, in the loader's texts, to update Enshwick for a newer format or to
  rebuild the mod for an older one or damaged declarations. Before, the loader got no manifest and
  could not name the mod. `ReadSectionError::Format` no longer gives advice
  in its message (#906).
- Dev reload counts every call a mod makes since it loaded, by its own
  handle: a mod that registers actions, channels, filters or other state
  in `ready` or a callback is refused a reload with those calls named,
  instead of leaving them with the old copy. Queries and one-shot changes
  never stop a reload (#886).

- `files::write_atomically` tries the backup and the rename again for
  about 50 ms while another program or writer briefly holds the file, and
  writers of one file at the same time no longer refuse each other's
  writes (#900).
- A code patch that a thread stands in, or would return into, through every
  wait at arming is not written under it: the patch waits, and its mod
  waits with it, its hooks and mid-function hooks passed by while the rest
  of it runs; the Mods screen, the status report and the log show it
  waiting. The patch is tried again after the first step of the running
  world, or the first interface frame on a client that does not host, then
  up to three more times a minute apart; a thread that cannot be checked
  counts as one inside. When it goes in, the hooks come on; after the last
  try the log names the patch with "a thread stayed inside it through every
  wait" and the mod is turned off. A loader patch does not wait: it is
  skipped and turns off only the feature it serves (#860, #903).
- A poisoned lock's warning names the line that took the lock, not a line
  inside the API (#821).
- The command file runs no line again when it does not read for a while,
  as when another program holds it open, and logs that at most once a
  minute and again when it reads. A file that is deleted and made again, or
  replaced by another file, runs from its start when it holds none of the
  lines that ran, and counts as an edit otherwise. A file that does not read
  at start skips only the lines it held then, and says how many (#861).
- A bad value in `mod-configs/enshwick.toml` holds closed only the
  protections it could stand for: its own key, or every missing one when the
  bad key is unknown or a table. A file that is not TOML names the line and
  column where it stops reading, and the log says the defaults apply
  instead of the rest of the file (#856).
- A `mods.toml` that does not read or parse, such as one saved in another
  encoding or held open by another program, is never written over with
  every mod on, and the mods off in its backup `mods.toml.bak` stay off. An
  `enshwick.lock` that does not read is left as it is. The loader writes
  `mods.toml`, `enshwick.lock`, `enshwick.toml`, a mod's default settings
  and the recipe template atomically, keeping the old file as `.bak` (#863).
- `Load::world_store` refuses the names `settings` and `save`, whose files
  the loader keeps in the same world folder, so a store no longer writes
  over the world's settings (#680).
- The loader and its stub load mods, native DLLs, chained proxies and the
  loader itself so that their imports resolve only from the DLL's own
  folder, the game's folder and the system folder, never from the current
  directory or `PATH`; a mod ships every DLL it imports beside itself. The
  crash report hooks the system's `dbghelp.dll`, never a proxy of that name
  (#869).

- Migrations run once per world, all or nothing: each world keeps its own
  record, `.migrations.toml`, and a copy of the files each migration
  changed under `.backup/`, put back when a write fails or the game stops
  part way. A record or store that does not read turns the mod off with
  the reason instead of being skipped or read as empty (#670).
- The loader's log files roll over during a run as they do at start, once
  `all.log` would pass 32 MiB or `loader.log` or a mod's file 8 MiB, so
  a server that runs for weeks no longer fills its disk with logs; the new
  file's first line says where the earlier lines went (#876).

- `Store` and `WorldStore` fill every key a file lacks from `T::default()`,
  so a store whose type gained a field still reads (#672).

- `WorldStore` writes a value again at the next save when its background
  write failed, instead of dropping the change (#667).

### Removed

- **Breaking:** the shared actions window is gone: the F7 key, its line on
  the HUD, the Actions button above the Mods screen's list, `Action::hint`,
  `sheet::Entry::hint` and `ActionCall::HINTS`. A mod's actions show under
  its name on the Mods screen, and its own controls and screen panels run
  them with `Actions::run` (#933).
- **Breaking:** dev reload is gone: the loader setting `dev_reload`, the
  copies under `mods/enshwick/reload/`, `ManifestFlags::RELOAD`, the
  `reload` key of a mod's table, `Mod::shutdown` and the
  `enshwick_mod_shutdown` export with `abi::exports::SHUTDOWN`. A changed
  mod takes a restart. The manifest `enshwick_mod_query` returns stays
  valid for the process. Delete `reload = true` and `shutdown`; an old
  `mods/enshwick/reload/` folder may be deleted by hand (#912).
- **Breaking:** the loader no longer watches data packs' files: a changed
  `loot.toml`, changes of values and Luau scripts take the next start, on a
  dedicated server as on a client. `Pack::same_content`,
  `Pack::same_beside_changes` and `enshwick-luau`'s `Runtime::reload`,
  `Runtime::runs` and `ReloadError` are gone (#912).
- **Breaking:** the console's `enshwick reload <mod id>` is gone; a mod
  reads a changed settings file as before, through its settings change
  event (#912).
- **Breaking:** the migration machinery is gone: `Load::migrate`,
  `migrations::Migration`, `Migrations`, `MigrationError`,
  `abi::MigrationStep`, `abi::Migration`, `abi::MigrationSet`,
  `abi::MigrateFn`, `HostApi::migrate`, the fake host's `migrate_settings`,
  `migrate_store`, `migrations` and `FakeHost::other_settings`, and the
  loader setting `migrations_dry_run`. The loader no longer rewrites a mod's
  settings or world stores for a new version and keeps no record: a mod that
  changes a format reads the old one itself. Delete `migrations_dry_run` from
  `mod-configs/enshwick.toml`; `mod-data/enshwick/migrations.toml` and each
  world's `.migrations.toml`, `.backup/` and `.under-way.toml` are no longer
  read and may be deleted by hand (#912).
- **Breaking:** content renames stand alone: `Load::rename_content` takes a
  `content::renames::Renames`, built with `Renames::new().rename(old, new)`,
  in place of `rename_content` steps, and `content::renames::current_id`
  replaces `migrations::current_id`. `HostApi::rename_content` takes an
  array of `abi::ContentRename` in the place of `HostApi::migrate`, so the
  ABI is 7.0: rebuild every mod (#912).
- The loader no longer saves the build table it carries to a game build
  without its own: it searches the patterns on every start. A
  `mod-data/enshwick/carried-<side>.txt` and its `.bak` left by an earlier
  build are no longer read and may be deleted by hand (#912).
- **Breaking:** `was: [...]` in `manifest!` and `Relation::WAS` are gone. A
  mod keeps one id: the loader no longer copies a former id's settings and
  data, no longer finds a mod, its requirements, points, calls, events,
  content or stats by a former id, and `migrations.toml` no longer keeps a
  `[carried]` table. `replaces: [...]` still takes ids of the old form,
  which `abi::is_valid_former_id`, now `abi::is_valid_old_form_id`,
  accepts (#756).

## [1.0.0-alpha.11] - 2026-10-28

### Added

- The `unstable` feature also holds what rests on engine facts not yet
  seen in the running game: the game rates (`rates`), changes of
  `Field::MaxStackSize` and `Field::Slots` from a mod (refused with `Off`
  without the feature), the damage filters, `events::PlayerDied`, the
  `killer` fields of `EntityDiedEvent`, and `Load::add_recipe`,
  `add_recipe_with` and `derive_recipes` (#649).

- `Load::request_base_limits` takes up to 179 bases: the loader grows the
  game's bases objects and keeps bases past 16 behind its array, on both
  sides, and `BaseLimit::max_bases` reads 179, or 16 on a build where the
  loader cannot arm every site it needs. A server holds the bases its mods
  asked for; a joined client with bases past 16 armed groups every base the
  server does, and one without groups 16. `FakeHost::without_base_limit`
  plays a build without the base code, `FakeHost::without_bases_past_16` a
  build that raises the zones but keeps 16 bases, and
  `Installed::requested_base_limits` tells the largest request; it replaces
  `base_limit_raised`. The docs of `altars::grouping` and
  `altars::base_limit` name both requests (#647).

- The `unstable` feature holds what may still change in any release, so
  1.0 promises only the rest: `world`, `map` with `Load::fog_of_war` and
  the map markers of `ui`, `actions`, `console`, the script binding
  (`script`, `#[derive(Script)]`, `#[script]`), calls (`#[call]`,
  `Load::serve_call`, `Load::caller`, `services::Call` and its kin) with
  the described calls, events and points of script runtimes,
  `Effects::teleport` and `Effects::ping`, the knowledge revokes and
  `Load::on_knowledge_input`, `Load::request_base_limits`, and the world
  edit variants of `WorldError`. Each item says so in its docs, the Luau
  definitions mark `world` and `caller`, and the book's Stability chapter
  names what is stable and how the ABI versions from 1.0 (#643).

- `Load::request_base_limits`, behind the new `unstable` feature, asks for
  up to 255 altars a Flame level (ABI 4.1,
  `HostApi::request_base_limits`), and `BaseLimit` tells the game's own
  bases and the most a mod may ask for, with `BaseLimit::past_vanilla`
  (#641).

- `abi::DAMAGE_TYPES` (`ENSHWICK_DAMAGE_TYPES` in C), the length of a hit's
  damage amounts, and `DamageTypeNew::index`, a type's place in them; the
  loader's damage and stats code count by them too (#637).

- `voxels::PER_METRE`, the voxels a metre on every axis, which the
  loader's terrain edits now read too (#632).

- `RecipeKind::ComfortFurniture` (`comfort-furniture` in a data pack, ABI
  code `RecipeKind::COMFORT_FURNITURE`) for placed props that carry the
  item tag of the game's Comfort Furniture crafting group; a prop with the
  Decorative tag too stays `Decorative` (#628).

### Changed

- Breaking: names the game has take its own. `DamageCause::Impact`,
  `FallDamage` and `Suffocated` (`DamageCheck::IMPACT`, `FALL_DAMAGE`,
  `SUFFOCATED`) replace `Hit`, `Fall` and `Drowning`;
  `Rate::FactoryProductionSpeed` and `ResourceDropStackAmount`
  (`RateId::FACTORY_PRODUCTION_SPEED`, `RESOURCE_DROP_STACK_AMOUNT`) replace
  `WorkshopSpeed` and `ResourceDrops`; `ExperienceGroup::ExplorationQuests`
  (`RateId::EXPERIENCE_EXPLORATION_QUESTS`) replaces `Exploration`;
  `Teleport::search_best_spawn_position` replaces `best_spawn`, and
  `Spawn::orientation` with `orientation_yaw` replaces `facing` and
  `facing_yaw`. Names that are enshwick's own say so in their docs (#648).
- Breaking: `abi::TickData::elapsed_steps()` is gone; read the field of
  the same name. Its accessors no longer guess for loaders of an older
  minor, which the ABI never pairs with a mod, and the ABI docs and the C
  header drop their per-minor history notes (#645).
- Breaking: `SkillNode` is `abi::SkillNode` alone; `world` no longer
  re-exports it (#643).
- Breaking: `WorldError` is `enshwick::WorldError`, shared by every module
  that acts on the server's world, and `ComponentType` is
  `ecs::ComponentType`, resolved with `ComponentType::resolve` in place of
  `World::component_type`; `WritableType::resolve` replaces
  `World::writable_type`. `ecs::Components::component` takes a
  `ComponentType` in place of its hash. Inventories, loot and the kill
  credit of `EntityDiedEvent` no longer need the world's reads (#642).
- Breaking: ABI 4.0. Filter inputs and the death record take the game's
  names. `Load::on_craft` hands the game's `CraftRecipeEvent` (`CraftRequest`
  is gone) and `CraftCheck` names its fields `crafting_operator_id`,
  `crafting_station_id`, `recipe_id` and a `u16` `recipe_amount`.
  `PlayerDeathData` has `cause_guid`, `was_deadly_fog` and the causes
  `IMPACT`, `FALL_DAMAGE` and `FOG`. `DamageCause::Fog` and `DeadlyFog`
  (`DamageCheck::FOG`, `DEADLY_FOG`) replace `Shroud` and `ShroudDeath`.
  `BedCheck` names `npc_id` and `bed_id`. The zone check is
  `Load::on_building_zone_check` with `BuildingZoneCheck`
  (`HostApi::building_zone_check_filter`), and the fake host's
  `building_zone_check` (#635).
- A mod whose `init` returns `Err(Off)` hands the loader its reason: the
  Mods screen shows it under the mod and the log names it, an `Off::text`
  in the reader's language (#634).
- Breaking: `Entity` has no serde form, since each load of a world numbers
  its entities anew; `Prop` has none either, and a `Capture` keeps
  `CapturedProp`, the item and transform without the entity. Entity data
  refuses an entity no load finds again, a player's character, with
  `SetError::NotKept` (`Status::NOT_KEPT`, ABI 3.5) (#631).
- Entity and item data keep each value under its entity's place, the
  template and position as the save keeps them or the container and slot,
  and find it again after a load; a value no place names for certain goes
  aside in the world's file and never attaches again (#631).

### Fixed

- With the base limit raised, every altar zone the game lists joins its
  base: the game copied only the first 16 into its bases, so zones past
  16 had no comfort, crafting from chests or survivors (#641).
- "Remove water" on a Flame Altar clears every zone of its base, not only
  the first 16 (#641).
- After a restart that followed any added or removed entity, an entity or
  item data value could land on another entity of the same template; values
  now follow their entity's place. Values of a file written before this
  stay aside, attached to nothing (#631).
- Item data on a leaving player's items goes aside in the world's file
  instead of being dropped with the items the game removes, and a place
  keys exactly as the save keeps it, a negative axis included (#640).

## [1.0.0-alpha.10] - 2026-10-28

### Added

- `Off::text(key, args)` turns a feature off with a text of the mod's
  texts: the Mods screen, the status summary and the log show it in the
  reader's language. A `ConfigError::text` turned into an `Off` stays a
  text (#630).
- `Hold::set_text(reason, key, args)` sets a hold whose reason the log
  shows in the reader's language while the mod's texts are loaded, and as
  `reason` when they are not; a hold file of one line reads as before
  (#630).
- `BuffTypeId` and `NpcId`, the game's id of a survivor or other NPC, are
  ABI ids with a `Shape` (ABI 3.4); they print as their number, points and
  events carry them, and a Luau script reads and answers them as
  `buff_type_id` and `npc_id`. Every id the game derives from a GUID has
  `named` (#623).
- `loot::LootContainerId`, the game's name for a loot roll's node and
  labels (#623).
- `RecipeKind::Decorative` (`decorative` in a data pack, ABI code
  `RecipeKind::DECORATIVE`) for placed props that carry the item tag of
  the game's Decorative crafting group, so `Recipes::Kind`,
  `Rate::RecipeOutput` and `RecipeView::kind` pick decorations by the
  game's own grouping (#616).
- The title screen shows the loader's line under the game's version,
  `Enshwick 1.0.0-alpha.9 · mods: 6`, with the mods off and a warning when
  the game version leaves features off (#624).
- `Language::OFFERED`, the languages the game's settings offer, and
  `Texts::missing_in(languages)`, each language's missing texts, so a mod's
  test holds its texts complete for every offered language (#618).
- Every id type prints as its number (`Display`): `Entity`, `ItemId`,
  `PlayerKey`, `RecipeId`, `AffixId`, `SkillNode` and `KnowledgeId` go
  into a log line as they are (#615).
- `KnowledgeId` names a knowledge value: a recipe's, a quest's or any
  other the world or a player keeps; a `RecipeId` converts into it
  (ABI 3.2) (#615).
- `ItemId::from_guid`, `RecipeId::from_guid` and `KnowledgeId::from_guid`
  give the game's id of a GUID with its type, in a constant too (#615).
- `KnowledgeId` and `SkillNode` have a `Shape`, so services, events and
  points carry them; a Luau script changes an answer of any id type
  (#619).
- `Load::on_tick` runs a tick handler on a period of wall time and returns
  a `TickPeriod`, whose `set_every` changes the period while the game runs;
  `Load::on_every_step` runs one on every server step. A step a handler
  skips never enters the mod (`HostApi::subscribe_tick`,
  `HostApi::set_tick_every`, `TickHandle`, ABI 3.3) (#614).
- `TickData` carries the steps and the time since the handler's last call,
  the average step interval and, with timed diagnostics, the mod's recent
  cost per handler call: `elapsed_steps`, `elapsed`, `step_interval` and
  `cost`, with `TickData::new` and `with_elapsed` for tests (ABI 3.3) (#614).
- The fake host runs steps as the loader does with `Installed::tick` (#614).
- `Map::frame` returns a `MapFrame`: where the scene's map starts, how far
  it reaches and the size of its cells, in metres, read from the game's
  data once per scene like the markers (ABI 3.4, `HostApi::map_frame` and
  `MapFrameInfo`); the fake host serves one with `FakeHost::map_frame`
  (#627).

### Changed

- Breaking: `survivors::SurvivorId` is `NpcId`; the buff and survivor ABI
  structs, `HostApi::remove_buff`, `summon_item` and the survivor status
  merge take `BuffTypeId` and `NpcId`, and `BuffTypeId::from_guid` takes a
  `Guid`. The fake host's `summon_items` and `survivor_status` carry the
  ids (#623).
- Breaking: `Roll::node` and `Roll::labels` are `Option<LootContainerId>`,
  `None` where the game has none, and `Source::nodes`, `labels`,
  `with_nodes` and `with_labels` take `LootContainerId` (#623).

- Breaking: `Load::on::<Ticked>` runs the handler every
  `Ticked::DEFAULT_EVERY` (100 ms) instead of on every step; so do Luau's
  `enshwick.on("Ticked", ...)` and the scheduler, whose `next_tick` runs on
  its next pass, within that period. `Load::on_every_step` keeps the old
  timing (#614).
- Breaking: `StepClock::step(instant)` is `StepClock::advance(elapsed,
  steps)`, and the mod's step clock moves by the time its ticks carry,
  without reading the clock itself (#614).
- The loader reads the clock once per server step and hands that reading
  to every part of the step that needs the time (#614).
- `abi::read_headed` copies a struct in one move when the writer's struct
  covers all of it, so a larger event struct costs a handler no more (#614).
- Breaking: `MapSnapshot` carries the map's `frame`, and
  `MAP_SNAPSHOT_FORMAT` is 2 (#627).
- Breaking: `Guid::id` is `Guid::fnv1a`, the bare hash, and `Id::id` is
  gone; `ItemId::from_guid(id.guid())` and its siblings take their place.
  `World::used_item` returns an `ItemId`, and `migrations::current_id`
  takes and returns one (#615).
- Breaking: the knowledge calls, `KnowledgeChanged`, `QuestCompleted` and
  their ABI structs take `KnowledgeId`; `Knowledge::quests` returns them.
  `SkillNode` is an ABI type, and `abi::SkillCheck::node` and
  `HostApi::skill_override` take it (#615).
- Breaking: the fake host's `survivor_lookup` takes a `SurvivorId` and
  `Option<Entity>` for no entity and returns the mod's `Answer`;
  `set_knowledge`, `grants`, `revokes` and `account_changes` carry
  `KnowledgeId`, and `None` for the world (#615).
- A text missing in a regional language comes from its main variety before
  English: `pt-br` from `pt`, `es-mx` from `es-419` and `es`, `fr-ca` from
  `fr`, `nb` from `no` (`Language::lookup_order`) (#618).
- The loader's own texts sit in a file per language, and every console
  answer, the status summary, the per-mod costs, the conflict report and
  the interface demo follow the reader's language (#618).
- A player's game tells the server its language when it joins; the
  loader's answers, join notes and mod names to that player follow it, and
  on a server `i18n::language()` is that player's language while the loader
  asks a mod about the player. The loader's setting `language` sets the
  language of its console. The join message protocol moved to version 2
  (#620).
- `ConfigError::text(key, args)` refuses settings with a text of the mod's
  texts, which each player reads in their own language; the `validate` of
  the `Config` derive takes any error that converts into `ConfigError`
  (`String`, `&str` or a `ConfigError`) (#620).
- The Mods screen scrolls its lists in the game's own scroll area instead
  of paging them, and a text setting takes typed text in the game's own
  text field, set on Enter. A game build without either keeps the pages
  and the read-only line, and the log names what is missing (#625).

### Removed

- Breaking: `Load::modify_recipes`, `RecipeEdit` and `modify_recipes_in`;
  a data change through `Load::change_recipes` or a `Rate::RecipeOutput`
  changes recipe counts by kind (#616).

### Fixed

- The Mods screen shows a secret setting as "(secret)" and never its value
  (#626).

## [1.0.0-alpha.9] - 2026-10-27

### Added

- `code::find_each_unique` finds many patterns in one pass over the code,
  each with the result `Pattern::find_unique` gives (#598).
- `World::account_action` asks the game to kick, ban or unban an account
  (`AccountAction`); a ban of an account that is not online goes straight
  onto the server's ban list (`HostApi::account_action`,
  `AccountActionType`). The fake keeps a ban list
  (`Installed::banned`, `removed`) (#586).
- With `allowlist = true` in the loader's settings, a dedicated server lets
  in only the accounts `mod-data/enshwick/allowlist.txt` lists, and its
  administrators; the console's `enshwick allowlist add|remove` and
  `World::set_allowlisted` change the list, `World::allowlisted` reads it
  (`HostApi::allowlist_set`, `HostApi::allowlist_has`, ABI 3.1). The
  fake keeps the list (`Installed::allowlisted`) (#602).
- `error_once!` and `info_every!`, so every level that has a once or a
  periodic macro has both (#597).

### Changed

- `World::account_action` refuses a kick or a ban of a hosted game's host,
  succeeds for a ban of an account on the ban list already, and refuses a
  ban the game's list does not take instead of answering that the action
  is missing; the console says so in its own words. The fake takes the
  host with `FakeHost::host_key` (#606).
- The crate docs say to depend on the crate from crates.io and carry a
  module map, the feature table and the target; docs.rs builds every
  feature. The Luau definitions describe events, codes and permission bits
  in plain words, without Rust paths (#604).
- Breaking: the ABI is 3.0. A mod built for ABI 2 is refused and needs a
  rebuild against this release; the C header's `ENSHWICK_ABI_MAJOR` is 3 (#599).
- Breaking: ids have their own types across the ABI and the safe API.
  `Entity`, `ItemId`, `PlayerKey` and `RecipeId` live in the ABI module,
  re-exported from the crate root, and the C header declares them as
  `EnshwickEntity`, `EnshwickItemId`, `EnshwickPlayerKey` and
  `EnshwickRecipeId`; `ecs::Entity` is now `enshwick::Entity`. Account keys
  (`Player::key`, `Peer::Player`, actions' player lists,
  `Knowledge::grant_account`, `World::account_action` and the allowlist),
  item ids (inventory ops and slots, drops, interactions, vetoes, summons,
  `ItemInstance`, terrain materials) and recipe ids (`CraftRequest`,
  `CraftRecipeEvent`) take them, and the fake host's methods do too.
  `affixes::AffixId` names an affix in this process (`Load::define_affix`,
  `Affixes::info`, `Line::affix`), and `world::SkillNode` a skill node
  (`Skill::node`). `Summon::ammo` is an `Option`. Point and event schemas
  name the four ABI ids as built-in types, so `cargo enshwick point` writes
  them as the API's types (#608).
- Breaking: `Load::on` subscribes to typed game events too, as Luau's
  `enshwick.on` does: `load.on::<EntityDiedEvent>(..)` replaces
  `load.on_game`. `events::Kind` is the one trait `on` takes; the loader
  events' `EVENT` constant and raw read moved to `events::LoaderEvent`,
  which `FakeHost::raise` takes. `game_events::Kind` is gone (#597).
- Breaking: `Load::event_type` and `Load::event_types` are
  `game_event_type` and `game_event_types`, the names Luau uses;
  `FakeHost::event_type` is `game_event_type` (#597).
- Breaking: `Load::on_skill` takes the `Skill` and finds its node itself;
  `Load::skill_node` and `SkillCheck::node` are gone, and `SkillCheck` is
  `#[non_exhaustive]` (#597).
- Breaking: `Load::buff_modifiers` is `add_buff_modifiers` (#597).
- Breaking: `mods::all()` lists the loaded mods; the crate root's
  `enshwick::mods()` and `Host::{mods, join_missing, replaced}` are gone,
  so each list has one call: `mods::all`, `mods::join_missing`,
  `mods::replaced`. `mods::label` is hidden: `ModEntry::label` names a mod (#597).
- Breaking: `ReadPackError::TooLarge` names the `limit` the file broke, and
  `PackFiles::read_dir` refuses a pack with more than `MAX_ICONS` icons
  (`ReadPackError::TooManyIcons`) or an icon over `MAX_ICON_FILE`; icon
  PNGs past `icons::MAX_SIDE` pixels do not decode (#596).
- `mods::join_missing` leaves out an entry with new text once the loader
  keeps 64 KiB of text servers advertised (#596).
- `declare_mod!` exports the five functions under their C names only
  outside test builds, so one test binary can declare several mods (#591).

### Fixed

- A server rates the settings queries, setting changes and actions queries
  of a player who is no administrator, 2 a second in bursts of 8 (#596).
- Settings errors name the key and the expected type, and the line where
  the text is at hand, never the value: the settings check's reason, the
  settings read at start, table settings, join values, migrations, and
  `ConfigError` and `MigrationError` made from a TOML error. The warning for
  a value held to its range no longer shows the old value (#594, #595).
- `Schema::parse` refuses a field or group key with an empty part, such as
  `auth..token`, and `Load::table_config` a field with an empty key; the
  loader refuses such a schema, and the mod's warning names the key (#593).

### Removed

- Breaking: the retired `HostApi` entries `retired_command`,
  `altar_zones_control`, `set_altar_zones` and `altar_limit`, with
  `AltarZoneSize` and `AltarLimitFn`; every later entry moves up. The ABI
  docs no longer say in which minor of ABI 2 a field came (#599).
- Breaking: `Load::settings`, which read the settings file into any
  `Deserialize` type past the stages, `sync` and `secret` checks: derive
  `Config` and call `Load::config` (#597).

## [1.0.0-alpha.8] - 2026-10-27

### Added

- `packs::script_files` lists the scripts `PackFiles::read_dir` would read,
  without reading them. The loader lists a Luau mod's scripts each time it
  looks, so a script added, removed or renamed while the game runs reloads
  the mod with `dev_reload` as an edited one does (#585).
- A Luau mod reads and changes inventories through `enshwick.inventory`:
  `slots`, `count`, `item_info`, `give`, `take`, `destroy_slot`,
  `move_slot` and `move_all`, the done function getting the `Outcome` as a
  table with `Done` and `OpError` by name. `Inventories::slots` and
  `Inventories::count` read an inventory on the server's game thread
  without a `Reader` (#581).
- A Luau mod calls another mod's call with `enshwick.caller(description)`,
  from the module `cargo enshwick call --luau` writes; the input's fields
  are numbers, booleans, entities and GUIDs, each checked against its
  layout hash (#577).
- Calls, described services: `#[call]` declares one function from a
  `repr(C)` input to an answer, `Load::serve_call` serves it and
  `Load::caller` gives a `Caller` that runs it from any thread
  (`services::Call`, `CallError`); `manifest!(calls: [..])` puts its schema
  into the manifest. `DescribedCall`, `Load::described_caller` and
  `Load::serve_described_call` do the same with bytes, for a runtime that
  knows a call from its schema. The loader matches callers by name, major,
  fields and sizes once every mod is initialised (`HostApi::serve_call`,
  `find_call`, `call`, `CallSpec`, ABI minor 95). The fake serves and calls
  them (`Installed::serve_call`, `call`, `served_calls`). A call's input
  and answer have no padding, which `Shape::PADDING_FREE` reports (#574).
- `Load::on_described_mod_event` and `Load::define_described_event`
  (`services::DescribedEvent`, `DescribedEmitter`) hear and send a mod
  event described at run time, its data as bytes in the definer's layout;
  `manifest!(events: [..])` puts the schema of each listed `#[event]` into
  the manifest, and `ModEvent::SCHEMA` and `DESCRIBED` carry it (#571).
- `load.fog_of_war()` returns the client's `FogOfWar`, which the loader
  copies as the game uncovers it: `FogOfWar::state`, `read` and `coverage`
  read what the player has uncovered, as a `FogState` and `FogImage`; the
  `fog_open` and `fog_read` entries and `FogInfo` (ABI 2.94) (#564).
- `Change::in_table` changes an item's count, chance or weight in one fill
  table alone, after its changes of every table; `Vanilla::loot_table`
  finds the table a container template rolls, with how many containers
  share it; a data pack's `loot.toml` takes `table = [...]` with the
  containers' template names. `FillTableLayout` and `template_table` read
  a template's name and table (#570).
- `Hold::for_world` holds the game for one world only
  (`mod-data/<id>/worlds/<key>/hold.txt`): the loader stops the game as
  that world starts to load without the mod, before its save is read, and
  on a build it leaves vanilla at start; `world_holds`, `held_worlds` and
  `WORLDS_DIR` read them. A data pack that raised a stack size now holds
  the worlds saved with it, not the whole data folder (#563).
- `Field::Station` and `Change::station` move a vanilla recipe to another
  workshop (`DataField::RECIPE_STATION`, ABI minor 93): the loader writes
  the recipe's workshop and lists it in the client's crafting menu at the
  new station alone, from the next start; a data pack's `recipes.toml`
  takes `station = "<station item>"`. `menu::move_recipes` and
  `recipe_workshops` do the same for a mod's own patches (#567).
- `Items::Backpacks` selects every backpack for a change of `Field::Slots`,
  which now reaches a backpack's own inventory too, for backpacks made
  after it (`DataSelect::BACKPACKS`, ABI minor 92); a data pack takes
  `items = "backpacks"`, and `ItemTraits::backpack_slots` reads a
  backpack's size (#566).
- `Load::extend_described_point` links into an extension point described at
  run time (`points::DescribedPoint`, `DescribedField`), the input and the
  answer as bytes in the owner's layout, as a script's schema gives them;
  the loader checks the layout hashes as for a typed link (#569).
- `Load::change_recipes` changes the output count, cost or time of every
  vanilla recipe a selection picks: `Recipes::Kind` by what the recipe
  makes, `Recipes::Workshop` by where it is made, read from the game's
  data by the loader (`DataSelect::RECIPE_KIND`, `DataSelect::WORKSHOP`,
  ABI minor 91); a data pack's `recipes.toml` takes `kind = [...]` and
  `workshop = [...]` for the same, and `RecipeKind::name` gives the names
  (#562).
- `Map::snapshot` takes the map as a `MapSnapshot` of players, Flame Altars
  with their bases, and markers, plain data for a process outside the game;
  `MapSnapshot::to_bytes` and `from_bytes` write and read it in a versioned
  format (`MAP_SNAPSHOT_FORMAT`) (#560).
- `script::game_events` lists every typed game event for script languages
  (`GameEventKind`); the game event structs, masks and values convert for
  scripts (a mask as its bit names, a value as its name) (#565).
- `load.map()` returns the `Map` of the game server: `Map::markers` and
  `Map::markers_in` list the world's map markers, each a `MapMarker` with
  its position, its `MapMarkerTypeId` (the key of the game's marker type
  registry) and its label's text tag; `Map::waypoint` reads the waypoint a
  player set (#557).
- The `world_quiet` entry answers whether a mod is on the server's game
  thread at a quiet moment while a world runs, where it may read live
  components (ABI 2.90) (#557).
- `Load::table_config` reads settings described by a `config::Schema` at
  run time, with the file, checks, stages, Mods screen and join values of a
  typed `Config`; `config::TableConfig::on_change` follows them, and
  `Schema::template` writes the file (#561).
- A pack's `pack.toml` declares settings in `[[setting]]` tables
  (`PackSetting`, `PackManifest::settings_schema`), checked as it is read (#561).
- `World::can_build` tells whether the server lets a player edit a box:
  `CanEditWorld`, and `CanEditBase` inside a building zone, as the game's
  terrain tools decide, plus two tests from the game's building cursor, the
  world's building area and strict no-build zones; a refusal is a
  `BuildRefusal` with the game's `CursorFlag` names (#547).
- `World::transaction` writes terrain edits as one transaction at the
  server's quiet moments, 64 bricks a step, and `World::undo` puts every
  brick it touched back, refusing when one has changed since;
  `World::transaction_status` follows it (#548).
- `WorldError::OutsideWorld` (`Status::OUTSIDE_WORLD`): voxel reads, terrain
  edits and transactions past the world's edge or above 1,024 metres are
  refused, where the game would read air (#548).
- `World::props_in` lists the props in a box, the things players placed
  from items, each with its `ItemId` and `WorldTransform`, told apart from
  the things a world comes with by the template their item places (#550).
- `World::capture` takes a region of up to `CAPTURE_TILES` reads of 32
  metres a side as a `Capture` of its voxels and props, counted from its
  corner; `Capture::rotated` turns it by quarter turns about the vertical
  and `Capture::translated` moves it (#550).
- `ItemId`, the game's number for an item, and `WorldTransform`, a
  position in metres with an orientation (#550).
- `Capture`, `VoxelBox`, `Voxel`, `VoxelMaterial`, `VoxelPos`, `Entity`,
  `ItemId`, `Prop` and `WorldTransform` implement serde's `Serialize` and
  `Deserialize`; a `VoxelBox` read must hold one voxel per place and end
  inside the world's coordinates (#550).
- `ItemData::placed_entity`, the GUID of the template a player places from
  the item, from `HostApi::item_info`; ABI minor 88 (#550).
- `Transaction::write_voxels`, `erase_block` and `place_block` write voxels,
  tear down building voxels and place building blocks inside a
  transaction, with the same undo; blocks and tear-downs raise the game's
  `BuildingPlaceEvent` and `BuildingTearDownEvent`. `VoxelBox::new`,
  `set` and `moved_to` build boxes to write (#551).
- `World::transaction_for` acts for a player: the loader checks the
  player's rights for every change before it writes, and a refusal shows
  as `TransactionStatus::NotAllowed`; the op kinds, records and
  `transaction_commit_for` are ABI minor 89 (#551).
- The conflict report lists code patches refused over another mod's
  bytes, vanilla values one mod's set overrides with both values, and
  components several mods write; the loader compares the game's code with
  the executable on disk while it starts and names the DLL or mod that
  changed it outside the API, after each load, `init` and `ready` (#555).
- A data pack's `items.toml` changes the stack size or container slots of
  every vanilla item of some categories (`category = ["Materials"]`), every
  item that stacks (`items = "stackable"`) or every container
  (`items = "containers"`), through the selections `Load::change_items`
  makes; `Pack::selections` lists them, and `ItemCategoryMask::NAMES` and
  `ItemCategoryMask::named` take the game's category names (#558).
- `Status::STATIC_HALF` (ABI minor 94), `WorldError::StaticHalf` and
  `FakeHost::static_half`: a write to the static half of a component
  type, the set-up data every entity of a template shares, is refused
  (#568).
- `Transaction::place_prop`, `remove_prop` and
  `remove_prop_discarding_state` place and remove props inside a
  transaction, with undo; `World::transaction_entities` lists the props a
  transaction placed. A prop that holds state a player would lose (items,
  growth, a production queue, assigned survivors, written text, fuel,
  logic wiring, a logic trigger) is never
  removed or taken away by an undo unless the mod opts in:
  `WorldError::HoldsState(PropState)` names what it holds. The op kinds,
  records and `transaction_entities` are ABI minor 96 (#552).
- `World::persists` tells whether a save keeps changes in a box: the
  game saves a world only inside its bases' zones, so props, blocks and
  terrain changed elsewhere are gone after a restart, a mod's as a
  player's. `HostApi::world_persists` is ABI minor 96 (#552).
- `WorldError::NotAProp` (`Status::NOT_A_PROP`, ABI minor 96): creatures
  placed from items (animals, pets, survivors) and flame altars are no
  props; prop removal refuses them in either mode (#552).

### Changed

- `#[point]` and `#[event]` refuse an input, answer or data with padding,
  as `#[call]` does, since a described link or listener reads every byte:
  add the bytes as fields. The example trading post's `price` gains a
  `count` (#587).
- `Map::markers` reads the markers the scene places from the game's data
  through the new `map_markers` entry and `MapMarkerInfo` (ABI 2.97), once
  per scene; `MapMarker` no longer names an entity (#573).
- An item a mod or pack changes by name keeps that change over its own
  selection of the same field, so a selection can leave out exceptions
  (#558).
- `Terrain::{add,dig,flatten}` write nothing and fail with
  `WorldError::NotLoaded` unless the whole area the edit can reach is
  loaded; the game would compute such an edit against air and merge it
  onto the ground once the area loads (#548).
- `World::entities` takes the world by value, as the other `World` calls
  do (#550).
- Item data (`Inventories::item_info`, `HostApi::item_info`) and the
  item checks of `World::props_in` and `Transaction::place_block` work from
  world start: the loader finds the item registry without waiting for a
  player to join (#551).
- `World::props_in` and `World::capture` leave out creatures placed from
  items (animals, pets, survivors) and flame altars, which are no props,
  and answer `WorldError::Missing` on a game build without their marks
  (#552).

### Fixed

- A Luau mod's answer to a point and its mod events take an enum over a
  64-bit integer, `usize` and `isize` included, as a number both ways;
  before, every such value was refused (#587).
- The loader's profile names the survivors feature `survivors`; it showed a
  mod id (#574).
- The loader and the fake leave a mod event subscription idle when it
  reads more bytes than the definer sends (#571).
- `Map::markers` listed no marker on a dedicated server, whose marker
  entities the entity walk does not find (#573).
- `World::component` and `World::get` read the static half of a component
  type, such as `FuelSetup`, as the template's set-up values; they gave the
  data of its dynamic half, `DynamicFuelSetup`, before (#568).

## [1.0.0-alpha.7] - 2026-10-26

### Added

- `load.voxels()` reads up to 64³ voxels of the server's world with
  `Voxels::read` and `Voxels::sample`, on the game thread at a quiet
  moment, as `VoxelBox` of `Voxel`s; `Status::NOT_LOADED` and
  `WorldError::NotLoaded` name a part of the world that is not loaded
  (#541).
- `Load::change_items` changes a stack size or container slots of every
  vanilla item a selection picks (`Items::Category`, `Items::Stackable`,
  `Items::Containers`), read from the game's data by the loader;
  `ItemCategoryMask` names every item category (#540).
- `Load::declare_change` declares a change of a vanilla value at init whose
  value the mod sets any time later through its `ChangeHandle`
  (`set`, `reset`); the loader merges it with every mod's and pack's
  changes and builds what it reaches again in place, so a stack size or
  chest size can follow a per-world setting (#542).
- The loader keeps every stack size above the game's that a world was saved
  with (`mod-data/enshwick/kept-stacks.toml`) and serves at least that size
  from the start, so lowering a size or removing the mod that raised it
  never cuts a saved stack; while any size is kept it holds the game on a
  build it would leave vanilla (#544).
- `script` describes the API for script languages beside the Rust items:
  `#[derive(Script)]` for types, `#[script(<part>)]` for the methods a script
  may call, a script entry for every loader event, and `NAMESPACES` (#543).
- A data pack folder with a `main.luau` reads its scripts:
  `PackFiles::scripts`, `SCRIPT_MAIN`, `SCRIPT_EXTENSION`,
  `DEFINITIONS_SUFFIX`, `MAX_SCRIPTS`, `MAX_SCRIPT_DEPTH` and
  `ReadPackError::TooManyScripts`; scripts count in the pack's hash (#545).

### Fixed

- Each mod handle has a scheduler queue of its own, so one copy of the API
  can run tasks for several packs and scripts (#543).

### Changed

- `ItemCategoryMask` lives in `content::categories`, beside the selections
  that use it, instead of `affixes` (#540).

## [1.0.0-alpha.6] - 2026-10-26

### Added

- `content::loot_tables` reads and changes the random loot table's stack
  sizes and every stack of a fill table; a data pack's `loot.toml` changes
  `stack_size`, `count`, `spawn_probability` and `weight` under the game's
  own names (#523).
- `altars::base_limit` names the most altar zones and bases the game groups
  now, and the zones its own grouping takes (#529).
- `ContainerKinds`, `ItemCategoryMask` and the event masks have a
  `const fn union`, so a combined set can be a `const`; `|` gives the same
  (#539).

### Changed

- The altar limit is a chain any mod joins: `Load::on_altar_limit` takes a
  filter that sees the Flame level, the game's own count and the count so
  far in an `AltarLimit` and changes `allowed`, in load order. The loader
  keeps every answer within the zones the game groups, and owns the
  raised maximum of 32 zones in 16 bases; `Load::raise_base_limit` puts it
  in force from `Mod::ready` on (#529).
- `World` is one pointer to what the mod resolved once, instead of 320
  bytes: a copy is cheap, and passing it by value or moving it into a
  closure no longer trips `clippy::large_types_passed_by_value`. Its
  methods take `self`, so calls read the same (#536).

### Removed

- `VANILLA_ALTARS_PER_FLAME_LEVEL`, `vanilla_altars`, `VANILLA_ALTAR_ZONES`
  and `RAISED_ALTAR_ZONES`: a filter reads the game's own count from
  `AltarLimit::vanilla`, and `altars::base_limit` names the zones (#529).

## [1.0.0-alpha.5] - 2026-10-26

### Added

- `Rate::RecipeOutput(kind)` multiplies what the recipes of each
  `RecipeKind` make, after every mod's own recipe patch;
  `RecipeEdit::scale_output_by` scales by a fraction and
  `RecipeKind::ALL` lists the kinds (#501).
- `progress::Skill` and `Load::skill_node` give the node of a skill the
  platform names on the running game build (#494).
- `game_events::EquipmentSlot`, the slot `EquipmentChanged` carries, and
  `inventory::OpError`, why an inventory operation failed, both named as
  the game names them (#494).

### Changed

- Altar zone sizes are a chain any mod joins: `Load::on_altar_zone` adds a
  filter that sees each altar's zone so far, the vanilla one first, in load
  order, and `ZoneSizes::changed` asks for a new sizing from any thread
  (#499).
- The loader owns the event loop, server loop and task worker hooks:
  `on_loop_wait`, `on_server_wait` and `cap_task_workers` keep their
  signatures but run through the host, handlers of several mods run in load
  order and the lowest worker cap wins; a fake host test runs them with
  `Installed::loop_wait`, `server_wait` and `task_workers`, no longer
  through `mid_hooks` or `run_at` (#486).
- Game settings, hit damage types and attack categories, experience
  sources, altar actions, equipment slots and inventory error codes resolve
  through engine ids and the game's constant names; no engine number is
  compiled into a mod (#494).
- `DamageTypes` and `AttackCategories` keep their bits private and combine
  with `|`; `EquipmentChanged::slot` is an `EquipmentSlot` and
  `OpResult::error` an `Option<OpError>` (#494).
- `World::writable_type` accepts the copies the game keeps for clients; a
  write to one is refused with `WorldError::Refused` (#494).
- Names of things the game names itself are the game's reflected names in
  Rust casing; the game settings enums read their values by the game's
  constant names (#506):

| Old | New |
| --- | --- |
| `game_events::DamageTypes` | `game_events::DamageTypeMask` |
| `DamageTypes::CUT, PIERCE, BLUNT` | `DamageTypeMask::PHYSICAL_CUT, PHYSICAL_PIERCE, PHYSICAL_BLUNT` |
| `DamageTypes::FIRE, ICE, SHROUD, LIGHTNING` | `DamageTypeMask::MAGICAL_FIRE, MAGICAL_ICE, MAGICAL_FOG, MAGICAL_LIGHTNING` |
| `game_events::AttackCategories` | `game_events::AttackCategoryMask` |
| `AttackCategories::AREA` | `AttackCategoryMask::AOE` |
| `game_events::ExperienceSource` | `game_events::ExperienceSourceType` |
| `game_events::AltarAction` | `game_events::FlameAltarActionType` |
| `AltarAction::SetSpawn` | `FlameAltarActionType::SetAsPreferredSpawnAltar` |
| `game_settings::Preset` | `game_settings::GameSettingsPreset` |
| `game_settings::Difficulty` | `game_settings::GenericDifficulty` |
| `game_settings::Amount` (`random_spawns`) | `game_settings::RandomSpawnerAmount` |
| `game_settings::Amount` (`aggro_pool`) | `game_settings::AggroPoolAmount` |
| `game_settings::StartleRepercussion` | `game_settings::TamingStartleRepercussion` |
| `Settings::player_health` | `Settings::player_health_factor` |
| `Settings::player_mana` | `Settings::player_mana_factor` |
| `Settings::player_stamina` | `Settings::player_stamina_factor` |
| `Settings::player_body_heat` | `Settings::player_body_heat_factor` |
| `Settings::player_diving_time` | `Settings::player_diving_time_factor` |
| `Settings::durability` | `Settings::enable_durability` |
| `Settings::starving` | `Settings::enable_starving_debuff` |
| `Settings::food_buff_duration` | `Settings::food_buff_duration_factor` |
| `Settings::hunger_to_starving` | `Settings::from_hunger_to_starving` |
| `Settings::shroud_time` | `Settings::shroud_time_factor` |
| `Settings::tombstone` | `Settings::tombstone_mode` |
| `Settings::glider_turbulence` | `Settings::enable_glider_turbulences` |
| `Settings::weather` | `Settings::weather_frequency` |
| `Settings::fishing` | `Settings::fishing_difficulty` |
| `Settings::mining_damage` | `Settings::mining_damage_factor` |
| `Settings::plant_growth` | `Settings::plant_growth_speed_factor` |
| `Settings::resource_drops` | `Settings::resource_drop_stack_amount_factor` |
| `Settings::factory_speed` | `Settings::factory_production_speed_factor` |
| `Settings::perk_recycling` | `Settings::perk_upgrade_recycling_factor` |
| `Settings::perk_cost` | `Settings::perk_cost_factor` |
| `Settings::experience_combat` | `Settings::experience_combat_factor` |
| `Settings::experience_mining` | `Settings::experience_mining_factor` |
| `Settings::experience_exploration` | `Settings::experience_exploration_quests_factor` |
| `Settings::random_spawns` | `Settings::random_spawner_amount` |
| `Settings::aggro_pool` | `Settings::aggro_pool_amount` |
| `Settings::enemy_damage` | `Settings::enemy_damage_factor` |
| `Settings::enemy_health` | `Settings::enemy_health_factor` |
| `Settings::enemy_stamina` | `Settings::enemy_stamina_factor` |
| `Settings::enemy_perception` | `Settings::enemy_perception_range_factor` |
| `Settings::boss_damage` | `Settings::boss_damage_factor` |
| `Settings::boss_health` | `Settings::boss_health_factor` |
| `Settings::pacify_enemies` | `Settings::pacify_all_enemies` |
| `Settings::startle` | `Settings::taming_startle_repercussion` |
| `Settings::day_length` | `Settings::day_time_duration` |
| `Settings::night_length` | `Settings::night_time_duration` |
| `Settings::curse` | `Settings::curse_modifier` |
- Typed game events and their fields take the game's reflected names;
  `Block`, `killer` and `killer_player` stay enshwick's (#518):

| Old | New |
| --- | --- |
| `game_events::Hit` | `game_events::HitEvent` |
| `Hit::attacker` | `HitEvent::root_source_id` |
| `Hit::source` | `HitEvent::source_id` |
| `Hit::target` | `HitEvent::target_id` |
| `Hit::root_target` | `HitEvent::root_target_id` |
| `Hit::weapon` | `HitEvent::weapon_pide_id` |
| `Hit::attack` | `HitEvent::attack_category` |
| `game_events::Died` | `game_events::EntityDiedEvent` |
| `Died::entity` | `EntityDiedEvent::dead_entity_id` |
| `game_events::LevelGained` | `game_events::LevelUpEvent` |
| `LevelGained::player` | `LevelUpEvent::player_entity_id` |
| `game_events::ExperienceGained` | `game_events::XpGainedEvent` |
| `ExperienceGained::player` | `XpGainedEvent::player_entity_id` |
| `game_events::CraftRequested` | `game_events::CraftRecipeEvent` |
| `CraftRequested::player` | `CraftRecipeEvent::crafting_operator_id` |
| `CraftRequested::station` | `CraftRecipeEvent::crafting_station_id` |
| `CraftRequested::recipe` | `CraftRecipeEvent::recipe_id` |
| `CraftRequested::amount` | `CraftRecipeEvent::recipe_amount` |
| `game_events::Crafted` | `game_events::CraftRecipeSuccessEvent` |
| `Crafted::workshop` | `CraftRecipeSuccessEvent::workshop_id` |
| `Crafted::recipe` | `CraftRecipeSuccessEvent::recipe_id` |
| `game_events::Interacted` | `game_events::AcceptOfferedInteractionEvent` |
| `Interacted::player` | `AcceptOfferedInteractionEvent::guest_entity_id` |
| `Interacted::target` | `AcceptOfferedInteractionEvent::host_entity_id` |
| `Interacted::offer` | `AcceptOfferedInteractionEvent::offer_id` |
| `game_events::Built` | `game_events::BuildingPlaceEvent` |
| `Built::item` | `BuildingPlaceEvent::tracking_item_id` |
| `game_events::TornDown` | `game_events::BuildingTearDownEvent` |
| `game_events::PickedUp` | `game_events::BuildingPickUpEvent` |
| `PickedUp::entity` | `BuildingPickUpEvent::picked_entity_id` |
| `game_events::AltarUsed` | `game_events::FlameAltarEvent` |
| `AltarUsed::player` | `FlameAltarEvent::player_entity_id` |
| `AltarUsed::altar` | `FlameAltarEvent::base_entity_id` |
| `AltarUsed::action` | `FlameAltarEvent::r#type` |
| `game_events::LootOpened` | `game_events::LootInteractionEvent` |
| `LootOpened::player` | `LootInteractionEvent::guest_id` |
| `LootOpened::container` | `LootInteractionEvent::host_id` |
| `LootOpened::take_all` | `LootInteractionEvent::loot_all` |
| `game_events::EquipmentChanged` | `game_events::HandleEquipmentSlotChangedEvent` |
| `EquipmentChanged::player` | `HandleEquipmentSlotChangedEvent::player_entity_id` |
| `EquipmentChanged::slot` | `HandleEquipmentSlotChangedEvent::equipment_slot` |
| `game_events::Revived` | `game_events::RevivedPlayerEvent` |
| `Revived::player` | `RevivedPlayerEvent::target_entity_id` |
| `Revived::by` | `RevivedPlayerEvent::source_entity_id` |
| `Block::player` | `Block::owner_id` |
- The other types, fields and constants the game names take the game's
  reflected names; the ones enshwick computes say so in their docs (#525):

| Old | New |
| --- | --- |
| `damage::DamageType` | `damage::DamageTypeNew` |
| `DamageType::Cut, Pierce, Blunt` | `DamageTypeNew::PhysicalCut, PhysicalPierce, PhysicalBlunt` |
| `DamageType::Fire, Ice, Shroud, Lightning` | `DamageTypeNew::MagicalFire, MagicalIce, MagicalFog, MagicalLightning` |
| `buffs::BuffKind` | `buffs::BuffSlotType` |
| `BuffKind::Terrain` | `BuffSlotType::TerrainDebuff` |
| `buffs::BuffId` | `buffs::BuffTypeId` |
| `BuffType::id, kind, duration, saved, hidden` | `BuffType::buff_type_id, slot, default_life_time, is_saved, hide_in_ui` |
| `Buff::id, entity, kind, duration` | `Buff::type_id, buff_entity_id, slot, life_time` |
| `knowledge::KnowledgeKind` | `knowledge::KnowledgeType` |
| `Known::kind, expected` | `Known::r#type, expected_flags` |
| `weather::FrontKind` | `weather::WeatherPhenomenonEffect` |
| `FrontKind::Clouds` | `WeatherPhenomenonEffect::None` |
| `affixes::Rarity` | `affixes::ItemRarity` |
| `affixes::Categories` | `affixes::ItemCategoryMask` |
| `Categories::INSTRUMENTS, FISHING_RODS` | `ItemCategoryMask::INSTRUMENT, FISHING_ROD` |
| `inventory::Category` | `inventory::InventoryCategory` |
| `Category::UNKNOWN` | `InventoryCategory::INVALID` |
| `ItemInfo::max_stack` | `ItemInfo::max_stack_size` |
| `content::types::ITEM_REGISTRY, RECIPE_REGISTRY, WORKSHOP_REGISTRY` | `content::types::ITEM_REGISTRY_RESOURCE, RECIPE_REGISTRY_RESOURCE, WORKSHOP_REGISTRY_RESOURCE` |
| `content::types::TEMPLATE, TEMPLATE_COLLECTION` | `content::types::TEMPLATE_RESOURCE, TEMPLATE_COLLECTION_RESOURCE` |
| `content::types::UI_IMAGE, UI_BUNDLE` | `content::types::UI_TEXTURE_RESOURCE, FB_UI_BUNDLE` |
| `content::items::Stat` | `content::items::ImpactUiValue` |
| `Stat::label, format` | `ImpactUiValue::loca_id, value_format` |
| `content::items::Meter` | `content::items::FuelSetup` |
| `Meter::max` | `FuelSetup::max_fuel_charges` |
| `changes::Field::MaxStack, Time` | `changes::Field::MaxStackSize, CraftingDuration` |
| `terrain::Material` | `terrain::TerrainTerraformingConfig` |
| `Material::id, item, health, flattens` | `TerrainTerraformingConfig::terrain_material, terrain_item, health_points, can_be_flattened` |
| `loot::Offer` | `loot::InteractionOfferRuntime` |
| `Offer::guest, accepted_at, disabled` | `InteractionOfferRuntime::guest_id, last_accept_time, is_disabled` |
| `Container::empty` | `Container::is_empty` |
| `effects::NpcOrder` | `effects::NpcCommandType` |
| `NpcOrder::GoTo` | `NpcCommandType::GoToPosition` |
| `TimeOfDay::day_length, night_length` | `TimeOfDay::day_time_duration, night_time_duration` |


### Removed

- `control_altar_zones`, `ZoneControl`, `ZoneSize` and `SetSizesError`
  (#499).
- `progress::AIRBORNE_ARCHITECT`, `OpResult::TOO_LARGE`,
  `OpResult::NO_ROOM`, `ExperienceSource::from_raw` and
  `AltarAction::from_raw` (#494).
- The game settings enums' `from_value` and `value` (#506).

## [1.0.0-alpha.4] - 2026-10-26

### Added

- `rates`: a mod declares a `Rate` with `Load::rate` and sets its factor;
  the loader scales experience by group, workshop speed, resource drops,
  comfort, skill points per level and loot stacks by the product of every
  mod's factor (#445).
- `Load::event_types()` lists every game event type the running game build
  has (#461).
- `RecipeView::kind()` and `RecipeKind` tell a building block, a roof,
  overgrowth or terrain from other recipes by the output item's data (#469).
- `i18n::MOD_NAME` and `i18n::MOD_DESCRIPTION`: a mod's texts give its name
  and description in the player's language on the Mods screen, in join
  notes, the console and the status summary (#470).
- A dedicated server runs lines typed into its console window or appended to
  `mod-data/enshwick/commands.txt`, as `enshwick ...` tools or as
  `<mod id> <action>` (#440, #452). The tools include `enshwick mods`,
  `world`, `entity`, `placed`, `reload`, `kick`, `ban` and `unban` (#455).
- A DLL in `mods/` without the Enshwick interface stays loaded as a native
  mod and is listed on the Mods screen and in `status.log` (#415).
- `logs/enshwick/report.md`, one file a player attaches to an issue (#418).

### Changed

- `enshwick` and `enshwick-gamedata` require the exact version of the
  crates they are released with, so an older `enshwick` never resolves a
  newer `enshwick-macros` (#484).

- A mod id without an author part is refused whatever ABI minor the mod was
  built for. Rebuild a mod built for ABI minor 69 or older with an
  `<author>.<name>` id (#436).
- Game events, content layouts and content types resolve through engine ids
  the loader maps to the running build; `content::types` are
  `ResourceType` handles (#460, #461).
- `rename_setting` and `remove_setting` take dotted keys and move a setting
  into or out of a table (#464).

### Removed

- `game_events::EVENT_TYPE_NAMES` and `Kind::TYPE_NAME` (#461).

## [1.0.0-alpha.3] - 2026-10-26

### Added

- Mod ids have an author part, `<author>.<name>`, each part of `[a-z0-9-]`,
  at most 64 characters; `abi::is_valid_id` checks one, and
  `cargo enshwick new` takes the new form (#378).
- `was: [...]` in `manifest!` for a renamed mod: it replaces its old ids,
  answers requirements and extension points named by them, and on its first
  start takes over the old id's settings file, texts and `mod-data` folder.
  `replaces: [...]` and `abi::is_valid_former_id` accept ids of the old form
  (#369, #378, #424).
- `Migration::take_setting` for a mod that folds another one in and takes its
  settings (#369).
- `points: [Type, ...]` in `manifest!`: a mod lists the extension points it
  defines, and their schemas go into its DLL. `cargo enshwick point` writes
  another mod's point definition from that DLL (#394).
- `#[config(group)]`: a setting that is a `Config` of its own becomes a table
  of the settings file, keyed `group.field` on the Mods screen and the
  console (#399).
- A list of up to 16 numbers with a range shows a slider per element on the
  Mods screen and takes `key[i]` on the console; `#[config(first = n)]` and
  the text `<key>.element` label the elements (#403, #425).
- `DamageCause::NAMED` lists every damage cause the API names (#400).
- `Texts::languages()` and `Texts::untranslated()` for a test that a mod's
  texts have no gaps (#397).
- The stub loads every DLL in `mods/chain/` before the loader, so another
  mod's own proxy DLL runs beside Enshwick (#414).

### Changed

- Game memory is read only as types valid for any bytes, through
  `enshwick::raw::read_at` and `read_ptr_at`. `abi::Headed` is an
  `unsafe trait` that requires `Zeroable` (#380).
- `loot::near` moved to `world::near` (#401).
- The fake host's builder is split by domain; `Installed::check_settings`
  returns `SettingsRefused` (#368).
- A number outside its range in a settings file is held to the range with a
  warning instead of refusing the whole file (#425).
- Of two files that declare one mod id, the highest version runs (#365).

### Removed

- `points::safes`, `points::survivors_per_altar`,
  `points::flame_altar_count` and `points::area_loot`: the API declares no
  mod's points. A mod that extended one writes its definition with
  `cargo enshwick point` (#394).
- The `enshwick-reflect` crate; its reader is `enshwick_gamedata::reflect`
  (#421).
- The probe and research mods moved to
  [devtools-mods](https://github.com/enshwick/devtools-mods) (#363).

## [1.0.0-alpha.2] - 2026-10-26

### Added

- A mod's actions run from the server's console as
  `<mod id> <action> <values>`, through `ActionCall::CONSOLE` (#336).

### Removed

- Chat commands: `commands`, `Reply`, `CommandContext`, `Load::add_command`,
  `Action::verbs` and `Actions::set_language`. Players and administrators
  use actions on the Mods screen, and servers the console (#336).

## [1.0.0-alpha.1] - 2026-10-26

The first version on crates.io, as `enshwick`, with `enshwick-macros`,
`enshwick-gamedata` and `cargo-enshwick` (#347).
