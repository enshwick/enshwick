# Changelog

## [0.2.0](https://github.com/themaiby/enshwick/compare/enshwick-loader-v0.1.6...enshwick-loader-v0.2.0) (2026-10-20)


### Features

* author keys: updates must be signed by the installed version's key ([#81](https://github.com/themaiby/enshwick/issues/81)) ([d9d46f3](https://github.com/themaiby/enshwick/commit/d9d46f38231234e958a8762e6c0fff5b6ddcb5d8))


### Bug Fixes

* load manifests and structs of an older or newer minor ([#76](https://github.com/themaiby/enshwick/issues/76)) ([4bd2e9f](https://github.com/themaiby/enshwick/commit/4bd2e9f04dc529c7036f02fec682c12310f0a31d))

## [0.1.6](https://github.com/themaiby/enshwick/compare/enshwick-loader-v0.1.5...enshwick-loader-v0.1.6) (2026-10-20)


### Features

* add the updater mod and report a server's missing mods ([#65](https://github.com/themaiby/enshwick/issues/65)) ([13dfe21](https://github.com/themaiby/enshwick/commit/13dfe21934d5e903aa4aeea56c7450e62e2963c4))
* add workshops and a Bulk Workbench for bulk recipes ([#67](https://github.com/themaiby/enshwick/issues/67)) ([71a28aa](https://github.com/themaiby/enshwick/commit/71a28aa90793f2285017608b282e134363bad849))
* **api:** add mod items to random loot with an exact chance ([#73](https://github.com/themaiby/enshwick/issues/73)) ([3133b67](https://github.com/themaiby/enshwick/commit/3133b67fc28254802d9ebb952db56176bdd15600))
* build-camera mod opens the building fly mode to every player ([#66](https://github.com/themaiby/enshwick/issues/66)) ([29b1ef9](https://github.com/themaiby/enshwick/commit/29b1ef9275480af172f683f0b241ddf2874b28c5))
* chat commands and entity labels on the server ([#62](https://github.com/themaiby/enshwick/issues/62)) ([1d9ea08](https://github.com/themaiby/enshwick/commit/1d9ea08b934a8da2eb2c6a64ac087e63f45b1d84))
* code patches and a lifted workshop recipe limit ([#60](https://github.com/themaiby/enshwick/issues/60)) ([f1181c4](https://github.com/themaiby/enshwick/commit/f1181c46f3234f034148538394ad3d2de15b4ab7))
* **flame-altar-count:** 32 altars in at most 16 bases ([#72](https://github.com/themaiby/enshwick/issues/72)) ([ecb0804](https://github.com/themaiby/enshwick/commit/ecb0804fe1d2ac636a56b739f6e9c971fca75b7d))
* hook diagnostics, a status summary and the dev-tools mod ([#71](https://github.com/themaiby/enshwick/issues/71)) ([0cfbf89](https://github.com/themaiby/enshwick/commit/0cfbf893782bba0b37f9743714075e224ec7eb89))
* join turns off extra mods and stays out when mods are missing ([#55](https://github.com/themaiby/enshwick/issues/55)) ([1778197](https://github.com/themaiby/enshwick/commit/17781970dbbefec098a45b6332f9ed647ae93510))
* join values: #[config(sync)] settings follow the server ([#59](https://github.com/themaiby/enshwick/issues/59)) ([7ff7bfe](https://github.com/themaiby/enshwick/commit/7ff7bfee8ab685d76ead9d211e5c57048d44e160))
* mid-function hooks and an inventory API ([#69](https://github.com/themaiby/enshwick/issues/69)) ([1c46b2e](https://github.com/themaiby/enshwick/commit/1c46b2e32447af4d92a532c3779f9ec02307e96f))
* run server mods in games the client hosts ([#61](https://github.com/themaiby/enshwick/issues/61)) ([4c6fe26](https://github.com/themaiby/enshwick/commit/4c6fe269e6137bba6e7190907929fa1b11033744))
* update sources in manifests and staged updates in the stub ([#57](https://github.com/themaiby/enshwick/issues/57)) ([20e72b1](https://github.com/themaiby/enshwick/commit/20e72b1025765d83f3da861ec46ccbb4a163443c))

## [0.1.5](https://github.com/themaiby/enshwick/compare/enshwick-loader-v0.1.4...enshwick-loader-v0.1.5) (2026-10-20)


### Features

* content and i18n in the API, and the safes mod ([#52](https://github.com/themaiby/enshwick/issues/52)) ([00ae927](https://github.com/themaiby/enshwick/commit/00ae9278664af9209bf1b0fba9137c25527bb397))
* world reads, players and building events on the server ([#50](https://github.com/themaiby/enshwick/issues/50)) ([837192e](https://github.com/themaiby/enshwick/commit/837192e7fa014188adb724555f0db804018e7821))

## [0.1.4](https://github.com/themaiby/enshwick/compare/enshwick-loader-v0.1.3...enshwick-loader-v0.1.4) (2026-10-20)


### Features

* events, with settings applied while the game runs ([#41](https://github.com/themaiby/enshwick/issues/41)) ([5eb9204](https://github.com/themaiby/enshwick/commit/5eb9204263da3410dcaeacb295dff33f14b87e80))
* game events: tick, world loaded and saved, players, shutdown ([#43](https://github.com/themaiby/enshwick/issues/43)) ([f23019b](https://github.com/themaiby/enshwick/commit/f23019b92710e1e223826dac29d4fff5f96a9f9d))
* name mod versions everywhere, checked at release ([#42](https://github.com/themaiby/enshwick/issues/42)) ([5dc88e3](https://github.com/themaiby/enshwick/commit/5dc88e3a296565b1691d3d54be726540a2255fc8))
* name the mod behind a crash in crash.log ([#45](https://github.com/themaiby/enshwick/issues/45)) ([c755862](https://github.com/themaiby/enshwick/commit/c755862b638604120ad4cbb1222c76e86af247d4))
* the loader's own hook engine, on iced-x86, instead of minhook ([#46](https://github.com/themaiby/enshwick/issues/46)) ([e9de94c](https://github.com/themaiby/enshwick/commit/e9de94cf7b7a4ebf1b49d92d9a4fd5542091aaaa))

## [0.1.3](https://github.com/themaiby/enshwick/compare/enshwick-loader-v0.1.2...enshwick-loader-v0.1.3) (2026-10-20)


### Features

* log levels per mod, debug, once and rate-limited lines ([#36](https://github.com/themaiby/enshwick/issues/36)) ([477bbbe](https://github.com/themaiby/enshwick/commit/477bbbe1e50d4809723f331a8c8a07f70cf391dc))

## [0.1.2](https://github.com/themaiby/enshwick/compare/enshwick-loader-v0.1.1...enshwick-loader-v0.1.2) (2026-10-20)


### Features

* **enshwick-furniture-comfort:** more comfort from furniture ([#22](https://github.com/themaiby/enshwick/issues/22)) ([7a17c37](https://github.com/themaiby/enshwick/commit/7a17c37bccba78ecdc6cd0394cae8e3ef7be16cd))

## [0.1.1](https://github.com/themaiby/enshwick/compare/enshwick-loader-v0.1.0...enshwick-loader-v0.1.1) (2026-10-19)


### Features

* **enshwick-api:** caller frame registers and mod build rows ([#11](https://github.com/themaiby/enshwick/issues/11)) ([c411f95](https://github.com/themaiby/enshwick/commit/c411f95cabb865ca7edaafdf46b1528febf4391b))
* **enshwick-api:** engine constants and ecs access ([#9](https://github.com/themaiby/enshwick/issues/9)) ([fbf5aeb](https://github.com/themaiby/enshwick/commit/fbf5aeb0dd3184c2adf8ddd898dabb36ffe8f979))
* **enshwick-flame-altar-zones:** clusters, growth and seams ([#10](https://github.com/themaiby/enshwick/issues/10)) ([3731821](https://github.com/themaiby/enshwick/commit/373182116f3fd213d491da7318d837071c2c1616))
* **enshwick-loader:** advertise the mod set and check it on join ([#14](https://github.com/themaiby/enshwick/issues/14)) ([81911a1](https://github.com/themaiby/enshwick/commit/81911a11509a8a48c14f665b21b32424167eb693))
* **enshwick-loader:** load mods and link their hook chains ([#5](https://github.com/themaiby/enshwick/issues/5)) ([3b8dc75](https://github.com/themaiby/enshwick/commit/3b8dc75a7f84a113e8d625b92ab5cec3242619bc))
