# Development

How to build Kition Slide from source, run its tests, take the README's pictures and publish the
kit's package. To use Slide, start with the [README](../README.md).

## Repository layout

| Path | Contents |
| --- | --- |
| `crates/` | The engine (Rust), in groups: `kdm`, `engine`, `formats`, `media`, `render`, `support`; compiled to WebAssembly for the editor |
| `bindings/` | The engine's two outlets: `wasm` (the in-page editor's engine, behind Kition's `KitDocument` contract) and `headless` (the `kition-slide` program for Kition's agent and `kition-kits-mcp`) |
| `web/` | The kit's editor (React, on kition-sdk's `@kition/sdk/editor`) |
| `agent/` | The agent guide and checks (KHP) |
| `conformance/` | The sample presentations the conformance checks open (KHP) |
| `kit.json` | The kit manifest (KHP) |
| `features.yaml`, `cases/` | The feature catalogue and its cases ([quality.md](quality.md)) |
| `docs/` | Documents: entry documents at the first level, parity documents in `docs/parity/`, feature documents in `docs/features/` |
| `assets/readme/` | The README's pictures |
| `tools/` | The repository's own tools: `tools/xtask` (`cargo xtask`) and `tools/scripts` (the visual reference pair selection) |

## Build and test

```sh
cargo test --workspace
cargo build --release -p kition-slide                  # the headless kit: target/release/kition-slide
cd web && pnpm install && pnpm engine && pnpm test:flow
```

`pnpm engine` builds `bindings/wasm` into `web/public/engine` and copies fonts from a
[kition-fonts](https://github.com/KitionAI/kition-fonts) checkout next to this repository (or the
one `KITION_FONTS_DIR` names). `pnpm dev` serves `dev.html`, which hosts the editor the way Kition
does; `web/package.json` lists the other browser tests (`test:ribbon`, `test:scroll`,
`test:table`, `test:chart`, …).

The check every change passes is `cargo xtask ci`, which the kit's CI run on GitHub runs with the
other tests ([quality.md](quality.md)); `cargo xtask --help`
lists the other tasks. `cargo xtask parity` recomputes the summary of
[parity/parity-checklist.md](parity/parity-checklist.md). The visual reference workflow, which uses
`tools/scripts`, is in [features/visual-reference-workflow.md](features/visual-reference-workflow.md).

### The headless kit for an agent

kition-sdk's `kition-kits-mcp` serves `kition-slide` as MCP tools (`slide_inspect`, `slide_run`,
`slide_check`, …) on a folder of presentations, for any MCP client; it finds `kit.json` above the
executable:

```sh
cargo build --release -p kition-slide
cargo install --git https://github.com/KitionAI/kition-sdk kition-kits --bin kition-kits-mcp
kition-kits-mcp --workspace <folder> --kit target/release/kition-slide
```

[agent/guide.md](../agent/guide.md) is the guide a model reads first; kition-sdk's
[agent guide](https://github.com/KitionAI/kition-sdk/blob/main/docs/sdk/agent.md) covers the
server's options.

### The editor's dependencies

By default `pnpm install` in `web/` installs kition-sdk's `@kition/sdk` and Kition Tessera's
`@kition/tessera` from npm, at the versions the committed `web/pnpm-lock.yaml` pins, so the editor
needs no other checkout. To develop against local checkouts instead, clone `kition-sdk` and
`kition-tessera` next to this repository (or set `KITION_SDK` and `KITION_TESSERA`), run
`pnpm install` in each of them and build the two packages (`pnpm build` in kition-sdk's
`packages/sdk` and kition-tessera's `packages/tessera`: their exports are the built `dist/`), then
run `KITION_LINK=1 pnpm install` in `web/`: `web/.pnpmfile.cjs` links them. The linked install
rewrites `web/pnpm-lock.yaml`; do not commit a lockfile changed that way.

## Take the README pictures

[`web/scripts/readme-shots.mjs`](../web/scripts/readme-shots.mjs) takes the editor pictures in
`assets/readme/` with kition-sdk's shared runner (`tools/readme-shots/` there): the development
host in Classic mode, light theme, English, 1440 × 900 at device scale 2, on the kition-showcase
samples `seed-pitch-deck`, `layout-gallery` and `budget-variance-table`. It needs kition-sdk and
kition-showcase checkouts next to this repository (or `KITION_SDK` and `KITION_SHOWCASE`), the
engine in `web/public/` (`pnpm engine`), and no HTTP proxy set:

```sh
cd web
node scripts/readme-shots.mjs                 # every picture, or name some: hero table
cd ../assets/readme
pngquant --quality 70-90 --force --ext .png hero.png layouts.png table.png transitions.png shapes.png
```

Each picture stays under 400 KB. The `made-*.png` pictures are copies of kition-showcase samples'
`thumb.png`. Every file in `assets/readme/` has a row in [attribution.md](attribution.md)
(`cargo xtask assets`).

## Publish

Kition ships the kit's package, `kition-slide-<version>.tgz` (the `slide-ui/` folder), a release
asset of this repository tagged `package-v<version>`. It is built on GitHub, never on a developer
machine: the Package workflow (`.github/workflows/package.yml`, which calls Kition's reusable
`kit-package.yml`) builds the commit it is started on, with `gh workflow run package.yml -R
KitionAI/kition-slide`, and publishes the Release with the package's lock entry,
`kition-slide-<version>.json`. Kition pins it with `node scripts/pin-kits.mjs slide` (see
`docs/kits.md` there). A version is published once: bump `version` in `kit.json` to publish a
change. Releasing is the owner's step: a change is done when it is green on `main`, and only the
owner bumps the version and starts the Package workflow.
