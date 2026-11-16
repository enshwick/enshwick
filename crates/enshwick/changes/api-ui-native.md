### Added

- `Frame::item_slot`, with the `unstable` feature, draws an item slot in the
  game's style in a window or a screen panel and says whether it was clicked
  or is under the cursor; `Frame::hovered_slot` reads the slot of the game's
  own inventory screen under the cursor.
- `Load::extend_item_tooltip`, with the `unstable` feature, adds lines under
  the game's rows in the tooltip and detail panel of every item with an entity
  of its own, asked once each time the tooltip shows another item.
- ABI 12.4: `UiWidget::ITEM_SLOT` with `UiWidget::item` and `count`, the
  hovered slot in `UiFrame`, `UiTooltipItem`, `UiTooltipFn` and
  `HostApi::ui_item_tooltip`.
- The `item-ui` example mod and the book's sections on item slots and item
  tooltip lines.
