# OpenPCB

One editable, revisioned board document for layout, checks, fabrication and assembly.

## 💡 What is OpenPCB?

OpenPCB keeps a printed circuit board in a single `.opcb` document. Copper layers
and stackup, footprints and pads, nets, tracks and vias, pours, keepouts, design
rules, waivers and the board outline all live in that one file instead of being
scattered across a folder of exchange files.

You can edit the document from the `open-pcb` command line, from the
`@wae/open-pcb` SDK, or through an agent that follows the bundled skill. Every
path goes through the same engine, so an edit, a check and an export always
describe the same revision. Writes are atomic, and every failure carries a stable
code, so scripts and agents can react without parsing prose.

A new board starts from the `blank` template with front and back copper plus two
silkscreen layers. Undo and redo work between CLI invocations.

**OpenPCB is not:**

- a schematic editor — it starts from a routed board, not a netlist-first design
  flow;
- a field solver — controlled impedance uses documented closed forms and says so;
- a live board viewport — it writes SVG, PDF and PNG previews, not an editor
  canvas;
- a place where unsupported work is guessed — the autorouter reports the
  connections it could not make, importers record what a document cannot hold,
  and an unsupported export fails before writing a file.

## 🚀 Getting started

### 1. Agent path (recommended)

Install the skills package, then point your agent at the skill file:

```bash
pnpm add @wae/open-pcb-skills
```

```text
node_modules/@wae/open-pcb-skills/skills/open-pcb/SKILL.md
```

A realistic prompt:

> Create a 60 mm × 40 mm two-layer board called Sensor Node. Add `VCC` and
> `GND` nets, place a QFN32 MCU at 30,20, route every two-pad net, run DRC, and
> export Gerber, Excellon drill and a BOM to `./out`.

### 2. Install and run

```bash
pnpm add @wae/open-pcb
pnpm exec open-pcb create --out blinker.opcb --title Blinker --template blank
pnpm exec open-pcb net add blinker.opcb --name VCC
pnpm exec open-pcb footprint add blinker.opcb --ref U1 --value MCU --footprint QFN32 --x 30 --y 20 --layer F.Cu
pnpm exec open-pcb track add blinker.opcb --net VCC --layer F.Cu --from 10,10 --to 30,10 --width 0.25
pnpm exec open-pcb outline set blinker.opcb --from 0,0 --to 60,40
pnpm exec open-pcb inspect blinker.opcb --checks drc,connectivity,unrouted
pnpm exec open-pcb export blinker.opcb --format gbr --out gerber
```

`inspect` exits `1` when a check fails, so it works as a gate in a script. The
same document is available from the SDK:

```ts
import { OpenPCB } from '@wae/open-pcb';

const pcb = new OpenPCB();
const board = await pcb.open({ path: 'blinker.opcb' });
console.log((await board.inspect()).passed);
await pcb.dispose();
```

`open-pcb capabilities` lists what the installed build supports. Read it before
assuming a format, check or export target works.

## ✨ Highlights

- **One editable board document.** Layers, stackup, footprints, pads, nets,
  tracks, vias, slots, pours, keepouts, text items, net classes and test points
  share one revision counter and one JSON document.
- **Routing that states its result.** A deterministic two-layer autorouter
  connects nets with at least two pads, treats applicable copper keepouts as
  hard obstacles, and returns unrouted edges rather than approximating them.
  Opt in to net-class targets and each net routes with its class width,
  clearance and via values.
- **Checks before fabrication.** Twenty-one checks cover DRC, connectivity,
  clearance, annular rings, track width, unrouted nets, stackup, outline,
  silkscreen, courtyard, netlist, hole spacing, copper and hole edge clearance,
  silkscreen over pads, differential pairs, dangling copper, net classes,
  keepouts and via-in-pad.
- **Fabrication and assembly outputs.** Write Gerber RS-274X with X2 object
  attributes, Excellon drill and drill maps, BOM, pick-and-place, placement JSON,
  test-point CSV and IPC-D-356A netlists.
- **Exchange without a second model.** Import KiCad PCB or IPC-2581 into the same
  document and export KiCad PCB or IPC-2581 back, with a stable loss ledger for
  constructs the document cannot hold.
- **Controlled impedance.** Declare copper thickness, dielectric height and
  permittivity, then compute microstrip, stripline, edge-coupled differential
  microstrip or edge-coupled differential stripline impedance and convert a
  target impedance to a trace width.
- **Built for agents.** The skills package records the command surface, preflight
  steps and evidence rules so an assistant can build and review a board without
  inventing capabilities.
- **Undo across runs.** The CLI keeps up to 32 snapshots in
  `<file>.history.json`, so `undo` and `redo` work between invocations.

## 🧪 Examples

- [Package README](projects/packages/open-pcb/README.md) — SDK surface, quick
  start and constraints.
- [Agent skill](projects/packages/open-pcb-skills/skills/open-pcb/SKILL.md) —
  commands, workflows and evidence rules.
- [CLI reference](documentation/cli.md) — every command, flag and example.
- [Capabilities](documentation/capabilities.md) — the exact support matrix.

## 📚 Documentation

- [Architecture](documentation/architecture.md) — board document, module
  boundaries, data flow and determinism.
- [Capabilities](documentation/capabilities.md) — formats, checks, loss ledgers
  and unsupported boundaries.
- [CLI reference](documentation/cli.md) — every command, flag and example.
- [Development](documentation/development.md) — build, test and release.

## 📜 License

[Mozilla Public License 2.0](License.md)
