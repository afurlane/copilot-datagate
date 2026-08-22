# Changelog

## [0.4.0](https://github.com/afurlane/copilot-datagate/compare/copilot-datagate-v0.3.0...copilot-datagate-v0.4.0) (2026-08-22)


### Features

* **config:** resolve project-aware config paths ([#46](https://github.com/afurlane/copilot-datagate/issues/46)) ([7d978ae](https://github.com/afurlane/copilot-datagate/commit/7d978ae326a30b80fbf462ddcd20ce71da080b62))

## [0.3.0](https://github.com/afurlane/copilot-datagate/compare/copilot-datagate-v0.2.1...copilot-datagate-v0.3.0) (2026-08-21)


### Features

* **mcp:** support postgres stdio benchmarks ([#41](https://github.com/afurlane/copilot-datagate/issues/41)) ([56af705](https://github.com/afurlane/copilot-datagate/commit/56af70591fb40f84e38c73e21cab84f8241e0c18))

## [0.2.1](https://github.com/afurlane/copilot-datagate/compare/copilot-datagate-v0.2.0...copilot-datagate-v0.2.1) (2026-08-21)


### Bug Fixes

* **release:** stabilize multiarch artifact packaging ([#38](https://github.com/afurlane/copilot-datagate/issues/38)) ([660e128](https://github.com/afurlane/copilot-datagate/commit/660e128f46bd240b2de8519793a64e4bc08f1cda))

## [0.2.0](https://github.com/afurlane/copilot-datagate/compare/copilot-datagate-v0.1.0...copilot-datagate-v0.2.0) (2026-08-21)


### Features

* **backend:** add PostgreSQL read-only connection pool ([#2](https://github.com/afurlane/copilot-datagate/issues/2)) ([3c75532](https://github.com/afurlane/copilot-datagate/commit/3c75532ad280be464538c5fc214c530b3e7d0fd0))
* **backend:** execute controlled selects as JSON rows ([#8](https://github.com/afurlane/copilot-datagate/issues/8)) ([4d99307](https://github.com/afurlane/copilot-datagate/commit/4d9930712fa89fe7e4481eb2ab5e7e75a56e1613))
* **backend:** introduce abstract read-only backend trait ([#23](https://github.com/afurlane/copilot-datagate/issues/23)) ([913ab78](https://github.com/afurlane/copilot-datagate/commit/913ab7893d899ad9387b2f9437edbed6c1c2fb84))
* **config:** activate named policy profiles ([#15](https://github.com/afurlane/copilot-datagate/issues/15)) ([35470f4](https://github.com/afurlane/copilot-datagate/commit/35470f4082c45e08f2e765299c5d20b6041dec50))
* **config:** add explicit multi-database backend selection ([#26](https://github.com/afurlane/copilot-datagate/issues/26)) ([0477670](https://github.com/afurlane/copilot-datagate/commit/0477670db71fbdc795e61cd561b835bcade66a09))
* **errors:** add sanitized structured public error contract ([#6](https://github.com/afurlane/copilot-datagate/issues/6)) ([f1701f9](https://github.com/afurlane/copilot-datagate/commit/f1701f9ff1e2806b7333021d5c86c606b4338af3))
* **mcp:** add policy-safe aggregate tool ([#16](https://github.com/afurlane/copilot-datagate/issues/16)) ([d1dc4e4](https://github.com/afurlane/copilot-datagate/commit/d1dc4e48cbad0a6f8354ad1176813616038ba855))
* **mcp:** add rmcp stdio transport ([#35](https://github.com/afurlane/copilot-datagate/issues/35)) ([a0d2fbf](https://github.com/afurlane/copilot-datagate/commit/a0d2fbfca0dd3c140ab9447e2a81610938a09f56))
* **mcp:** add typed search tool with policy-safe ilike ([#13](https://github.com/afurlane/copilot-datagate/issues/13)) ([d50a8c3](https://github.com/afurlane/copilot-datagate/commit/d50a8c383f0a4e8a49b2907df0deb9aabd01c6a5))
* **mcp:** add typed select tool contract ([#7](https://github.com/afurlane/copilot-datagate/issues/7)) ([43cbf42](https://github.com/afurlane/copilot-datagate/commit/43cbf42026525656f5f0dba4e8c22581dbd28e10))
* **mcp:** stabilize versioned api descriptor ([#33](https://github.com/afurlane/copilot-datagate/issues/33)) ([b06815b](https://github.com/afurlane/copilot-datagate/commit/b06815b5782b10bd740ae90c7a45aab837f5c678))
* **observability:** add runtime performance profiling ([#22](https://github.com/afurlane/copilot-datagate/issues/22)) ([63488e1](https://github.com/afurlane/copilot-datagate/commit/63488e113da8c3165ee4a94fedeebca7cdda05b2))
* **observability:** add select metrics foundation ([#12](https://github.com/afurlane/copilot-datagate/issues/12)) ([5c189e3](https://github.com/afurlane/copilot-datagate/commit/5c189e3677f673e3af2e255118e052ac42719a11))
* **observability:** add structured JSONL audit log foundation ([#5](https://github.com/afurlane/copilot-datagate/issues/5)) ([c08482d](https://github.com/afurlane/copilot-datagate/commit/c08482d49e88008cd3199a68913344dc5cdf09e3))
* **observability:** add structured logging formats ([#21](https://github.com/afurlane/copilot-datagate/issues/21)) ([c493b30](https://github.com/afurlane/copilot-datagate/commit/c493b30d2ae06ec5e7a070e25a38c50136bfdf1b))
* **policy:** add multi-schema table resolution ([#20](https://github.com/afurlane/copilot-datagate/issues/20)) ([03f7266](https://github.com/afurlane/copilot-datagate/commit/03f7266365209fbf92c3d722cbffd53ee8d9a0f0))
* **policy:** enforce per-column filter operator constraints ([#18](https://github.com/afurlane/copilot-datagate/issues/18)) ([53fcf47](https://github.com/afurlane/copilot-datagate/commit/53fcf47f4c52a7eda5798ce41df916bdc55fefba))
* **policy:** implement base policy engine with config-driven allow-lists ([#1](https://github.com/afurlane/copilot-datagate/issues/1)) ([8f2bcc1](https://github.com/afurlane/copilot-datagate/commit/8f2bcc119acf0302cd12938e03f240b393f5cde1))
* **policy:** support atomic policy reloads ([#29](https://github.com/afurlane/copilot-datagate/issues/29)) ([c66366e](https://github.com/afurlane/copilot-datagate/commit/c66366e9a568d4ec139d23c8b9796767adcc56f6))
* **query:** add advanced range and full-text filters ([#17](https://github.com/afurlane/copilot-datagate/issues/17)) ([803c976](https://github.com/afurlane/copilot-datagate/commit/803c97697532bd022c93b8284e2a64a26cced40b))
* **query:** add policy-validated parameterized select builder ([#4](https://github.com/afurlane/copilot-datagate/issues/4)) ([380bfa4](https://github.com/afurlane/copilot-datagate/commit/380bfa4152ab533cfb6b8b69e98552925b3365fb))
* **safety:** add server-side fixed-window rate limiting ([#9](https://github.com/afurlane/copilot-datagate/issues/9)) ([1ee2403](https://github.com/afurlane/copilot-datagate/commit/1ee24032104580417a2876873d615c7f807c98a4))
* **safety:** enforce MCP output size limits ([#11](https://github.com/afurlane/copilot-datagate/issues/11)) ([249d4cb](https://github.com/afurlane/copilot-datagate/commit/249d4cb32019d1d4c5f082bddd18ca743d9547c5))
* **safety:** enforce query complexity budgets server-side ([#10](https://github.com/afurlane/copilot-datagate/issues/10)) ([d43ea91](https://github.com/afurlane/copilot-datagate/commit/d43ea91631bf4eb003b5d0fc1b7c16e07d624d86))
* **schema:** add constraint metadata to catalog ([#19](https://github.com/afurlane/copilot-datagate/issues/19)) ([3428f02](https://github.com/afurlane/copilot-datagate/commit/3428f0290a51c0fbdbfcb5da33f56838eecbbe89))
* **schema:** load PostgreSQL schema catalog safely ([#3](https://github.com/afurlane/copilot-datagate/issues/3)) ([47c7c61](https://github.com/afurlane/copilot-datagate/commit/47c7c61cabddeb97c4610a23cde73e9b979df6d1))
* **security:** harden mcp request envelope validation ([#28](https://github.com/afurlane/copilot-datagate/issues/28)) ([0a02997](https://github.com/afurlane/copilot-datagate/commit/0a029977366a81b1566609b9e176a12bf596a5a0))
* support read-only MySQL/MariaDB backend ([#25](https://github.com/afurlane/copilot-datagate/issues/25)) ([f68faf8](https://github.com/afurlane/copilot-datagate/commit/f68faf8df0e4c811057254e5542caf680dfc5f23))
* support read-only SQLite backend ([#24](https://github.com/afurlane/copilot-datagate/issues/24)) ([d058dac](https://github.com/afurlane/copilot-datagate/commit/d058dac0cf519a4f1afa3d8dc7b72460328e0e5c))


### Bug Fixes

* **release:** configure root package for manifest mode ([#36](https://github.com/afurlane/copilot-datagate/issues/36)) ([a96e8de](https://github.com/afurlane/copilot-datagate/commit/a96e8dec60498abf32b98db8a1382239a188f44e))
* **release:** point manifest at root package ([#34](https://github.com/afurlane/copilot-datagate/issues/34)) ([c9aa095](https://github.com/afurlane/copilot-datagate/commit/c9aa095b08d243f6c005990f075a7372f9aba4d2))
