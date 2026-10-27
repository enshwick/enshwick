--!strict
local count = {}

function count.online(players: number): string
	if players == 1 then
		return "1 player online"
	end
	return `{players} players online`
end

return count
