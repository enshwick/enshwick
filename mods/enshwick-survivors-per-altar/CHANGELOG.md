# Changelog

## [0.5.0](https://github.com/themaiby/enshwick/compare/enshwick-survivors-per-altar-v0.4.0...enshwick-survivors-per-altar-v0.5.0) (2026-10-21)


### ⚠ BREAKING CHANGES

* **api:** name mod content <mod>:<name> and derive its GUIDs ([#154](https://github.com/themaiby/enshwick/issues/154))

### Features

* **api:** give items a meter, tooltip lines and fill on give ([#128](https://github.com/themaiby/enshwick/issues/128)) ([11beb9b](https://github.com/themaiby/enshwick/commit/11beb9b762be4d3d6b80cc81b2c3baffb2ab3131))
* **api:** name mod content &lt;mod&gt;:&lt;name&gt; and derive its GUIDs ([#154](https://github.com/themaiby/enshwick/issues/154)) ([38f46ce](https://github.com/themaiby/enshwick/commit/38f46ce596e9fa6c25c0d673558ee56887160bfb))
* **survivors-per-altar:** copies come only from copy staffs ([#140](https://github.com/themaiby/enshwick/issues/140)) ([ab540fb](https://github.com/themaiby/enshwick/commit/ab540fb687900e7350f452f3403d4bebabd9f919))
* **survivors-per-altar:** give copy staffs a violet icon ([#144](https://github.com/themaiby/enshwick/issues/144)) ([5843f56](https://github.com/themaiby/enshwick/commit/5843f5699cf83547e6f57586ee41079e86fc6723))
* **survivors-per-altar:** give copy staffs charges ([#131](https://github.com/themaiby/enshwick/issues/131)) ([2ff5952](https://github.com/themaiby/enshwick/commit/2ff59529267880d296fb50f43efa2a7e24b3d9d0))

## [0.4.0](https://github.com/themaiby/enshwick/compare/enshwick-survivors-per-altar-v0.3.0...enshwick-survivors-per-altar-v0.4.0) (2026-10-20)


### ⚠ BREAKING CHANGES

* **api:** name, describe and credit mods, with recommends and breaks ([#126](https://github.com/themaiby/enshwick/issues/126))
* **api:** config stages, ranges, choices, schema and /config ([#130](https://github.com/themaiby/enshwick/issues/130))

### Features

* **api:** config stages, ranges, choices, schema and /config ([#130](https://github.com/themaiby/enshwick/issues/130)) ([8a19e89](https://github.com/themaiby/enshwick/commit/8a19e89afe6ac74d09d9a32c66b1fc133e81bc68))
* **api:** name, describe and credit mods, with recommends and breaks ([#126](https://github.com/themaiby/enshwick/issues/126)) ([edfb07c](https://github.com/themaiby/enshwick/commit/edfb07c59676c441f1e8b06ddc2791a80594ae6c))

## [0.3.0](https://github.com/themaiby/enshwick/compare/enshwick-survivors-per-altar-v0.2.1...enshwick-survivors-per-altar-v0.3.0) (2026-10-20)


### ⚠ BREAKING CHANGES

* **api:** one naming scheme, run-time errors and named abi values ([#110](https://github.com/themaiby/enshwick/issues/110))

### Features

* **api:** altar zones and bases in the platform ([#97](https://github.com/themaiby/enshwick/issues/97)) ([e45cc7b](https://github.com/themaiby/enshwick/commit/e45cc7b58c6d82db2affa56b4fbfa89bd4fc3824))
* **api:** survivor lookups, beds, staffs and menu in the platform ([#103](https://github.com/themaiby/enshwick/issues/103)) ([166e1ba](https://github.com/themaiby/enshwick/commit/166e1ba837d789bbd9a36f9bab41e8530666d4ef))


### Bug Fixes

* **api:** audit the modder API for soundness, ABI and reuse ([#95](https://github.com/themaiby/enshwick/issues/95)) ([d800142](https://github.com/themaiby/enshwick/commit/d800142991a2d93aaf943af9f6e5cc629fa54d68))
* **loader:** safety and robustness pass from the conventions review ([#111](https://github.com/themaiby/enshwick/issues/111)) ([52ed976](https://github.com/themaiby/enshwick/commit/52ed976a30d65d7007a56202ed855fbb55e69740))
* **mods:** audit fixes across every mod ([#94](https://github.com/themaiby/enshwick/issues/94)) ([e83b2d6](https://github.com/themaiby/enshwick/commit/e83b2d6d9f788f3cf9cc60d32fdcdb95196b76d4))


### Other

* **api:** one naming scheme, run-time errors and named abi values ([#110](https://github.com/themaiby/enshwick/issues/110)) ([cbe72c5](https://github.com/themaiby/enshwick/commit/cbe72c5e92104d374272a5b192272e6e276dd37b))

## [0.2.1](https://github.com/themaiby/enshwick/compare/enshwick-survivors-per-altar-v0.2.0...enshwick-survivors-per-altar-v0.2.1) (2026-10-20)


### Bug Fixes

* **survivors-per-altar:** require flame-altar-zones across 0.x ([#93](https://github.com/themaiby/enshwick/issues/93)) ([4a00ead](https://github.com/themaiby/enshwick/commit/4a00eada80d4dc70065fca8dcba6f43f4bb3bb02))

## [0.2.0](https://github.com/themaiby/enshwick/compare/enshwick-survivors-per-altar-v0.1.6...enshwick-survivors-per-altar-v0.2.0) (2026-10-20)


### ⚠ BREAKING CHANGES

* consolidate into Enshwick Loader, one API crate and one tool ([#85](https://github.com/themaiby/enshwick/issues/85))

### Other

* consolidate into Enshwick Loader, one API crate and one tool ([#85](https://github.com/themaiby/enshwick/issues/85)) ([5ddfde4](https://github.com/themaiby/enshwick/commit/5ddfde457d24f8e000d2dee1c74508f89582d526))

## [0.1.6](https://github.com/themaiby/enshwick/compare/enshwick-survivors-per-altar-v0.1.5...enshwick-survivors-per-altar-v0.1.6) (2026-10-20)


### Features

* author keys: updates must be signed by the installed version's key ([#81](https://github.com/themaiby/enshwick/issues/81)) ([d9d46f3](https://github.com/themaiby/enshwick/commit/d9d46f38231234e958a8762e6c0fff5b6ddcb5d8))

## [0.1.5](https://github.com/themaiby/enshwick/compare/enshwick-survivors-per-altar-v0.1.4...enshwick-survivors-per-altar-v0.1.5) (2026-10-20)


### Features

* run server mods in games the client hosts ([#61](https://github.com/themaiby/enshwick/issues/61)) ([4c6fe26](https://github.com/themaiby/enshwick/commit/4c6fe269e6137bba6e7190907929fa1b11033744))
* **survivors-per-altar:** add single-use copy staffs per survivor ([#74](https://github.com/themaiby/enshwick/issues/74)) ([e0f3e6c](https://github.com/themaiby/enshwick/commit/e0f3e6cae7ff80d212fcf7e099cef09654c63075))
* update sources in manifests and staged updates in the stub ([#57](https://github.com/themaiby/enshwick/issues/57)) ([20e72b1](https://github.com/themaiby/enshwick/commit/20e72b1025765d83f3da861ec46ccbb4a163443c))

## [0.1.4](https://github.com/themaiby/enshwick/compare/enshwick-survivors-per-altar-v0.1.3...enshwick-survivors-per-altar-v0.1.4) (2026-10-20)

## [0.1.3](https://github.com/themaiby/enshwick/compare/enshwick-survivors-per-altar-v0.1.2...enshwick-survivors-per-altar-v0.1.3) (2026-10-20)

## [0.1.2](https://github.com/themaiby/enshwick/compare/enshwick-survivors-per-altar-v0.1.1...enshwick-survivors-per-altar-v0.1.2) (2026-10-20)

## [0.1.1](https://github.com/themaiby/enshwick/compare/enshwick-survivors-per-altar-v0.1.0...enshwick-survivors-per-altar-v0.1.1) (2026-10-19)


### Features

* **enshwick-survivors-per-altar:** a copy of each survivor per altar ([#13](https://github.com/themaiby/enshwick/issues/13)) ([f807ce7](https://github.com/themaiby/enshwick/commit/f807ce7d996ae1a8910685546428e87d6b9bdd74))
