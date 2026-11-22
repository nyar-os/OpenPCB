# OpenPCB maintainer documentation

This folder holds the repository material that is kept out of the product
README: architecture, the exact capability matrix, the CLI reference and the
development guide.

| Document | Contents |
|----------|----------|
| [architecture.md](architecture.md) | Board model, module boundaries, data flow, determinism, error semantics, history ownership and workspace layout. |
| [capabilities.md](capabilities.md) | Capability matrix, import and export status, loss ledgers, checks and every unsupported boundary. |
| [cli.md](cli.md) | Every `open-pcb` command and flag, with compact examples. |
| [development.md](development.md) | Environment, build, test, native addon staging, CI and release. |
| [AGENTS.md](../AGENTS.md) | Rules for agents and contributors working in this repository. |

Product-facing entry points live elsewhere:

- [README.md](../README.md) — product overview and getting started.
- [open-pcb package README](../projects/packages/open-pcb/README.md) — SDK exports,
  usage and constraints.
- [open-pcb-skills README](../projects/packages/open-pcb-skills/README.md) — skill
  package usage and an example prompt.
- [open-pcb-ui README](../projects/packages/open-pcb-ui/README.md) — Studio window
  scope and launch.
- [OpenPCB skill](../projects/packages/open-pcb-skills/skills/open-pcb/SKILL.md) —
  agent workflows and evidence rules.
