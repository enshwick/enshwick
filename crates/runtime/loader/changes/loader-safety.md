### Fixed

- A world whose mod folders belonged to another world, and of which one folder
  could not be renamed aside, renames back the folder it renamed first. The
  worlds' mod folders then turn off until the game restarts, and the player is
  told that "world mod folders" are off, so no world reads another world's mod
  settings or data. A taken `.old-<date>-<time>` name gets `-2` on (#1294).
- A step with more than 1024 destroy requests checks every item data record
  against the world, so no value stays on an entity id the game gives to a new
  entity (#1294).
- Saved stat modifiers whose write failed are written again at the next save
  (#1294).
- Content mods serve is kept up to 3072 contents or 512 MiB in all; past that
  `content_serve` answers `REFUSED` for the rest of the process. Serving no
  longer copies every served content's entry, and the content request reads
  only a request count and array that lie in readable memory, else passes the
  call to the game (#1294).
- `inventory_submit` answers `BUSY` while 4096 requests wait (#1294).
- The log writes its queued lines before the shutdown goes on and before a
  crash report, so the last lines are not lost (#1294).
- A panic in the settings watch turns off only the loader's settings watch or
  the world settings watch, not both, and the thread goes on (#1294).
- The held chests' Quick Stash turns off again after a save whose save link
  panicked (#1294).
- The knowledge queue, the world's terrain materials record and the mod record
  are not written when their text does not serialize, instead of being
  replaced by an empty file (#1294).
- The loader's own file writes on the game thread wait about 30 ms per step for
  a file another program holds, not a second, so the game does not stall for
  seconds (#1294).
