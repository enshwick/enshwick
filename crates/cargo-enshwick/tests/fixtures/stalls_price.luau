--!strict
-- The extension point `you.stalls:price` of you.stalls 1.2.0.
-- Written by `cargo enshwick point you.stalls:price --luau`. Write it again for a newer
-- you.stalls rather than edit it: the runtime links by the layout below.

--- A "fair" price, in coins.
export type Price = {
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

--- A "fair" price, in coins.
return table.freeze({
	name = "you.stalls:price",
	mode = "override",
	refusable = true,
	input = { size = 12, fields = {
		{ name = "character", offset = 0, read = "entity", hash = "13059086432849901944" },
		{ name = "deal", offset = 4, read = "u32", hash = "4757988806913211573" },
		{ name = "own", offset = 8, read = "bool", hash = "3608188286181557718" },
		{ name = "row", offset = 9, read = "u8", hash = "13924221380763841713" },
		{ name = "place", offset = 10, read = "u16", hash = "3252260880205171709" },
	} },
	output = { size = 4, hash = "14918346923295848190", read = "u32" },
	Deal = table.freeze({ BUY = 0, SELL = 1 }),
})
