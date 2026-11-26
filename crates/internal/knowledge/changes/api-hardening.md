### Changed

- **Breaking:** `on_skill` takes a handler that answers anything
  `Into<Verdict>` (#1295).
- **Breaking:** `Knowledge::grant_account` and `revoke_account` take a
  `PlayerRef`, and `quests::Holder::Player` holds one; `Holder::player`
  builds it (#1295).
