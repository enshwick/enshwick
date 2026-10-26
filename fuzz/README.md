# Fuzz targets

libFuzzer targets for what Enshwick reads from other players, from files
people edit and from game images. They form a Cargo workspace of their own,
built by cargo-fuzz with the nightly compiler and AddressSanitizer.

| Target | Input |
| --- | --- |
| `wire_frames` | frame lines from another player's game, put back together, and the control messages they carry |
| `remote_message` | a settings or actions message from another player |
| `remote_edit` | a remote settings edit, applied to a settings file |
| `join_lists` | the mod lists in a server's advertisement and a player's hello |
| `server_rules` | a server's query rules reply |
| `action_sheet` | an actions sheet and the arguments of an action |
| `pattern` | a code pattern, run over code |
| `table` | a build table, its patterns run over code |
| `archive` | the game's archive: its maps, resources across chunks and contents |
| `reflection` | a game image's headers and reflection data |
| `migrations` | a mod's settings migrations on an edited file, and the record of what ran |
| `mod_manifests` | mod manifests, resolved into a load order |
| `settings_files` | the mod list, its lock, a settings schema, texts, a pack manifest |

Targets that take several fields split their input at the byte `0xff`, which
no UTF-8 text holds, so a seed is text fields joined by that byte.

## Running

```sh
cargo install cargo-fuzz --locked
cargo +nightly fuzz run -O --target-dir target/fuzz table target/fuzz/corpus/table fuzz/corpus/table
```

The first corpus directory takes what the run finds; `fuzz/corpus/<target>`
holds the small seed corpus kept in the repository. A crash is written to
`fuzz/artifacts/<target>/` and replays with
`cargo +nightly fuzz run -O --target-dir target/fuzz <target> <file>`.
A crash becomes a regression test beside the code it found, then a fix.

The nightly CI lane runs every target for a few minutes.
