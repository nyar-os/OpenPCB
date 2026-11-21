# @wae/open-pcb

TypeScript authoring surface for OpenPCB: one editable board document behind PCB
layout, inspection, fabrication and assembly. The session entry point is
`new OpenPCB(options?)`.

## Status

The TypeScript surface is wired to the shared native model. `new
OpenPCB(options?)` creates a session; `create` and `open` return an editable
`Board` with `inspect`, `save`, `export`, `close`, `undo`, `redo`,
`addFootprint`, `moveFootprint`, `updateFootprint`, `deleteFootprint`,
`addTrack`, `deleteTrack`, `addVia`, `deleteVia`, `addNet`, `route`, `addLayer`,
`updateLayer`, `setOutline`, `setRules` and `list`. `importBoard` reads a KiCad
PCB or IPC-2581 file, and `importKicadPcb` stays as the KiCad-only spelling of
the same operation. Each summary exposes `canUndo`, `canRedo` and the design
rules.

Implemented outputs are Gerber RS-274X, drill, drill map SVG, BOM,
pick-and-place, placement JSON, SVG, vector PDF, 1024x768 RGB PNG, KiCad PCB and
IPC-2581. Implemented checks are DRC, connectivity, clearance, annular rings,
annular-ring, track-width, unrouted, stackup, outline, silkscreen, courtyard,
netlist, hole-to-hole, copper-to-edge, edge-clearance, silkscreen-over-pad,
diffpair and dangling-copper. STEP export stays unsupported and rejects with
`capability/not-implemented`.

Classification is not a support claim: reads, writes and exports are reported by
`capabilities()`, never inferred from an extension or a method name.

## Example

```ts
import { OpenPCB } from '@wae/open-pcb';

const pcb = new OpenPCB();
const board = await pcb.create({ title: 'Blinker' });
await board.addNet({ name: 'VCC' });
await board.addFootprint({ reference: 'U1', value: 'MCU', footprint: 'QFN32', x: 30, y: 20, layer: 'F.Cu' });
await board.addTrack({ net: 'VCC', layer: 'F.Cu', width: 0.25, from: [10, 10], to: [30, 10] });
await board.addVia({ net: 'VCC', at: [20, 20], diameter: 0.6, drill: 0.3 });
await board.setOutline({ from: [0, 0], to: [100, 80] });
await board.setRules({ holeClearance: 0.3, solderMaskExpansion: 0.1 });
await board.route({ width: 0.25, clearance: 0.2, net: 'VCC' });
await board.addLayer({ name: 'Inner1', kind: 'copper' });
await board.moveFootprint({ id: 'fp-1', x: 40, y: 25, rotation: 90 });
await board.undo();
await board.redo();

const report = await board.inspect();
await board.save({ target: 'blinker.opcb' });
await board.export({ format: 'drl', target: 'blinker.drl' });
await board.export({ format: 'drill_map', target: 'blinker.drill-map.svg' });
await board.export({ format: 'placement', target: 'blinker.placement.json' });
await board.export({ format: 'pdf', target: 'blinker.pdf' });
await board.export({ format: 'png', target: 'blinker.png' });
await board.export({ format: 'kicad_pcb', target: 'blinker.kicad_pcb' });
await board.export({ format: 'ipc2581', target: 'blinker.ipc2581' });
const gerber = await board.export({ format: 'gbr', target: 'gerber' });
console.log(gerber.artifacts?.length); // 7

const imported = await pcb.importBoard({ path: 'blinker.ipc2581' });
console.log(imported.format, imported.losses.length);
await imported.board.close();

await board.close();
await pcb.dispose();
```

## Boundaries

- Board semantics, persistence, history and the writers live in
  `open-pcb-model`; this package maps values, typed arguments and stable error
  codes.
- Undo and redo are per handle: `open` and `create` start with empty history,
  and `save` does not add a step. `board/history-empty` reports an exhausted
  direction.
- Gerber export writes a directory of seven files (`F_Cu`, `B_Cu`, `F_SilkS`,
  `B_SilkS`, `F_Mask`, `B_Mask`, `Edge_Cuts`) and needs a board outline. The two
  silkscreen files carry the legend tracks the document holds; they are valid
  empty layers when the board has no silkscreen geometry. The mask files open
  every pad on their copper side by `solderMaskExpansion` per side (0.05 mm
  default); vias stay tented and inner-layer pads open no mask.
- The design rules are readable from every `BoardSummary` (`rules`) and editable
  with `board.setRules()`; values are millimetres, a partial update keeps the
  other rules, and a negative value is rejected.
- PDF export is a single-page vector document sized to A4 landscape or to the
  board. KiCad PCB and IPC-2581 exports are deterministic projections of the
  native document; no net names are invented for KiCad.
- `importBoard` detects the format from the content and reports it. KiCad PCB
  and IPC-2581 imports keep every construct the document can hold and return a
  loss ledger for the rest; `importKicadPcb` is the KiCad-only spelling.
- `board.route()` is a deterministic two-layer orthogonal router. It returns
  `{ routed, unrouted, tracks, vias }`; an edge it cannot place is reported, not
  approximated, and it never invents connectivity.
- `annular-ring` evaluates vias only because the document model has no pad
  drill. `track-width` errors below 0.1 mm and warns below 0.15 mm. `unrouted`
  warns with the number of disconnected pad groups per net. `edge-clearance`
  measures a component pad bounding box (or placement point) and every via drill
  against the rectangular outline; it uses the document rules
  `componentToEdge` (0.25 mm default) and `holeToEdge` (0.3 mm default).
- The `silkscreen` check reports `silkscreen/no-reference-text` as a warning
  because the model stores no silkscreen text, and still errors when a pad is
  not fully inside the outline. The `courtyard` check expands each footprint's
  pad bounding box by 0.25 mm and reports overlaps as errors.
- No second board model: the CLI, the SDK and the studio consume one revision.
- Native access goes through the Node-API binding.

## Scripts

```bash
pnpm --filter @wae/open-pcb run typecheck
pnpm --filter @wae/open-pcb run build
pnpm --filter @wae/open-pcb run test
```
