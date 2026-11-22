# @wae/open-pcb-skills

Agent instructions for OpenPCB: the command surface, the preflight steps and the
evidence rules an assistant needs to build and review a board without inventing
a capability.

## Install

```bash
pnpm add @wae/open-pcb-skills
```

Point your agent at the skill file:

```text
node_modules/@wae/open-pcb-skills/skills/open-pcb/SKILL.md
```

## Example prompt

> Create a 60 mm × 40 mm two-layer board called Sensor Node. Add `VCC` and
> `GND` nets, place a QFN32 MCU at 30,20, route every two-pad net, run DRC, and
> export Gerber, Excellon drill and a BOM to `./out`.

The skill tells the agent to read `open-pcb capabilities` first, to report the
commands it actually ran, and to state which checks passed, failed or were
waived.

## Links

- [OpenPCB skill](./skills/open-pcb/SKILL.md)
- [OpenPCB README](../../../README.md)
- [CLI reference](../../../documentation/cli.md)
- [Capabilities](../../../documentation/capabilities.md)
