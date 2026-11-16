### Added

- With the `unstable` feature, `Load::add_container` adds a container type: a prop copied from a vanilla
  container whose template holds `NewContainer::slots` slots, whole rows of 8
  up to 128.
- With the `unstable` feature, `Load::container_rule` decides, stack by stack, what enters the containers
  placed from an item, a mod's own or a vanilla one: a drag, a swap, Take
  All, Deposit All or Stacks, Refill Stacks and an area deposit are all asked,
  and a refused stack stays where it was.
- With the `unstable` feature, an interaction filter's `Interaction::moved` carries what a move moves:
  its kind (`interactions::Transfer`, `Interaction::transfer`), the game's
  `InventoryActionType` and amount, the stack, both containers and both
  slots.
- With the `unstable` feature, `Batch::hide_in_ui` gives without the pickup message, and
  `Inventories::show_pickup` shows the message without a give.
- `Interaction::new` builds an ABI call with the other fields zero.
- The Luau mirror: `enshwick.inventory.add_container`,
  `enshwick.inventory.container_rule`, `enshwick.inventory.show_pickup`,
  `hide_in_ui` in a batch and `moved` in an interaction.
- ABI 12.9: `HostApi::container_rule` with `ContainerPut` and
  `ContainerRuleFn`, `HostApi::show_pickup`, `InventoryRequest::HIDE_IN_UI`,
  and the move fields of `Interaction` with its `ACTION_*` and `TRANSFER_*`
  constants.

### Changed

- With the `unstable` feature, `interactions::Interaction` has a new public
  field, `moved`: code built with it that writes an `Interaction` as a struct
  literal names `moved: None`.
