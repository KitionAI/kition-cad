# Development

## Repository layout

| Path | Contents |
|---|---|
| `crates/` | The engine (Rust), in groups: `support/` (`geom`, `color`, `fonts`), `engine/` (`doc`, the drawing model; `constraints`; `engine`, the commands), `formats/` (`dxf`, `dwg`, `io`), `render/` (`render`) and `kdm/` (`kdm-model`). Layers are in [architecture.md](architecture.md) |
| `bindings/` | `wasm`: `kition-cad-kit`, the engine behind Kition's `KitDocument` contract, compiled to WebAssembly for the editor; `headless`: `kition-cad`, the headless kit executable for Kition's agent |
| `web/` | The kit's editor (React, on kition-sdk's `@kition/kit-editor`); `web/dev.html` is a host for development |
| `kit.json` | The kit manifest (KHP) |
| `agent/` | The agent guide (`guide.md`) and checks |
| `conformance/` | The agent conformance fixtures (`.dxf` drawings) that cases and tests refer to |
| `features.yaml`, `cases/` | The feature catalogue and its cases ([quality.md](quality.md)) |
| `docs/` | These documents: the entry documents, `features/` (one document per feature) and `parity/` (the comparison with the target application; `parity/parity-checklist.md` is written by `cargo xtask parity`) |
| `examples/` | `apartment.dxf`, a sample drawing that the engine's tests open (`bindings/wasm/tests/document_text.rs`, `crates/kdm/kdm-model/tests/roundtrip.rs`) |
| `tools/` | `tools/xtask`: `cargo xtask assets`, `layers`, `wasm`, `ci`, `corpus`, `stats`, `parity`, `version` and `ai`; `tools/xtask/data/menu-catalog.txt` is the reference menu tree `parity` compares with |

## Build and test

```sh
cargo test --workspace
cargo build --release -p kition-cad                       # the headless kit executable
cd web && pnpm install && pnpm engine && pnpm test:flow   # the editor
cargo xtask ci                                            # the check every change passes
```

`pnpm test:flow` drives the editor in the development host (`web/dev.html`) in Chromium and WebKit;
`web/package.json` lists the other browser tests (`test:interactions`, `test:precision`,
`test:polar`, `test:modes`, ...). Six P0 features are proved by command-level DXF save-and-reopen
round trips (`crates/engine/engine/tests/roundtrip.rs`); `features.yaml` and its cases under
`cases/` are the rest of the feature catalogue ([quality.md](quality.md)), and
[writing-format.md](writing-format.md) lists every command.

By default the editor's dependencies come from npm: `pnpm install` in `web/` installs the published
`@kition/*` packages at the versions the committed `web/pnpm-lock.yaml` pins, so no other checkout
is needed. To develop against local checkouts instead, run `KITION_LINK=1 pnpm install` in `web/`:
`web/.pnpmfile.cjs` then links `@kition/kdm`, `@kition/kit-editor`, `@kition/kit-engine`,
`@kition/kit-host`, `@kition/kit-ui-protocol` and `@kition/kit-dev-host` from a kition-sdk checkout,
and `@kition/tessera-ui`, `@kition/tessera-tokens` and `@kition/tessera-icons` from a kition-tessera
checkout (by default `../kition-sdk` and `../kition-tessera` beside this repository; `KITION_SDK`
and `KITION_TESSERA` name others). A package whose folder is not there stays on npm. The linked
install rewrites `web/pnpm-lock.yaml`; a lockfile changed that way must not be committed.

`cargo xtask` is an alias in `.cargo/config.toml` for `cargo run --package xtask --`. Every asset
file under `assets/`, `docs/images/` and `examples/` needs a row in
[attribution.md](attribution.md); `cargo xtask assets` checks it.

## Run the headless kit for an agent

`kition-cad` (`bindings/headless`, `headless` in `kit.json`) serves one drawing at a time over the
Kit Protocol; kition-sdk's `kition-kits-mcp` turns it into MCP tools (`cad_create`, `cad_inspect`,
`cad_commands`, `cad_run`, `cad_check`, ...) on a workspace folder for any MCP client:

```sh
cargo build --release -p kition-cad      # target/release/kition-cad
# in a kition-sdk checkout:
cargo build --release -p kition-kits --bin kition-kits-mcp
target/release/kition-kits-mcp --workspace <folder> --kit <kition-cad>/target/release/kition-cad
```

How an agent uses the tools is in [agent/guide.md](../agent/guide.md); kition-sdk's
[agent guide](https://github.com/KitionAI/kition-sdk/blob/main/docs/sdk/agent.md) covers the server.

## Take the README pictures

[`web/scripts/readme-shots.mjs`](../web/scripts/readme-shots.mjs) takes the README's editor pictures
(`assets/readme/hero.png`, `see-1.png` … `see-4.png`) with kition-sdk's shared runner
(`tools/readme-shots/`). It opens two showcase samples in the development host (`web/dev.html`),
`cad/house-lighting-circuit-schematic` (the hero, blocks, layers) and `cad/open-office-desk-layout`
(a dimension, a copy), at 1440 × 900, device scale 2, in Classic mode, the light theme and English,
and works the editor as a person would: clicks, drags and the wheel on the drawing, the Layers panel,
and commands typed at the command line. It needs the engine built in `web/public/` (`pnpm engine`),
and kition-sdk and kition-showcase checkouts beside this repository (or `KITION_SDK` and
`KITION_SHOWCASE` naming them):

```sh
cd web
node scripts/readme-shots.mjs            # every picture
node scripts/readme-shots.mjs see-2      # only the named ones
```

Run it with no HTTP proxy set; `CHROME` may name a Chrome or Chromium executable to use instead of
Playwright's. The points it clicks are measured on the drawings fitted to a 1440 × 900 window, so a
change to the editor's layout may need them measured again. When `pngquant` is installed the script
shrinks each picture with `pngquant --quality 70-90`; a README picture is at most 400 KB.
`made-1.png` … `made-4.png` are copies of showcase samples' `thumb.png` (two-bedroom-apartment-plan,
spur-gear-m4-z24, garden-house-site-plan, office-car-park-layout). Every picture has a row in
[attribution.md](attribution.md).

## Publish

Kition ships the kit's package. Publish it from a Kition checkout with
`scripts/publish-kit.sh cad` (see `docs/kits.md` there): it builds the editor and the engine from
the pushed `main`, attaches `kition-cad-<version>.tgz` (the `cad-ui/` folder) to a
`package-v<version>` release of this repository, and pins it in Kition's `kits/kits.lock.json`. A
version is published once: bump `version` in `kit.json` to publish a change.
