# Development

## Environment

- Node.js 18 or newer. CI uses Node 24.
- pnpm 10.28.0, pinned by `packageManager` in `package.json`.
- A Rust nightly toolchain with `rustfmt` and `clippy`. The workspace uses
  edition 2024 with `rust-version = "1.90"`.
- A C toolchain for the platform, because `pcb-napi` builds a native addon.
- Network access on the first build: `Cargo.toml` pins cross-repo git
  dependencies by full revision.

## Install

```bash
pnpm install --frozen-lockfile
```

## Build

`wae build` is the project driver. It runs in a fixed order: native, TypeScript,
UI, skills, staging.

```bash
pnpm run build          # full build
pnpm run build:napi     # native addon for the current host
pnpm run build:ts       # @wae/open-pcb TypeScript
pnpm run build:ui       # @wae/open-pcb-ui static assets
pnpm run stage:native   # copy built addons into the platform packages
```

Build outputs:

- `projects/packages/open-pcb/dist/index.js`, `dist/index.d.ts` and `dist/cli.js`
- `projects/packages/open-pcb/dist/gui/index.html` (staged UI)
- `projects/packages/open-pcb-<platform>-<arch>/open-pcb.<platform>-<arch>.node`

The UI static directory from `projects/packages/open-pcb-ui/dist/static` is
copied into `projects/packages/open-pcb/dist/gui`; the GUI entry is expected at
both `dist/cli.js` and `dist/gui/index.html`.

The studio package is built with VMZ directly:

```bash
pnpm --filter @wae/open-pcb-ui run check   # vmz check
pnpm --filter @wae/open-pcb-ui run build   # vmz build --profile static
pnpm --filter @wae/open-pcb-ui run serve   # local preview
```

## Test

```bash
pnpm test               # every package test
pnpm run test:rust      # cargo test --release
pnpm run check:rust     # fmt check, clippy -D warnings, crate tests
pnpm typecheck          # tsc --noEmit across packages
```

The `@wae/open-pcb` test script compiles the package and runs
`node --test dist/smoke.test.js`. The smoke test needs a native addon for the
current host, so build and stage the addon before running it from a clean
checkout.

`check:rust` runs:

```bash
cargo fmt -p open-pcb-model -p open-pcb-napi -- --check
cargo clippy -p open-pcb-model -p open-pcb-napi --all-targets -- -D warnings
cargo test -p open-pcb-model -p open-pcb-napi
```

## Native addon staging

`wae.config.ts` is the source of truth for targets and package names. It declares
five targets:

| Platform | Triple | Package | Node file |
|----------|--------|---------|-----------|
| win32 x64 | `x86_64-pc-windows-msvc` | `@wae/open-pcb-win32-x64` | `open-pcb.win32-x64.node` |
| darwin x64 | `x86_64-apple-darwin` | `@wae/open-pcb-darwin-x64` | `open-pcb.darwin-x64.node` |
| darwin arm64 | `aarch64-apple-darwin` | `@wae/open-pcb-darwin-arm64` | `open-pcb.darwin-arm64.node` |
| linux x64 | `x86_64-unknown-linux-gnu` | `@wae/open-pcb-linux-x64` | `open-pcb.linux-x64.node` |
| linux arm64 | `aarch64-unknown-linux-gnu` | `@wae/open-pcb-linux-arm64` | `open-pcb.linux-arm64.node` |

The addon is built from the `open-pcb-napi` crate with cdylib name
`open_pcb_napi`. Staging copies the built file into the matching platform
package, where the package `main` points at it.

At runtime the SDK resolves the addon in this order:

1. `OPEN_PCB_NATIVE`, when set to a built `.node` path.
2. The `@wae/open-pcb-<platform>-<arch>` optional dependency.
3. A `capability/not-implemented` error with an install hint.

Never import a platform package from application code; install `@wae/open-pcb`
and let resolution pick the artifact.

## CI

Both check workflows run on pushes and pull requests to `master` and `dev`.

`check-rust.yml`:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`check-typescript.yml`:

```bash
pnpm install --frozen-lockfile
pnpm typecheck
pnpm run build:ts
```

## Release and publish

`publish-npm.yml` is the only npm trusted workflow. Keep the filename; the
Trusted Publisher contract binds repository `nyar-os/OpenPCB`, workflow file
`publish-npm.yml` and environment `NPM_PUBLISH`.

Triggers:

- Push a `v*` tag.
- Run the workflow manually with an existing release tag.

The `native` job builds all five targets with
`npx -y -p @wae/wae@0.0.6 wae native build --target <target>` and uploads each
`.node`. The `publish` job downloads them, copies each file into its platform
package, runs `pnpm run build:ts`, then publishes with:

```bash
pnpm exec nifty publish --access public --version="${RELEASE_TAG#v}"
```

Real versions publish only through this OIDC workflow. Local `nifty publish`
runs are for `0.0.0` placeholder stubs, not for a release.

## Documentation

Keep the reader-facing [README](../README.md) free of architecture, workspace
layout and build commands. Update [capabilities.md](capabilities.md) when the
model's capability matrix changes, and update [cli.md](cli.md) when a command or
flag changes.
