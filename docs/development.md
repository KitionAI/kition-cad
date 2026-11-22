# Development

## Repository layout

| Path | Contents |
|---|---|
| `crates/` | The engine (Rust), in groups: `support/` (`geom`, `color`, `fonts`), `engine/` (`doc`, the drawing model; `constraints`; `engine`, the commands), `formats/` (`dxf`, `dwg`, `io`), `render/` (`render`) and `kdm/` (`kdm-model`). Layers are in [architecture.md](architecture.md) |
| `bindings/` | `wasm`: `kition-cad-kit`, the engine behind Kition's `KitDocument` contract, compiled to WebAssembly for the editor; `headless`: `kition-cad`, the headless kit executable for Kition's agent |
| `web/` | The kit's editor (React, on kition-sdk's `@kition/sdk/editor`); `web/dev.html` is a host for development |
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
cargo xtask ci                                            # the kit's own checks (CI runs them)
```

On your machine, run only the fast checks: `cargo fmt --all -- --check`, `cargo check -p
<touched crates> --all-targets`, `cargo xtask wasm` and `pnpm typecheck` in `web/`. The check every
change passes is the kit's CI run on GitHub (`.github/workflows/ci.yml`): clippy, `cargo test
--workspace`, `cargo xtask ci` and every `test:*` script of `web/` ([quality.md](quality.md)); a
change is done when the CI run of HEAD passes and HEAD is on `origin/main`.

`pnpm test:flow` drives the editor in the development host (`web/dev.html`) in Chromium and WebKit;
`web/package.json` lists the other browser tests (`test:interactions`, `test:precision`,
`test:polar`, `test:modes`, ...). Six P0 features are proved by command-level DXF save-and-reopen
round trips (`crates/engine/engine/tests/roundtrip.rs`); `features.yaml` and its cases under
`cases/` are the rest of the feature catalogue ([quality.md](quality.md)), and
[writing-format.md](writing-format.md) lists every command.

By default the editor's dependencies come from npm: `pnpm install` in `web/` installs the published
`@kition/sdk` (the editor, `@kition/sdk/editor`, and the development host, `@kition/sdk/dev-host`)
and `@kition/tessera` (its icons, `@kition/tessera/icons`) at the versions the committed
`web/pnpm-lock.yaml` pins, so no other checkout is needed. To develop against local checkouts
instead, run `KITION_LINK=1 pnpm install` in `web/`: `web/.pnpmfile.cjs` then links `@kition/sdk`
from a kition-sdk checkout's `packages/sdk` and `@kition/tessera` from a kition-tessera checkout's
`packages/tessera` (by default `../kition-sdk` and `../kition-tessera` beside this repository;
`KITION_SDK` and `KITION_TESSERA` name others). Both packages' `exports` point at their built
`dist/`, so build them in the checkouts first. A package whose folder is not there stays on npm.
The linked install rewrites `web/pnpm-lock.yaml`; a lockfile changed that way must not be
committed.

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

Kition ships the kit's package, `kition-cad-<version>.tgz` (the `cad-ui/` folder), a release
asset of this repository tagged `package-v<version>`. It is built on GitHub, never on a developer
machine: the Package workflow (`.github/workflows/package.yml`, which calls Kition's reusable
`kit-package.yml`) builds the commit it is started on, with `gh workflow run package.yml -R
KitionAI/kition-cad`, and publishes the Release with the package's lock entry,
`kition-cad-<version>.json`. Kition pins it with `node scripts/pin-kits.mjs cad` (see
`docs/kits.md` there). A version is published once: bump `version` in `kit.json` to publish a
change. Releasing is the owner's step: agents do not start the Package workflow or change a
version.
