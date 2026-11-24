### Added

- `events::QuestStarted`, `events::QuestStepDone` and `events::QuestProgressed`: the loader derives them from the knowledge that quest steps test, for the game's quests and mods' quests, and a step that a game query decides is asked again for three steps after a change of the knowledge its query reads. What a player holds when they join raises nothing.
- `Load::on_quest_reward` with `quests::QuestPayment` and `quests::QuestPay`: a filter keeps, changes or refuses the experience the game pays for a quest, in the game's own payment; a refused quest pays nothing and still completes. A filter sees whether one before it refused (`allowed`); other values leave a refusal standing, and `QuestPay::Allow` pays a refused quest again. Luau has `enshwick.quests.on_quest_reward`.
- ABI 12.22: `Event::QUEST_STARTED`, `Event::QUEST_STEP_DONE`, `Event::QUEST_PROGRESSED`, `abi::QuestStepData`, `abi::QuestProgressData`, `abi::QuestPayment`, `abi::QuestRewardFilterFn` and `HostApi::quest_reward_filter`.
- `Knowledge::set` and `Knowledge::set_world` with `knowledge::KnowledgeNotice`, unstable: a mod sets a knowledge value exactly, quietly with every bit marked seen, or shown, granting what it gains the game's way. Luau has `enshwick.knowledge.set` and `enshwick.knowledge.set_world`. ABI 12.22: `HostApi::knowledge_set`, `KnowledgeChange::QUIET` and `KnowledgeChange::SHOWN`.

### Changed

- `rewards::Action::Quest` takes a quest id and follows that quest; `rewards::Action::AnyQuest` follows every quest, as `Action::Quest` did. In Luau, `"Quest"` becomes `"AnyQuest"`, and `{ Quest = id }` follows one quest.
- `Knowledge::revoke` keeps the seen mark of a flags value, as the game's own grant does, instead of clearing it.
