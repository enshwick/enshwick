# Optimizer

Less CPU for an Enshrouded server, on Windows and under Wine or Proton, such
as the Docker images for Linux hosts, and the numbers to check it. Players
notice nothing.

The mod finds out where it runs, the dedicated server or the game, on
Windows or under Wine, and turns on only what was measured to help there.

| Change          | Windows server | Wine server         | Windows client | Wine client |
| --------------- | -------------- | ------------------- | -------------- | ----------- |
| Exact waits     | on             | on                  | off            | off         |
| Task workers    | off            | on with a CPU limit | off            | off         |
| Server wait cap | off            | off                 | off            | off         |

An idle dedicated server, process CPU, 100 % being one core (details under
[Measurements](#measurements)):

| Server                        | Without the mod | With the mod |
| ----------------------------- | --------------- | ------------ |
| Windows                       | 19.9 %          | 7.9 %        |
| Proton in Docker              | 37.7 %          | 16.1 %       |
| Proton in Docker, 4-CPU limit | 38.2 %          | 6.7 %        |

## What it changes

- **Exact waits.** The engine's event loop waits for its next deadline in
  whole milliseconds. With less than a millisecond left it asks for a 0 ms
  wait over and over until the deadline passes: about 0.6 ms of every 60 Hz
  frame of the main loop, about 0.9 ms of every 5 ms of the network session's
  loop, and 0.1 ms after every slow server pass. The mod sleeps that last
  fraction instead, then lets the game's own 0 ms wait run once, so the game
  still sees every event, and the mod waits on no handle of the game's. Under
  Wine the sleep is a plain Unix sleep; on Windows it is a high-resolution
  timer, which wakes on average 0.17 ms after the deadline, where the polling
  would have ended on it. On the dedicated server only: on the game the frame
  pacing of the window has not been measured.
- **Task workers.** The engine starts a task worker thread per logical CPU,
  up to 8 per group plus the ECS queue. Under Wine those are the host's CPUs,
  whatever the container's limit, so a server in a container limited to 4 CPUs
  on a 32-thread host runs 8 workers per group that wake hundreds of times a
  second each. With a CPU limit, the mod starts no more workers than the limit
  rounded up, and at least 2. The game's own log shows the result:
  `[task] Group 'Default' Worker:6 Main:2` is 4 workers and 2 main slots. The
  dedicated server's own start-up waits for the loader, so the cap is in place
  before the task system starts.
- **Server wait cap.** With players online the server's game loop waits 1 ms
  after each pass: 700 to 900 passes a second for 60 simulation steps. The cap
  lets it wait longer, never past the next step: a wait can wake one timer
  tick late (the timer resolution on Windows, a millisecond under Wine), so
  the longer wait ends a tick and half a millisecond before the step is due,
  and stays at 1 ms where that leaves less. Off by default everywhere:
  what it saves was measured only with an emulated player (see below), and
  what arrives between two steps may wait up to the cap.
- **Measure mode.** A line a minute in the log and the status summary: the
  platform and what runs on it, the CPU of the process, its main thread, the
  server loop's thread and its busiest groups of threads, the loop and step
  rates, the exact waits slept and how late they woke, and the server's "Bad
  Performance" warnings.
- **Environment report.** At start, one line names what the server runs on.
  Under Wine: the Wine version and host, the fast synchronisation available
  (ntsync, esync, fsync), the CPU governor, the container's CPU limit and the
  CPUs the game saw, and a line of advice for each thing on the host that is
  known to help and missing. On Windows: the logical CPUs.

An earlier community fix made every wait of every thread longer; CPU went
down, but players spawned under the ground and gliding broke. This mod does
not do that: the exact waits only sleep time the game would have spent
polling, and the simulation keeps its 60 steps a second.

```text
info  [optimizer] environment: Wine 11.0 on Linux 7.0.14-linuxkit, Proton variables set; sync: WINEESYNC=1, WINEFSYNC=1, WINENTSYNC=1; governor: unknown; cgroup CPU limit: 4.00 CPUs; logical CPUs: 16
info  [optimizer] the game started task workers: Default 6, Background 4, Shader 0 (it saw 16 logical CPUs)
info  [optimizer] measure: Wine server: exact waits on, task workers at most 4, server wait vanilla; CPU: process 7.7%, main thread 1.1%, server loop thread 2.3% (100% = one core); server loop 24/s, steps 24.4/s; exact waits 257/s of 962 µs on average, 81 µs late; Bad Performance 0; threads: GameServer 2.3%, Session 1.8%, Task-Default x4 1.3%, main 1.1%, P2pReceiveThread 0.4%, SteamworksSystem 0.2%
```

## Settings

`mod-configs/optimizer.toml`, written on first start; each
world keeps its own copy of it (see [Settings](../../README.md#settings)):

```toml
exact_waits = "auto"
task_workers = 0
server_wait_cap_ms = 1
measure = true
```

- `exact_waits`: `"auto"` where the table above says on, `"on"` everywhere,
  the game included, `"off"` never. Turning them off takes effect at once;
  turning them on takes effect at the next start if they were off at start.
- `task_workers`: the most workers each task group of the dedicated server
  starts, 1 to 64, read at start. 0 is automatic: the container's CPU limit
  under Wine, rounded up and at least 2, else as many as the game picks.
- `server_wait_cap_ms`: 1 to 8. 1 is vanilla. Before keeping more, check in
  game that gliding, joining or respawning and combat feel the same.
- `measure`: the line a minute. It costs a few atomic counters and a look at
  the threads once a minute.

A setting that forces exact waits or a task worker cap where the table says
off is logged as such.

## On the host

The biggest wins under Wine are outside any mod:

- **ntsync.** Recent Wine and Proton builds wait through the kernel's
  `/dev/ntsync` (Linux 6.14 and later) instead of the Wine server. Load the
  module (`modprobe ntsync`), pass the device into the container
  (`--device /dev/ntsync`) and check that the environment line says
  `/dev/ntsync present`. Without it, fsync (`WINEFSYNC=1`, the default of
  Proton) still beats the Wine server.
- **The `performance` governor.** With `powersave` or `schedutil` the CPU
  clocks down between the short bursts of a game loop, and every pass takes
  longer. `cpupower frequency-set -g performance` on the host.
- **CPU limits.** Wine counts the host's CPUs, not the container's limit. The
  mod caps the task workers to the limit; a node of the size you want, with
  requests and no limit, avoids the question.

## Measurements

Idle dedicated server on a copy of a played world, nobody online, two runs
of each, 3 minutes each after 2 minutes of loading; process CPU, 100 % being
one core. Windows: Windows 11, Ryzen 9 9950X3D. Proton: the
`mornedhels/enshrouded-server:stable-proton` image (GE-Proton 11-9, Wine
11.0, fsync, no ntsync) in Docker Desktop's Hyper-V virtual machine with 16
CPUs on the same machine; compare those rows with each other, not with
Windows.

| Server and settings                            | CPU    | Busiest threads                                   |
| ---------------------------------------------- | ------ | ------------------------------------------------- |
| Windows, no mod                                | 19.9 % |                                                   |
| Windows, the mod with everything off           | 19.9 % | Session 9.8 %, main 3.7 %, Task-Default x8 3.2 %  |
| Windows, defaults                              | 7.9 %  | Task-Default x8 3.4 %, GameServer 2.6 %           |
| Windows, defaults and `task_workers = 4`       | 4.7 %  | GameServer 2.3 %, Task-Default x4 0.9 %           |
| Proton, no mod                                 | 37.7 % |                                                   |
| Proton, the mod with everything off            | 38.0 % | Session 19.6 %, Task-Default x8 8.6 %, main 4.7 % |
| Proton, defaults                               | 16.1 % | Task-Default x8 9.3 %, GameServer 3.3 %           |
| Proton, defaults and `task_workers = 4`        | 7.1 %  | GameServer 2.3 %, Task-Default x4 1.3 %           |
| Proton, 4-CPU limit, no mod                    | 38.2 % |                                                   |
| Proton, 4-CPU limit, exact waits only          | 16.5 % | Task-Default x8 10.0 %, GameServer 3.5 %          |
| Proton, 4-CPU limit, defaults (4 task workers) | 6.7 %  | GameServer 2.3 %, Task-Default x4 1.3 %           |

Every run kept 24 simulation steps a second (the idle rate) and logged no Bad
Performance warning. The exact waits slept about 240 times a second on
Windows and 260 under Proton, for about 0.9 ms each, and woke on average
0.17 ms after the deadline on Windows and 0.08 ms under Proton.

With players online the server loop runs 700 to 900 passes a second. With
that loop emulated (the server loop forced to its online wait; real players
were not available), one run each: under Proton the process used 49.8 % with
everything off, 20.0 % with the defaults and 14.6 % with `task_workers = 4`;
with a 4-CPU limit 20.1 % with exact waits only and 10.9 % with the defaults.
The server wait cap at 4 ms took the Proton server from 29.6 % to 20.3 % but
changed nothing measurable on Windows, so it waits for a check with real
players. Capping the task workers on a server without a CPU limit halves
idle CPU again, but its cost with players was not measured, so it stays a
setting there.

## How to measure your own server

Compare a run with a change on against one with it off, on a test copy of
the server, never on the server people play on:

1. Copy the server and a copy of the save to a test instance with the same
   image and tag.
2. Start it with the change off, let it load, and keep it running a few
   minutes idle, then a few minutes with one player standing still. Note the
   `measure:` lines in `logs/enshwick/mods/optimizer.log`.
3. Turn the change on, restart, repeat.
4. Compare the process and thread CPU and the Bad Performance count. On a
   Linux host, `pidstat -t -p <pid> 5` shows each thread and `ps -o %cpu -C
   wineserver` the Wine server.

## Install

Download [`enshwick-optimizer.zip`](https://github.com/themaiby/enshwick/releases/download/dev/enshwick-optimizer.zip)
and extract it into the folder that holds `enshrouded.exe` or
`enshrouded_server.exe`. It needs Enshwick Loader:
[`enshwick-loader.zip`](https://github.com/themaiby/enshwick/releases/download/dev/enshwick-loader.zip)
alone, or [`enshwick.zip`](https://github.com/themaiby/enshwick/releases/download/dev/enshwick.zip)
with every mod. These are development builds of the newest commit: until
the first release, 1.0.0, there are no others.

## Who needs it

The dedicated server. In the game, hosting a game or not, every change is
off by default, as the table says, since none was measured there; a setting
forces one on (the task workers stay as the game picks, which only the
dedicated server lets the mod change), and measure mode and the environment
line run. Players need nothing, and it does not count in the join check.

Licensed under [MIT](../../LICENSE).
