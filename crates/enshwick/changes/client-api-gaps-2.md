### Added

- `input`: `CommandAction::PressWindowItem`, `PickWindowItem` and `SetWindowText` press a button or a switch, pick an option and set a field's text in the newest window a server's mod shows this client, by the window's and the item's ids, observed once the window shows it, behind `unstable`.
- `input`: `Input::client_windows`, `ClientWindowItem` and `WindowItemKind` list the items of the windows the client holds with their ids, kinds and lines, behind `unstable`.
- `input`: `CancelReason::ItemOff` for a window command on an item the window draws off, behind `unstable`.
- `abi`: `ClientCommand` gains `item` and `text` and the kinds `WINDOW_PRESS` to `WINDOW_TEXT`; `ClientCommandStatus::ITEM_OFF`; `ClientWindowItem` and `HostApi::client_windows`; ABI 12.24.
