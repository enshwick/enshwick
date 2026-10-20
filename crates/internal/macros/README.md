# enshwick-loader-macros

Internal, not for mods: a part of [Enshwick Loader](../../../README.md). Mods
depend on [`enshwick-loader-api`](../../enshwick-loader-api) only.

Macros for [`enshwick-loader-api`](../../enshwick-loader-api), re-exported
from there: `#[derive(Config)]` writes a mod's commented settings file from its
type, and `#[hook]` declares a hook from its body.

Licensed under [MIT](../../../LICENSE).
