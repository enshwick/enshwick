# Survivors per altar

Every friend's house gets its own Blacksmith, Alchemist and the rest. A survivor
can have a copy at every Flame Altar: a cluster of N altars holds at most N
copies of a survivor, one per altar zone, and a copy always stands inside a
zone.

- The survivor's summoning staff, used in an altar zone without a copy, places a
  new copy there; in a zone that has one, it moves that copy.
- Each copy works where it stands and sleeps in a bed of its own altar. A bed at
  another altar is never given to it, and a bed it held from elsewhere is
  cleared.
- The survivor menu shows the worst status of the copies, so "no bed" means some
  copy still needs one.
- The original survivor is never touched; removing the mod lets the game remove
  the copies.

## Settings

`mods/survivors-per-altar.toml`, written on first start:

```toml
per_altar = true
```

## Who needs it

The server and every player. It needs Flame altar zones and counts in the join
check.
