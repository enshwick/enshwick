# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change. What mod authors
see is in the changelog of `enshwick`.

## [Unreleased]

### Changed

- `domains.toml` puts each module in a domain, lowest first, and
  `cargo xtask check` refuses a module that names a domain above its own
  (#1044).
- Each domain turns its own errors into `Off` and keeps its state on
  `Load` in a typed slot, so `Load` names no domain (#1044).
- The script namespaces are one list per domain, joined in the domain
  order (#1044).
- `ResourceType` lives here in the content domain, and `EngineFacts` in the
  base below it (#1044).
