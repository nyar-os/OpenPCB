# AGENTS.md

## Purpose

OpenPCB keeps one editable, revisioned `.opcb` board document and exposes it to
people, scripts and agents. Board semantics (edits, DRC, routing, impedance,
fabrication and exchange writers) live in the Rust model. The SDK, CLI, studio
and agent skill are surfaces over that model.

## Documentation map

| File | Audience | Contents |
|------|----------|----------|
| [README.md](README.md) | users | Product overview and getting started. |
| [documentation/README.md](documentation/README.md) | maintainers | Index of the maintainer documentation. |
| [documentation/architecture.md](documentation/architecture.md) | maintainers | Board model, boundaries, flow, errors, history. |
| [documentation/capabilities.md](documentation/capabilities.md) | maintainers | Exact capability matrix, loss ledgers, checks, boundaries. |
| [documentation/cli.md](documentation/cli.md) | maintainers | Every command and flag. |
| [documentation/development.md](documentation/development.md) | maintainers | Build, test, staging, CI and release. |
| [projects/packages/open-pcb/README.md](projects/packages/open-pcb/README.md) | npm users | SDK exports, usage and constraints. |
| [projects/packages/open-pcb-skills/README.md](projects/packages/open-pcb-skills/README.md) | agent users | Skill package usage and example prompt. |
| [projects/packages/open-pcb-ui/README.md](projects/packages/open-pcb-ui/README.md) | studio users | Studio window scope and launch. |

## Architecture invariants

1. **The Rust model is the only implementation.** `projects/crates/pcb-model`
   owns the board struct, validation, edits, checks, router, stackup, impedance,
   importers and exporters. Do not add a second implementation in TypeScript.
2. **`pcb-napi` maps values only.** It exposes one operation per export, returns
   JSON, prefixes error codes and hosts the WebView. It must not hold domain
   rules.
3. **The SDK and CLI do not duplicate domain logic.** `projects/packages/open-pcb`
   parses arguments, calls one native operation and renders the result. The CLI
   is assembly only; see the header of `src/cli.ts`.
4. **The capability matrix is the only support claim.** `capabilities()` in the
   model crate is the source of truth. Do not claim a format, check or export
   target that is not `supported` there, and add a test before promoting one.
5. **Document what exists.** README and docs must describe committed, tested
   behavior only. Remove a claim when the code does not back it.

## Verified commands

```bash
pnpm install --frozen-lockfile
pnpm run build            # wae build: native, ts, ui, skills, staging
pnpm run build:napi       # native addon for the current host
pnpm run build:ts         # @wae/open-pcb TypeScript
pnpm run stage:native     # copy addons into platform packages
pnpm typecheck            # tsc --noEmit across packages
pnpm test                 # every package test
pnpm run test:rust        # cargo test --release
pnpm run check:rust       # fmt check, clippy -D warnings, crate tests
```

CI also runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets
-- -D warnings` and `cargo test --workspace` on Rust, and
`pnpm install --frozen-lockfile && pnpm typecheck && pnpm run build:ts` on
TypeScript. Both run on `master` and `dev`.

## Change checklist

- [ ] Behavior change starts in `pcb-model`; the TypeScript surface only forwards.
- [ ] New or changed capability updates `capabilities()` and
  `documentation/capabilities.md`.
- [ ] New or changed command or flag updates `documentation/cli.md`.
- [ ] New loss code updates the ledger list in `documentation/capabilities.md`.
- [ ] `pnpm typecheck` and the relevant Rust tests pass.
- [ ] `git diff --check` reports no whitespace errors.
- [ ] No `git add -A`. Stage explicit paths.
- [ ] Do not commit generated addons, `dist/`, or sidecar history files.

## Commit style

Use an emoji prefix and a short imperative subject:

```text
✨ Add edge-coupled differential impedance to the stackup helpers
🐛 Fix pad object attributes in Gerber X2 output
📝 Rewrite the root and package readmes for readers
```

Keep one logical change per commit. Documentation moves and README rewrites are
separate from behavior changes.
