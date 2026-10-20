# Optimizer

Less CPU for a server that runs under Wine or Proton, such as the Docker
images for Linux hosts, and the numbers to check it. Players notice nothing.

- **Exact waits.** The engine's event loop waits for its next deadline in
  whole milliseconds. With less than a millisecond left it asks for a 0 ms wait
  over and over until the deadline passes: about 0.6 ms of every 60 Hz frame,
  and 0.1 ms after every slow server pass. Under Wine each of those waits is a
  call into the Wine server or the kernel. The mod sleeps that last fraction of
  a millisecond instead, then lets the game's own 0 ms wait run once, so the
  game still sees every event. It never waits longer than the game asked and
  never takes an event from the game. On by default under Wine only.
- **Server wait cap.** With players online the server's game loop waits 1 ms
  after each pass: about 900 passes a second for 60 simulation steps. The cap
  lets it wait longer, never past the next step. Off (vanilla) by default.
- **Measure mode.** A line a minute in the log and the status summary: the CPU
  of the process, its main thread and the server loop's thread, the loop and
  step rates, the exact waits slept and the server's "Bad Performance"
  warnings.
- **Environment report.** At start, one line names what the server runs on:
  the Wine version and host, the fast synchronisation available (ntsync,
  esync, fsync), the CPU governor, the container's CPU limit and the CPUs the
  game saw.

The mod changes two places in the event loop and nothing else. An earlier
community fix made every wait of every thread longer; CPU went down, but
players spawned under the ground and gliding broke. This mod does not do that.

On a Windows test server, idle, with `exact_waits = "on"`:

```text
info  [optimizer] environment: Windows, not Wine; logical CPUs: 32, the game saw 32 (task workers: Default 10, Background 8, Shader 0)
info  [optimizer] measure: CPU: process 12.7%, main thread 1.7%, server loop thread 2.5% (100% = one core); server loop 24/s, steps 24.3/s; exact waits 129/s of 359 µs on average; Bad Performance 0
```

## Settings

`mod-configs/optimizer.toml`, written on first start:

```toml
exact_waits = "auto"
server_wait_cap_ms = 1
measure = true
```

- `exact_waits`: `"auto"` under Wine and Proton only, `"on"` on Windows too,
  `"off"` never. Turning them off takes effect at once; turning them on takes
  effect at the next start if they were off at start.
- `server_wait_cap_ms`: 1 to 8. 1 is vanilla; 2 to 4 saves CPU with players
  online. Before you keep it, check in game that gliding, joining or respawning
  and combat feel the same.
- `measure`: the line a minute. It costs a few atomic counters.

## On the host

The biggest wins reported by server owners are outside any mod:

- **ntsync.** Recent Wine and Proton builds can use the kernel's
  `/dev/ntsync` (Linux 6.14 and later) for waits and events instead of the
  Wine server. Owners report a large drop in CPU. Load the module
  (`modprobe ntsync`), pass the device into the container
  (`--device /dev/ntsync`) and check that the environment line says
  `/dev/ntsync present`. Without ntsync, esync or fsync (`WINEESYNC=1`,
  `WINEFSYNC=1`) still beat the Wine server.
- **The `performance` governor.** With `powersave` or `schedutil` the CPU
  clocks down between the short bursts of a game loop, and every pass takes
  longer. `cpupower frequency-set -g performance` on the host; the
  environment line shows the governor the container sees.
- **CPU limits.** Wine counts the host's CPUs, not the container's limit, and
  the game starts task workers for each of them. In a container with a CPU
  limit on a large host those workers fight over the quota. Prefer CPU
  requests without a limit, or a node of the size you want.

## How to measure

Compare a run with the mod's changes on against one with them off, on a test
copy of the server, never on the server people play on:

1. Copy the server and a copy of the save to a test instance with the same
   image and tag.
2. Start it with `exact_waits = "off"`, let it load, and keep it running 10
   minutes idle, then 10 minutes with one player standing still. Note the
   `measure:` lines in `logs/enshwick/mods/optimizer.log`.
3. Set `exact_waits = "auto"`, restart, repeat. The same for
   `server_wait_cap_ms = 4` against 1, with players online.
4. Compare the process and thread CPU and the Bad Performance count. On the
   host, `pidstat -t -p <pid> 5` shows each thread, `ps -o %cpu -C
   wineserver` the Wine server (it matters without ntsync), and `strace -f -c
   -p <pid>` for 30 seconds counts the system calls.

On a Windows test server the same lines work; there `exact_waits = "on"`
forces the change. On a Windows test server, idle, measured from outside over
3 minutes after loading: the process used 20.6 % of one core with the exact
waits off and 13.3 % with them on, in each of two runs. Under Wine, where every
poll costs more, the gain should be larger; that is what to measure.

## Who needs it

The server, the dedicated one or a game a client hosts. On a client it also
serves the client's own event loop when `exact_waits` applies. Players need
nothing, and it does not count in the join check.

Licensed under [MIT](../../LICENSE).
