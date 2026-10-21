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
  has not listed yet after a load stays, also where its spot lies in the
  grown zone of another altar already listed.
- Removing the mod lets the game remove the copies.
- A copy staff holds 3 charges when full, shown as a bar on its slot, and its
  tooltip names how many it holds when full; the last charge uses the staff
  up when it places a copy. Bosses drop one at 10 %, chests that need a key or
  a lockpick hold one at 2 %, a random survivor's each time; a dropped staff
  is full, or holds a random count when `copy_staff_charges_min` is above 0.
  A staff from before charges gets full charges when the world loads, when
  its player joins and, in a chest, within a minute, on a dedicated server,
  in single player and in a hosted game alike. Removing the mod removes
  the staffs from inventories.

## Settings

`mod-configs/survivors-per-altar.toml`, written on first start; each
world keeps its own copy of it (see [Settings](../../README.md#settings)):

```toml
per_altar = true
copy_staffs = true
copy_staff_charges = 3
copy_staff_charges_min = 0
copy_staff_boss_chance = 0.10
copy_staff_locked_chest_chance = 0.02
vanilla_staff_places_copies = false
```

- `per_altar`: `false` turns the mod off.
- `copy_staffs`: whether copy staffs drop and work.
- `copy_staff_charges`: the charges of a full copy staff, 1 to 100.
- `copy_staff_charges_min`: a dropped staff holds from this many up to
  `copy_staff_charges`, at random; 0 drops every staff full. It may not be
  above `copy_staff_charges`.
- `copy_staff_boss_chance`, `copy_staff_locked_chest_chance`: the chance that a
  boss's corpse holds a staff, and that a chest which needs a key or a lockpick
  holds one, from 0 to 1; 0 turns that source off. A value above 1 counts as 1
  and below 0 as 0, with a warning in the log.
- `vanilla_staff_places_copies`: `true` lets the survivor's own staff place a
  copy at an altar without one and move the copy an altar has, as before copy
  staffs; `false` moves the survivor itself, as in vanilla.

`copy_staffs` is the server's: a joined player runs with the server's value,
and a change applies while the game runs. Off, copy staffs neither drop nor
work, and the staffs players hold stay for when they are on again. The other
settings apply after a restart.

## Install

Download [`enshwick-survivors-per-altar.zip`](https://github.com/themaiby/enshwick/releases/download/dev/enshwick-survivors-per-altar.zip)
and extract it into the folder that holds `enshrouded.exe` or
`enshrouded_server.exe`. It needs Enshwick Loader:
[`enshwick-loader.zip`](https://github.com/themaiby/enshwick/releases/download/dev/enshwick-loader.zip)
alone, or [`enshwick.zip`](https://github.com/themaiby/enshwick/releases/download/dev/enshwick.zip)
with every mod. These are development builds of the newest commit: until
the first release, 1.0.0, there are no others.

## Who needs it

The server and every player. It counts in the join check. It reads the altar
zones from the platform, so it works with or without Flame altar zones and
follows their grown zones when that mod runs.

Licensed under [MIT](../../LICENSE).
