### Changed

- The world module leaves `unstable`, except props, build rights, captures
  and transactions (#1106).
- `World::broadcast` sends through `chat_all`, and a chat to account 0 is
  refused (#1106).
- Building zone checks and positions are in metres; `DaytimeInfo` times
  name their unit; `AccountAction::Unban` and `altars::set_base_limit`
  replace `UnBan` and `set_base_cap` (#1106).
