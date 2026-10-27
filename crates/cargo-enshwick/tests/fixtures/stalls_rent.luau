--!strict
-- The call `you.stalls:rent` v2 of you.stalls 1.2.0.
-- Written by `cargo enshwick call you.stalls:rent --luau`. Write it again for a newer
-- you.stalls rather than edit it: the runtime calls by the layout below.

--- The rent of a stall, in coins.
export type Rent = {
	--- The player's character.
	character: number,
	--- What the player does.
	deal: number,
	--- Whether the stall is the player's own.
	own: boolean,
	--- The stall's row.
	row: number,
	--- The stall's place in its row.
	place: number,
}

--- The rent of a stall, in coins.
return table.freeze({
	name = "you.stalls:rent",
	major = 2,
	input = { size = 12, fields = {
		{ name = "character", offset = 0, read = "entity", hash = "13059086432849901944" },
		{ name = "deal", offset = 4, read = "u32", hash = "4757988806913211573", shape = { name = "Deal", kind = "values", repr = "u32", values = { { name = "BUY", value = 0 }, { name = "SELL", value = 1 } } } },
		{ name = "own", offset = 8, read = "bool", hash = "3608188286181557718" },
		{ name = "row", offset = 9, read = "u8", hash = "13924221380763841713" },
		{ name = "place", offset = 10, read = "u16", hash = "3252260880205171709" },
	} },
	output = { size = 8, hash = "5081719993871294428", read = "u64" },
	Deal = table.freeze({ BUY = 0, SELL = 1 }),
})
