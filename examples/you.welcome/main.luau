--!strict
-- Welcome: greets each player who joins, counts their visits to the world,
-- and says how many are online.

local count = require("./lib/count")

-- Each player's visits, by account key, kept beside the world's save.
local visits = enshwick.world_store("visits")

enshwick.on("PlayerJoined", function(player)
	local joined = enshwick.world.player(player.entity)
	if joined then
		local times = 1
		visits:update(function(data)
			local seen = if type(data) == "table" then data else {}
			times = (seen[joined.key] or 0) + 1
			seen[joined.key] = times
			return seen
		end)
		enshwick.world.tell(joined, `{enshwick.settings().greeting}, {joined.name}: visit {times}`)
	end
	enshwick.world.broadcast(count.online(#enshwick.world.players()))
end)

-- A world's settings apply as it loads.
enshwick.on("SettingsReplaced", function()
	enshwick.log.info(`this world greets with "{enshwick.settings().greeting}"`)
end)

-- A key for administrators that tells everyone how many are online. The mod
-- runs on the server alone, so the server sends the control to each player's
-- controls page, and a press runs the action on the server.
local texts = enshwick.i18n.texts_from([[
[en]
"count" = "Tell everyone how many are online"

[uk]
"count" = "Сказати всім, скільки гравців онлайн"
]])
local actions = enshwick.actions.actions(texts)
actions:add({ id = "count", label = "count", admins = true }, function()
	local online = count.online(#enshwick.world.players())
	enshwick.world.broadcast(online)
	return enshwick.actions.done(online)
end)
actions:control("count", "F8")

enshwick.schedule.every(enshwick.settings().announce_minutes * 60, function()
	enshwick.log.info(count.online(#enshwick.world.players()))
	return true
end)
