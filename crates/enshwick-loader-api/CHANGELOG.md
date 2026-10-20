# Changelog

## [0.3.0](https://github.com/themaiby/enshwick/compare/enshwick-loader-api-v0.2.0...enshwick-loader-api-v0.3.0) (2026-10-20)


### ⚠ BREAKING CHANGES

* consolidate into Enshwick Loader, one API crate and one tool ([#85](https://github.com/themaiby/enshwick/issues/85))

### Bug Fixes

* build the docs without rustdoc link errors ([#87](https://github.com/themaiby/enshwick/issues/87)) ([57b02cf](https://github.com/themaiby/enshwick/commit/57b02cf899cf0a0bfc97633ea47339e630f0c0e5))


### Other

* consolidate into Enshwick Loader, one API crate and one tool ([#85](https://github.com/themaiby/enshwick/issues/85)) ([5ddfde4](https://github.com/themaiby/enshwick/commit/5ddfde457d24f8e000d2dee1c74508f89582d526))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * enshwick-loader-macros bumped from 0.2.0 to 0.3.0

## [0.2.0](https://github.com/themaiby/enshwick/compare/enshwick-api-v0.1.5...enshwick-api-v0.2.0) (2026-10-20)


### ⚠ BREAKING CHANGES

* building materials x2 doubles vanilla building recipe outputs ([#82](https://github.com/themaiby/enshwick/issues/82))

### Features

* author keys: updates must be signed by the installed version's key ([#81](https://github.com/themaiby/enshwick/issues/81)) ([d9d46f3](https://github.com/themaiby/enshwick/commit/d9d46f38231234e958a8762e6c0fff5b6ddcb5d8))
* building materials x2 doubles vanilla building recipe outputs ([#82](https://github.com/themaiby/enshwick/issues/82)) ([5f6ee1f](https://github.com/themaiby/enshwick/commit/5f6ee1ff583a7bbddaf580e6f0c764e47558ba85))
* **updater:** read url(...) sources from a documented JSON format ([#77](https://github.com/themaiby/enshwick/issues/77)) ([a05aa2e](https://github.com/themaiby/enshwick/commit/a05aa2ed0c2838a5b3ff6a7adc2d06329d99eed1))


### Bug Fixes

* load manifests and structs of an older or newer minor ([#76](https://github.com/themaiby/enshwick/issues/76)) ([4bd2e9f](https://github.com/themaiby/enshwick/commit/4bd2e9f04dc529c7036f02fec682c12310f0a31d))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * enshwick-macros bumped from 0.1.2 to 0.2.0

## [0.1.5](https://github.com/themaiby/enshwick/compare/enshwick-api-v0.1.4...enshwick-api-v0.1.5) (2026-10-20)


### Features

* add enshwick-test fake loader and the #[hook] attribute ([#58](https://github.com/themaiby/enshwick/issues/58)) ([90cb962](https://github.com/themaiby/enshwick/commit/90cb962e95f455d89f964eb6030661e46af30b7e))
* add the updater mod and report a server's missing mods ([#65](https://github.com/themaiby/enshwick/issues/65)) ([13dfe21](https://github.com/themaiby/enshwick/commit/13dfe21934d5e903aa4aeea56c7450e62e2963c4))
* add workshops and a Bulk Workbench for bulk recipes ([#67](https://github.com/themaiby/enshwick/issues/67)) ([71a28aa](https://github.com/themaiby/enshwick/commit/71a28aa90793f2285017608b282e134363bad849))
* **api:** add mod items to random loot with an exact chance ([#73](https://github.com/themaiby/enshwick/issues/73)) ([3133b67](https://github.com/themaiby/enshwick/commit/3133b67fc28254802d9ebb952db56176bdd15600))
* chat commands and entity labels on the server ([#62](https://github.com/themaiby/enshwick/issues/62)) ([1d9ea08](https://github.com/themaiby/enshwick/commit/1d9ea08b934a8da2eb2c6a64ac087e63f45b1d84))
* code patches and a lifted workshop recipe limit ([#60](https://github.com/themaiby/enshwick/issues/60)) ([f1181c4](https://github.com/themaiby/enshwick/commit/f1181c46f3234f034148538394ad3d2de15b4ab7))
* **flame-altar-count:** 32 altars in at most 16 bases ([#72](https://github.com/themaiby/enshwick/issues/72)) ([ecb0804](https://github.com/themaiby/enshwick/commit/ecb0804fe1d2ac636a56b739f6e9c971fca75b7d))
* hook diagnostics, a status summary and the dev-tools mod ([#71](https://github.com/themaiby/enshwick/issues/71)) ([0cfbf89](https://github.com/themaiby/enshwick/commit/0cfbf893782bba0b37f9743714075e224ec7eb89))
* join values: #[config(sync)] settings follow the server ([#59](https://github.com/themaiby/enshwick/issues/59)) ([7ff7bfe](https://github.com/themaiby/enshwick/commit/7ff7bfee8ab685d76ead9d211e5c57048d44e160))
* mid-function hooks and an inventory API ([#69](https://github.com/themaiby/enshwick/issues/69)) ([1c46b2e](https://github.com/themaiby/enshwick/commit/1c46b2e32447af4d92a532c3779f9ec02307e96f))
* run server mods in games the client hosts ([#61](https://github.com/themaiby/enshwick/issues/61)) ([4c6fe26](https://github.com/themaiby/enshwick/commit/4c6fe269e6137bba6e7190907929fa1b11033744))
* shared experience mod ([#68](https://github.com/themaiby/enshwick/issues/68)) ([43f9b60](https://github.com/themaiby/enshwick/commit/43f9b60c700f72feebfe59e3eca8ac705988ac11))
* update sources in manifests and staged updates in the stub ([#57](https://github.com/themaiby/enshwick/issues/57)) ([20e72b1](https://github.com/themaiby/enshwick/commit/20e72b1025765d83f3da861ec46ccbb4a163443c))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * enshwick-macros bumped from 0.1.1 to 0.1.2

## [0.1.4](https://github.com/themaiby/enshwick/compare/enshwick-api-v0.1.3...enshwick-api-v0.1.4) (2026-10-20)


### Features

* content and i18n in the API, and the safes mod ([#52](https://github.com/themaiby/enshwick/issues/52)) ([00ae927](https://github.com/themaiby/enshwick/commit/00ae9278664af9209bf1b0fba9137c25527bb397))
* derived recipes in the API, and the building-yield mod ([#54](https://github.com/themaiby/enshwick/issues/54)) ([150c65c](https://github.com/themaiby/enshwick/commit/150c65c254d11219410b76777b62c776bcc44097))
* world reads, players and building events on the server ([#50](https://github.com/themaiby/enshwick/issues/50)) ([837192e](https://github.com/themaiby/enshwick/commit/837192e7fa014188adb724555f0db804018e7821))

## [0.1.3](https://github.com/themaiby/enshwick/compare/enshwick-api-v0.1.2...enshwick-api-v0.1.3) (2026-10-20)


### Features

* a scheduler for work on the server's game thread ([#44](https://github.com/themaiby/enshwick/issues/44)) ([3f3889a](https://github.com/themaiby/enshwick/commit/3f3889ac6300d77a4ab482603c571fe767f63ee5))
* cargo enshwick, to start a mod and install it into the game ([#38](https://github.com/themaiby/enshwick/issues/38)) ([58ea01c](https://github.com/themaiby/enshwick/commit/58ea01cca185612ab188783dc7035a580e41f031))
* events, with settings applied while the game runs ([#41](https://github.com/themaiby/enshwick/issues/41)) ([5eb9204](https://github.com/themaiby/enshwick/commit/5eb9204263da3410dcaeacb295dff33f14b87e80))
* game events: tick, world loaded and saved, players, shutdown ([#43](https://github.com/themaiby/enshwick/issues/43)) ([f23019b](https://github.com/themaiby/enshwick/commit/f23019b92710e1e223826dac29d4fff5f96a9f9d))
* name mod versions everywhere, checked at release ([#42](https://github.com/themaiby/enshwick/issues/42)) ([5dc88e3](https://github.com/themaiby/enshwick/commit/5dc88e3a296565b1691d3d54be726540a2255fc8))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * enshwick-macros bumped from 0.1.0 to 0.1.1

## [0.1.2](https://github.com/themaiby/enshwick/compare/enshwick-api-v0.1.1...enshwick-api-v0.1.2) (2026-10-20)


### Features

* log levels per mod, debug, once and rate-limited lines ([#36](https://github.com/themaiby/enshwick/issues/36)) ([477bbbe](https://github.com/themaiby/enshwick/commit/477bbbe1e50d4809723f331a8c8a07f70cf391dc))

## [0.1.1](https://github.com/themaiby/enshwick/compare/enshwick-api-v0.1.0...enshwick-api-v0.1.1) (2026-10-19)


### Features

* **enshwick-api:** caller frame registers and mod build rows ([#11](https://github.com/themaiby/enshwick/issues/11)) ([c411f95](https://github.com/themaiby/enshwick/commit/c411f95cabb865ca7edaafdf46b1528febf4391b))
* **enshwick-api:** engine constants and ecs access ([#9](https://github.com/themaiby/enshwick/issues/9)) ([fbf5aeb](https://github.com/themaiby/enshwick/commit/fbf5aeb0dd3184c2adf8ddd898dabb36ffe8f979))
