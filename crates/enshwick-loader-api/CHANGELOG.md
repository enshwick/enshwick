# Changelog

## [0.9.1](https://github.com/themaiby/enshwick/compare/enshwick-loader-api-v0.9.0...enshwick-loader-api-v0.9.1) (2026-10-21)


### Other

* **enshwick-loader-api:** Synchronize enshwick versions


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * enshwick-loader-macros bumped from 0.9.0 to 0.9.1

## [0.9.0](https://github.com/themaiby/enshwick/compare/enshwick-loader-api-v0.8.2...enshwick-loader-api-v0.9.0) (2026-10-21)


### ⚠ BREAKING CHANGES

* **updater:** accept common GitHub release forms and unsigned updates ([#198](https://github.com/themaiby/enshwick/issues/198))
* **loader:** show only updates of installed mods on the Mods screen ([#196](https://github.com/themaiby/enshwick/issues/196))

### Features

* **loader:** name the killer entity of a player killed by a hit ([#202](https://github.com/themaiby/enshwick/issues/202)) ([c81f863](https://github.com/themaiby/enshwick/commit/c81f8630e192ec6dceba4c395c4639aafea27e7b))
* **loader:** show only updates of installed mods on the Mods screen ([#196](https://github.com/themaiby/enshwick/issues/196)) ([aee82ac](https://github.com/themaiby/enshwick/commit/aee82acb3a6a0f9d8aa2ebbe414a7a31961c1cd7))
* **updater:** accept common GitHub release forms and unsigned updates ([#198](https://github.com/themaiby/enshwick/issues/198)) ([51d3e47](https://github.com/themaiby/enshwick/commit/51d3e4754a34496c5c90ab4b610f3621f0a92d85))


### Bug Fixes

* **loader:** refuse world changes inside a step ([#199](https://github.com/themaiby/enshwick/issues/199)) ([56f7b79](https://github.com/themaiby/enshwick/commit/56f7b799d8ba90cce10fcd5f1385f5410e995555))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * enshwick-loader-macros bumped from 0.8.2 to 0.9.0

## [0.8.2](https://github.com/themaiby/enshwick/compare/enshwick-loader-api-v0.8.1...enshwick-loader-api-v0.8.2) (2026-10-21)


### Features

* **api:** name vanilla content in packs by debug name ([#193](https://github.com/themaiby/enshwick/issues/193)) ([033fda9](https://github.com/themaiby/enshwick/commit/033fda9c6fa7f616c780d690e1b5758550a086de))
* **api:** read the game's own texts and resources from its archive ([#191](https://github.com/themaiby/enshwick/issues/191)) ([ebaf029](https://github.com/themaiby/enshwick/commit/ebaf029db0cf95f6cb63602d6b0f617e8dc451e0))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * enshwick-loader-macros bumped from 0.8.1 to 0.8.2

## [0.8.1](https://github.com/themaiby/enshwick/compare/enshwick-loader-api-v0.8.0...enshwick-loader-api-v0.8.1) (2026-10-21)


### Features

* **api:** read the skill nodes a character unlocked ([#186](https://github.com/themaiby/enshwick/issues/186)) ([1a56dde](https://github.com/themaiby/enshwick/commit/1a56dde397e0ef4fad18a0ab9a4250ed17712d85))
* **loader:** filter drowning, freezing and admin kills ([#181](https://github.com/themaiby/enshwick/issues/181)) ([215297f](https://github.com/themaiby/enshwick/commit/215297f9059d2a21c0d741f34b014988bc5e0993))
* **loader:** keep game settings with the world ([#179](https://github.com/themaiby/enshwick/issues/179)) ([3a1d819](https://github.com/themaiby/enshwick/commit/3a1d819f5f4e34570ce1547b6b8e55368d74f664))
* **loader:** refuse knowledge a client grants itself ([#173](https://github.com/themaiby/enshwick/issues/173)) ([a6c2ee7](https://github.com/themaiby/enshwick/commit/a6c2ee7376ec8166b0aa7de9b5cda90b7c28fafa))
* **loader:** report player deaths and read the shroud timer ([#185](https://github.com/themaiby/enshwick/issues/185)) ([9bcd23c](https://github.com/themaiby/enshwick/commit/9bcd23c64665aeb814bf0bc08109ce48ec62eeed))
* **loader:** show mod catalogs with updates and mods on the Mods screen ([#180](https://github.com/themaiby/enshwick/issues/180)) ([2e93fb7](https://github.com/themaiby/enshwick/commit/2e93fb7ec219b7c4b4fcab0ff45dcd4c409340d6))
* **loader:** take knowledge back and change it for absent players ([#183](https://github.com/themaiby/enshwick/issues/183)) ([13e4883](https://github.com/themaiby/enshwick/commit/13e4883e75df99ecd6101754608161c87c39633d))
* **loader:** watch knowledge requests by default before refusing them ([#190](https://github.com/themaiby/enshwick/issues/190)) ([ba2efcd](https://github.com/themaiby/enshwick/commit/ba2efcd87974e62d758b6c9a85fef53f46d2f026))
* **updater:** page through releases, word dialogs in the game language ([#170](https://github.com/themaiby/enshwick/issues/170)) ([c6bd27a](https://github.com/themaiby/enshwick/commit/c6bd27a947073a2e4a40dc822f2e3f64606c20a4))


### Bug Fixes

* **loader:** close crash paths found in the loader audit ([#182](https://github.com/themaiby/enshwick/issues/182)) ([7e4fab8](https://github.com/themaiby/enshwick/commit/7e4fab8a0b70efe438d43b6d9649af97c112e9cb))
* **loader:** keep settings texts for the life of the process ([#175](https://github.com/themaiby/enshwick/issues/175)) ([1d56467](https://github.com/themaiby/enshwick/commit/1d5646749a7a9f9f60ebdb966352393d2ab3f499))
* **survivors-per-altar:** sync copy_staffs, forget a last altar removed ([#176](https://github.com/themaiby/enshwick/issues/176)) ([551ab93](https://github.com/themaiby/enshwick/commit/551ab93eccbf0bc3b8913613635ce9f6419ee940))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * enshwick-loader-macros bumped from 0.8.0 to 0.8.1

## [0.8.0](https://github.com/themaiby/enshwick/compare/enshwick-loader-api-v0.7.0...enshwick-loader-api-v0.8.0) (2026-10-21)


### ⚠ BREAKING CHANGES

* **optimizer:** rename and tune per platform for Windows and Proton ([#165](https://github.com/themaiby/enshwick/issues/165))

### Features

* **api:** attach typed mod data to any entity, per world ([#161](https://github.com/themaiby/enshwick/issues/161)) ([b0fee70](https://github.com/themaiby/enshwick/commit/b0fee7049cf8c82f791a45716d3ff14d01267dd0))
* **loader:** add content packs of TOML files, signed and reloadable ([#158](https://github.com/themaiby/enshwick/issues/158)) ([7598b97](https://github.com/themaiby/enshwick/commit/7598b977434dfb207cd5c0d30c6cdd936c0cb0cc))
* **loader:** never start a mod that a renamed mod replaces ([#166](https://github.com/themaiby/enshwick/issues/166)) ([650993c](https://github.com/themaiby/enshwick/commit/650993c7b392be07fc16c4ca47925952c64f1487))
* **loader:** reload mods while a development server runs ([#164](https://github.com/themaiby/enshwick/issues/164)) ([16494a1](https://github.com/themaiby/enshwick/commit/16494a1f52b8c473876b6af9ce014f93a71afbc0))


### Performance

* **optimizer:** rename and tune per platform for Windows and Proton ([#165](https://github.com/themaiby/enshwick/issues/165)) ([71cf9b5](https://github.com/themaiby/enshwick/commit/71cf9b51b0d35b09d12dd4974e2dc91d174e2c0b))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * enshwick-loader-macros bumped from 0.7.0 to 0.8.0

## [0.7.0](https://github.com/themaiby/enshwick/compare/enshwick-loader-api-v0.6.0...enshwick-loader-api-v0.7.0) (2026-10-21)


### ⚠ BREAKING CHANGES

* **api:** name mod content mod:name and derive its GUIDs
* **api:** check service layouts and add typed mod events ([#146](https://github.com/themaiby/enshwick/issues/146))
* **loader:** mod settings per world ([#145](https://github.com/themaiby/enshwick/issues/145))
* **api:** name, describe and credit mods, with recommends and breaks ([#126](https://github.com/themaiby/enshwick/issues/126))
* **api:** config stages, ranges, choices, schema and /config ([#130](https://github.com/themaiby/enshwick/issues/130))
* **api:** one naming scheme, run-time errors and named abi values ([#110](https://github.com/themaiby/enshwick/issues/110))
* consolidate into Enshwick Loader, one API crate and one tool ([#85](https://github.com/themaiby/enshwick/issues/85))

### Features

* **api:** a safe loot pass over each container check ([#102](https://github.com/themaiby/enshwick/issues/102)) ([150d611](https://github.com/themaiby/enshwick/commit/150d6116326741ab0855759251f3ee25b053858b))
* **api:** add stat kinds, modifiers and built-in stats ([#143](https://github.com/themaiby/enshwick/issues/143)) ([dabec8b](https://github.com/themaiby/enshwick/commit/dabec8b85843c33fd3b7a3d07b17271451e7b344))
* **api:** add, dig and flatten terrain, and list its materials ([#125](https://github.com/themaiby/enshwick/issues/125)) ([032b1c9](https://github.com/themaiby/enshwick/commit/032b1c99513dad768d2e380ee339c42ece53ec90))
* **api:** altar limit and bases past 16 zones in the platform ([#101](https://github.com/themaiby/enshwick/issues/101)) ([a8e5ca9](https://github.com/themaiby/enshwick/commit/a8e5ca9bafc6321d70c81f2bc1458e34a1ec8175))
* **api:** altar zones and bases in the platform ([#97](https://github.com/themaiby/enshwick/issues/97)) ([e45cc7b](https://github.com/themaiby/enshwick/commit/e45cc7b58c6d82db2affa56b4fbfa89bd4fc3824))
* **api:** attach typed data to single items ([#133](https://github.com/themaiby/enshwick/issues/133)) ([4d938a9](https://github.com/themaiby/enshwick/commit/4d938a9c457857c7da77066b6fe8c2ecbe6d5ea3))
* **api:** check service layouts and add typed mod events ([#146](https://github.com/themaiby/enshwick/issues/146)) ([aa701ce](https://github.com/themaiby/enshwick/commit/aa701cef773075939cb480dc1c01e91474883196))
* **api:** config stages, ranges, choices, schema and /config ([#130](https://github.com/themaiby/enshwick/issues/130)) ([8a19e89](https://github.com/themaiby/enshwick/commit/8a19e89afe6ac74d09d9a32c66b1fc133e81bc68))
* **api:** draw lines and bars on the HUD and markers on the map ([#134](https://github.com/themaiby/enshwick/issues/134)) ([11d05b3](https://github.com/themaiby/enshwick/commit/11d05b3701942738d3fa49d73726af363af9133b))
* **api:** draw with the game's own widgets and show its dialogs ([#129](https://github.com/themaiby/enshwick/issues/129)) ([8601c70](https://github.com/themaiby/enshwick/commit/8601c70f8a0678a50f41301467d713bf41fab855))
* **api:** engine events from the server's ECS event streams ([#104](https://github.com/themaiby/enshwick/issues/104)) ([73cdaaf](https://github.com/themaiby/enshwick/commit/73cdaafb179d62d9891cb614a3b03f3e3fc783fa))
* **api:** entity queries, destroy, saves and per-world stores ([#105](https://github.com/themaiby/enshwick/issues/105)) ([ffd6090](https://github.com/themaiby/enshwick/commit/ffd6090628a40bd416e19767fcdc89ef7faba853))
* **api:** filter damage by cause and type before it is applied ([#121](https://github.com/themaiby/enshwick/issues/121)) ([a349c50](https://github.com/themaiby/enshwick/commit/a349c50d002eaedafdf9d10873c740d76bdd7140))
* **api:** give items a meter, tooltip lines and fill on give ([#128](https://github.com/themaiby/enshwick/issues/128)) ([11beb9b](https://github.com/themaiby/enshwick/commit/11beb9b762be4d3d6b80cc81b2c3baffb2ab3131))
* **api:** give items icons of their own ([#142](https://github.com/themaiby/enshwick/issues/142)) ([a0c0228](https://github.com/themaiby/enshwick/commit/a0c02288e4c586fc21e01e8497e1fe2da2d7ce76))
* **api:** interaction filters for chests and placed pieces ([#99](https://github.com/themaiby/enshwick/issues/99)) ([7a47400](https://github.com/themaiby/enshwick/commit/7a47400e8c74afee1e1fcfde60bc32aae24ba6d7))
* **api:** list, give and remove buffs and debuffs ([#122](https://github.com/themaiby/enshwick/issues/122)) ([78a7b89](https://github.com/themaiby/enshwick/commit/78a7b89b2fbc3722600529f32f718f1d073bfee8))
* **api:** messages between a mod's server and client parts ([#116](https://github.com/themaiby/enshwick/issues/116)) ([671de20](https://github.com/themaiby/enshwick/commit/671de2003d7a7016131b39db81b59efda4bd732a))
* **api:** name mod content mod:name and derive its GUIDs ([38f46ce](https://github.com/themaiby/enshwick/commit/38f46ce596e9fa6c25c0d673558ee56887160bfb))
* **api:** name, describe and credit mods, with recommends and breaks ([#126](https://github.com/themaiby/enshwick/issues/126)) ([edfb07c](https://github.com/themaiby/enshwick/commit/edfb07c59676c441f1e8b06ddc2791a80594ae6c))
* **api:** raise allowed game events and teleport entities ([#113](https://github.com/themaiby/enshwick/issues/113)) ([5a42ea6](https://github.com/themaiby/enshwick/commit/5a42ea670417aba93ad18fba57d36db1190de2d2))
* **api:** read and change the server's game settings ([#120](https://github.com/themaiby/enshwick/issues/120)) ([159227a](https://github.com/themaiby/enshwick/commit/159227a6d666999d4d1985b1920c590729055288))
* **api:** read and grant knowledge, quests and their events ([#123](https://github.com/themaiby/enshwick/issues/123)) ([0a656e5](https://github.com/themaiby/enshwick/commit/0a656e585e8c6575c1ff10089d413b2aea672d7b))
* **api:** read and set the time of day, its speed and day turns ([#119](https://github.com/themaiby/enshwick/issues/119)) ([b7505e6](https://github.com/themaiby/enshwick/commit/b7505e697beccf7fda0dfdb4db0b8220cf4e7f42))
* **api:** read and start weather, and report chunk loads ([#124](https://github.com/themaiby/enshwick/issues/124)) ([a36528f](https://github.com/themaiby/enshwick/commit/a36528f1d683b8446c175be2de7635a50f9033db))
* **api:** roll affixes onto items from loot, crafting or a mod ([#147](https://github.com/themaiby/enshwick/issues/147)) ([c40ab9b](https://github.com/themaiby/enshwick/commit/c40ab9be39b5e6fee00652aeb5c03117067ec340))
* **api:** run mod migrations once per world and record them ([#150](https://github.com/themaiby/enshwick/issues/150)) ([0bf89e9](https://github.com/themaiby/enshwick/commit/0bf89e9887a6518f26ab87caefba43149438b88c))
* **api:** show notices on a player's screen ([#117](https://github.com/themaiby/enshwick/issues/117)) ([b174549](https://github.com/themaiby/enshwick/commit/b174549819df1e5285b2b74b626732bf3599ee58))
* **api:** skill, comfort and experience hooks in the platform ([#100](https://github.com/themaiby/enshwick/issues/100)) ([c810a02](https://github.com/themaiby/enshwick/commit/c810a02a60d5d05b8872bf8628afc27123868aa1))
* **api:** spawn entities from templates ([#114](https://github.com/themaiby/enshwick/issues/114)) ([63b6e3f](https://github.com/themaiby/enshwick/commit/63b6e3f2da5d44ad05d0ccc0bcad34582469a64c))
* **api:** survivor lookups, beds, staffs and menu in the platform ([#103](https://github.com/themaiby/enshwick/issues/103)) ([166e1ba](https://github.com/themaiby/enshwick/commit/166e1ba837d789bbd9a36f9bab41e8530666d4ef))
* **api:** typed engine events, kill credit and character stats ([#106](https://github.com/themaiby/enshwick/issues/106)) ([99e1d93](https://github.com/themaiby/enshwick/commit/99e1d93ce64e899a9db5c5c872f199ac92bac6f4))
* **api:** veto crafting and building ([#115](https://github.com/themaiby/enshwick/issues/115)) ([7009d56](https://github.com/themaiby/enshwick/commit/7009d568e78eee315a6f8631df6463342e7b1fd2))
* **api:** write components marked changed, set health stamina mana ([#112](https://github.com/themaiby/enshwick/issues/112)) ([98a104f](https://github.com/themaiby/enshwick/commit/98a104fdc9eb06c525eabbcd73c286a039a5e211))
* **loader:** a Mods screen beside Settings with each mod's settings ([#137](https://github.com/themaiby/enshwick/issues/137)) ([45c2400](https://github.com/themaiby/enshwick/commit/45c240061c0d00578d7c9e28afe26755802316a6))
* **loader:** label mods' settings in the player's language ([#141](https://github.com/themaiby/enshwick/issues/141)) ([0cbc46f](https://github.com/themaiby/enshwick/commit/0cbc46fd311d1bc187b9c89236e10d83a429b27a))
* **loader:** mod settings per world ([#145](https://github.com/themaiby/enshwick/issues/145)) ([c06612b](https://github.com/themaiby/enshwick/commit/c06612bf26b00d4cd6175fd4dac343bb637a5ccc))
* **loader:** push changed join values and edit server settings remotely ([#138](https://github.com/themaiby/enshwick/issues/138)) ([58bba7c](https://github.com/themaiby/enshwick/commit/58bba7c378f92d5bf832d1f82aa09626591a2899))
* **loader:** ship build tables as signed data with a pattern fallback ([#155](https://github.com/themaiby/enshwick/issues/155)) ([1df2e3f](https://github.com/themaiby/enshwick/commit/1df2e3fdcaeaeac3ca082a18976e6a3f1d3a46c9))
* **survivors-per-altar:** copies come only from copy staffs ([#140](https://github.com/themaiby/enshwick/issues/140)) ([ab540fb](https://github.com/themaiby/enshwick/commit/ab540fb687900e7350f452f3403d4bebabd9f919))


### Bug Fixes

* **api:** audit the modder API for soundness, ABI and reuse ([#95](https://github.com/themaiby/enshwick/issues/95)) ([d800142](https://github.com/themaiby/enshwick/commit/d800142991a2d93aaf943af9f6e5cc629fa54d68))
* build the docs without rustdoc link errors ([#87](https://github.com/themaiby/enshwick/issues/87)) ([57b02cf](https://github.com/themaiby/enshwick/commit/57b02cf899cf0a0bfc97633ea47339e630f0c0e5))
* **loader:** safety and robustness pass from the conventions review ([#111](https://github.com/themaiby/enshwick/issues/111)) ([52ed976](https://github.com/themaiby/enshwick/commit/52ed976a30d65d7007a56202ed855fbb55e69740))
* **probe:** move the clock only after the weather checks are done ([#151](https://github.com/themaiby/enshwick/issues/151)) ([54c22ed](https://github.com/themaiby/enshwick/commit/54c22edf317990f4258e74133903d4f1a390603b))


### Other

* **api:** one naming scheme, run-time errors and named abi values ([#110](https://github.com/themaiby/enshwick/issues/110)) ([cbe72c5](https://github.com/themaiby/enshwick/commit/cbe72c5e92104d374272a5b192272e6e276dd37b))
* consolidate into Enshwick Loader, one API crate and one tool ([#85](https://github.com/themaiby/enshwick/issues/85)) ([5ddfde4](https://github.com/themaiby/enshwick/commit/5ddfde457d24f8e000d2dee1c74508f89582d526))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * enshwick-loader-macros bumped from 0.6.0 to 0.7.0

## [0.6.0](https://github.com/themaiby/enshwick/compare/enshwick-loader-api-v0.5.0...enshwick-loader-api-v0.6.0) (2026-10-21)


### ⚠ BREAKING CHANGES

* **api:** name mod content <mod>:<name> and derive its GUIDs ([#154](https://github.com/themaiby/enshwick/issues/154))
* **api:** check service layouts and add typed mod events ([#146](https://github.com/themaiby/enshwick/issues/146))
* **loader:** mod settings per world ([#145](https://github.com/themaiby/enshwick/issues/145))

### Features

* **api:** add stat kinds, modifiers and built-in stats ([#143](https://github.com/themaiby/enshwick/issues/143)) ([dabec8b](https://github.com/themaiby/enshwick/commit/dabec8b85843c33fd3b7a3d07b17271451e7b344))
* **api:** attach typed data to single items ([#133](https://github.com/themaiby/enshwick/issues/133)) ([4d938a9](https://github.com/themaiby/enshwick/commit/4d938a9c457857c7da77066b6fe8c2ecbe6d5ea3))
* **api:** check service layouts and add typed mod events ([#146](https://github.com/themaiby/enshwick/issues/146)) ([aa701ce](https://github.com/themaiby/enshwick/commit/aa701cef773075939cb480dc1c01e91474883196))
* **api:** draw lines and bars on the HUD and markers on the map ([#134](https://github.com/themaiby/enshwick/issues/134)) ([11d05b3](https://github.com/themaiby/enshwick/commit/11d05b3701942738d3fa49d73726af363af9133b))
* **api:** draw with the game's own widgets and show its dialogs ([#129](https://github.com/themaiby/enshwick/issues/129)) ([8601c70](https://github.com/themaiby/enshwick/commit/8601c70f8a0678a50f41301467d713bf41fab855))
* **api:** give items a meter, tooltip lines and fill on give ([#128](https://github.com/themaiby/enshwick/issues/128)) ([11beb9b](https://github.com/themaiby/enshwick/commit/11beb9b762be4d3d6b80cc81b2c3baffb2ab3131))
* **api:** give items icons of their own ([#142](https://github.com/themaiby/enshwick/issues/142)) ([a0c0228](https://github.com/themaiby/enshwick/commit/a0c02288e4c586fc21e01e8497e1fe2da2d7ce76))
* **api:** name mod content &lt;mod&gt;:&lt;name&gt; and derive its GUIDs ([#154](https://github.com/themaiby/enshwick/issues/154)) ([38f46ce](https://github.com/themaiby/enshwick/commit/38f46ce596e9fa6c25c0d673558ee56887160bfb))
* **api:** roll affixes onto items from loot, crafting or a mod ([#147](https://github.com/themaiby/enshwick/issues/147)) ([c40ab9b](https://github.com/themaiby/enshwick/commit/c40ab9be39b5e6fee00652aeb5c03117067ec340))
* **api:** run mod migrations once per world and record them ([#150](https://github.com/themaiby/enshwick/issues/150)) ([0bf89e9](https://github.com/themaiby/enshwick/commit/0bf89e9887a6518f26ab87caefba43149438b88c))
* **loader:** a Mods screen beside Settings with each mod's settings ([#137](https://github.com/themaiby/enshwick/issues/137)) ([45c2400](https://github.com/themaiby/enshwick/commit/45c240061c0d00578d7c9e28afe26755802316a6))
* **loader:** label mods' settings in the player's language ([#141](https://github.com/themaiby/enshwick/issues/141)) ([0cbc46f](https://github.com/themaiby/enshwick/commit/0cbc46fd311d1bc187b9c89236e10d83a429b27a))
* **loader:** mod settings per world ([#145](https://github.com/themaiby/enshwick/issues/145)) ([c06612b](https://github.com/themaiby/enshwick/commit/c06612bf26b00d4cd6175fd4dac343bb637a5ccc))
* **loader:** push changed join values and edit server settings remotely ([#138](https://github.com/themaiby/enshwick/issues/138)) ([58bba7c](https://github.com/themaiby/enshwick/commit/58bba7c378f92d5bf832d1f82aa09626591a2899))
* **loader:** ship build tables as signed data with a pattern fallback ([#155](https://github.com/themaiby/enshwick/issues/155)) ([1df2e3f](https://github.com/themaiby/enshwick/commit/1df2e3fdcaeaeac3ca082a18976e6a3f1d3a46c9))
* **survivors-per-altar:** copies come only from copy staffs ([#140](https://github.com/themaiby/enshwick/issues/140)) ([ab540fb](https://github.com/themaiby/enshwick/commit/ab540fb687900e7350f452f3403d4bebabd9f919))


### Bug Fixes

* **probe:** move the clock only after the weather checks are done ([#151](https://github.com/themaiby/enshwick/issues/151)) ([54c22ed](https://github.com/themaiby/enshwick/commit/54c22edf317990f4258e74133903d4f1a390603b))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * enshwick-loader-macros bumped from 0.5.0 to 0.6.0

## [0.5.0](https://github.com/themaiby/enshwick/compare/enshwick-loader-api-v0.4.0...enshwick-loader-api-v0.5.0) (2026-10-20)


### ⚠ BREAKING CHANGES

* **api:** name, describe and credit mods, with recommends and breaks ([#126](https://github.com/themaiby/enshwick/issues/126))
* **api:** config stages, ranges, choices, schema and /config ([#130](https://github.com/themaiby/enshwick/issues/130))

### Features

* **api:** add, dig and flatten terrain, and list its materials ([#125](https://github.com/themaiby/enshwick/issues/125)) ([032b1c9](https://github.com/themaiby/enshwick/commit/032b1c99513dad768d2e380ee339c42ece53ec90))
* **api:** config stages, ranges, choices, schema and /config ([#130](https://github.com/themaiby/enshwick/issues/130)) ([8a19e89](https://github.com/themaiby/enshwick/commit/8a19e89afe6ac74d09d9a32c66b1fc133e81bc68))
* **api:** filter damage by cause and type before it is applied ([#121](https://github.com/themaiby/enshwick/issues/121)) ([a349c50](https://github.com/themaiby/enshwick/commit/a349c50d002eaedafdf9d10873c740d76bdd7140))
* **api:** list, give and remove buffs and debuffs ([#122](https://github.com/themaiby/enshwick/issues/122)) ([78a7b89](https://github.com/themaiby/enshwick/commit/78a7b89b2fbc3722600529f32f718f1d073bfee8))
* **api:** messages between a mod's server and client parts ([#116](https://github.com/themaiby/enshwick/issues/116)) ([671de20](https://github.com/themaiby/enshwick/commit/671de2003d7a7016131b39db81b59efda4bd732a))
* **api:** name, describe and credit mods, with recommends and breaks ([#126](https://github.com/themaiby/enshwick/issues/126)) ([edfb07c](https://github.com/themaiby/enshwick/commit/edfb07c59676c441f1e8b06ddc2791a80594ae6c))
* **api:** raise allowed game events and teleport entities ([#113](https://github.com/themaiby/enshwick/issues/113)) ([5a42ea6](https://github.com/themaiby/enshwick/commit/5a42ea670417aba93ad18fba57d36db1190de2d2))
* **api:** read and change the server's game settings ([#120](https://github.com/themaiby/enshwick/issues/120)) ([159227a](https://github.com/themaiby/enshwick/commit/159227a6d666999d4d1985b1920c590729055288))
* **api:** read and grant knowledge, quests and their events ([#123](https://github.com/themaiby/enshwick/issues/123)) ([0a656e5](https://github.com/themaiby/enshwick/commit/0a656e585e8c6575c1ff10089d413b2aea672d7b))
* **api:** read and set the time of day, its speed and day turns ([#119](https://github.com/themaiby/enshwick/issues/119)) ([b7505e6](https://github.com/themaiby/enshwick/commit/b7505e697beccf7fda0dfdb4db0b8220cf4e7f42))
* **api:** read and start weather, and report chunk loads ([#124](https://github.com/themaiby/enshwick/issues/124)) ([a36528f](https://github.com/themaiby/enshwick/commit/a36528f1d683b8446c175be2de7635a50f9033db))
* **api:** show notices on a player's screen ([#117](https://github.com/themaiby/enshwick/issues/117)) ([b174549](https://github.com/themaiby/enshwick/commit/b174549819df1e5285b2b74b626732bf3599ee58))
* **api:** spawn entities from templates ([#114](https://github.com/themaiby/enshwick/issues/114)) ([63b6e3f](https://github.com/themaiby/enshwick/commit/63b6e3f2da5d44ad05d0ccc0bcad34582469a64c))
* **api:** veto crafting and building ([#115](https://github.com/themaiby/enshwick/issues/115)) ([7009d56](https://github.com/themaiby/enshwick/commit/7009d568e78eee315a6f8631df6463342e7b1fd2))
* **api:** write components marked changed, set health stamina mana ([#112](https://github.com/themaiby/enshwick/issues/112)) ([98a104f](https://github.com/themaiby/enshwick/commit/98a104fdc9eb06c525eabbcd73c286a039a5e211))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * enshwick-loader-macros bumped from 0.4.0 to 0.5.0

## [0.4.0](https://github.com/themaiby/enshwick/compare/enshwick-loader-api-v0.3.1...enshwick-loader-api-v0.4.0) (2026-10-20)


### ⚠ BREAKING CHANGES

* **api:** one naming scheme, run-time errors and named abi values ([#110](https://github.com/themaiby/enshwick/issues/110))

### Features

* **api:** a safe loot pass over each container check ([#102](https://github.com/themaiby/enshwick/issues/102)) ([150d611](https://github.com/themaiby/enshwick/commit/150d6116326741ab0855759251f3ee25b053858b))
* **api:** altar limit and bases past 16 zones in the platform ([#101](https://github.com/themaiby/enshwick/issues/101)) ([a8e5ca9](https://github.com/themaiby/enshwick/commit/a8e5ca9bafc6321d70c81f2bc1458e34a1ec8175))
* **api:** altar zones and bases in the platform ([#97](https://github.com/themaiby/enshwick/issues/97)) ([e45cc7b](https://github.com/themaiby/enshwick/commit/e45cc7b58c6d82db2affa56b4fbfa89bd4fc3824))
* **api:** engine events from the server's ECS event streams ([#104](https://github.com/themaiby/enshwick/issues/104)) ([73cdaaf](https://github.com/themaiby/enshwick/commit/73cdaafb179d62d9891cb614a3b03f3e3fc783fa))
* **api:** entity queries, destroy, saves and per-world stores ([#105](https://github.com/themaiby/enshwick/issues/105)) ([ffd6090](https://github.com/themaiby/enshwick/commit/ffd6090628a40bd416e19767fcdc89ef7faba853))
* **api:** interaction filters for chests and placed pieces ([#99](https://github.com/themaiby/enshwick/issues/99)) ([7a47400](https://github.com/themaiby/enshwick/commit/7a47400e8c74afee1e1fcfde60bc32aae24ba6d7))
* **api:** skill, comfort and experience hooks in the platform ([#100](https://github.com/themaiby/enshwick/issues/100)) ([c810a02](https://github.com/themaiby/enshwick/commit/c810a02a60d5d05b8872bf8628afc27123868aa1))
* **api:** survivor lookups, beds, staffs and menu in the platform ([#103](https://github.com/themaiby/enshwick/issues/103)) ([166e1ba](https://github.com/themaiby/enshwick/commit/166e1ba837d789bbd9a36f9bab41e8530666d4ef))
* **api:** typed engine events, kill credit and character stats ([#106](https://github.com/themaiby/enshwick/issues/106)) ([99e1d93](https://github.com/themaiby/enshwick/commit/99e1d93ce64e899a9db5c5c872f199ac92bac6f4))


### Bug Fixes

* **api:** audit the modder API for soundness, ABI and reuse ([#95](https://github.com/themaiby/enshwick/issues/95)) ([d800142](https://github.com/themaiby/enshwick/commit/d800142991a2d93aaf943af9f6e5cc629fa54d68))
* **loader:** safety and robustness pass from the conventions review ([#111](https://github.com/themaiby/enshwick/issues/111)) ([52ed976](https://github.com/themaiby/enshwick/commit/52ed976a30d65d7007a56202ed855fbb55e69740))


### Other

* **api:** one naming scheme, run-time errors and named abi values ([#110](https://github.com/themaiby/enshwick/issues/110)) ([cbe72c5](https://github.com/themaiby/enshwick/commit/cbe72c5e92104d374272a5b192272e6e276dd37b))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * enshwick-loader-macros bumped from 0.3.1 to 0.4.0

## [0.3.1](https://github.com/themaiby/enshwick/compare/enshwick-loader-api-v0.3.0...enshwick-loader-api-v0.3.1) (2026-10-20)


### Other

* **enshwick-loader-api:** Synchronize enshwick versions


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * enshwick-loader-macros bumped from 0.3.0 to 0.3.1

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
