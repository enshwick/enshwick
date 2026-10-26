#!/usr/bin/env bash
# Packs the release build in target/release into the archives players
# download, in the current folder: enshwick.zip (the stub, the loader and
# every mod), enshwick-loader.zip (the stub and the loader) and <package>.zip
# for each mod. Writes `<package>\t<library>` for each mod to the file named
# by the first argument.
set -euo pipefail

list=${1:?usage: tools/pack.sh <mods.tsv>}

pack() { (cd "$1" && 7z a -tzip "../$2" . > /dev/null); }

mkdir -p all/mods/enshwick loader/mods/enshwick
for dir in all loader; do
  cp target/release/powrprof.dll "$dir/"
  cp target/release/enshwick_loader.dll "$dir/mods/enshwick/enshwick.dll"
  cp LICENSE "$dir/"
done
# Mods are the packages under mods/; other DLLs in target, such as proc
# macros, stay out. Each archive is named after the package, which is the
# release component, and the DLL after the library target, whatever the
# folder is called. jq on Windows ends lines with CRLF. A development mod
# marks itself `[package.metadata.enshwick] release = false` and stays out.
cargo metadata --no-deps --format-version 1 --locked |
  jq -r '.workspace_root as $root | .packages[]
    | select(.manifest_path | ltrimstr($root) | gsub("\\\\"; "/") | startswith("/mods/"))
    | select(.metadata.enshwick.release != false)
    | [.name, (.targets[] | select(.kind | index("cdylib")) | .name)] | @tsv' |
  tr -d '\r' > "$list"
[ -s "$list" ] || { echo "cargo metadata lists no mods"; exit 1; }
while IFS=$'\t' read -r name lib; do
  dll="target/release/$lib.dll"
  mkdir -p "$name/mods"
  cp "$dll" "$name/mods/$name.dll"
  cp LICENSE "$name/"
  cp "$dll" "all/mods/$name.dll"
  pack "$name" "$name.zip"
done < "$list"
pack loader enshwick-loader.zip
pack all enshwick.zip
