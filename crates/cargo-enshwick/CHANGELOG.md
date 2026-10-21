# Changelog

## [0.8.2](https://github.com/themaiby/enshwick/compare/cargo-enshwick-v0.8.1...cargo-enshwick-v0.8.2) (2026-10-21)


### Features

* **api:** name vanilla content in packs by debug name ([#193](https://github.com/themaiby/enshwick/issues/193)) ([033fda9](https://github.com/themaiby/enshwick/commit/033fda9c6fa7f616c780d690e1b5758550a086de))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * enshwick-loader bumped from 0.8.1 to 0.8.2

## [0.8.1](https://github.com/themaiby/enshwick/compare/cargo-enshwick-v0.8.0...cargo-enshwick-v0.8.1) (2026-10-21)


### Other

* **cargo-enshwick:** Synchronize enshwick versions


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * enshwick-loader bumped from 0.8.0 to 0.8.1

## [0.8.0](https://github.com/themaiby/enshwick/compare/cargo-enshwick-v0.7.0...cargo-enshwick-v0.8.0) (2026-10-21)


### Features

* **cargo-enshwick:** report what works on a game build with compat ([#162](https://github.com/themaiby/enshwick/issues/162)) ([4bc3978](https://github.com/themaiby/enshwick/commit/4bc3978461c7f72bb07421bd2c96ad9fe6213e28))
* **loader:** add content packs of TOML files, signed and reloadable ([#158](https://github.com/themaiby/enshwick/issues/158)) ([7598b97](https://github.com/themaiby/enshwick/commit/7598b977434dfb207cd5c0d30c6cdd936c0cb0cc))
* **loader:** never start a mod that a renamed mod replaces ([#166](https://github.com/themaiby/enshwick/issues/166)) ([650993c](https://github.com/themaiby/enshwick/commit/650993c7b392be07fc16c4ca47925952c64f1487))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * enshwick-loader bumped from 0.7.0 to 0.8.0

## [0.7.0](https://github.com/themaiby/enshwick/compare/cargo-enshwick-v0.6.0...cargo-enshwick-v0.7.0) (2026-10-21)


### Other

* **cargo-enshwick:** Synchronize enshwick versions

## [0.6.0](https://github.com/themaiby/enshwick/compare/cargo-enshwick-v0.5.0...cargo-enshwick-v0.6.0) (2026-10-21)


### ⚠ BREAKING CHANGES

* **api:** name mod content <mod>:<name> and derive its GUIDs ([#154](https://github.com/themaiby/enshwick/issues/154))

### Features

* **api:** name mod content &lt;mod&gt;:&lt;name&gt; and derive its GUIDs ([#154](https://github.com/themaiby/enshwick/issues/154)) ([38f46ce](https://github.com/themaiby/enshwick/commit/38f46ce596e9fa6c25c0d673558ee56887160bfb))
* **loader:** ship build tables as signed data with a pattern fallback ([#155](https://github.com/themaiby/enshwick/issues/155)) ([1df2e3f](https://github.com/themaiby/enshwick/commit/1df2e3fdcaeaeac3ca082a18976e6a3f1d3a46c9))

## [0.5.0](https://github.com/themaiby/enshwick/compare/cargo-enshwick-v0.4.0...cargo-enshwick-v0.5.0) (2026-10-20)


### ⚠ BREAKING CHANGES

* **api:** name, describe and credit mods, with recommends and breaks ([#126](https://github.com/themaiby/enshwick/issues/126))
* **api:** config stages, ranges, choices, schema and /config ([#130](https://github.com/themaiby/enshwick/issues/130))

### Features

* **api:** config stages, ranges, choices, schema and /config ([#130](https://github.com/themaiby/enshwick/issues/130)) ([8a19e89](https://github.com/themaiby/enshwick/commit/8a19e89afe6ac74d09d9a32c66b1fc133e81bc68))
* **api:** name, describe and credit mods, with recommends and breaks ([#126](https://github.com/themaiby/enshwick/issues/126)) ([edfb07c](https://github.com/themaiby/enshwick/commit/edfb07c59676c441f1e8b06ddc2791a80594ae6c))

## [0.4.0](https://github.com/themaiby/enshwick/compare/cargo-enshwick-v0.3.1...cargo-enshwick-v0.4.0) (2026-10-20)


### Features

* **api:** skill, comfort and experience hooks in the platform ([#100](https://github.com/themaiby/enshwick/issues/100)) ([c810a02](https://github.com/themaiby/enshwick/commit/c810a02a60d5d05b8872bf8628afc27123868aa1))


### Bug Fixes

* **api:** audit the modder API for soundness, ABI and reuse ([#95](https://github.com/themaiby/enshwick/issues/95)) ([d800142](https://github.com/themaiby/enshwick/commit/d800142991a2d93aaf943af9f6e5cc629fa54d68))

## [0.3.1](https://github.com/themaiby/enshwick/compare/cargo-enshwick-v0.3.0...cargo-enshwick-v0.3.1) (2026-10-20)


### Other

* **cargo-enshwick:** Synchronize enshwick versions

## [0.3.0](https://github.com/themaiby/enshwick/compare/cargo-enshwick-v0.2.0...cargo-enshwick-v0.3.0) (2026-10-20)


### ⚠ BREAKING CHANGES

* consolidate into Enshwick Loader, one API crate and one tool ([#85](https://github.com/themaiby/enshwick/issues/85))

### Other

* consolidate into Enshwick Loader, one API crate and one tool ([#85](https://github.com/themaiby/enshwick/issues/85)) ([5ddfde4](https://github.com/themaiby/enshwick/commit/5ddfde457d24f8e000d2dee1c74508f89582d526))

## [0.2.0](https://github.com/themaiby/enshwick/compare/cargo-enshwick-v0.1.3...cargo-enshwick-v0.2.0) (2026-10-20)


### Features

* author keys: updates must be signed by the installed version's key ([#81](https://github.com/themaiby/enshwick/issues/81)) ([d9d46f3](https://github.com/themaiby/enshwick/commit/d9d46f38231234e958a8762e6c0fff5b6ddcb5d8))

## [0.1.3](https://github.com/themaiby/enshwick/compare/cargo-enshwick-v0.1.2...cargo-enshwick-v0.1.3) (2026-10-20)

## [0.1.2](https://github.com/themaiby/enshwick/compare/cargo-enshwick-v0.1.1...cargo-enshwick-v0.1.2) (2026-10-20)

## [0.1.1](https://github.com/themaiby/enshwick/compare/cargo-enshwick-v0.1.0...cargo-enshwick-v0.1.1) (2026-10-20)


### Features

* cargo enshwick, to start a mod and install it into the game ([#38](https://github.com/themaiby/enshwick/issues/38)) ([58ea01c](https://github.com/themaiby/enshwick/commit/58ea01cca185612ab188783dc7035a580e41f031))
* name mod versions everywhere, checked at release ([#42](https://github.com/themaiby/enshwick/issues/42)) ([5dc88e3](https://github.com/themaiby/enshwick/commit/5dc88e3a296565b1691d3d54be726540a2255fc8))
