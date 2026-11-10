--!strict
-- Trail: a quest, a lore page and a skill node. rewards.toml sets the
-- knowledge entries below as a player joins and picks things up.

local content = enshwick.content

-- Tool_Torch01, the reward the quest shows and the script gives.
local TORCH = "224097e5-ed0d-49d2-9d23-43256a6682b3"

-- The game's lore category "General": the journal lists a page only under
-- one of its own categories.
local GENERAL = "f94a5ece-d3fb-4f32-8d6e-e72bdffaba39"

local TEXTS: { [string]: { en: string, uk: string } } = {
	["trail.name"] = { en = "A Trail Through the Woods", uk = "Стежка крізь ліс" },
	["trail.wood"] = { en = "Gather wood", uk = "Збери деревину" },
	["trail.wood.text"] = { en = "Pick up any wood logs.", uk = "Підбери будь-які колоди." },
	["trail.resin"] = { en = "Gather resin", uk = "Збери смолу" },
	["trail.resin.text"] = {
		en = "Pick up resin. The trail pays a torch.",
		uk = "Підбери смолу. Стежка дасть смолоскип.",
	},
	["lore.name"] = { en = "The Old Trail", uk = "Стара стежка" },
	["lore.entry"] = { en = "First Mark", uk = "Перша позначка" },
	["lore.entry.text"] = {
		en = "Someone cut marks into the bark here, one for each day of walking.",
		uk = "Хтось вирізав тут позначки на корі, по одній за кожен день ходу.",
	},
	["endurance.name"] = { en = "Trail Endurance", uk = "Витривалість стежки" },
	["endurance.text"] = {
		en = "Long walks make for strong legs.",
		uk = "Довгі переходи зміцнюють ноги.",
	},
}

-- The tag of the mod's own text `key`, with its texts given.
local function text(key: string): string
	local tag = content.guid(`you.trail:{key}`)
	content.add_text(tag, "en", TEXTS[key].en)
	content.add_text(tag, "uk", TEXTS[key].uk)
	return tag
end

local function player_bool(name: string): number
	return content.add_knowledge({ id = `you.trail:{name}`, scope = "Player", kind = "Bool" })
end

local function is_set(entry: number): KnowledgeQuery
	return { id = entry, scope = "Player", test = "IsSet" }
end

local given = is_set(player_bool("given"))
local wood = is_set(player_bool("wood"))
local resin = is_set(player_bool("resin"))

local trail = content.add_quest({
	id = "you.trail:trail",
	name = text("trail.name"),
	source = "None",
	kind = "PlayerQuest",
	steps = {
		{ name = text("trail.wood"), text = text("trail.wood.text"), shown_when = given, done_when = wood },
		{ name = text("trail.resin"), text = text("trail.resin.text"), shown_when = wood, done_when = resin },
	},
	rewards = { { Item = TORCH } },
})

-- The game gives a quest's experience and only draws its item, so the mod
-- gives the torch itself. What a full inventory leaves out is the mod's to
-- decide: this one tells the player. A server does not know a player's
-- language here, so the notice says it in both.
local NO_ROOM = "No room for the trail's torch. / Немає місця для смолоскипа стежки."
local torch = content.item_id(TORCH)

enshwick.on("QuestCompleted", function(done)
	if done.quest ~= trail then
		return
	end
	enshwick.inventory.give_what_fits(done.entity, torch, 1, function(left, outcome)
		if outcome.done ~= "Committed" then
			enshwick.log.warn(`torch not given: {outcome.done}`)
			return
		end
		local player = enshwick.world.player(done.entity)
		if left > 0 and player then
			enshwick.world.notify(player, NO_ROOM)
		end
	end)
end)

content.add_journal_page({
	id = "you.trail:lore",
	category = GENERAL,
	name = text("lore.name"),
	entries = { { name = text("lore.entry"), text = text("lore.entry.text"), shown_when = given } },
})

-- The node copies the tree's first attribute and sits beside the first root,
-- at the first place far enough from every node: the loader refuses a node
-- closer than half the tree's smallest gap between two nodes.
local nodes = content.skill_nodes()
local root, attribute
for _, node in nodes do
	if node.kind == "Root" and not root then
		root = node
	elseif node.kind == "Attribute" and not attribute then
		attribute = node
	end
end

local function apart(a: SkillNode, x: number, y: number): number
	return math.sqrt((a.x - x) ^ 2 + (a.y - y) ^ 2)
end

local gap = math.huge
for i, a in nodes do
	for j = i + 1, #nodes do
		gap = math.min(gap, apart(a, nodes[j].x, nodes[j].y))
	end
end

local function clear(x: number, y: number): boolean
	for _, node in nodes do
		if apart(node, x, y) <= gap / 2 then
			return false
		end
	end
	return true
end

local function place(root: SkillNode, like: SkillNode)
	for distance = gap, 6 * gap, gap / 2 do
		for _, side in { { 1, 0 }, { 0, 1 }, { -1, 0 }, { 0, -1 } } do
			local x, y = root.x + side[1] * distance, root.y + side[2] * distance
			if clear(x, y) then
				local node = content.add_skill_node({
					id = "you.trail:endurance",
					like = like.id,
					x = x,
					y = y,
					links_from = { root.id },
					name = text("endurance.name"),
					description = text("endurance.text"),
					cost = 1,
				})
				enshwick.log.info(`node {node} copies {like.id} at {x}, {y}, linked from root {root.id}`)
				return
			end
		end
	end
	enshwick.log.warn("no free place beside the first root")
end

if root and attribute then
	place(root, attribute)
end
