# Runs the tests of the Windows-only crates off Windows: `cargo xtask check
# --windows`. The Wine prefix is created at build time so runs start fast.
# Debian's Wine, not GE-Proton's as on CI: GE-Proton 11, like every Wine 11,
# aborts making its prefix under the amd64 emulation Docker uses on Apple
# silicon (an assertion in anon_mmap_fixed, 2026-10-27).
FROM --platform=linux/amd64 debian:trixie-slim@sha256:f6b08b09a05725f9431502d3b5714f5c4576e82c1a84fa1debe0c24a7e0c3ec4
RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends wine64 wine \
    && rm -rf /var/lib/apt/lists/*
RUN WINEDEBUG=-all wineboot -i
