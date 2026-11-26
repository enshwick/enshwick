### Added

- `world::PlayerRef` names one player by account key, by character or by a
  `Player`; `map.uncover`, `uncover_all` and `waypoint`, `World::tell`,
  `notify`, `can_build`, `respawn_point` and `set_respawn_point`,
  `Knowledge::grant_account` and `revoke_account`, and `quests::Holder` take
  it (#1295).
- `vetoes::Verdict` answers every filter: `Pass`, `Allow`, `Refuse` and
  `RefuseBecause` (#1295).
- `Entity::from_raw` builds an entity from the engine's number and refuses 0
  (#1295).
- The `console` and `services` modules, `Load::limit_altar_level` and
  `altars::set_base_limit` are stable (#1295).

### Changed

- **Breaking:** public records, option structs, game events and growing enums
  are `#[non_exhaustive]`; option structs build with `new` or `Default`, and
  optional fields are set by assignment (#1295).
- **Breaking:** the ABI's headed structs and `HostApi` are
  `#[non_exhaustive]`; code outside the ABI builds a headed struct with
  `abi::headed!` or `Headed::blank()`. The ABI version stays 1.0 (#1295).
- **Breaking:** the engine layouts (`BuffLayout`, `ItemLayout`,
  `RecipeLayout`, `LootLayout`, `MenuLayout`, `WorkshopLayout`,
  `ContainerLayout`, `ImageLayout`, `FillTableLayout`) leave the stable
  surface, and `EngineFacts` is sealed (#1295).
- **Breaking:** `World::entities`, `near` and `containers`, every
  `inventory::Reader` read, `Inventories::count` and `slots`, and
  `Buffs::has` and `types` return `Result<_, WorldError>` instead of an empty
  answer off the game thread; `set_label`, `altars::set_base_limit` and
  `Frame::play` return typed errors (#1295).
- **Breaking:** `on_craft`, `on_build`, `on_join`, `on_rename`,
  `container_rule` and `on_skill` take a filter that answers anything
  `Into<Verdict>`, a `bool` and an `Option<bool>` included; `on_chat` takes
  anything `Into<ChatVerdict>`, a `bool` and a `Verdict` included.
  `BuildVerdict` is gone (#1295).
- **Breaking:** in Luau, a filter of `on_craft`, `on_join`, `on_rename`,
  `container_rule` or `on_skill` that answers a string or any value but
  `true`, `false` and nil refuses; before, only `false` refused (#1295).
- **Breaking:** `quests::Holder::Player` holds a `PlayerRef`;
  `Holder::player` builds one (#1295).
- **Breaking:** `Entity`'s field is private and `Entity` has no `Default`
  (#1295).
- **Breaking:** the `ALL` lists of `PlayerPermissions`, `ui::Response`,
  `schema_section::Kind` and `ItemSetCategory` are `&'static [Self]` (#1295).
- **Breaking:** `GameSettings::read` returns a `SettingsRead` (#1295).
- **Breaking:** `World::component` needs the `unstable` feature, and
  `NewBuff::saved` is gone for its `is_saved` field (#1295).
- **Breaking:** error types are named for what failed:
  `console::RunLineError`, `inventory::RunBatchError` and `RunOpError`,
  `item_data::SetItemDataError`, `services::CallServiceError`,
  `state::StoreFileError` and `content::changes::SetFieldError`; `PackError`
  is `#[non_exhaustive]` (#1295).
- **Breaking:** a Luau error the API raises is a table of `kind`, `message`
  and `call`, which prints as `call: message` (#1295).
- **Breaking:** in Luau, `map.uncover` takes a vector, and the fog of war's
  methods stay on its own object (#1295).
- The `Load` methods behind the `typed` feature say that they are unstable
  (#1295).

### Fixed

- The Luau definitions no longer promise a `Ticket` from `inventory.submit`,
  leak a link from `change_quest`, or call `can_build` a yes-or-no answer
  (#1295).
