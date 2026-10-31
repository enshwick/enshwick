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
| `item_records` | the mod item data records a server sends a client |
| `notices` | notice messages from a server, queued and shown as the client's steps show them |
| `action_sheet` | an actions sheet and the arguments of an action |
| `pattern` | a code pattern, run over code |
| `table` | a build table, its patterns run over code |
| `reflection` | a game image's headers and reflection data |
| `migrations` | a mod's settings migrations on an edited file, and the record of what ran |
| `mod_manifests` | mod manifests, resolved into a load order |
| `point_schemas` | another mod's point schemas, from its DLL or as text, written as definitions |
| `settings_files` | the mod list, its lock, a settings schema, texts, a pack manifest |
| `item_save` | a world's file of mod item data |

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

On a pull request CI runs `cargo xtask check --part fuzz`: the targets
whose input the change reaches share 45 seconds, each at least 5, over the
seed corpus. A push to `main` runs every target the same way and fills the
build cache pull requests start from. CI builds with the dated nightly that
[`setup-rust`](../.github/actions/setup-rust/action.yml) pins and hands
xtask in `ENSH_FUZZ_TOOLCHAIN`; without it xtask takes `nightly`. Which modules reach which target is the table in
[`tools/xtask/src/fuzz.rs`](../tools/xtask/src/fuzz.rs); a new target gets
a row there. Before a pull request that changes what a target reads, also
run that target here for longer, a minute or more with
`-- -max_total_time=60` after the command above.
