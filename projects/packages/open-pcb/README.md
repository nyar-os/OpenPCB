# @wae/open-pcb

Typed session for editing one OpenPCB board document, with every check, report
and export driven by the same engine as the `open-pcb` command.

## Install

```bash
pnpm add @wae/open-pcb
```

Node.js 18 or newer is required.

## Quick start

```ts
import { OpenPCB } from '@wae/open-pcb';

const pcb = new OpenPCB();
const board = await pcb.create({ title: 'Sensor Node' });
await board.addNet({ name: 'VCC' });
await board.addFootprint({ reference: 'U1', value: 'MCU', footprint: 'QFN32', x: 30, y: 20, layer: 'F.Cu' });
await board.addTrack({ net: 'VCC', layer: 'F.Cu', from: [10, 10], to: [30, 10], width: 0.25 });
await board.setOutline({ from: [0, 0], to: [60, 40] });
await board.route();
console.log((await board.inspect()).passed);
await board.save({ target: 'sensor-node.opcb' });
await pcb.dispose();
```

`dispose()` closes every board the session opened. A board's `close()` releases
just that document.

## Session

- `new OpenPCB({ locale?, history? })` opens nothing; `history` is `memory` by
  default or `sidecar` for file-backed undo.
- `create(options)` starts a board from the `blank` template.
- `open({ path })` opens a native `.opcb` document.
- `importBoard({ path, target? })` reads KiCad PCB or IPC-2581 and reports the
  format, counts and loss ledger.
- `inspect(input, options?)` checks a document without keeping a handle.
- `capabilities()` returns the support matrix of the installed build.
- `dispose()` releases the session and all handles.

## Board handle

Every board exposes `id`, `revision`, `summary`, `canUndo` and `canRedo`, plus:

- **Document:** `save`, `export`, `inspect`, `list`, `close`, `undo`, `redo`.
- **Layers and outline:** `addLayer`, `updateLayer`, `setOutline`.
- **Footprints and pads:** `addFootprint`, `updateFootprint`, `moveFootprint`,
  `deleteFootprint`, `addPad`, `updatePad`, `deletePad`.
- **Nets and classes:** `addNet`, `addNetClass`, `updateNetClass`,
  `assignNetClass`, `deleteNetClass`, `addDifferentialPair`,
  `deleteDifferentialPair`.
- **Copper edits:** `addTrack`, `updateTrack`, `splitTrack`, `deleteTrack`,
  `addVia`, `updateVia`, `deleteVia`, `setViaBackDrill`, `addSlot`,
  `updateSlot`, `deleteSlot`, `addZone`, `updateZone`, `deleteZone`,
  `addKeepout`, `updateKeepout`, `deleteKeepout`, `addText`, `updateText`,
  `deleteText`, `listTexts`, `addTeardrops`, `stitchVias`.
- **Routing:** `route(options?)`; `useNetClasses: true` follows each net's
  declared class width, clearance and via targets.
- **Reports:** `zoneReport`, `zoneAudit`, `keepoutReport`, `drillReport`,
  `copperBalance`, `netLengths`, `fabManifest`.
- **Rules, stackup and impedance:** `setRules`, `setStackup`, `impedance`,
  `impedanceWidth`, `impedanceTable`.
- **Test points and policy:** `addTestPoint`, `deleteTestPoint`, `addWaiver`,
  `deleteWaiver`, `setSeverity`, `deleteSeverity`.
- **SDK-only geometry:** `panelize(options)`, `outlineGeometry(options)`.

## Errors and formats

`OpenPCBError` carries a stable `code`; compare codes, never messages.
`NotImplementedError`, `errorCodes` and `isOpenPCBError` are exported for the
same purpose. `boardFormats`, `formatRegistry`, `formatInfo`, `formatFromPath`
and `defaultIntentFor` classify paths and formats.

## Constraints

- The package is ESM-only and exports a single entry point, `@wae/open-pcb`.
- The engine is a native addon resolved from
  `@wae/open-pcb-<platform>-<arch>`. Do not import a platform package directly;
  set `OPEN_PCB_NATIVE` to a built `.node` file to override resolution.
- Without an addon, calls reject with `capability/not-implemented` instead of
  returning an empty board.
- `history: 'sidecar'` keeps up to 32 snapshots in `<file>.history.json` beside
  the target; the default memory session writes no sidecar.
- Saving over an existing path needs `overwrite: true`.
- Read `capabilities()` before assuming a format, check or export target works.

## Links

- [OpenPCB README](../../../README.md)
- [Capabilities](../../../documentation/capabilities.md)
- [CLI reference](../../../documentation/cli.md)
