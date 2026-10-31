# Running a server in Docker or Kubernetes

A dedicated server in a container gets Enshwick the way any server does: the
files next to `enshrouded_server.exe`. Nothing installs them for you; the
image, or a volume, carries them at versions you chose.

## Bake the files in

Put `powrprof.dll` and `mods/` into the image beside the server, or into a
volume mounted there, from files you keep:

- `enshwick.zip` holds the loader and every mod of this repository;
  `enshwick-loader.zip` holds the loader alone, and `enshwick-<id>.zip` a
  single mod;
- until the first release, 1.0.0, these exist only as the development build,
  `https://github.com/enshwick/enshwick/releases/download/dev/<file>`, which
  every new build replaces. Download the files once, keep them in your own
  storage (an artifact store, a bucket, the image's build context) and build
  from there, so that a rebuild of the image gets the same files;
- pin each archive by its SHA-256, which GitHub shows beside every release
  file, so the image is built from exactly the file you checked.

A template to fill in, with the archive in the build context; every `<...>`
is yours to set:

```dockerfile
FROM alpine:<version> AS enshwick
RUN apk add --no-cache unzip
ARG ENSHWICK_SHA256=<the archive's SHA-256>
WORKDIR /enshwick
COPY enshwick.zip ./
RUN echo "$ENSHWICK_SHA256  enshwick.zip" | sha256sum -c - \
 && unzip -q enshwick.zip -d files

FROM <your Enshrouded server image>
COPY --from=enshwick /enshwick/files/ <folder that holds enshrouded_server.exe>/
```

Mods of other authors follow the same steps with their release URLs.

## Change versions by rolling out

A new version is a new image or a new volume, rolled out like any other
change. Never update files inside a running container or pod: the next
restart would start a set nobody built, and replicas would drift apart.

After a game update, a loader without the new build's table runs
on the code patterns of the newest table it embeds, and only what they miss
stays off; `build_patterns = false` in `mod-configs/enshwick.toml` keeps such
a build vanilla instead. The loader saves what the patterns found in
`mod-data/enshwick/carried-server.txt` and reads it back while the game and
the loader stay the same, so only the first start after an update searches;
keep `mod-data/` on a volume to keep that file. Rebuild the image with a
loader that has the table once there is one.

## Move a world

Mods keep each world's data under `mod-data/<mod id>/worlds/<key>/`, and the
key holds a hash of the save's folder, such as `world0-1a2b3c4d`. A world
whose save moved to another folder gets a new key, such as a hosted world
brought to a dedicated server.

The loader finds the old data by itself. When a world with a save but no
mod data under its key loads, and one other key of the same save name has
data, the loader takes it over. It moves the folders when the old save is
gone and copies them when the old save is still there, and logs which.

When two or more keys of the same save name have data, the loader moves
nothing and logs the keys. The status summary lists them too. Stop the
server and rename the right folder under each `mod-data/<mod id>/worlds/`
to the new key, which the log names as `world key <key>, from <path>`.

## Check what loaded

At every start the loader writes `mods/enshwick/enshwick.lock`: its own
version and SHA-256, the game build, and each running mod's id, version, file
and SHA-256 (see [The loader and the ABI](concepts/loader-and-abi.md)).
Compare it with the lock of a server you trust, or with the hashes of the
files you baked in, to see that a pod runs exactly the image you meant.

## Greet players

Set `motd` in `mod-configs/enshwick.toml` to a message of the day. Every
player reads it in chat when they join:

```toml
motd = """
Welcome to Ember Vale.
Restarts at 06:00 UTC. Rules: discord.gg/example
"""
```

It is plain text. The loader keeps the first 4 lines and 500 bytes, drops
empty lines and control characters, and logs a warning when it cuts the
text. A change applies at the next join, without a restart.

## Mods from Nexus

Nexus downloads need a login, and through its API a Premium account, so no
build or server automation can fetch them. Download such a mod once yourself,
keep the file in your own storage (an artifact store, a bucket, the image's
build context) and bake it from there, by version.

## Linux hosts

A Linux image runs the Windows server under Proton: GE-Proton or Valve's,
pinned by version, run outside Steam. Plain Wine works too, but Proton
carries the fixes games need first. Set the DLL override so that the game
loads Enshwick's `powrprof.dll`:

```dockerfile
ENV WINEDLLOVERRIDES="powrprof=n,b"
```

[The optimizer](https://github.com/enshwick/mods/tree/main/optimizer)
lowers the server's CPU under Wine and Proton, and with a container CPU limit
starts no more task workers than the limit allows.
