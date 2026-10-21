# Changelog

## [0.6.0](https://github.com/themaiby/enshwick/compare/enshwick-updater-v0.5.1...enshwick-updater-v0.6.0) (2026-10-21)


### ⚠ BREAKING CHANGES

* **updater:** accept common GitHub release forms and unsigned updates ([#198](https://github.com/themaiby/enshwick/issues/198))
* **loader:** show only updates of installed mods on the Mods screen ([#196](https://github.com/themaiby/enshwick/issues/196))
* **api:** name, describe and credit mods, with recommends and breaks ([#126](https://github.com/themaiby/enshwick/issues/126))
* **api:** config stages, ranges, choices, schema and /config ([#130](https://github.com/themaiby/enshwick/issues/130))
* **api:** one naming scheme, run-time errors and named abi values ([#110](https://github.com/themaiby/enshwick/issues/110))
* consolidate into Enshwick Loader, one API crate and one tool ([#85](https://github.com/themaiby/enshwick/issues/85))

### Features

* add the updater mod and report a server's missing mods ([#65](https://github.com/themaiby/enshwick/issues/65)) ([13dfe21](https://github.com/themaiby/enshwick/commit/13dfe21934d5e903aa4aeea56c7450e62e2963c4))
* **api:** config stages, ranges, choices, schema and /config ([#130](https://github.com/themaiby/enshwick/issues/130)) ([8a19e89](https://github.com/themaiby/enshwick/commit/8a19e89afe6ac74d09d9a32c66b1fc133e81bc68))
* **api:** draw with the game's own widgets and show its dialogs ([#129](https://github.com/themaiby/enshwick/issues/129)) ([8601c70](https://github.com/themaiby/enshwick/commit/8601c70f8a0678a50f41301467d713bf41fab855))
* **api:** name, describe and credit mods, with recommends and breaks ([#126](https://github.com/themaiby/enshwick/issues/126)) ([edfb07c](https://github.com/themaiby/enshwick/commit/edfb07c59676c441f1e8b06ddc2791a80594ae6c))
* author keys: updates must be signed by the installed version's key ([#81](https://github.com/themaiby/enshwick/issues/81)) ([d9d46f3](https://github.com/themaiby/enshwick/commit/d9d46f38231234e958a8762e6c0fff5b6ddcb5d8))
* **loader:** add content packs of TOML files, signed and reloadable ([#158](https://github.com/themaiby/enshwick/issues/158)) ([7598b97](https://github.com/themaiby/enshwick/commit/7598b977434dfb207cd5c0d30c6cdd936c0cb0cc))
* **loader:** never start a mod that a renamed mod replaces ([#166](https://github.com/themaiby/enshwick/issues/166)) ([650993c](https://github.com/themaiby/enshwick/commit/650993c7b392be07fc16c4ca47925952c64f1487))
* **loader:** ship build tables as signed data with a pattern fallback ([#155](https://github.com/themaiby/enshwick/issues/155)) ([1df2e3f](https://github.com/themaiby/enshwick/commit/1df2e3fdcaeaeac3ca082a18976e6a3f1d3a46c9))
* **loader:** show mod catalogs with updates and mods on the Mods screen ([#180](https://github.com/themaiby/enshwick/issues/180)) ([2e93fb7](https://github.com/themaiby/enshwick/commit/2e93fb7ec219b7c4b4fcab0ff45dcd4c409340d6))
* **loader:** show only updates of installed mods on the Mods screen ([#196](https://github.com/themaiby/enshwick/issues/196)) ([aee82ac](https://github.com/themaiby/enshwick/commit/aee82acb3a6a0f9d8aa2ebbe414a7a31961c1cd7))
* **updater:** accept common GitHub release forms and unsigned updates ([#198](https://github.com/themaiby/enshwick/issues/198)) ([51d3e47](https://github.com/themaiby/enshwick/commit/51d3e4754a34496c5c90ab4b610f3621f0a92d85))
* **updater:** page through releases, word dialogs in the game language ([#170](https://github.com/themaiby/enshwick/issues/170)) ([c6bd27a](https://github.com/themaiby/enshwick/commit/c6bd27a947073a2e4a40dc822f2e3f64606c20a4))
* **updater:** read url(...) sources from a documented JSON format ([#77](https://github.com/themaiby/enshwick/issues/77)) ([a05aa2e](https://github.com/themaiby/enshwick/commit/a05aa2ed0c2838a5b3ff6a7adc2d06329d99eed1))


### Bug Fixes

* **mods:** audit fixes across every mod ([#94](https://github.com/themaiby/enshwick/issues/94)) ([e83b2d6](https://github.com/themaiby/enshwick/commit/e83b2d6d9f788f3cf9cc60d32fdcdb95196b76d4))
* **updater:** delete a renamed mod's old DLL after a fresh install ([#206](https://github.com/themaiby/enshwick/issues/206)) ([58d2134](https://github.com/themaiby/enshwick/commit/58d2134d700786f92cae1fd917998eac9376b1df))
* **updater:** never stage over an unlisted DLL or through path aliases ([#221](https://github.com/themaiby/enshwick/issues/221)) ([1b0ab4a](https://github.com/themaiby/enshwick/commit/1b0ab4af83230dde1a17c9e592b4f5e9c3687c9e))


### Other

* **api:** one naming scheme, run-time errors and named abi values ([#110](https://github.com/themaiby/enshwick/issues/110)) ([cbe72c5](https://github.com/themaiby/enshwick/commit/cbe72c5e92104d374272a5b192272e6e276dd37b))
* consolidate into Enshwick Loader, one API crate and one tool ([#85](https://github.com/themaiby/enshwick/issues/85)) ([5ddfde4](https://github.com/themaiby/enshwick/commit/5ddfde457d24f8e000d2dee1c74508f89582d526))

## [0.5.1](https://github.com/themaiby/enshwick/compare/enshwick-updater-v0.5.0...enshwick-updater-v0.5.1) (2026-10-21)


### Bug Fixes

* **updater:** delete a renamed mod's old DLL after a fresh install ([#206](https://github.com/themaiby/enshwick/issues/206)) ([58d2134](https://github.com/themaiby/enshwick/commit/58d2134d700786f92cae1fd917998eac9376b1df))

## [0.5.0](https://github.com/themaiby/enshwick/compare/enshwick-updater-v0.4.5...enshwick-updater-v0.5.0) (2026-10-21)


### ⚠ BREAKING CHANGES

* **updater:** accept common GitHub release forms and unsigned updates ([#198](https://github.com/themaiby/enshwick/issues/198))
* **loader:** show only updates of installed mods on the Mods screen ([#196](https://github.com/themaiby/enshwick/issues/196))

### Features

* **loader:** show only updates of installed mods on the Mods screen ([#196](https://github.com/themaiby/enshwick/issues/196)) ([aee82ac](https://github.com/themaiby/enshwick/commit/aee82acb3a6a0f9d8aa2ebbe414a7a31961c1cd7))
* **updater:** accept common GitHub release forms and unsigned updates ([#198](https://github.com/themaiby/enshwick/issues/198)) ([51d3e47](https://github.com/themaiby/enshwick/commit/51d3e4754a34496c5c90ab4b610f3621f0a92d85))

## [0.4.5](https://github.com/themaiby/enshwick/compare/enshwick-updater-v0.4.4...enshwick-updater-v0.4.5) (2026-10-21)

## [0.4.4](https://github.com/themaiby/enshwick/compare/enshwick-updater-v0.4.3...enshwick-updater-v0.4.4) (2026-10-21)


### Features

* **loader:** show mod catalogs with updates and mods on the Mods screen ([#180](https://github.com/themaiby/enshwick/issues/180)) ([2e93fb7](https://github.com/themaiby/enshwick/commit/2e93fb7ec219b7c4b4fcab0ff45dcd4c409340d6))
* **updater:** page through releases, word dialogs in the game language ([#170](https://github.com/themaiby/enshwick/issues/170)) ([c6bd27a](https://github.com/themaiby/enshwick/commit/c6bd27a947073a2e4a40dc822f2e3f64606c20a4))

## [0.4.3](https://github.com/themaiby/enshwick/compare/enshwick-updater-v0.4.2...enshwick-updater-v0.4.3) (2026-10-21)


### Features

* **loader:** add content packs of TOML files, signed and reloadable ([#158](https://github.com/themaiby/enshwick/issues/158)) ([7598b97](https://github.com/themaiby/enshwick/commit/7598b977434dfb207cd5c0d30c6cdd936c0cb0cc))
* **loader:** never start a mod that a renamed mod replaces ([#166](https://github.com/themaiby/enshwick/issues/166)) ([650993c](https://github.com/themaiby/enshwick/commit/650993c7b392be07fc16c4ca47925952c64f1487))

## [0.4.2](https://github.com/themaiby/enshwick/compare/enshwick-updater-v0.4.1...enshwick-updater-v0.4.2) (2026-10-21)

## [0.4.1](https://github.com/themaiby/enshwick/compare/enshwick-updater-v0.4.0...enshwick-updater-v0.4.1) (2026-10-21)


### Features

* **api:** draw with the game's own widgets and show its dialogs ([#129](https://github.com/themaiby/enshwick/issues/129)) ([8601c70](https://github.com/themaiby/enshwick/commit/8601c70f8a0678a50f41301467d713bf41fab855))
* **loader:** ship build tables as signed data with a pattern fallback ([#155](https://github.com/themaiby/enshwick/issues/155)) ([1df2e3f](https://github.com/themaiby/enshwick/commit/1df2e3fdcaeaeac3ca082a18976e6a3f1d3a46c9))

## [0.4.0](https://github.com/themaiby/enshwick/compare/enshwick-updater-v0.3.0...enshwick-updater-v0.4.0) (2026-10-20)


### ⚠ BREAKING CHANGES

* **api:** name, describe and credit mods, with recommends and breaks ([#126](https://github.com/themaiby/enshwick/issues/126))
* **api:** config stages, ranges, choices, schema and /config ([#130](https://github.com/themaiby/enshwick/issues/130))

### Features

* **api:** config stages, ranges, choices, schema and /config ([#130](https://github.com/themaiby/enshwick/issues/130)) ([8a19e89](https://github.com/themaiby/enshwick/commit/8a19e89afe6ac74d09d9a32c66b1fc133e81bc68))
* **api:** name, describe and credit mods, with recommends and breaks ([#126](https://github.com/themaiby/enshwick/issues/126)) ([edfb07c](https://github.com/themaiby/enshwick/commit/edfb07c59676c441f1e8b06ddc2791a80594ae6c))

## [0.3.0](https://github.com/themaiby/enshwick/compare/enshwick-updater-v0.2.1...enshwick-updater-v0.3.0) (2026-10-20)


### ⚠ BREAKING CHANGES

* **api:** one naming scheme, run-time errors and named abi values ([#110](https://github.com/themaiby/enshwick/issues/110))

### Bug Fixes

* **mods:** audit fixes across every mod ([#94](https://github.com/themaiby/enshwick/issues/94)) ([e83b2d6](https://github.com/themaiby/enshwick/commit/e83b2d6d9f788f3cf9cc60d32fdcdb95196b76d4))


### Other

* **api:** one naming scheme, run-time errors and named abi values ([#110](https://github.com/themaiby/enshwick/issues/110)) ([cbe72c5](https://github.com/themaiby/enshwick/commit/cbe72c5e92104d374272a5b192272e6e276dd37b))

## [0.2.1](https://github.com/themaiby/enshwick/compare/enshwick-updater-v0.2.0...enshwick-updater-v0.2.1) (2026-10-20)

## [0.2.0](https://github.com/themaiby/enshwick/compare/enshwick-updater-v0.1.2...enshwick-updater-v0.2.0) (2026-10-20)


### ⚠ BREAKING CHANGES

* consolidate into Enshwick Loader, one API crate and one tool ([#85](https://github.com/themaiby/enshwick/issues/85))

### Other

* consolidate into Enshwick Loader, one API crate and one tool ([#85](https://github.com/themaiby/enshwick/issues/85)) ([5ddfde4](https://github.com/themaiby/enshwick/commit/5ddfde457d24f8e000d2dee1c74508f89582d526))

## [0.1.2](https://github.com/themaiby/enshwick/compare/enshwick-updater-v0.1.1...enshwick-updater-v0.1.2) (2026-10-20)


### Features

* author keys: updates must be signed by the installed version's key ([#81](https://github.com/themaiby/enshwick/issues/81)) ([d9d46f3](https://github.com/themaiby/enshwick/commit/d9d46f38231234e958a8762e6c0fff5b6ddcb5d8))
* **updater:** read url(...) sources from a documented JSON format ([#77](https://github.com/themaiby/enshwick/issues/77)) ([a05aa2e](https://github.com/themaiby/enshwick/commit/a05aa2ed0c2838a5b3ff6a7adc2d06329d99eed1))

## [0.1.1](https://github.com/themaiby/enshwick/compare/enshwick-updater-v0.1.0...enshwick-updater-v0.1.1) (2026-10-20)


### Features

* add the updater mod and report a server's missing mods ([#65](https://github.com/themaiby/enshwick/issues/65)) ([13dfe21](https://github.com/themaiby/enshwick/commit/13dfe21934d5e903aa4aeea56c7450e62e2963c4))
