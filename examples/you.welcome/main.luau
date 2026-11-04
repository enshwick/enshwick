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

enshwick.schedule.every(enshwick.settings().announce_minutes * 60, function()
	enshwick.log.info(count.online(#enshwick.world.players()))
	return true
end)
