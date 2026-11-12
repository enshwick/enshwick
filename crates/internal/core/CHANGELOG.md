# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change. What mod authors
see is in the changelog of `enshwick`.

## [Unreleased]

### Changed

- The actions domain left for `enshwick-actions`: `actions`, `chat`,
  `commands`, `console`, `controls`, `input`, `interactions` and `vetoes`.
  `Load` forwards their methods there, their script-only methods from
  `load/<module>/script.rs`, and the fake's action target conversions come
  from the crate's own hidden `__fake`; `bytemuck` is no longer a
  dependency (#1054).
- The knowledge domain left for `enshwick-knowledge`: `knowledge`,
  `progress`, `quests` and `rewards`. `Load` forwards their methods there,
  their script-only methods from `load/<module>/script.rs`, and `packs`
  replaces a pack's rewards through the domain's hidden `__internal`
  (#1053).
- `Bound` moved down to `enshwick-base`, and the base, content, world and
  inventory domains keep their `Bound` implementations in their own crates;
  `script/namespaces/` keeps only the namespace lists and core's own
  implementations (#1053).
- The inventory domain left for `enshwick-inventory`: `inventory`,
  `items`, `item_data`, `entity_data`, `affixes`, `stats`, `damage`,
  `effects`, `survivors` and `loot`. `Load` forwards their methods there;
  their script namespaces and `Bound` implementations stay here, and
  `actions` and `packs` reach the domain's private parts through its hidden
  `__internal` (#1052).
- The world domain left for `enshwick-world`: `World` with its methods,
  `world`, `map`, `altars`, `buffs`, `daytime`, `game_settings`, `terrain`,
  `voxels`, `weather`, `WorldError` and the typed handles. `Load` forwards
  their methods there; `limit_altar_level` and `on_fog_uncovered` keep
  their bodies here, since they join the world with crafting vetoes and
  the interface's frames, and the domains above reach the world's private
  parts through its hidden `__internal` (#1050).
- The content domain left for `enshwick-content`: `content`, `rates`, the
  shape samples of the generated typed items, and `KnowledgeType` and
  `Scope`, which `knowledge` re-exports. `Load` forwards its content
  methods there through the crate's method traits; the typed handles stay
  here, in the world domain, and the domain map lists no edge up its
  order (#1049).
- The base left for `enshwick-base`. `Load` holds its handle and forwards
  its methods there with `#[forward(base)]`, keeping their docs, examples
  and `#[script]`; the base's `Bound` implementations and namespace list
  stay here, in `script/namespaces/base.rs` (#1046).
- `domains.toml` names each peeled domain's crate, and `cargo xtask check`
  refuses one that depends on a crate above it (#1046).
- The rate rule `may_declare_rate` lives in `rates` (#1046).

- `domains.toml` puts each module in a domain, lowest first, and
  `cargo xtask check` refuses a module that names a domain above its own
  (#1044).
- Each domain turns its own errors into `Off` and keeps its state on
  `Load` in a typed slot, so `Load` names no domain (#1044).
- The script namespaces are one list per domain, joined in the domain
  order (#1044).
- `ResourceType` lives here in the content domain, and `EngineFacts` in the
  base below it (#1044).
