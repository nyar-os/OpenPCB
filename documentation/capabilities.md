# Capabilities

`open-pcb capabilities` prints this matrix, and `OpenPCB.capabilities()` returns
it as an object. The model crate owns both, so the CLI, the SDK and the studio
cannot disagree. A format, check or target is listed as supported only when a
test exercises it.

## Project

| Surface | Level |
|---------|-------|
| Import | `partial` |
| Edit | `partial` |
| Export | `partial` |

Those values describe the product as a whole. The per-format and per-target
entries below are the precise claims.

## Formats

`edit` is `unsupported` for every exchange format; those files become a native
document before any edit. `step` is declared but implemented in neither
direction.

| Format | Import | Export | Notes |
|--------|--------|--------|-------|
| `kicad_pcb` | `supported` | `supported` | Import reports a loss ledger. |
| `ipc2581` | `supported` | `supported` | Import reports a loss ledger. |
| `gbr` | `unsupported` | `supported` | Writes a directory of Gerber files. |
| `drl` | `unsupported` | `supported` | Excellon drill file with via, pad and routed-slot holes. |
| `drill_map` | `unsupported` | `supported` | SVG hole map covering vias, pad drills and routed slots. |
| `pos` | `unsupported` | `supported` | Pick-and-place CSV. |
| `placement` | `unsupported` | `supported` | Millimetre JSON. |
| `bom` | `unsupported` | `supported` | Bill of materials CSV. |
| `testpoints` | `unsupported` | `supported` | Probe report CSV. |
| `ipc_d_356` | `unsupported` | `supported` | IPC-D-356A netlist. |
| `step` | `unsupported` | `unsupported` | Rejects with `capability/not-implemented`. |
| `svg` | `unsupported` | `supported` | Vector preview writer. |
| `pdf` | `unsupported` | `supported` | Vector PDF writer; A4 landscape with the board scaled to fill the printable area and centred between the title and legend bands. Pale layer colours are swapped for dark ink so the silkscreen stays legible on white paper. |
| `png` | `unsupported` | `supported` | Fixed 1024 × 768 RGB raster. |

Declared preview renderers: `svg` is `supported`; `png` and `pdf` are
`unsupported` as preview renderers even though they are export targets.

Declared export targets: all fourteen formats above. Every target is
`supported` except `step`.

## Checks

All declared checks are supported. Running `inspect` without a list runs all of
them.

```text
drc              connectivity       clearance          annular_rings
annular-ring     track-width        unrouted           stackup
outline          silkscreen         courtyard          netlist
hole-to-hole     hole-to-track      hole-to-pad        copper-to-edge
edge-clearance   silkscreen-over-pad diffpair          dangling-copper
netclass         keepout            via-in-pad         pad-overlap
solder-mask      testpoints         stencil
```

`annular_rings` and `annular-ring` are both accepted spellings.

`hole-to-track` measures every drill hole (a via or a drilled pad) against each
track of a different net. It uses the nearest point on the track segment and the
strictest clearance of the two nets, including net-class clearance. Every track
is compared regardless of the layers the track or the hole spans, so a blind or
buried via is treated as a through-hole feature and there is no per-layer
filtering.

`hole-to-pad` measures the same drill-hole set against every pad box. A pad hole
is not measured against its own pad, same-net pairs are skipped (including two
unassigned objects), and the check uses the strictest clearance of the two nets,
including net-class clearance. Pads are compared on every layer, so each drill
hole is a through-hole feature and there is no per-layer filtering.

`diffpair` compares declared pairs against their width, gap and skew targets and
also infers pairs from net names for nets that no declared pair covers. Two nets
form an inferred pair when they share a base name and end in `_P`/`_N` or
`+`/`-`; skew above 0.5 mm warns `drc/diffpair-skew`, and a half without a
counterpart warns `drc/diffpair-unpaired`.

`solder-mask` measures the mask apertures of outer-layer pads. Each opening is
the pad's rotated copper box grown by the document's solder-mask expansion per
side; vias stay tented and inner-layer pads open no mask. Two openings of
different nets that overlap or touch report `drc/solder-mask-bridge` as an
error, because no mask separates the nets; a web narrower than 0.1 mm reports
`drc/solder-mask-sliver` as a warning. Pads of the same net are skipped, so a
thermal or merged same-net opening stays clean, and a web at exactly 0.1 mm is
accepted.

`stencil` measures the solder-paste windows of outer-layer pads. Each window is
the pad's copper rectangle, because the paste writer applies no expansion and
vias stay tented. The area ratio is the opening area divided by the opening
perimeter times the stencil thickness; the aspect ratio is the opening's short
side divided by the thickness. An area ratio below 0.66 reports
`drc/stencil-area-ratio` and an aspect ratio below 1.5 reports
`drc/stencil-aspect-ratio`, both as warnings. The foil thickness, area-ratio
minimum and aspect-ratio minimum are design rules and default to 0.12 mm, 0.66
and 1.5. The model stores rectangular pads, so every window is measured as a
rectangle; there is no round or polygon aperture geometry to approximate.

## Feature matrix

| Capability | Level |
|------------|-------|
| Through, blind and buried vias | `supported` |
| Micro vias | `supported` |
| Differential pairs | `supported` |
| Back drill | `supported` |
| Teardrops | `linear` supported; `arc` unsupported |
| Design rules | `supported` |
| Solder-mask Gerber layers | `supported` |
| Solder-paste Gerber layers | `supported` |
| Gerber X2 file, aperture and object attributes | `supported` |
| Waivers and severity overrides | `supported` |
| Routing (orthogonal and `chamfer45` styles, opt-in net-class targets via `useNetClasses`) | `supported` |
| Copper pours and the zone report | `supported` |
| Silkscreen text items | `supported` |
| Keepouts and the keepout report | `supported` |
| Via stitching | `supported` |
| Pad drills (stored, circular only) and their fabrication output | `supported` |
| Plated routed slots and the drill report | `supported` |
| Copper area and balance report | `supported` |
| Routed net-length report with longest-net ranking (`lengths`, `Board.netLengths`) | `supported` |
| Net class, width and clearance audit (`net audit`, `Board.netAudit`) | `supported` |
| Test point coverage report (`testpoint coverage`, `Board.testpointCoverage`) | `supported` |
| Copper pour connectivity audit (`zone audit`, `Board.zoneAudit`) | `supported` |
| Fabrication file manifest (`fab manifest`, `Board.fabManifest`) | `supported` |
| Panelization (`panel`, `Board.panelize`) | `supported` |
| Solder-mask aperture check (`solder-mask`) | `supported` |
| Stencil aperture check (`stencil`) | `supported` |
| Panel tooling holes and breakaway report (`panel`) | `supported` |
| Polygon outline geometry (`Board.outlineGeometry`, SDK only) | `supported` |
| Net classes (checks and opt-in routing targets, editable in place) | `supported` |
| Test points | `supported` |
| Stackup | `supported` |
| Single-ended and edge-coupled differential impedance | `supported` |
| Impedance-to-width table (`stackup table`, `Board.impedanceTable`) | `supported` |
| Per-net and per-pair impedance report (`stackup report`, `Board.impedanceReport`) | `supported` |

## Import loss ledgers

Loss codes are stable strings reported per item. A supported import can still
lose constructs; the ledger says exactly which ones.

KiCad PCB (`kicad_pcb`) loss codes:

```text
kicad/construct-not-imported            kicad/outline-not-rectangular
kicad/net-zero-named                    kicad/net-duplicate
kicad/footprint-invalid                 kicad/footprint-without-reference
kicad/footprint-duplicate-reference     kicad/footprint-layer-missing
kicad/footprint-graphics                kicad/footprint-property
kicad/footprint-text                    kicad/footprint-model
kicad/footprint-zone                    kicad/footprint-attr
kicad/footprint-field-not-modeled       kicad/pad-invalid
kicad/pad-size-invalid                  kicad/pad-number-duplicate
kicad/pad-shape                         kicad/pad-type
kicad/pad-drill                         kicad/pad-rotation
kicad/pad-layer                         kicad/pad-net-unknown
kicad/pad-field-not-modeled             kicad/track-invalid
kicad/track-layer-missing               kicad/track-net-unknown
kicad/via-invalid                       kicad/via-layer-span
kicad/via-type                          kicad/via-net-unknown
```

IPC-2581 (`ipc2581`) loss codes:

```text
ipc2581/construct-not-imported          ipc2581/outline-not-rectangular
ipc2581/net-duplicate                   ipc2581/net-unknown
ipc2581/package-without-name            ipc2581/package-duplicate
ipc2581/package-missing                 ipc2581/component-without-reference
ipc2581/component-duplicate-reference   ipc2581/component-layer-unmapped
ipc2581/pin-invalid                     ipc2581/pin-without-size
ipc2581/pin-number-duplicate            ipc2581/track-invalid
ipc2581/track-layer-missing             ipc2581/track-without-width
ipc2581/hole-invalid                    ipc2581/hole-not-plated
ipc2581/back-drill-invalid              ipc2581/units-unknown
```

Both readers keep copper layers, nets, footprints and pads, tracks, through,
blind and buried vias and a rectangular profile. KiCad stores a pad's single
circular `(drill d)`; an oval or slotted pad drill stays in the ledger under
`kicad/pad-drill` instead of being approximated. KiCad also imports a micro via
when its span bridges two adjacent copper layers and one end is an outer layer;
any other micro span stays in the ledger under `kicad/via-layer-span` instead of
being approximated.

## Unsupported boundaries

### Import and export

- STEP is neither imported nor exported.
- PDF text uses the base-14 Helvetica encoding, so characters outside printable
  ASCII degrade to `?`. The export then reports one `pdf/text-encoding` loss
  with the dropped character count; `svg` keeps full Unicode.
- One Excellon file cannot carry per-hole depth, so blind, buried and micro
  vias are drilled as through holes. The export then reports one
  `drl/partial-span-via` loss with the affected count; `ipc2581` keeps the
  exact spans.
- Gerber files declare the 4.6 coordinate format. A board whose geometry
  exceeds ±9999.999999 mm fails the `gbr` export with a clear error instead of
  writing coordinates a reader would misparse.
- PNG is a fixed 1024 × 768 top-down raster. It draws copper, silkscreen
  outlines and text through a built-in 5 × 7 font, but has no viewport and no
  anti-aliasing.
- There is no interactive board viewport. SVG is a writer, not a viewer.
- Classification is not a support claim. An extension can be listed in the
  format registry while its entry stays `unsupported`.

### Outline, panels and stitching

- The stored board outline is rectangular. Closure and self-intersection are
  guaranteed by construction, so `edge-clearance` measures component pad
  bounding boxes (or placement points) and via drills against the four edges.
- `Board.outlineGeometry` validates, measures and offsets a caller-supplied
  polygon, or measures the stored rectangle. It does not replace the stored
  outline, and arc segments are rejected.
- `Board.panelize` arrays the rectangular outline, reports the panel outline,
  placements, seam lines, tab placeholders, board and panel areas, utilization,
  the breakaway area and four optional rail tooling holes, and writes no
  fabrication file. Gap defaults to 0, rail to 0, tab style to `v-cut`, mouse-bite
  width to 5 and tab spacing to 20. `breakaway.area` is the panel area minus
  every board area and equals `railArea + gapArea`; `breakaway.percent` is that
  share of the panel area. Tooling holes need a positive rail: the diameter and
  inset must keep every hole inside the rail band, and the inset defaults to
  half the rail. Tooling holes are report geometry only and reach no fabrication
  file.
- Via stitching follows the rectangular outline only, places through vias and
  skips positions that already hold a via. It does not test the new vias against
  existing copper, use a polygon outline or choose a net by itself.

### Routing

- The router is a deterministic spanning-tree router. `style: "orthogonal"`
  (the default) tries four L-shaped candidates per edge and reports an edge as
  unrouted rather than approximating a path.
- `style: "chamfer45"` tries paths made of an axis-aligned segment, a 45-degree
  diagonal and another axis-aligned segment, on each configured layer. The
  diagonal keeps both axis directions of the edge, so its angle is exactly 45
  degrees and the whole path stays inside the pair's axis-aligned bounding box.
  Two orientations and three diagonal shares per layer let a blocked diagonal
  slide along the edge.
- A chamfered edge falls back to an orthogonal L path when the diagonal has no
  length (the edge is axis-aligned), lies outside the rectangular outline, or no
  variant satisfies copper, pad, via and keepout clearance. The report then
  marks `fallback` as `diagonal-length-zero`, `diagonal-outside-outline` or
  `chamfer-clearance`. When the orthogonal fallback also fails, the edge stays
  unrouted.
- `chamfer45` adds `style` and an `edges` array to the report. Each edge carries
  `net`, `from`, `to`, `routed`, `style`, `fallback`, `reason` and `segments`;
  each segment carries its two points, layer, width and style. The default
  report keeps its historical shape and adds no field.
- The router still does not rip up, route multi-layer inner copper, or use
  arbitrary angles, curves or arcs. Keepout avoidance therefore appears as an
  orthogonal fallback or an unrouted edge when no candidate circumvents the
  region. Chamfered segments are ordinary tracks, so DRC and clearance checks
  read them exactly like orthogonal copper.
- Net-class routing is opt-in. With `useNetClasses` (CLI `--netclass`) each net
  uses its declared class width, clearance and via diameter; a missing class or
  class field falls back to the explicit option and then to the built-in
  default. Without the option the explicit arguments apply, so existing calls
  produce the same copper. When enabled the report adds `widthSource`
  (`netclass`, `argument` or `default`) to each net and unrouted edge.
- Defaults are width 0.25, clearance 0.2 and via diameter 0.6 board units.

### Fabrication and assembly

- Gerber writes one file per copper layer plus `F_SilkS`, `B_SilkS`, `F_Mask`,
  `B_Mask`, `F_Paste`, `B_Paste` and `Edge_Cuts`. Gerber export needs an outline.
- Every Gerber file carries `%TF.FileFunction`, `%TF.Part`, `%TF.FilePolarity`
  and `%TA.AperFunction`. Copper files also write object attributes: tracks and
  vias declare `%TO.N`, pads declare `%TO.N`, `%TO.C` and `%TO.P`, and every
  object block closes with `%TD*%`. Pour regions carry no `%TO.N`, and no part
  data beyond the component reference and pin is written. Attribute values
  replace Gerber-reserved separators (`,`, `*`, `%`) with `_`.
- Rectangular pad apertures are not rotated in Gerber output.
- Solder-mask openings are rectangular, follow the pad's outer copper side and
  add the document's expansion per side. Vias stay tented and inner-layer pads
  open no mask. Solder-paste windows are rectangular and equal to the outer pad
  size; there is no paste expansion rule and mask-defined shapes are not written.
- Pad drills are stored as one circular diameter per pad and must be smaller than
  the pad's short side; a violating drill is rejected with `pad/invalid-drill`.
  `annular-ring` evaluates vias and drilled pads with the same minimum ring rule,
  and `hole-to-hole` compares every via and drilled-pad hole by net, so a pad
  without a drill contributes to neither check. `hole-to-track` measures those
  same holes against every track of another net, and `hole-to-pad` measures them
  against every pad box of another net. The Excellon file and drill map
  list via, pad and routed-slot holes; a pad without a drill contributes nothing
  to those files.
- A micro via is a typed via, not a smaller drill: it must bridge two adjacent
  copper layers and at least one end must be `F.Cu` or `B.Cu`. Other spans are
  rejected with `via/micro-span`, and the drill report, Excellon file and drill
  map treat the via as an ordinary plated hole.
- Plated routed slots live in `.opcb` and reach the Excellon file and drill map
  only. Gerber, IPC-2581, KiCad PCB, SVG, PDF and PNG output do not carry them,
  and the router and DRC checks do not treat a slot as copper or as a hole.
- Back-drill depth and stub live in `.opcb` and IPC-2581 and appear in the
  Excellon file as `;Back drill` comments and as dashed rings in the drill map.
  KiCad PCB import and export keep the via but not the back-drill information.
- Copper pours are stored as closed polygons and written as `G36`/`G37` regions
  in their layer's Gerber file and in the SVG/PDF previews. KiCad PCB and
  IPC-2581 exchange do not carry them, and the PNG raster omits the fill.
- The copper pour audit is a connectivity measure, not a fill calculation. A
  pour is connected when its polygon touches a same-net pad box, via circle or
  track segment on its own layer; a pour with no anchor is `isolated`, and a
  pour with no net is `unassigned`. Each stored polygon is measured alone, so
  adjacent same-net polygons are not merged into one island, and pour copper
  that only reaches same-net copper through another pour is reported isolated.
- The fabrication manifest is a dry run of the Gerber writer. It lists the files
  the writer plans, in writer order, with the object counts that writer would
  draw, and flags a missing outline, duplicate file names, empty copper layers,
  missing silkscreen layers, orphan mask or paste sides, missing drill data and
  missing footprints. It writes no file, measures no fabrication process and
  does not compare the layer set against a specific stackup. Assembly and drill
  counts describe the CSV and Excellon writers, not a machine program.
- `placement` writes millimetre JSON; `pos` keeps its CSV columns and appends the
  footprint layer. Neither writer invents pad geometry or rotations.
- IPC-D-356 writes the header, `327` pad records, `317`/`307` via records and the
  `999` terminator with fixed 80-column lines. It writes no trace records,
  board-outline records or test channel assignments.

### Checks and policy

- `via-in-pad` is a manufacturability warning, not an electrical error.
- `pad-overlap` compares rotated pad rectangles on one layer. Two pads of
  different nets that share copper report `drc/pad-overlap`; a shared edge is
  contact, not overlap, so tangent pads in one footprint stay clean. A pad with
  no net and a pad with one count as different nets.
- `solder-mask` reads the apertures the mask writer implies: each outer pad's
  rotated copper box grown by the document expansion per side. Openings of
  different nets that overlap or touch report `drc/solder-mask-bridge`, and a
  web below 0.1 mm reports `drc/solder-mask-sliver`; same-net openings and inner
  copper are not compared. The Gerber mask writer still emits an unrotated
  rectangle per pad, so a rotated pad is measured by the check's rotated box
  rather than by the written aperture.
- `stencil` reads the paste windows the paste writer implies: each outer-layer
  pad's rectangle sized to the copper pad, with no expansion. The check measures
  the area ratio and the short-side aspect ratio in millimetres against the
  stencil design rules; both findings are warnings. Inner copper and vias open
  no paste and are skipped, and the pad's stored rectangle is measured directly,
  so footprint rotation does not change either ratio. The model has no round or
  polygon pad aperture, so an odd-shaped opening cannot occur.
- `diffpair` infers pairs from net naming only: one shared base name with `_P`
  and `_N` (or `+` and `-`). Length is the sum of straight track segments, so
  via barrels and copper inside pads are not counted. An inferred pair uses the
  fixed 0.5 mm skew limit because it has no declared target.
- The net-length report sums straight track segments in document units, so a
  routed net reports its exact centreline length. It counts the vias assigned to
  a net without measuring their barrels, lists only declared nets, and reports
  zeros for a declared net with no copper. Alongside the declaration-ordered
  `nets` array it ranks the same nets by length in `ranking`, longest first;
  equal rounded lengths fall back to the net name, so the order is deterministic.
- The copper balance report sums feature areas, so overlapping copper counts once
  per feature rather than once as a union. Coverage and imbalance need a
  rectangular outline; without one the report still lists per-layer areas.
- The net audit measures clearance only between track segments of two different
  declared nets on a shared copper layer, using the edge-to-edge gap and the
  strictest class clearance of the pair or the 0.2 mm baseline. Pads, vias and
  pours take no part, a track on a non-copper layer is ignored, and copper whose
  net is not declared is not measured. Width conformance compares every track on
  a declared net with that class's width; a net without a class width reports
  `none`. The report is read-only and never edits the board.
- The test point coverage report repeats the `testpoints` classification per
  declared net: a net is covered only by a declared probe on one of its pads or
  vias, a connectable net without a probe is a warning, and a net with no pad and
  no via is an error. A probe whose net is not declared still appears in the
  probe list and sets `unassignedProbes`; it covers no net row. `coveragePercent`
  divides covered nets by connectable nets and is null when none is connectable.
- A waiver silences a finding by code and, when set, by object id; it never hides
  the finding from the report. Overrides apply before waivers, so `passed` is
  false only while a non-waived error remains. There is no waiver expiry or
  reviewer approval workflow.
- Teardrops are linear only and become ordinary copper tracks, so they are
  indistinguishable from hand-routed segments after export.

### Stackup and impedance

- The stackup stores one copper thickness, one dielectric height and one
  permittivity. It describes one homogeneous dielectric, not a per-layer stackup.
- Differential structures are edge-coupled only; there is no coplanar,
  broadside-coupled or asymmetric pair.
- The IPC-2141 closed forms replace a field solver. The differential correction
  matches measured boards best for `0.1 <= w / h <= 3` and `0.1 <= s / h <= 3`;
  outside that range it is still computed but is an estimate.
- Width conversion rejects a target outside the formula's reachable range.
- The impedance report measures every track on a declared net. `auto` (the
  default) uses microstrip on `F.Cu` and `B.Cu` and stripline on inner copper; a
  declared structure applies to every row, and a differential name is measured
  as the single-ended geometry behind it in a net row. A net without copper
  tracks is omitted while every declared pair is listed. The report uses the
  stored stackup unless the request overrides it, and it is read-only.

### Silkscreen and text

- Silkscreen text is a stored item with text, layer, position, rotation, size and
  thickness. `text add|update|delete|list` and `Board.addText()` manage it. The
  SVG and PDF writers draw it, and the PNG raster draws it with a built-in
  5 × 7 font; the Gerber silkscreen files and IPC-2581 writer do not.
- Every preview outlines each footprint with a silkscreen rectangle around its
  pad bounding box and prints the reference designator above it. The outline is
  derived from the pads, not read from the footprint file.
- The `silkscreen` check warns `silkscreen/no-reference-text` when no text item
  matches a footprint reference, and still checks pad placement against the
  outline.
- Silkscreen legend geometry can also be stored as tracks on silkscreen layers.
  The SVG and PDF writers draw them, Gerber emits them into `F_SilkS` and
  `B_SilkS`, and `silkscreen-over-pad` reads them. DRC treats a silkscreen track
  as legend rather than a copper violation.
- `silkscreen-over-pad` also boxes each silkscreen text item from its anchor
  (`0.6 × size` per character to the right, one `size` above and below) and
  reports a legend that shares interior area with a pad on the same side. The
  box ignores the stored rotation, matching the previews' fit. A shared edge is
  contact, not overlap, so tangent silk and pads stay clean.
- Footprint graphics are not stored. KiCad import reports them with
  `kicad/footprint-graphics`.

### Keepouts and test points

- Keepouts are design intent: the `keepout` check reads them and the router
  rejects every track and via that would enter one (track centreline plus half
  width, via diameter). They are not written to Gerber, drill data, IPC-2581 or
  KiCad output. The area and perimeter report measures declared polygons, not
  copper actually excluded.
- The test point report lists only explicitly declared pads and vias. It does not
  infer probes from nets or create panel or fixture geometry.
- The `testpoints` check treats a net as covered only by a declared test point on
  one of its pads or vias; copper geometry never implies a probe. A connectable
  net with no declared test point warns `testpoints/net-not-covered`. A declared
  net with neither a pad nor a via has no physical contact point, so it errors
  `testpoints/net-unconnected`. A net that appears only on tracks or copper
  pours has no contact point either, so it reports the same error.

### Document lifecycle

- `blank` is the only template.
- Undo history is not stored inside `.opcb`; it is per-handle or in a sidecar.
- There is no merge of concurrent edits against an older revision.
