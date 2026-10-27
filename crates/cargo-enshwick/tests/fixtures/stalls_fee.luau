--!strict
-- The call `you.stalls:fee` v1 of you.stalls 1.2.0.
-- Written by `cargo enshwick call you.stalls:fee --luau`. Write it again for a newer
-- you.stalls rather than edit it: the runtime calls by the layout below.

--- The fee for holding a stall, in coins.
export type Fee = {
	--- The player's character.
	character: number,
	--- How many days the stall is held.
	days: number,
	--- The stall's place in its row.
	place: number,
	--- Whether the stall is the player's own.
	own: boolean,
	--- The stall's row.
	row: number,
}

--- The fee for holding a stall, in coins.
return table.freeze({
	name = "you.stalls:fee",
	major = 1,
	input = { size = 12, fields = {
		{ name = "character", offset = 0, read = "entity", hash = "13059086432849901944" },
		{ name = "days", offset = 4, read = "u32", hash = "5981015992674252258" },
		{ name = "place", offset = 8, read = "u16", hash = "7716891694140350527" },
		{ name = "own", offset = 10, read = "bool", hash = "17590301545955930516" },
		{ name = "row", offset = 11, read = "u8", hash = "18388852194699020531" },
	} },
	output = { size = 8, hash = "5081719993871294428", read = "u64" },
})
