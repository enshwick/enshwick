--!strict
-- Welcome: greets each player who joins and says how many are online.

local count = require("./lib/count")

enshwick.on("PlayerJoined", function(player)
	local joined = enshwick.world.player(player.entity)
	if joined then
		enshwick.world.tell(joined, `{enshwick.settings().greeting}, {joined.name}`)
	end
	enshwick.world.broadcast(count.online(#enshwick.world.players()))
end)

enshwick.scheduler.every(enshwick.settings().announce_minutes * 60, function()
	enshwick.log.info(count.online(#enshwick.world.players()))
	return true
end)
