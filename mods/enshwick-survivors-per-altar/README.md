# Survivors per altar

Every friend's house gets its own Blacksmith, Alchemist and the rest. A survivor
can have a copy at every Flame Altar: a cluster of N altars holds at most N
copies of a survivor, one per altar zone, and a copy always stands inside a
zone.

- The survivor's own summoning staff works as in vanilla: it moves the
  survivor itself, the original, however many copies stand in the world. In
  an altar zone that holds a copy, the original moves in and that copy is
  removed: one survivor per zone. Copies elsewhere stay where they are.
- Copy staffs make copies: one per workshop survivor, such as "Copy staff:
  Oswald Anders", its icon the survivor's staff in violet. The survivor's
  name comes from the game in the player's language, as the game spells it
  ("Посох копії: Освальд Андерс" in Ukrainian). At an altar zone:
  - without a copy and without the original, it places a new copy there and
    spends a charge;
  - with a copy, it moves that copy to where it is aimed, free;
  - where the original stands, it is refused and spends nothing;
  - outside every altar zone, it is refused and spends nothing.

  A survivor never has more entities, the original included, than there are
  altars: a cluster of N altars holds at most N.
- Each copy works where it stands and sleeps in a bed of its own altar. A bed at
  another altar is never given to it, and a bed it held from elsewhere is
  cleared.
- The survivor menu shows the worst status of the copies, so "no bed" means some
  copy still needs one.
- When an altar is removed, the last one too, its copies go with its zone ten
  seconds later; the original stays where it is. A copy whose altar the game
  has not listed yet after a load stays.
- Removing the mod lets the game remove the copies.
- A copy staff holds 3 charges when full, shown as a bar on its slot, and its
  tooltip names how many it holds when full; the last charge uses the staff
  up when it places a copy. Bosses drop one at 10 %, chests that need a key or
  a lockpick hold one at 2 %, a random survivor's each time; a dropped staff
  is full, or holds a random count when `copy_staff_charges_min` is above 0.
  A staff from before charges gets full charges when the world loads, when
  its player joins and, in a chest, within a minute. Removing the mod removes
  the staffs from inventories.

## Settings

`mod-configs/survivors-per-altar.toml`, written on first start:

```toml
per_altar = true
copy_staffs = true
# The charges of a full copy staff, from 1 to 100.
copy_staff_charges = 3
# A dropped staff holds from this many up to copy_staff_charges, at random;
# 0 drops every staff full.
copy_staff_charges_min = 0
# From 0 to 1; 0 turns that source off.
copy_staff_boss_chance = 0.10
copy_staff_locked_chest_chance = 0.02
# true: the survivor's own staff places a copy at an altar without one and
# moves the copy an altar has, as before copy staffs.
vanilla_staff_places_copies = false
```

`copy_staffs` is the server's: a joined player runs with the server's value,
and a change applies while the game runs. Off, copy staffs neither drop nor
work, and the staffs players hold stay for when they are on again. The other
settings apply after a restart.

## Install

Download [`enshwick-survivors-per-altar.zip`](https://github.com/themaiby/enshwick/releases/latest/download/enshwick-survivors-per-altar.zip),
always the newest version of this mod, and extract it into the folder that
holds `enshrouded.exe` or `enshrouded_server.exe`. It needs Enshwick Loader:
[`enshwick-loader.zip`](https://github.com/themaiby/enshwick/releases/latest/download/enshwick-loader.zip)
alone, or [`enshwick.zip`](https://github.com/themaiby/enshwick/releases/latest/download/enshwick.zip)
with every mod.

## Who needs it

The server and every player. It counts in the join check. It reads the altar
zones from the platform, so it works with or without Flame altar zones and
follows their grown zones when that mod runs.

Licensed under [MIT](../../LICENSE).
