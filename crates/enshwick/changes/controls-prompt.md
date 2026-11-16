### Added

- `Load::prompt` gives the prompt of every object placed from an item one
  of the game's verbs (`interactions::Verb`), the game's empty interaction
  in place of the object's own action, and the mod's own text in the
  verb's place on clients with Enshwick (`interactions::ObjectPrompt`;
  unstable). Luau has it as `interactions.prompt`.
- `FakeHost::prompt` reads the prompt a mod gave an item.
- ABI 12.3: `HostApi::interaction_prompt` and `abi::InteractionPrompt`.
