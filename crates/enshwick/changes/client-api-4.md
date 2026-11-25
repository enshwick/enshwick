### Added

- `input::ClientSnapshot` reads the local player's `position`, camera `look`,
  `health` and `stamina` with their maxima, and the action bar's selected
  slot on any client, a joined one included; `SnapshotPart` gains
  `Position`, `Look`, `Vitals` and `Actionbar`. ABI 12.28.
- `Input::client_slots` lists the local player's inventory slots from the
  client's own copy of the world, each a `PlayerSlot` that says whether it
  is an action bar slot; `abi::ClientSlot` and `HostApi::client_slots`.
  ABI 12.28.
- `CommandAction::Craft`, `MoveItem`, `Equip`, `TakeAll`, `DepositAll` and
  `Trash` write the game's own crafting, item transfer, equipment and item
  requests; observed once the client's copy of the inventory changes.
  `abi::ClientCommand` gains `slot`, `target_slot`, `entity` and `amount`
  and the kinds `CRAFT` to `TRASH`. ABI 12.28.
- Fake host: `set_client_slots`.
