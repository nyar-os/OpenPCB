---
name: open-pcb
description: Create, edit, inspect, route and export PCB board documents with OpenPCB. Use when a task involves board stackup, layers, outline, footprints, nets, tracks, vias, autorouting, undo/redo, DRC, connectivity or clearance review, or Gerber, drill, BOM, position, PNG, PDF and SVG outputs.
---

# open-pcb

## Status

Executable MVP for the board document. `@wae/open-pcb`, the `open-pcb` CLI and
the model crate create, open, save, edit, inspect and export one revisioned
`.opcb` document.

Implemented:

- Create, open, save, list, inspect, export, undo, redo and close a board.
- Add nets, footprints, pads, tracks and vias; move, update and delete
  footprints; delete tracks and vias.
- Add, rename, hide and lock layers; set a rectangular outline.
- Route every net with at least two pads through a deterministic two-layer
  autorouter; it reports unroutable edges instead of approximating them.
- Declare differential pairs (`diffpair add`) with a target width, gap and
  maximum length skew; the `diffpair` check compares the routed copper against
  those targets.
- Checks: `drc`, `connectivity`, `clearance`, `annular_rings`, `annular-ring`,
  `track-width`, `unrouted`, `stackup`, `outline`, `silkscreen`, `courtyard`,
  `netlist`, `diffpair`, `dangling-copper`.
- Exports: `gbr` (Gerber RS-274X directory), `drl` (Excellon), `bom`, `pos`,
  `svg`, `pdf`, `png` (1024x768 RGB raster), `kicad_pcb` and `ipc2581`.
- Import a KiCad PCB or IPC-2581 document with a loss ledger for unrepresentable
  constructs; the CLI and SDK detect the format from the content.

Not implemented: STEP export, the interactive viewport renderer, silkscreen
text/shapes, pad drills (so `annular-ring` covers vias only), diagonal or rip-up
routing, and templates other than `blank`. Rectangular pad apertures are not
rotated in Gerber output. All unimplemented targets stay `unsupported` in the
capability matrix and reject with `capability/not-implemented`. Do not fill the
gap with local parsers, routers, geometry code or fabricated reports.

Always confirm the installed matrix with `open-pcb capabilities` before promising
a format, check or target.

## Commands

| Command | Purpose |
|---------|---------|
| `open-pcb capabilities` | Print the implemented format, check, renderer and export matrix. |
| `open-pcb create --out <file.opcb> --title <title> --template <name>` | Create a board with F.Cu, B.Cu and silkscreen; the outline starts unset. `blank` is the only implemented template. |
| `open-pcb open <file.opcb>` | Open a document and print its revision and layers. |
| `open-pcb import <file.kicad_pcb\|file.ipc2581> --out <file.opcb>` | Import a KiCad PCB or IPC-2581 document into a native board and report the detected format plus the loss ledger. |
| `open-pcb list <file.opcb>` | Print the revision, stackup and object counts. |
| `open-pcb undo <file.opcb>` / `open-pcb redo <file.opcb>` | Restore the previous or next saved document. The CLI keeps up to 32 snapshots in `<file>.history.json`. |
| `open-pcb net add <file.opcb> --name <name>` | Declare a net in place. |
| `open-pcb diffpair add <file.opcb> --positive <net> --negative <net> --width <mm> --gap <mm> --max-skew <mm>` | Declare a differential pair and its routing targets. |
| `open-pcb diffpair delete <file.opcb> --id <id>` | Delete a differential pair by id. |
| `open-pcb footprint add <file.opcb> --ref <ref> --value <value> --footprint <package> --rotation <deg> --layer <layer>` | Place a footprint in place. Position comes from `--x` and `--y` (both default to 0). |
| `open-pcb footprint move <file.opcb> --id <id> -x <mm> -y <mm> --rotation <deg> --layer <layer>` | Move, rotate or re-layer a footprint in place. `-x` and `-y` also accept the `--x` and `--y` spelling; `--layer` is optional. |
| `open-pcb footprint update <file.opcb> --id <id> --value <value> --footprint <package>` | Update the value or package of a footprint. |
| `open-pcb footprint delete <file.opcb> --id <id>` | Delete a footprint by its stable id. |
| `open-pcb track add <file.opcb> --net <name> --layer <layer> --from <x,y> --to <x,y> --width <mm>` | Route one track segment in place. |
| `open-pcb track delete <file.opcb> --id <id>` | Delete a track by id. |
| `open-pcb via add <file.opcb> --net <name> --at <x,y> --diameter <mm> --drill <mm>` | Place a via in place. |
| `open-pcb via delete <file.opcb> --id <id>` | Delete a via by id. |
| `open-pcb route <file.opcb> --width <mm> --clearance <mm> --via <mm> --net <name>` | Route every two-pad net, or only `--net`. Defaults: width 0.25, clearance 0.2, via diameter 0.6. Prints `{routed, unrouted, tracks, vias}`. |
| `open-pcb layer add <file.opcb> --name <name> --kind <kind> --color <css>` | Add a layer. `--kind` defaults to `copper`; `--color` defaults to the kind colour. |
| `open-pcb layer rename <file.opcb> --id <id> --name <name>` | Rename a layer and rewrite every footprint and track reference. |
| `open-pcb layer visible <file.opcb> --id <id> --visible <true\|false>` | Set layer visibility. |
| `open-pcb layer lock <file.opcb> --id <id> --locked <true\|false>` | Lock or unlock a layer. Edits on a locked layer fail with `board/layer-locked`. |
| `open-pcb outline set <file.opcb> --from <x,y> --to <x,y>` | Set the rectangular board outline; width and height must be positive. |
| `open-pcb inspect <file.opcb> --checks <list>` | Run the implemented checks. |
| `open-pcb export <file.opcb> --format <format> --out <file-or-directory>` | Write one implemented projection: `gbr` (directory), `drl`, `bom`, `pos`, `svg`, `pdf`, `png`, `kicad_pcb` or `ipc2581`. |
| `open-pcb gui` | Open the OpenPCB Studio window. |

The editing commands open the document, validate one mutation, increment the
revision and write the file atomically. A rejected edit leaves the file
unchanged. Unsupported export formats fail before a file is written. Every
mutating command also records the previous document for CLI undo/redo.

## Preflight

1. Run `open-pcb capabilities` and confirm the format, check and export entries
   the task needs. A method name or file extension is not a capability.
2. Read the board with `open-pcb list` and confirm the revision, units, stackup
   and object counts before editing.
3. If the task needs an unsupported target, stop and report it. Never substitute
   a different tool and call the result equivalent.

## Workflows

### Build a board document

1. `open-pcb create --out <file> --title <title> --template <name>` with `blank`.
2. Declare nets with `open-pcb net add`.
3. Place footprints with `open-pcb footprint add`, using `--x` and `--y` for the
   position, then route tracks and place vias.
4. Optionally run `open-pcb route` to connect every net with two or more pads;
   read `unrouted` and do not describe an unrouted edge as connected.
5. Set the board outline with `open-pcb outline set`, then run `open-pcb inspect`
   and fix every error finding before exporting.
6. Confirm changes with `open-pcb undo` / `open-pcb redo` when a step needs to be
   rolled back; finish with `open-pcb list`.

### DRC review

1. `open-pcb list <file>` and record the revision under review.
2. Run `open-pcb inspect <file> --checks <list>` with
   `drc,connectivity,clearance,annular-ring,track-width,unrouted,stackup,outline,silkscreen,courtyard,netlist,diffpair,dangling-copper`.
3. Report each finding with its code, layer and object. `passed` is false when
   any error-severity issue remains. `annular-ring` evaluates vias because the
   model has no pad drill; `track-width` errors below 0.1mm and warns below
   0.15mm; `unrouted` warns with the number of disconnected pad groups;
   `diffpair` compares routed length and spacing against the declared targets;
   `dangling-copper` warns about track ends that reach no pad, via or same-net
   track.
4. There is no interactive layer view. `svg` export is the only vector visual
   artifact and `png` is a fixed raster; review them as files, not as a viewport.

### Fabrication and assembly export

1. Confirm the stackup, drill sizes and outline the export depends on. Gerber
   edge cuts and the outline checks need `open-pcb outline set` first.
2. Export `gbr` to an explicit directory; the writer produces `F_Cu.gbr`,
   `B_Cu.gbr`, `F_SilkS.gbr`, `B_SilkS.gbr` and `Edge_Cuts.gbr`. Export `drl`,
   `pos`, `bom`, `svg`, `pdf`, `png`, `kicad_pcb` and `ipc2581` to explicit
   paths.
3. State exactly which targets were written and which reject, then inspect the
   files for the expected markers (`%FSLAX46Y46*%` and `M02*` in every Gerber
   file, `M48` and `M30` in drill, a `Designator` header in position, `<svg` in
   the SVG).

## Hard rules

- Never claim a check ran when it did not, and never fabricate DRC,
  connectivity, clearance or annular ring results. A board without an outline
  fails `stackup` and `outline` with `drc/outline-missing`.
- Never describe a format as supported because an extension is listed; read the
  capability matrix. Gerber export needs an outline and writes five files; it is
  not a single-file target.
- Keep one board model: copper layers, stackup, footprints, nets, tracks and vias
  must agree on the same revision.
- Do not place, route or resize objects with ad-hoc geometry outside the model.
- The autorouter is deterministic. Report `unrouted` verbatim; never treat a
  zero-track result as connected copper.
- Treat every finding as located on its layer and object; a count alone is not a
  review.

## Safety

- Confirm with the user before any command that writes or overwrites a board
  document, and state the target path first. The `add` commands rewrite the input
  file in place by design.
- Never delete, flatten or re-encode source libraries or fabrication outputs
  without explicit confirmation.
- Fabrication and assembly outputs are manufacturing inputs; state exactly what
  was exported and what was not.

## References

- `packages/open-pcb/src/cli.ts` for the exact command and flag surface.
- `packages/open-pcb/src/types.ts` for board, editing and report shapes.
- `crates/model/src/lib.rs` for the capability matrix this skill must match.
