### Changed

- `list_item`, `clone_item`, `add_recipe_to`, `derive_recipes_in` and both
  `list_workshop` helpers reserve the build's final length before they copy
  the base; `list_recipes` and `move_recipes` hand over their build in one
  reserve (#1136).
