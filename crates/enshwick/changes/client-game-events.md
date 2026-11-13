### Added

- Observe client-world events through raw or generated typed handlers, without claiming persistent UI selection state (#1078).
- Append ABI 11.28 client subscriptions and model their independent delivery and per-mod limits in the fake host (#1078).
- Copy client events under a separate loader batch and fuse with bounded copying and traversal (#1078).
