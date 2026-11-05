--!strict
-- Shrine: the script layer over the data. items.toml binds the tonic's
-- actions to the two global functions below.

-- Each player's tonics, by account key, kept beside the world's save.
local drunk = enshwick.world_store("drunk")

-- Runs when a player drinks a tonic; returning nothing lets them.
function drink_tonic(used)
	local player = enshwick.world.player(used.player)
	if not player then
		return
	end
	local times = 1
	drunk:update(function(data)
		local seen = if type(data) == "table" then data else {}
		times = (seen[player.key] or 0) + 1
		seen[player.key] = times
		return seen
	end)
	enshwick.log.info(`{player.name} drank shrine tonic number {times}`)
end

-- Runs when a player throws a tonic away: a tonic is never thrown away.
function keep_tonic()
	return false
end
