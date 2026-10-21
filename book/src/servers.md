# Running a server in Docker or Kubernetes

A dedicated server in a container gets Enshwick the way any server does: the
files next to `enshrouded_server.exe`. Nothing installs them for you; the
image, or a volume, carries them at versions you chose.

## Bake the files in at a pinned version

Put `powrprof.dll` and `mods/` into the image beside the server, or into a
volume mounted there, at fixed versions:

- `enshwick.zip` from the platform release holds the loader and every mod of
  this repository:
  `https://github.com/themaiby/enshwick/releases/download/enshwick-loader-v<version>/enshwick.zip`;
- a single mod comes from its own release:
  `https://github.com/themaiby/enshwick/releases/download/enshwick-<id>-v<version>/enshwick-<id>.zip`,
  beside `enshwick-loader.zip` (the loader alone) from the platform release;
- every archive has a `.minisig` beside it. Check it with Enshwick's public
  key, given in the [README](https://github.com/themaiby/enshwick#install),
  before the files go in. The trusted comment names the tag and the file.

Avoid `releases/latest/download/...` in a build: it changes under you.

Until the first release, 1.0.0, there are no versioned releases, only the
development build at `releases/download/dev/<file>`, which every new build
replaces. Download it once, keep the files in your own storage and build from
there; the trusted comment of its signatures is `dev <short sha> <file>`.

A template to fill in; every `<...>` is yours to set:

```dockerfile
FROM alpine:<version> AS enshwick
RUN apk add --no-cache curl minisign unzip
ARG ENSHWICK_VERSION=<version>
ARG ENSHWICK_KEY=<public key from the README>
WORKDIR /enshwick
RUN base=https://github.com/themaiby/enshwick/releases/download/enshwick-loader-v${ENSHWICK_VERSION} \
 && curl -fsSLO "$base/enshwick.zip" \
 && curl -fsSLO "$base/enshwick.zip.minisig" \
 && minisign -Vm enshwick.zip -P "$ENSHWICK_KEY" \
 && unzip -q enshwick.zip -d files

FROM <your Enshrouded server image>
COPY --from=enshwick /enshwick/files/ <folder that holds enshrouded_server.exe>/
```

Mods of other authors follow the same steps with their release URLs and the
key their README publishes.

## Change versions by rolling out

A new version is a new image or a new volume, rolled out like any other
change. Never update files inside a running container or pod: the next
restart would start a set nobody built, and replicas would drift apart.

The [updater](https://github.com/themaiby/enshwick/tree/main/mods/enshwick-updater)
is a client mod: a dedicated server skips it, so it never changes files in a
container. After a game update, a loader without the new build's table runs
on the code patterns of the newest table it embeds; rebuild the image with a
loader that has the table.

## Check what loaded

At every start the loader writes `mods/enshwick/enshwick.lock`: its own
version and SHA-256, the game build, and each running mod's id, version, file
and SHA-256 (see [The loader and the ABI](concepts/loader-and-abi.md)).
Compare it with the lock of a server you trust, or with the hashes of the
files you baked in, to see that a pod runs exactly the image you meant.

## Mods from Nexus

Nexus downloads need a login, and through its API a Premium account, so no
build or server automation can fetch them. Download such a mod once yourself,
keep the file in your own storage (an artifact store, a bucket, the image's
build context) and bake it from there, by version.

## Linux hosts

A Linux image runs the Windows server under Wine or Proton. Set the DLL
override so that the game loads Enshwick's `powrprof.dll`:

```dockerfile
ENV WINEDLLOVERRIDES="powrprof=n,b"
```

[The optimizer](https://github.com/themaiby/enshwick/tree/main/mods/enshwick-optimizer)
lowers the server's CPU under Wine and Proton, and with a container CPU limit
starts no more task workers than the limit allows.
