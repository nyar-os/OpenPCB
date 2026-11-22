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
Exit code is `1` when `passed` is false.

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
    --w <mm> --h <mm> [--net <name>]
```

All of `--footprint`, `--number`, `--x`, `--y`, `--w` and `--h` are required.
Pad positions are relative to the footprint origin.

### `footprint pad update`, `delete`

```bash
open-pcb footprint pad update <file> --footprint <id> --number <n>
    [--x <mm>] [--y <mm>] [--w <mm>] [--h <mm>] [--net <name>]
open-pcb footprint pad delete <file> --footprint <id> --number <n>
```

An update needs at least one of `--x`, `--y`, `--w`, `--h` or `--net`.

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
    [--net <name>] [--from-layer <layer>] [--to-layer <layer>]
```

`--at`, a positive `--diameter` and a positive `--drill` are required. The layer
span defaults to a through via.

### `via update`, `delete`, `backdrill`

```bash
open-pcb via update <file> --id <id> [--net <name>] [--at <x,y>] [--diameter <mm>] [--drill <mm>]
open-pcb via delete <file> --id <id>
open-pcb via backdrill <file> --id <id> [--depth <mm>] [--stub <mm>]
```

An update needs at least one of `--net`, `--at`, `--diameter` or `--drill`.
`backdrill` needs `--depth` or `--stub`; `--depth` is greater than zero and
`--stub` is non-negative.

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

open-pcb diffpair add <file> --positive <net> --negative <net> --width <mm> --gap <mm> [--max-skew <mm>]
open-pcb diffpair delete <file> --id <id>

open-pcb netclass add <file> --name <name> [--width <mm>] [--clearance <mm>]
    [--via-diameter <mm>] [--via-drill <mm>]
open-pcb netclass assign <file> --net <name> --class <name-or-id>
open-pcb netclass delete <file> --id <id>
```

## Test points

```bash
open-pcb testpoint add <file> --target <footprint-id:pad-number|via-id>
open-pcb testpoint delete <file> --id <id>
```

The target is an existing pad key or via id.

## Routing and copper tools

```bash
open-pcb route <file> [--width <mm>] [--clearance <mm>] [--via <mm>] [--net <name>] [--netclass]
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
`teardrop` implements `linear` only. `stitch` requires a net and a positive spacing and
reports how many vias it placed and skipped. `copper balance` prints per-layer
area, coverage when an outline exists, and the imbalance between copper sides.
`drill report` prints hole count, slot count, tool count and back-drilled count.
`lengths` prints one entry per declared net with its track count, summed track
centreline length in document units and via count, plus the totals.

```bash
open-pcb route board.opcb --net VCC --width 0.25
open-pcb route board.opcb --netclass
open-pcb stitch board.opcb --net GND --spacing 5 --inset 1
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
```

`rules set` needs at least one option. `stackup set` needs all three values;
`--dielectric-constant` must be at least 1. `stackup impedance` needs a positive
`--width`; `stackup width` needs a positive `--impedance`. `--gap` is required
for `diffMicrostrip` and `diffStripline`, and rejected for the single-ended
structures. The structure defaults to `microstrip`. Unspecified electrical
values come from the stored stackup.

```bash
open-pcb stackup set board.opcb --copper-thickness 0.035 --dielectric-height 0.2 --dielectric-constant 4.2
open-pcb stackup impedance board.opcb --width 0.25 --structure diffMicrostrip --gap 0.2
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

- `Board.panelize(options)` returns a panel outline, placements, seams, tab
  placeholders, areas and utilization.
- `Board.outlineGeometry(options)` validates and measures a supplied polygon, or
  measures the stored rectangle, and can return one offset polygon.
- `Board.impedanceTable(options)` returns a width/impedance table across a stackup
  sweep.
