# CLI reference

The `open-pcb` binary is installed by `@wae/open-pcb`. Every command parses
arguments, calls one SDK method and prints the result; all board behavior lives
in the model crate.

```bash
pnpm add @wae/open-pcb
pnpm exec open-pcb --help
```

## Conventions

- The document path is the first positional argument:
  `open-pcb <command> [file] [flags]`.
- Coordinates use `x,y` in millimetres, for example `--from 10,10`.
- Polygons use at least three `x,y` pairs separated by spaces or semicolons, for
  example `--points "0,0 20,0 20,10 0,10"`.
- Object ids come from command output (`track-3`, `via-2`, `keepout-1`).
- `--overwrite` is accepted by `create` and `import`.
- The CLI opens every document with sidecar history, so `undo` and `redo` work
  between invocations.
- Exit codes: `0` success, `1` inspection did not pass or the GUI failed, `2` a
  usage or argument failure.

## Session commands

### `capabilities`

```bash
open-pcb capabilities
```

Prints the engine package and version, project levels, formats with export,
export targets, checks and renderers.

### `create`

```bash
open-pcb create --out <file> [--title <title>] [--template blank] [--overwrite]
```

`--out` is required. `blank` is the only template. The command saves and closes
the new document.

```bash
open-pcb create --out blinker.opcb --title Blinker
```

### `open` and `list`

```bash
open-pcb open <file>
open-pcb list <file>
```

Both print the board summary: id, title, revision, units, layers and counts.

### `undo` and `redo`

```bash
open-pcb undo <file>
open-pcb redo <file>
```

Each moves the document one revision and prints the new summary. An empty
history exits with `2` and the `board/history-empty` code.

### `import`

```bash
open-pcb import <file> --out <file> [--overwrite]
```

Accepts KiCad PCB or IPC-2581, detected from content. Prints the format, counts
and the number of loss entries. The JSON result carries `losses` and
`lossCodes`.

```bash
open-pcb import board.kicad_pcb --out board.opcb
```

### `inspect`

```bash
open-pcb inspect <file> [--checks <list>]
```

`--checks` is a comma-separated list. Omitting it runs every supported check.
Exit code is `1` when `passed` is false. `solder-mask` reports merged mask
openings of different nets as errors and mask webs under 0.1 mm as warnings.

```bash
open-pcb inspect board.opcb --checks drc,connectivity,unrouted
```

### `export`

```bash
open-pcb export <file> --format <target> --out <file-or-directory>
```

Targets: `kicad_pcb`, `gbr`, `drl`, `drill_map`, `pos`, `placement`, `bom`,
`testpoints`, `ipc_d_356`, `step`, `ipc2581`, `pdf`, `svg`, `png`. `step` rejects
with `capability/not-implemented` before writing.

```bash
open-pcb export board.opcb --format gbr --out gerber
open-pcb export board.opcb --format bom --out bom.csv
```

### `gui`

```bash
open-pcb gui
```

Opens OpenPCB Studio. A window failure exits with `1`.

## Footprints and pads

### `footprint add`

```bash
open-pcb footprint add <file> --ref <ref> [--value <value>] [--footprint <name>]
    [--x <mm>] [--y <mm>] [--rotation <degrees>] [--layer <layer>]
```

`--ref` is required; `--x` and `--y` default to 0.

### `footprint delete`, `move`, `update`

```bash
open-pcb footprint delete <file> --id <id>
open-pcb footprint move <file> --id <id> [--x <mm>] [--y <mm>] [--rotation <deg>] [--layer <layer>]
open-pcb footprint update <file> --id <id> [--value <value>] [--footprint <name>]
```

`move` needs at least one of `--x`, `--y`, `--rotation` or `--layer`. `update`
needs `--value` or `--footprint`.

### `footprint pad add`

```bash
open-pcb footprint pad add <file> --footprint <id> --number <n> --x <mm> --y <mm>
    --w <mm> --h <mm> [--net <name>] [--drill <mm>]
```

All of `--footprint`, `--number`, `--x`, `--y`, `--w` and `--h` are required.
Pad positions are relative to the footprint origin. `--drill` stores one
circular finished drill; it must be positive and smaller than the pad's short
side, otherwise the edit fails with `pad/invalid-drill`.

### `footprint pad update`, `delete`

```bash
open-pcb footprint pad update <file> --footprint <id> --number <n>
    [--x <mm>] [--y <mm>] [--w <mm>] [--h <mm>] [--net <name>] [--drill <mm>]
open-pcb footprint pad delete <file> --footprint <id> --number <n>
```

An update needs at least one of `--x`, `--y`, `--w`, `--h`, `--net` or
`--drill`. A smaller `--w` or `--h` that would leave the stored drill equal to
or larger than the short side is rejected.

## Tracks

### `track add`

```bash
open-pcb track add <file> --layer <layer> --from <x,y> --to <x,y> --width <mm> [--net <name>]
```

`--layer`, `--from`, `--to` and a positive `--width` are required.

```bash
open-pcb track add board.opcb --net VCC --layer F.Cu --from 10,10 --to 30,10 --width 0.25
```

### `track update`, `split`, `delete`

```bash
open-pcb track update <file> --id <id> [--net <name>] [--layer <layer>]
    [--width <mm>] [--from <x,y>] [--to <x,y>]
open-pcb track split <file> --id <id> --x <mm> --y <mm>
open-pcb track delete <file> --id <id>
```

`split` divides one track at the given point and prints both new ids.

## Vias

### `via add`

```bash
open-pcb via add <file> --at <x,y> --diameter <mm> --drill <mm>
    [--net <name>] [--from-layer <layer>] [--to-layer <layer>] [--type micro]
```

`--at`, a positive `--diameter` and a positive `--drill` are required. The layer
span defaults to a through via. `--type micro` marks a micro via: the span must
bridge two adjacent copper layers with `F.Cu` or `B.Cu` at one end, otherwise
the command fails with `via/micro-span`. `--type` accepts only `micro`; omit it
and the span decides through, blind or buried.

### `via update`, `delete`, `backdrill`

```bash
open-pcb via update <file> --id <id> [--net <name>] [--at <x,y>] [--diameter <mm>] [--drill <mm>] [--type micro]
open-pcb via delete <file> --id <id>
open-pcb via backdrill <file> --id <id> [--depth <mm>] [--stub <mm>]
```

An update needs at least one of `--net`, `--at`, `--diameter`, `--drill` or
`--type`. A `--type micro` update keeps the existing span and rejects it with
`via/micro-span` when that span is not two adjacent copper layers with one outer
layer. `backdrill` needs `--depth` or `--stub`; `--depth` is greater than zero
and `--stub` is non-negative.

## Slots

```bash
open-pcb slot add <file> --from <x,y> --to <x,y> --width <mm> [--net <name>]
open-pcb slot update <file> --id <id> [--net <name>] [--from <x,y>] [--to <x,y>] [--width <mm>]
open-pcb slot delete <file> --id <id>
```

Slots are plated routed slots. They reach the Excellon drill file and drill map
only.

## Zones and keepouts

```bash
open-pcb zone add <file> --layer <layer> --points <list> [--net <name>]
open-pcb zone update <file> --id <id> [--layer <layer>] [--net <name>] [--points <list>]
open-pcb zone delete <file> --id <id>
open-pcb zone list <file>
open-pcb zone audit <file>
```

```bash
open-pcb keepout add <file> --points <list> [--layer <layer>]
open-pcb keepout update <file> --id <id> [--layer <layer>] [--points <list>]
open-pcb keepout delete <file> --id <id>
open-pcb keepout list <file>
```

`zone add` requires `--layer` and `--points`. `keepout add` requires `--points`;
omitting `--layer` means every layer (`*`). `zone list` prints per-layer and total
copper area. `keepout list` prints count and total declared area.

`zone audit` reports whether each pour touches copper of its own net on its own
layer: a pad whose box touches the polygon, a via whose circle touches it, or a
track whose endpoint lies inside it or whose segment crosses an edge. A pour with
no anchor is `isolated`; a pour without a net is `unassigned`. The report lists
the anchor counts and areas per layer, and the summary prints
`zones=<n> connected=<n> isolated=<n> unassigned=<n> isolatedArea=<mm2>`.
Adjacent same-net pour polygons are measured one at a time, so they are not
merged into one island.

## Text

```bash
open-pcb text add <file> --text <text> --layer <layer> --at <x,y>
    [--rotation <deg>] [--size <mm>] [--thickness <mm>]
open-pcb text update <file> --id <id> [--text <text>] [--layer <layer>] [--at <x,y>]
    [--rotation <deg>] [--size <mm>] [--thickness <mm>]
open-pcb text delete <file> --id <id>
open-pcb text list <file>
```

`--text`, `--layer` and `--at` are required on add. Text items are drawn by the
SVG preview and the KiCad PCB writer.

## Nets, differential pairs and net classes

```bash
open-pcb net add <file> --name <name>
open-pcb net audit <file>

open-pcb diffpair add <file> --positive <net> --negative <net> --width <mm> --gap <mm> [--max-skew <mm>]
open-pcb diffpair delete <file> --id <id>

open-pcb netclass add <file> --name <name> [--width <mm>] [--clearance <mm>]
    [--via-diameter <mm>] [--via-drill <mm>]
open-pcb netclass update <file> --id <id> [--name <name>] [--width <mm>]
    [--clearance <mm>] [--via-diameter <mm>] [--via-drill <mm>]
open-pcb netclass assign <file> --net <name> --class <name-or-id>
open-pcb netclass delete <file> --id <id>
```

`netclass update` edits one class in place and needs at least one of `--name`,
`--width`, `--clearance`, `--via-diameter` or `--via-drill`. Omitted fields keep
their current value, and a field cannot be cleared because an absent option is
the only empty form the model accepts. The class id never changes. Assignments
live on the class itself, so renaming a class keeps every net in it. A name that
another class already uses fails with `session/argument-invalid`, and an unknown
id fails with `object/not-found`.

`net audit` lists every declared net with its class targets, measured track
width range, via sizes and the narrowest clearance to another net. Clearance is
the edge-to-edge gap between two track segments of different declared nets on a
shared copper layer, and the required value is the strictest class clearance of
the pair or the 0.2 mm baseline. A track narrower than its class width reports
`net/width-below-class`, and a pair closer than its required clearance reports
`net/clearance-below-class`. The report is read-only.

## Test points

```bash
open-pcb testpoint add <file> --target <footprint-id:pad-number|via-id>
open-pcb testpoint delete <file> --id <id>
open-pcb testpoint coverage <file>
```

The target is an existing pad key or via id. `testpoint coverage` reports, per
declared net, how many pads and vias it offers and how many declared test points
touch it. A connectable net with no declared probe warns
`testpoints/net-not-covered`, and a net with neither a pad nor a via errors
`testpoints/net-unconnected`, because copper geometry never implies a probe.
The summary prints the covered share and the status.

## Routing and copper tools

```bash
open-pcb route <file> [--width <mm>] [--clearance <mm>] [--via <mm>] [--net <name>] [--netclass]
    [--style orthogonal|chamfer45]
open-pcb teardrop <file> [--shape linear] [--length <mm>]
open-pcb stitch <file> --net <name> --spacing <mm> [--inset <mm>]
    [--diameter <mm>] [--drill <mm>]
open-pcb copper balance <file>
open-pcb drill report <file>
open-pcb lengths <file>
```

`route` defaults to width 0.25, clearance 0.2 and via diameter 0.6. `--netclass`
makes each net use its declared net class for width, clearance and via values
per field, falling back to the explicit arguments and then to those defaults.
`--style` defaults to `orthogonal`; `chamfer45` joins two axis-aligned segments
with a 45-degree diagonal and falls back to an orthogonal path per edge when no
clear diagonal fits, reporting the reason in the JSON `edges` array.
`teardrop` implements `linear` only. `stitch` requires a net and a positive spacing and
reports how many vias it placed and skipped. `copper balance` prints per-layer
area, coverage when an outline exists, and the imbalance between copper sides.
`drill report` prints hole count, pad hole count, slot count, tool count and
back-drilled count. `holeCount` and `holes` keep their historical via-only
meaning; `padHoleCount` and `padHoles` list drilled pads, and a tool entry gains
`padHoles` when it also cuts pads.
`lengths` prints one entry per declared net with its track count, summed track
centreline length in document units and via count, plus the totals and the
longest-net ranking. The JSON `ranking` array orders the declared nets by length,
longest first, with equal lengths ordered by net name; the summary prints the
top entry as `longest=<net>(<length>)`.

```bash
open-pcb route board.opcb --net VCC --width 0.25
open-pcb route board.opcb --netclass
open-pcb route board.opcb --style chamfer45
open-pcb stitch board.opcb --net GND --spacing 5 --inset 1
```

## Fabrication manifest

```bash
open-pcb fab manifest <file>
```

`fab manifest` is a dry run of the Gerber writer: it lists every file the
writer would emit, in writer order, with its X2 `function`, source layer and
object count, and adds the drill tool and assembly row counts. It writes no
file. The report flags a missing outline (an error, because Gerber export
rejects the board), two planned files that map to one name, a copper layer with
no tracks, pads, vias or pours, a missing silkscreen layer, mask or paste files
whose copper side is absent, a board with no drill data and a board with no
footprints. `status` is `error` when any error finding exists, `warn` when only
warnings remain and `pass` otherwise. The summary prints `files=<n>`, `copper=<n>`,
`outline=yes|no`, `tools=<n>`, `placements=<n>`, `status=<s>` and `issues=<n>`.

```bash
open-pcb fab manifest board.opcb
```

## Panelization

```bash
open-pcb panel <file> --columns <count> --rows <count> [--gap-x <mm>] [--gap-y <mm>]
    [--rail <mm>] [--tab v-cut|mouse-bite|none] [--tab-width <mm>] [--tab-spacing <mm>]
    [--tooling-hole <mm>] [--tooling-inset <mm>]
```

`panel` arrays the rectangular outline and reports the panel outline, every
placement, seam lines, tab placeholders, the board and panel areas, the
utilization, the breakaway area and any tooling holes. `--columns` and `--rows`
are positive integers. `--gap-x` and `--gap-y` default to 0, `--rail` to 0 and
`--tab` to `v-cut`. Mouse-bite tabs default to a width of 5 mm and a spacing of
20 mm, need a positive gap wherever boards meet, and need a width no larger
than the spacing. The report writes no fabrication file.

`breakaway.area` is the panel area minus every board area, split into
`breakaway.railArea` and `breakaway.gapArea`, and `breakaway.percent` is its
share of the panel area. `--tooling-hole` places four holes with that finished
diameter in the rails and needs a positive `--rail`; `--tooling-inset` sets the
distance from the panel edge to a hole centre and defaults to half the rail.
The diameter and inset must keep every hole inside the rail, otherwise the
command fails. `toolingHoles` lists corner, position and diameter; the summary
appends `breakaway=<area>mm2` and, when holes exist, `tooling=<count>`.

```bash
open-pcb panel board.opcb --columns 2 --rows 2 --gap-x 5 --gap-y 5 --rail 2 --tab mouse-bite
open-pcb panel board.opcb --columns 2 --rows 2 --rail 3 --tooling-hole 2 --tooling-inset 1.5
```

## Layers and outline

```bash
open-pcb layer add <file> --name <name> [--kind <kind>] [--color <css>]
open-pcb layer rename <file> --id <id> --name <name>
open-pcb layer visible <file> --id <id> --visible true|false
open-pcb layer lock <file> --id <id> --locked true|false

open-pcb outline set <file> --from <x,y> --to <x,y>
```

Layer kinds: `copper`, `silkscreen`, `soldermask`, `assembly`, `mechanical`.
The outline is one rectangle.

## Rules, stackup and impedance

```bash
open-pcb rules show <file>
open-pcb rules set <file> [--hole-clearance <mm>] [--copper-to-edge <mm>]
    [--component-to-edge <mm>] [--hole-to-edge <mm>] [--solder-mask-expansion <mm>]

open-pcb stackup set <file> --copper-thickness <mm> --dielectric-height <mm>
    --dielectric-constant <er>
open-pcb stackup show <file>

open-pcb stackup impedance <file> --width <mm>
    [--structure microstrip|stripline|diffMicrostrip|diffStripline]
    [--gap <mm>] [--dielectric-height <mm>] [--dielectric-constant <er>]
    [--copper-thickness <mm>]
open-pcb stackup width <file> --impedance <ohm>
    [--structure microstrip|stripline|diffMicrostrip|diffStripline]
    [--gap <mm>] [--dielectric-height <mm>] [--dielectric-constant <er>]
    [--copper-thickness <mm>]
open-pcb stackup table <file> [--targets <ohm,ohm,...>]
    [--structure microstrip|stripline|diffMicrostrip|diffStripline]
    [--gap <mm>] [--dielectric-height <mm>] [--dielectric-constant <er>]
    [--copper-thickness <mm>]
open-pcb stackup report <file>
    [--structure auto|microstrip|stripline|diffMicrostrip|diffStripline]
    [--dielectric-height <mm>] [--dielectric-constant <er>]
    [--copper-thickness <mm>]
```

`rules set` needs at least one option. `stackup set` needs all three values;
`--dielectric-constant` must be at least 1. `stackup impedance` needs a positive
`--width`; `stackup width` needs a positive `--impedance`. `--gap` is required
for `diffMicrostrip` and `diffStripline`, and rejected for the single-ended
structures. The structure defaults to `microstrip`. Unspecified electrical
values come from the stored stackup. `stackup table` prints the width/impedance
report as JSON; `--targets` defaults to `50`, and each value must be positive,
finite and at most 64 targets.

`stackup report` prints JSON with one row per declared net and one row per
declared differential pair. The net rows list the tracked width and impedance
range plus the structures used; the pair rows carry the differential, odd-mode
and even-mode impedances. `--structure` defaults to `auto`, which uses
microstrip on `F.Cu` and `B.Cu` and stripline on inner copper; any other name
applies to every row. Values come from the stored stackup unless the electrical
options override them. An invalid `--structure` fails with
`open-pcb.cli.structure-invalid`.

```bash
open-pcb stackup set board.opcb --copper-thickness 0.035 --dielectric-height 0.2 --dielectric-constant 4.2
open-pcb stackup impedance board.opcb --width 0.25 --structure diffMicrostrip --gap 0.2
open-pcb stackup table board.opcb --targets 40,50,60 --structure microstrip
open-pcb stackup report board.opcb --structure auto
```

## Waivers and severity

```bash
open-pcb waiver add <file> --code <code> [--object <id>] [--reason <text>]
open-pcb waiver delete <file> --id <id>

open-pcb severity set <file> --code <code> --severity info|warning|error
open-pcb severity delete <file> --code <code>
```

Overrides apply before waivers. A waiver keeps the finding in the report and
marks it as waived.

## SDK-only operations

These board methods have no CLI subcommand:

- `Board.outlineGeometry(options)` validates and measures a supplied polygon, or
  measures the stored rectangle, and can return one offset polygon.
