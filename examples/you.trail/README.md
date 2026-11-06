# Trail

A Luau mod that adds a quest of two steps, a lore page and a skill node to
the game's own journal and skill tree. Copy the folder into the game's
`mods/` folder as `mods/you.trail/`, on the server and on each client.

| File | What it adds |
| --- | --- |
| `pack.toml` | The pack, a mod of side both: the client draws the journal and the tree from its own data |
| `main.luau` | Three knowledge entries of each player, the quest, the lore page, the skill node and their texts in English and Ukrainian |
| `rewards.toml` | The actions that set the knowledge entries and so move the quest on |

## Try it

1. Start the server and the client with the mod, and join with a new
   character. Joining gives the quest and shows the lore page.
2. Open the journal: the quest "A Trail Through the Woods" asks for wood,
   and the page "The Old Trail" is under "Trail Notes".
3. Pick up wood logs: the first step is done and the second asks for resin.
4. Pick up resin: the quest is complete and pays a torch.
5. Open the skill tree: the node "Trail Endurance" sits beside the first
   root, costs 1 skill point and gives what the tree's first attribute node
   gives.

The book's chapters
[Knowledge entries](../../book/src/content/knowledge.md),
[Quests and journal pages](../../book/src/content/journal.md),
[Skill nodes and perks](../../book/src/content/skills.md) and
[Rewards for actions](../../book/src/content/rewards.md) describe each call.
