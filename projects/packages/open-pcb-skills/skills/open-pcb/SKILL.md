---
name: open-pcb
description: Create, edit, inspect, route and export PCB board documents with OpenPCB. Use when a task involves board stackup, controlled impedance, layers, outline, footprints, nets, tracks, vias, autorouting, undo/redo, DRC, connectivity or clearance review, or Gerber, drill, BOM, position, IPC-D-356, PNG, PDF and SVG outputs.
---

# open-pcb

## Status

Executable MVP for the board document. `@wae/open-pcb`, the `open-pcb` CLI and
the model crate create, open, save, edit, inspect and export one revisioned
`.opcb` document.

Implemented:

- Create, open, save, list, inspect, export, undo, redo and close a board.
- Add nets, footprints, pads, tracks, vias and plated routed slots; record a
  back-drill depth and remaining stub on a through via; move, update and delete
  footprints; add, update and delete their pads; update, delete tracks, vias and
  slots in place.
- Split a routed track at a point on its centreline (`track split`): the original
  keeps its id as the first segment and a new `track-N` becomes the second, with
  net, layer, width and the teardrop marker preserved. A point at an endpoint or
  off the segment is rejected, and the split is one revision and one undo step.
- Add, rename, hide and lock layers; set a rectangular outline.
- Report copper area per copper layer (`copper balance`) from tracks, vias, pads
  and pours, plus each layer's coverage and the imbalance across layers.
- Estimate a panel from the rectangular outline (`panel`, `Board.panelize`):
  columns, rows, gaps, an edge rail and a `v-cut`, `mouse-bite` or `none` tab
  style. The report gives the panel outline, every placement, seam lines, tab
  placeholders and the board and panel areas with utilization.
- Measure a polygonal board outline (`Board.outlineGeometry`): validate at least
  three distinct vertices with no self-intersection, report area and perimeter in
  millimetres and offset the polygon outward or inward. An offset that collapses
  a corner or turns the outline inside out is rejected.
- Declare copper pour regions on copper layers as closed polygons (`zone add`);
  each one is written as a `G36`/`G37` region in its layer's Gerber file and
  drawn by the SVG and PDF previews.
- Declare copper keepout regions (`keepout add`) on one copper layer or on `*`
  for the whole stack; the `keepout` check flags tracks, vias, pads and pours
  that enter one.
- Generate a stitching via fence (`stitch`) on a declared net around the inset
  board outline; the ring is evenly spaced, deterministic and skips positions
  that already hold a via.
- Read and edit the design rules (`rules show`, `rules set`): drill-hole
  clearance, copper-, component- and hole-to-edge gaps and the solder-mask
  expansion. Values are millimetres; a partial update keeps the other rules.
- Declare a physical stackup (`stackup set`) with copper thickness, dielectric
  height and permittivity; compute microstrip or stripline single-ended
  impedance (`stackup impedance`) and convert a target impedance to a trace
  width (`stackup width`). Per-request overrides replace the stored values.
- Build a controlled-impedance trace-width table (`Board.impedanceTable`) for a
  list of target impedances; each row gives the suggested width and the
  impedance recomputed from it on the same stackup.
- Declare net classes (`netclass add`), move nets into them (`netclass assign`)
  and let their optional width, clearance, via diameter and via drill targets
  drive the `netclass` check; `clearance` also uses the strictest class value.
- Mark existing pads and vias as test points (`testpoint add` with
  `footprint-id:pad-number` or a via id) and export `testpoints` as a CSV probe
  report with designator, net, layer and millimetre coordinates. Export
  `ipc_d_356` for an IPC-D-356A netlist with fixed 80-column `327` pad and
  `317`/`307` via records.
- Waive a finding by code (`waiver add`) or override a code's severity
  (`severity set`). A waived error no longer blocks `passed`, but the issue stays
  in the report with `waived: true`, its waiver id and reason.
- Route every net with at least two pads through a deterministic two-layer
  autorouter; it reports unroutable edges instead of approximating them.
- Declare differential pairs (`diffpair add`) with a target width, gap and
  maximum length skew; the `diffpair` check compares the routed copper against
  those targets.
- Generate linear teardrops (`teardrop`) at pad-track junctions as ordinary
  copper tracks; a repeat run skips existing segments. Arc teardrops stay
  unsupported.
- Checks: `drc`, `connectivity`, `clearance`, `annular_rings`, `annular-ring`,
  `track-width`, `unrouted`, `stackup`, `outline`, `silkscreen`, `courtyard`,
  `netlist`, `hole-to-hole`, `copper-to-edge`, `edge-clearance`,
  `silkscreen-over-pad`, `diffpair`, `dangling-copper`, `netclass`, `keepout`,
  `via-in-pad`, `pad-overlap`, `testpoints`. The `via-in-pad` check warns about
  vias whose centre lands inside a pad they share copper with; `pad-overlap`
  errors when two nets' rotated pads share copper on one layer;
  `silkscreen-over-pad` errors when a same-side silkscreen track or text box
  overlaps a pad, while tangent legend stays clean; `testpoints`
  warns about connectable nets with no declared test point and errors on nets
  with no pad or via.
- Exports: `gbr` (Gerber RS-274X directory), `drl` (Excellon), `drill_map`
  (hole map SVG with tool legend and back-drill marks), `bom`, `pos`,
  `placement` (millimetre JSON), `testpoints` (CSV probe report), `ipc_d_356`
  (IPC-D-356A netlist), `svg`, `pdf`, `png` (1024x768 RGB raster), `kicad_pcb`
  and `ipc2581`. The Gerber directory holds one file per copper layer plus
  `F_SilkS`, `B_SilkS`, `F_Mask`, `B_Mask`, `F_Paste`, `B_Paste` and
  `Edge_Cuts`; every file carries Gerber X2 file and aperture attributes
  (`%TF.FileFunction`, `%TF.Part`, `%TF.FilePolarity`, `%TA.AperFunction`).
- Import a KiCad PCB or IPC-2581 document with a loss ledger for unrepresentable
  constructs; the CLI and SDK detect the format from the content.

Not implemented: STEP export, the interactive viewport renderer, silkscreen
text/shapes, oval or slotted pad drills (a single circular pad drill is stored
and reaches KiCad PCB, `annular-ring`, the drill map and the Excellon file),
diagonal or rip-up routing, and templates other than `blank`. Rectangular pad apertures are not rotated in Gerber output. Back-drill depth and
stub live in `.opcb` and IPC-2581 and appear as `;Back drill` comments in the
Excellon file and as dashed rings in the drill map; KiCad PCB exchange does not
carry back drill. The outline is rectangular, so closure and self-intersection
are guaranteed by construction and `edge-clearance` only measures component and
drill spacing to the four edges. A polygon outline can be validated, measured and
offset with `Board.outlineGeometry`, but the stored outline, previews, exports and
edge checks stay rectangular and arc segments are not accepted. Teardrops are only generated as linear copper
tracks; arc teardrops stay unimplemented. The test point report lists only
explicitly declared pads and vias, never inferred probes. Keepouts are design
intent only: the `keepout` check reads them, but they are not written to Gerber,
drill data, IPC-2581 or KiCad output and the router does not avoid them. Plated
routed slots live in `.opcb` and reach the Excellon file (one `G85` segment) and
the drill map only; Gerber, IPC-2581, KiCad PCB, SVG, PDF and PNG output do not
carry them, and DRC does not treat a slot as copper or as a hole. Updating a slot
edits the model and that drill input only: the width is the routed tool
diameter, and no other geometry is recomputed. The copper
balance report sums feature areas rather than a union, and coverage and imbalance
need a rectangular outline. Via
stitching follows the rectangular outline only, places through vias, skips
occupied positions and does not test the new vias against existing copper. Panel
estimation reads the rectangular outline only and reports seam and tab
placeholders; it writes no fabrication file. Solder-mask openings are rectangular
and cover outer-layer pads only; vias stay tented and inner-layer pads open no
mask. Solder-paste windows are rectangular and exactly the outer pad size, with
no paste expansion rule. The stackup is a single copper thickness, dielectric
height and permittivity, so impedance is single-ended only: no differential,
coplanar or per-layer stackup, and the IPC-2141 closed forms replace a field
solver. The impedance table lists single-ended widths only, never differential
spacing. IPC-D-356 output writes the header, `327` pads, `317`/`307` vias and the
`999` terminator; it writes no net conductor segment, board-outline or test
channel records. All unimplemented targets stay
`unsupported` in the capability matrix and reject with
`capability/not-implemented`. Do not fill the gap with local parsers, routers,
geometry code or fabricated reports.

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
| `open-pcb teardrop <file.opcb> --shape linear --length <mm>` | Generate linear teardrops at pad-track junctions. Arc is rejected. |
| `open-pcb footprint add <file.opcb> --ref <ref> --value <value> --footprint <package> --rotation <deg> --layer <layer>` | Place a footprint in place. Position comes from `--x` and `--y` (both default to 0). |
| `open-pcb footprint move <file.opcb> --id <id> -x <mm> -y <mm> --rotation <deg> --layer <layer>` | Move, rotate or re-layer a footprint in place. `-x` and `-y` also accept the `--x` and `--y` spelling; `--layer` is optional. |
| `open-pcb footprint update <file.opcb> --id <id> --value <value> --footprint <package>` | Update the value or package of a footprint. |
| `open-pcb footprint delete <file.opcb> --id <id>` | Delete a footprint by its stable id. |
| `open-pcb footprint pad add <file.opcb> --footprint <id> --number <n> --x <mm> --y <mm> --w <mm> --h <mm> [--net <name>] [--drill <mm>]` | Add a pad to a footprint, optionally with a circular drill smaller than its short side. |
| `open-pcb footprint pad update <file.opcb> --footprint <id> --number <n> [--x <mm>] [--y <mm>] [--w <mm>] [--h <mm>] [--net <name>] [--drill <mm>]` | Update the geometry, net or drill of a pad; at least one field is required. |
| `open-pcb footprint pad delete <file.opcb> --footprint <id> --number <n>` | Delete a pad from a footprint. |
| `open-pcb track add <file.opcb> --net <name> --layer <layer> --from <x,y> --to <x,y> --width <mm>` | Route one track segment in place. |
| `open-pcb track update <file.opcb> --id <id> [--net <name>] [--layer <layer>] [--width <mm>] [--from <x,y>] [--to <x,y>]` | Update a track in place; at least one field is required. |
| `open-pcb track split <file.opcb> --id <id> --x <mm> --y <mm>` | Split one track at a point on its centreline into two collinear tracks. The original keeps its id and a new `track-N` becomes the second segment; a point at an endpoint or off the segment is rejected. |
| `open-pcb track delete <file.opcb> --id <id>` | Delete a track by id. |
| `open-pcb via add <file.opcb> --net <name> --at <x,y> --diameter <mm> --drill <mm>` | Place a via in place. |
| `open-pcb via update <file.opcb> --id <id> [--net <name>] [--at <x,y>] [--diameter <mm>] [--drill <mm>]` | Update a via in place; at least one field is required. |
| `open-pcb via backdrill <file.opcb> --id <id> --depth <mm> --stub <mm>` | Record back-drill depth and remaining stub on a through via. Blind and buried vias are rejected. |
| `open-pcb via delete <file.opcb> --id <id>` | Delete a via by id. |
| `open-pcb slot add <file.opcb> [--net <name>] --from <x,y> --to <x,y> --width <mm>` | Add a plated routed slot; the width is the tool diameter and the endpoints must differ. |
| `open-pcb slot update <file.opcb> --id <id> [--net <name>] [--from <x,y>] [--to <x,y>] [--width <mm>]` | Update a routed slot in place; at least one field is required and the endpoints must still differ. A new width changes the Excellon and drill-map input only. |
| `open-pcb slot delete <file.opcb> --id <id>` | Delete a slot by id. |
| `open-pcb copper balance <file.opcb>` | Print per-layer copper area, coverage and the coverage imbalance. |
| `open-pcb panel <file.opcb> --columns <count> --rows <count> [--gap-x <mm>] [--gap-y <mm>] [--rail <mm>] [--tab <style>] [--tab-width <mm>] [--tab-spacing <mm>]` | Estimate a panel with all placements, seams, tab placeholders, areas and utilization. `--tab` defaults to `v-cut`. |
| `open-pcb route <file.opcb> --width <mm> --clearance <mm> --via <mm> --net <name>` | Route every two-pad net, or only `--net`. Defaults: width 0.25, clearance 0.2, via diameter 0.6. Prints `{routed, unrouted, tracks, vias}`. |
| `open-pcb stitch <file.opcb> --net <name> --spacing <mm> [--inset <mm>] [--diameter <mm>] [--drill <mm>]` | Generate a stitching via fence around the inset rectangular outline. |
| `open-pcb layer add <file.opcb> --name <name> --kind <kind> --color <css>` | Add a layer. `--kind` defaults to `copper`; `--color` defaults to the kind colour. |
| `open-pcb layer rename <file.opcb> --id <id> --name <name>` | Rename a layer and rewrite every footprint and track reference. |
| `open-pcb layer visible <file.opcb> --id <id> --visible <true\|false>` | Set layer visibility. |
| `open-pcb layer lock <file.opcb> --id <id> --locked <true\|false>` | Lock or unlock a layer. Edits on a locked layer fail with `board/layer-locked`. |
| `open-pcb outline set <file.opcb> --from <x,y> --to <x,y>` | Set the rectangular board outline; width and height must be positive. |
| `open-pcb zone add <file.opcb> --layer <layer> --net <name> --points <list>` | Add a copper pour region as a closed polygon of at least three `x,y` points separated by spaces or semicolons. The layer must be copper. |
| `open-pcb zone delete <file.opcb> --id <id>` | Delete a copper pour region by id. |
| `open-pcb keepout add <file.opcb> [--layer <layer>] --points <list>` | Add a copper keepout polygon; `--layer` defaults to `*` for every copper layer. |
| `open-pcb keepout delete <file.opcb> --id <id>` | Delete a keepout region by id. |
| `open-pcb netclass add <file.opcb> --name <name> [--width <mm>] [--clearance <mm>] [--via-diameter <mm>] [--via-drill <mm>]` | Declare a net class with optional routing targets. |
| `open-pcb netclass assign <file.opcb> --net <name> --class <name-or-id>` | Move one declared net into a class; a net belongs to one class at a time. |
| `open-pcb netclass delete <file.opcb> --id <id>` | Delete a net class by id. |
| `open-pcb testpoint add <file.opcb> --target <pad-key-or-via-id>` | Mark an existing pad (`footprint-id:pad-number`) or via as a test point. |
| `open-pcb testpoint delete <file.opcb> --id <id>` | Delete a test point by id. |
| `open-pcb rules show <file.opcb>` | Print the design rules the checks and writers use. |
| `open-pcb rules set <file.opcb> --hole-clearance <mm> --copper-to-edge <mm> --component-to-edge <mm> --hole-to-edge <mm> --solder-mask-expansion <mm>` | Replace one or more design rules in place; at least one value is required and none may be negative. |
| `open-pcb stackup set <file.opcb> --copper-thickness <mm> --dielectric-height <mm> --dielectric-constant <er>` | Declare the physical stackup used by impedance calculations. |
| `open-pcb stackup show <file.opcb>` | Print the declared stackup, or `stackup unset`. |
| `open-pcb stackup impedance <file.opcb> --width <mm> [--structure microstrip\|stripline] [--dielectric-height <mm>] [--dielectric-constant <er>] [--copper-thickness <mm>]` | Compute single-ended impedance, defaulting to microstrip and the stored stackup. |
| `open-pcb stackup width <file.opcb> --impedance <ohm> [--structure microstrip\|stripline] [--dielectric-height <mm>] [--dielectric-constant <er>] [--copper-thickness <mm>]` | Convert a target impedance to the trace width that reaches it. |
| `open-pcb waiver add <file.opcb> --code <code> --object <id> --reason <text>` | Waive a finding code, optionally narrowed to one object. `--object` and `--reason` are optional. |
| `open-pcb waiver delete <file.opcb> --id <id>` | Delete a waiver by id. |
| `open-pcb severity set <file.opcb> --code <code> --severity <info\|warning\|error>` | Override the severity of one finding code. |
| `open-pcb severity delete <file.opcb> --code <code>` | Remove the severity override of one finding code. |
| `open-pcb inspect <file.opcb> --checks <list>` | Run the implemented checks. |
| `open-pcb export <file.opcb> --format <format> --out <file-or-directory>` | Write one implemented projection: `gbr` (directory), `drl`, `drill_map`, `bom`, `pos`, `placement`, `testpoints`, `ipc_d_356`, `svg`, `pdf`, `png`, `kicad_pcb` or `ipc2581`. |
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
   `drc,connectivity,clearance,annular-ring,track-width,unrouted,stackup,outline,silkscreen,courtyard,netlist,hole-to-hole,copper-to-edge,edge-clearance,silkscreen-over-pad,diffpair,dangling-copper,netclass,keepout,via-in-pad,pad-overlap,testpoints`.
3. Report each finding with its code, layer and object. `passed` is false when
   any non-waived error-severity issue remains; `active` and `waived` count the
   findings after policy is applied. A waived issue stays in `issues` with
   `waived: true` and its `waiverId`, so never call it fixed. `annular-ring`
   evaluates vias and drilled pads with the same minimum ring rule; `track-width` errors below
   0.1mm and warns below 0.15mm; `unrouted` warns with the number of disconnected
   pad groups; `diffpair` compares routed length and spacing against the declared
   targets; `edge-clearance` measures component bodies and via drills against the
   rectangular outline using `componentToEdge` and `holeToEdge`;
   `dangling-copper` warns about track ends that reach no pad, via or same-net
   track; `hole-to-hole` errors when a via or drilled-pad hole sits closer than
   `holeClearance` to a hole on another net, naming both objects; `via-in-pad`
   warns about vias whose centre lands inside a pad on a
   layer the via reaches, naming both objects; `testpoints` warns
   `testpoints/net-not-covered` for a connectable net without a declared test
   point and errors `testpoints/net-unconnected` for a net with no pad or via.
4. Waivers and severity overrides are document state: confirm the code, the
   optional object id and the reason before writing them, and say that a waiver
   accepts the finding rather than repairing it.
5. There is no interactive layer view. `svg` export is the only vector visual
   artifact and `png` is a fixed raster; review them as files, not as a viewport.

### Fabrication and assembly export

1. Confirm the stackup, drill sizes and outline the export depends on. Gerber
   edge cuts and the outline checks need `open-pcb outline set` first.
2. Export `gbr` to an explicit directory; the writer produces `F_Cu.gbr`,
   `B_Cu.gbr`, `F_SilkS.gbr`, `B_SilkS.gbr`, `F_Mask.gbr`, `B_Mask.gbr`,
   `F_Paste.gbr`, `B_Paste.gbr` and `Edge_Cuts.gbr`. Export `drl`, `drill_map`,
   `pos`, `placement`, `testpoints`, `ipc_d_356`, `bom`, `svg`, `pdf`, `png`,
   `kicad_pcb` and `ipc2581` to explicit paths.
   `testpoints` writes a probe CSV with a `Target,Designator,Pad,Kind,Net,Layer,X,Y`
   header; only explicitly declared test points appear. `ipc_d_356` writes
   80-column IPC-D-356A records and ends with `999`.
3. State exactly which targets were written and which reject, then inspect the
   files for the expected markers (`%TF.FileFunction` and `%FSLAX46Y46*%` and
   `M02*` in every Gerber file, `M48` and `M30` in drill, a `Designator` header in position, a
   `placements` array in placement, `OpenPCB drill map` in the drill map,
   `<svg` in the SVG). Back-drilled vias appear again under `;Back drill`
   comments with their depth and stub and gain a dashed ring in the drill map.
   Each routed slot is a `G85` segment under a tool whose diameter is the slot
   width and a round-capped line in the drill map.
   Each mask file flashes a rectangular opening per pad on its side, expanded by
   `solderMaskExpansion` per side; vias never open a mask. Each paste file
   flashes a rectangular stencil window exactly the pad size on its side; vias
   never open paste. Copper pour regions
   appear as `G36`/`G37` regions in the Gerber file of their layer. KiCad PCB and
   IPC-2581 exchange do not carry zones, and the PNG raster omits the pour fill.

## Hard rules

- Never claim a check ran when it did not, and never fabricate DRC,
  connectivity, clearance or annular ring results. A board without an outline
  fails `stackup` and `outline` with `drc/outline-missing`. A waivered finding is
  accepted, not repaired; report it with its waiver id and reason.
- Never describe a format as supported because an extension is listed; read the
  capability matrix. Gerber export needs an outline and writes nine files; it is
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
