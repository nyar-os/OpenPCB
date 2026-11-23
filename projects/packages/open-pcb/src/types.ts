/**
 * Authoring types of `@wae/open-pcb`.
 *
 * This is the authoring surface: `new OpenPCB(options?)` creates a session,
 * `create`/`open` return an editable `Board`, and every board and layer carries
 * a stable ID plus a revision.
 *
 * Board layout lives in the native engine: copper layers, stackup, footprints,
 * nets, tracks, vias and the fabrication or assembly projections all run in
 * `open-pcb-model`. This package maps values and owns argument validation.
 */

import type { BoardFormat, BoardIntent } from './formats.js';

/** Opaque board identity. */
export type BoardId = string;
/** Opaque layer identity; the index is order only. */
export type LayerId = string;
/** Opaque object identity. */
export type ObjectId = string;

/** Hard limits applied before work starts. */
export interface ResourceBudgets {
    readonly maxDocumentBytes?: number;
    readonly maxObjects?: number;
    readonly maxLayers?: number;
}

/** Structured diagnostic emitted during import, inspection, render or fabrication export. */
export interface BoardDiagnostic {
    readonly code: string;
    readonly severity: 'info' | 'warning' | 'error';
    readonly revision: number;
    readonly layerId?: LayerId;
    readonly objectId?: ObjectId;
    readonly recoverable: boolean;
    readonly message: string;
}

/** Where undo history lives. `memory` keeps it per handle; `sidecar` persists it beside the document. */
export type HistoryMode = 'memory' | 'sidecar';

/** Session options. Constructing a session opens no window, file or socket. */
export interface OpenPCBOptions {
    /** BCP-47 tag used for diagnostics and default copy. Defaults to `en`. */
    readonly locale?: string;
    /** Undo history storage. Defaults to `memory`; `sidecar` writes `<path>.history.json`. */
    readonly history?: HistoryMode;
    readonly budgets?: ResourceBudgets;
    readonly diagnostics?: (event: BoardDiagnostic) => void;
}

/** Disclosure level. `partial` always ships a loss path and diagnostic codes. */
export type SupportLevel = 'supported' | 'partial' | 'unsupported';

/** Severity of one diagnostic or inspection finding. */
export type Severity = 'info' | 'warning' | 'error';

/** Via type as classified by the copper span it covers. */
export type ViaKind = 'through' | 'blind' | 'buried' | 'micro';

/** Board checks understood by `board.inspect()`: layout, stackup and connectivity rules. */
export type InspectCheck =
    | 'drc'
    | 'connectivity'
    | 'clearance'
    | 'annular_rings'
    | 'annular-ring'
    | 'track-width'
    | 'unrouted'
    | 'stackup'
    | 'outline'
    | 'silkscreen'
    | 'courtyard'
    | 'netlist'
    | 'hole-to-hole'
    | 'hole-to-track'
    | 'hole-to-pad'
    | 'copper-to-edge'
    | 'edge-clearance'
    | 'silkscreen-over-pad'
    | 'diffpair'
    | 'dangling-copper'
    | 'netclass'
    | 'keepout'
    | 'via-in-pad'
    | 'testpoints';

/** Rectangular board outline in board units. */
export interface BoardOutline {
    readonly from: readonly [number, number];
    readonly to: readonly [number, number];
}

/** Layer role in the stackup. */
export type LayerKind = 'copper' | 'silkscreen' | 'soldermask' | 'assembly' | 'mechanical';

/** Coordinate unit of one board document. */
export type BoardUnits = 'mm' | 'mil';

/** Design-rule values the inspection checks and writers read, in millimetres. */
export interface BoardRules {
    /** Minimum gap between two drill holes of different nets. */
    readonly holeClearance: number;
    /** Minimum copper-to-board-edge gap. */
    readonly copperToEdge: number;
    /** Minimum component-to-board-edge gap. */
    readonly componentToEdge: number;
    /** Minimum drill-hole-to-board-edge gap. */
    readonly holeToEdge: number;
    /** Solder-mask expansion around each pad, per side. */
    readonly solderMaskExpansion: number;
}

/** Physical layer stack used by controlled-impedance calculations. */
export interface BoardStackup {
    /** Copper foil thickness, in millimetres. */
    readonly copperThickness: number;
    /** Dielectric thickness between adjacent copper layers, in millimetres. */
    readonly dielectricHeight: number;
    /** Relative permittivity of the dielectric; at least 1. */
    readonly dielectricConstant: number;
}

/** Trace geometry accepted by the impedance helpers. */
export type ImpedanceStructure = 'microstrip' | 'stripline' | 'diffMicrostrip' | 'diffStripline';

/** Options of `board.impedance()`. */
export interface ImpedanceOptions {
    /** Trace geometry; defaults to `microstrip`. */
    readonly structure?: ImpedanceStructure;
    /** Trace width, in millimetres. */
    readonly width: number;
    /** Edge-to-edge gap of a differential pair, in millimetres; required for a pair. */
    readonly gap?: number;
    /** Overrides the stored dielectric height. */
    readonly dielectricHeight?: number;
    /** Overrides the stored dielectric constant. */
    readonly dielectricConstant?: number;
    /** Overrides the stored copper thickness. */
    readonly copperThickness?: number;
}

/** Result of `board.impedance()`. */
export interface ImpedanceReport {
    readonly revision: number;
    readonly structure: ImpedanceStructure;
    readonly width: number;
    /** Single-ended impedance, or differential impedance for a pair, in ohms. */
    readonly impedance: number;
    /** Edge gap of a pair, in millimetres; absent for a single trace. */
    readonly gap?: number;
    /** Odd-mode impedance of each half of a pair, in ohms. */
    readonly oddImpedance?: number;
    /** Even-mode impedance of each half of a pair, in ohms. */
    readonly evenImpedance?: number;
    readonly copperThickness: number;
    readonly dielectricHeight: number;
    readonly dielectricConstant: number;
}

/** Options of `board.impedanceWidth()`. */
export interface ImpedanceWidthOptions {
    /** Trace geometry; defaults to `microstrip`. */
    readonly structure?: ImpedanceStructure;
    /** Target single-ended or differential impedance, in ohms. */
    readonly impedance: number;
    /** Edge-to-edge gap of a differential pair, in millimetres; required for a pair. */
    readonly gap?: number;
    /** Overrides the stored dielectric height. */
    readonly dielectricHeight?: number;
    /** Overrides the stored dielectric constant. */
    readonly dielectricConstant?: number;
    /** Overrides the stored copper thickness. */
    readonly copperThickness?: number;
}

/** Result of `board.impedanceWidth()`. */
export interface ImpedanceWidthReport {
    readonly revision: number;
    readonly structure: ImpedanceStructure;
    readonly impedance: number;
    /** Trace width that reaches the target, in millimetres. */
    readonly width: number;
    /** Edge gap of a pair, in millimetres; absent for a single trace. */
    readonly gap?: number;
    /** Odd-mode impedance of each half of a pair, in ohms. */
    readonly oddImpedance?: number;
    /** Even-mode impedance of each half of a pair, in ohms. */
    readonly evenImpedance?: number;
    readonly copperThickness: number;
    readonly dielectricHeight: number;
    readonly dielectricConstant: number;
}

/** Options of `board.impedanceTable()`. */
export interface ImpedanceTableOptions {
    /** Trace geometry; defaults to `microstrip`. */
    readonly structure?: ImpedanceStructure;
    /** Target single-ended or differential impedances in ohms; defaults to `[50]`. */
    readonly targets?: readonly number[];
    /** Edge-to-edge gap of a differential pair, in millimetres; required for a pair. */
    readonly gap?: number;
    /** Overrides the stored dielectric height. */
    readonly dielectricHeight?: number;
    /** Overrides the stored dielectric constant. */
    readonly dielectricConstant?: number;
    /** Overrides the stored copper thickness. */
    readonly copperThickness?: number;
}

/** One target impedance and the trace width that reaches it. */
export interface ImpedanceTableRow {
    readonly targetImpedance: number;
    /** Suggested trace width, in millimetres. */
    readonly width: number;
    /** Impedance recomputed from the suggested width, in ohms. */
    readonly impedance: number;
    /** Odd-mode impedance of each half of a pair, in ohms. */
    readonly oddImpedance?: number;
    /** Even-mode impedance of each half of a pair, in ohms. */
    readonly evenImpedance?: number;
}

/** Result of `board.impedanceTable()`. */
export interface ImpedanceTableReport {
    readonly revision: number;
    readonly units: 'mm';
    readonly structure: ImpedanceStructure;
    readonly targets: number;
    /** Edge gap of a pair, in millimetres; absent for a single trace. */
    readonly gap?: number;
    readonly copperThickness: number;
    readonly dielectricHeight: number;
    readonly dielectricConstant: number;
    readonly rows: readonly ImpedanceTableRow[];
}

/** Options of `board.impedanceReport()`. */
export interface ImpedanceReportOptions {
    /** Trace structure; `auto` (default) selects the geometry from each copper layer. */
    readonly structure?: ImpedanceStructure | 'auto';
    /** Overrides the stored dielectric height. */
    readonly dielectricHeight?: number;
    /** Overrides the stored dielectric constant. */
    readonly dielectricConstant?: number;
    /** Overrides the stored copper thickness. */
    readonly copperThickness?: number;
}

/** One declared net and the impedance range of its copper tracks. */
export interface ImpedanceNetRow {
    readonly net: string;
    readonly tracks: number;
    /** Trace widths of the measured tracks, in millimetres. */
    readonly widthMin: number;
    readonly widthMax: number;
    /** Impedances of the measured tracks, in ohms. */
    readonly impedanceMin: number;
    readonly impedanceMax: number;
    /** Structures used by the measured tracks, sorted. */
    readonly structures: readonly string[];
}

/** One declared differential pair and its computed pair impedances. */
export interface ImpedancePairRow {
    readonly id: string;
    readonly positive: string;
    readonly negative: string;
    /** Target trace width, in millimetres. */
    readonly width: number;
    /** Target edge-to-edge gap, in millimetres. */
    readonly gap: number;
    readonly structure: string;
    readonly impedance: number;
    /** Odd-mode impedance of each half, in ohms. */
    readonly oddImpedance?: number;
    /** Even-mode impedance of each half, in ohms. */
    readonly evenImpedance?: number;
}

/** Result of `board.impedanceReport()`. */
export interface ImpedanceSummaryReport {
    readonly revision: number;
    readonly units: 'mm';
    /** The requested structure, or `auto` when the geometry follows each layer. */
    readonly structure: ImpedanceStructure | 'auto';
    readonly copperThickness: number;
    readonly dielectricHeight: number;
    readonly dielectricConstant: number;
    readonly netCount: number;
    readonly pairCount: number;
    readonly nets: readonly ImpedanceNetRow[];
    readonly pairs: readonly ImpedancePairRow[];
}

/** Copper area of one copper layer, in square millimetres. */
export interface CopperBalanceLayer {
    readonly layer: LayerId;
    /** Total feature area; overlapping copper is counted once per feature. */
    readonly area: number;
    readonly tracks: number;
    readonly vias: number;
    readonly pads: number;
    readonly zones: number;
    /** Area divided by the rectangular board area; `null` without an outline. */
    readonly coveragePercent: number | null;
}

/** Result of `board.copperBalance()`. Areas are in square millimetres. */
export interface CopperBalanceReport {
    readonly revision: number;
    readonly units: 'mm2';
    /** Rectangular board area; `null` without an outline. */
    readonly boardArea: number | null;
    readonly totalArea: number;
    readonly layers: readonly CopperBalanceLayer[];
    readonly minCoveragePercent: number | null;
    readonly maxCoveragePercent: number | null;
    /** Difference between the highest and lowest layer coverage, in percentage points. */
    readonly imbalancePercent: number | null;
}

/** One declared net in the routed-length report. Lengths use document units. */
export interface NetLengthEntry {
    readonly name: string;
    readonly tracks: number;
    /** Summed track centreline length, in document units. */
    readonly length: number;
    readonly vias: number;
}

/** Totals across every declared net in the routed-length report. */
export interface NetLengthTotals {
    readonly tracks: number;
    /** Summed track centreline length, in document units. */
    readonly length: number;
    readonly vias: number;
}

/** One declared net in the longest-net ranking. Lengths use document units. */
export interface NetLengthRankingEntry {
    readonly name: string;
    /** Summed track centreline length, in document units. */
    readonly length: number;
}

/** Result of `board.netLengths()`. */
export interface NetLengthsReport {
    readonly report: 'open-pcb-net-lengths';
    readonly revision: number;
    readonly nets: readonly NetLengthEntry[];
    /** The same nets ordered by length, longest first, ties by name. */
    readonly ranking: readonly NetLengthRankingEntry[];
    readonly totals: NetLengthTotals;
}

/** Class targets that apply to one net, in millimetres; null when the net has no class value. */
export interface NetAuditTargets {
    readonly width: number | null;
    readonly clearance: number | null;
    readonly viaDiameter: number | null;
    readonly viaDrill: number | null;
}

/** Measured track-width range of one net, in millimetres. */
export interface NetAuditWidth {
    readonly min: number;
    readonly max: number;
}

/** Narrowest measured clearance from one net to another. */
export interface NetAuditClearance {
    readonly to: string;
    readonly actual: number;
    readonly required: number;
    readonly ok: boolean;
}

/** One declared net in the net audit. */
export interface NetAuditNet {
    readonly name: string;
    readonly class: string | null;
    readonly targets: NetAuditTargets;
    readonly tracks: number;
    readonly width: NetAuditWidth | null;
    readonly vias: number;
    readonly viaDiameter: number | null;
    readonly viaDrill: number | null;
    readonly pads: number;
    readonly widthStatus: 'pass' | 'below' | 'none';
    readonly clearance: NetAuditClearance | null;
}

/** One measured track pair between two declared nets. */
export interface NetAuditPair {
    readonly a: string;
    readonly b: string;
    readonly actual: number;
    readonly required: number;
    readonly ok: boolean;
    readonly layer: LayerId;
    readonly aTrack: ObjectId;
    readonly bTrack: ObjectId;
}

/** One net audit finding. */
export interface NetAuditIssue {
    readonly code: string;
    readonly severity: 'info' | 'warning' | 'error';
    readonly objectId: string;
    readonly net: string;
    readonly other: string | null;
    readonly layer: string | null;
    readonly message: string;
}

/** Result of `board.netAudit()`. */
export interface NetAuditReport {
    readonly report: 'open-pcb-net-audit';
    readonly revision: number;
    readonly units: 'mm';
    readonly status: 'pass' | 'warn' | 'error';
    readonly netCount: number;
    readonly classCount: number;
    readonly pairCount: number;
    readonly nets: readonly NetAuditNet[];
    readonly pairs: readonly NetAuditPair[];
    readonly issues: readonly NetAuditIssue[];
}

/** One declared net in the test point coverage report. */
export interface TestPointCoverageNet {
    readonly name: string;
    readonly pads: number;
    readonly vias: number;
    readonly probes: number;
    readonly layers: readonly LayerId[];
    readonly covered: boolean;
}

/** One declared test point in the coverage report. */
export interface TestPointCoverageProbe {
    readonly id: ObjectId;
    readonly target: string;
    readonly kind: 'pad' | 'via';
    readonly net: string | null;
    readonly layer: LayerId;
    /** Reference designator for a pad; an empty string for a via. */
    readonly designator: string;
    /** Pad number for a pad; an empty string for a via. */
    readonly pad: string;
    readonly at: readonly [number, number];
}

/** One test point coverage finding. */
export interface TestPointCoverageIssue {
    readonly code: string;
    readonly severity: 'info' | 'warning' | 'error';
    readonly objectId: string;
    readonly message: string;
}

/** Result of `board.testpointCoverage()`. */
export interface TestPointCoverageReport {
    readonly report: 'open-pcb-testpoint-coverage';
    readonly revision: number;
    readonly units: 'mm';
    readonly status: 'pass' | 'warn' | 'error';
    readonly netCount: number;
    readonly coveredCount: number;
    readonly uncoveredCount: number;
    readonly unconnectedCount: number;
    readonly probeCount: number;
    readonly unassignedProbes: number;
    /** Covered share of connectable nets, in percent; null when no net is connectable. */
    readonly coveragePercent: number | null;
    readonly nets: readonly TestPointCoverageNet[];
    readonly probes: readonly TestPointCoverageProbe[];
    readonly issues: readonly TestPointCoverageIssue[];
}

/** One copper pour area entry of a layer. */
export interface ZoneAreaLayer {
    readonly layer: LayerId;
    readonly zones: number;
    /** Enclosed pour area, in square millimetres. */
    readonly area: number;
}

/** One copper pour region with its measured geometry. */
export interface ZoneAreaEntry {
    readonly id: ObjectId;
    readonly layer: LayerId;
    readonly net: string | null;
    readonly vertices: number;
    /** Enclosed pour area, in square millimetres. */
    readonly area: number;
    /** Polygon perimeter, in millimetres. */
    readonly perimeter: number;
}

/** Result of `board.zoneReport()`. */
export interface ZoneAreaReport {
    readonly revision: number;
    readonly units: 'mm2';
    readonly count: number;
    readonly totalArea: number;
    readonly layers: readonly ZoneAreaLayer[];
    readonly zones: readonly ZoneAreaEntry[];
}

/** One keepout region with its measured geometry. */
export interface KeepoutAreaEntry {
    readonly id: ObjectId;
    /** Copper layer id, or `*` for every copper layer. */
    readonly layer: string;
    readonly vertices: number;
    /** Enclosed keepout area, in square millimetres. */
    readonly area: number;
    /** Polygon perimeter, in millimetres. */
    readonly perimeter: number;
}

/** Result of `board.keepoutReport()`. */
export interface KeepoutAreaReport {
    readonly revision: number;
    readonly units: 'mm2';
    readonly count: number;
    readonly totalArea: number;
    readonly items: readonly KeepoutAreaEntry[];
}

/** One drill tool and the hits it drives. */
export interface DrillTool {
    /** Stable tool number, ordered by ascending diameter. */
    readonly tool: number;
    /** Finished diameter, in millimetres. */
    readonly diameter: number;
    readonly holes: number;
    readonly slots: number;
}

/** One via hole in the drill report. */
export interface DrillHole {
    readonly id: ObjectId;
    readonly net: string | null;
    readonly tool: number;
    /** Hole centre, in millimetres. */
    readonly at: readonly [number, number];
    /** Finished drill diameter, in millimetres. */
    readonly diameter: number;
    readonly kind: ViaKind;
    readonly backDrilled: boolean;
    readonly fromLayer?: LayerId;
    readonly toLayer?: LayerId;
}

/** One plated routed slot in the drill report. */
export interface DrillSlot {
    readonly id: ObjectId;
    readonly net: string | null;
    readonly tool: number;
    /** First centreline end, in millimetres. */
    readonly from: readonly [number, number];
    /** Second centreline end, in millimetres. */
    readonly to: readonly [number, number];
    /** Slot width, equal to the tool diameter, in millimetres. */
    readonly width: number;
    /** Centreline length, in millimetres. */
    readonly length: number;
}

/** Result of `board.drillReport()`. */
export interface DrillReport {
    readonly revision: number;
    readonly units: 'mm';
    readonly tools: readonly DrillTool[];
    readonly holeCount: number;
    readonly slotCount: number;
    readonly backDrilled: number;
    readonly vias: Readonly<Record<'through' | 'blind' | 'buried' | 'micro', number>>;
    readonly holes: readonly DrillHole[];
    readonly slots: readonly DrillSlot[];
}

/** One file the Gerber writer plans to emit. */
export interface FabManifestFile {
    /** File name inside the Gerber directory. */
    readonly name: string;
    /** Gerber X2 `%TF.FileFunction` value the writer declares. */
    readonly function: string;
    readonly kind: 'copper' | 'silkscreen' | 'soldermask' | 'paste' | 'profile';
    /** Source copper or silkscreen layer id; `null` for the profile. */
    readonly layer: string | null;
    /** Objects the file would draw: tracks, pads, vias or pours. */
    readonly objects: number;
}

/** Drill tools and hits the Excellon file would carry. */
export interface FabManifestDrill {
    readonly tools: number;
    readonly holes: number;
    readonly slots: number;
    readonly padHoles: number;
}

/** Assembly rows the pick-and-place and BOM writers would emit. */
export interface FabManifestAssembly {
    readonly placements: number;
    readonly bomGroups: number;
}

/** One manifest consistency finding. */
export interface FabManifestIssue {
    readonly code: string;
    readonly severity: 'info' | 'warning' | 'error';
    /** File name or layer the finding names; empty for a board-level finding. */
    readonly objectId: string;
    readonly message: string;
}

/** Result of `board.fabManifest()`. Nothing is written. */
export interface FabManifestReport {
    readonly report: 'open-pcb-fab-manifest';
    readonly revision: number;
    readonly units: 'mm';
    readonly status: 'pass' | 'warn' | 'error';
    /** Whether the board has an outline; Gerber export needs one. */
    readonly outline: boolean;
    readonly fileCount: number;
    /** Copper layer ids in stack order. */
    readonly copperLayers: readonly string[];
    readonly files: readonly FabManifestFile[];
    readonly drill: FabManifestDrill;
    readonly assembly: FabManifestAssembly;
    readonly issues: readonly FabManifestIssue[];
}

/** Objects that connect one pour to its net. */
export interface ZoneAuditAnchors {
    readonly pads: number;
    readonly vias: number;
    readonly tracks: number;
}

/** One copper pour region in the connectivity audit. */
export interface ZoneAuditEntry {
    readonly id: ObjectId;
    readonly layer: LayerId;
    readonly net: string | null;
    readonly vertices: number;
    /** Enclosed pour area, in square millimetres. */
    readonly area: number;
    /** Polygon perimeter, in millimetres. */
    readonly perimeter: number;
    /** Same-net copper the pour touches on its own layer. */
    readonly anchors: ZoneAuditAnchors;
    readonly status: 'connected' | 'isolated' | 'unassigned';
}

/** Per-layer totals of the copper pour audit. */
export interface ZoneAuditLayer {
    readonly layer: LayerId;
    readonly zones: number;
    readonly connected: number;
    readonly isolated: number;
    readonly unassigned: number;
    /** Enclosed pour area, in square millimetres. */
    readonly area: number;
    readonly isolatedArea: number;
}

/** One pour audit finding. */
export interface ZoneAuditIssue {
    readonly code: string;
    readonly severity: 'info' | 'warning' | 'error';
    readonly objectId: string;
    readonly layer: string;
    readonly net: string | null;
    readonly message: string;
}

/** Result of `board.zoneAudit()`. */
export interface ZoneAuditReport {
    readonly report: 'open-pcb-zone-audit';
    readonly revision: number;
    readonly units: 'mm2';
    readonly status: 'pass' | 'warn';
    readonly zoneCount: number;
    readonly connectedCount: number;
    readonly isolatedCount: number;
    readonly unassignedCount: number;
    readonly totalArea: number;
    readonly isolatedArea: number;
    readonly layers: readonly ZoneAuditLayer[];
    readonly zones: readonly ZoneAuditEntry[];
    readonly issues: readonly ZoneAuditIssue[];
}

/** Tab style used between adjacent boards in a panel. */
export type PanelTabStyle = 'v-cut' | 'mouse-bite' | 'none';

/** Options of `board.panelize()`. Lengths are in millimetres. */
export interface PanelizeOptions {
    /** Boards across one row; at least one. */
    readonly columns: number;
    /** Rows of boards; at least one. */
    readonly rows: number;
    /** Gap between neighbouring columns; defaults to 0. */
    readonly gapX?: number;
    /** Gap between neighbouring rows; defaults to 0. */
    readonly gapY?: number;
    /** Border around the array; defaults to 0. */
    readonly rail?: number;
    /** Tab style; defaults to `v-cut`. */
    readonly tab?: PanelTabStyle;
    /** Length of one mouse-bite tab; defaults to 5. */
    readonly tabWidth?: number;
    /** Distance between tab centres; defaults to 20. */
    readonly tabSpacing?: number;
    /** Finished diameter of the four rail tooling holes; omit for none. */
    readonly toolingHole?: number;
    /** Distance from the panel edge to a tooling-hole centre; defaults to half the rail. */
    readonly toolingHoleInset?: number;
}

/** One board placement inside a panel, in millimetres. */
export interface PanelPlacement {
    readonly column: number;
    readonly row: number;
    readonly from: readonly [number, number];
    readonly to: readonly [number, number];
}

/** One internal seam a `v-cut` scores, in millimetres. */
export interface PanelSeam {
    readonly orientation: 'vertical' | 'horizontal';
    readonly at: number;
    readonly from: readonly [number, number];
    readonly to: readonly [number, number];
    readonly length: number;
}

/** One mouse-bite tab placeholder over a gap, in millimetres. */
export interface PanelTab {
    readonly orientation: 'vertical' | 'horizontal';
    readonly x: number;
    readonly y: number;
    readonly width: number;
    readonly height: number;
}

/** One rail tooling hole, in millimetres. */
export interface PanelToolingHole {
    readonly corner: 'bottom-left' | 'bottom-right' | 'top-right' | 'top-left';
    readonly x: number;
    readonly y: number;
    readonly diameter: number;
}

/** Panel area that is removed before assembly, in square millimetres. */
export interface PanelBreakaway {
    /** Panel area minus every board area; `railArea + gapArea`. */
    readonly area: number;
    /** Share of the panel area that is breakaway, in percent. */
    readonly percent: number;
    /** Border area around the board array. */
    readonly railArea: number;
    /** Area of the gaps between neighbouring boards. */
    readonly gapArea: number;
}

/** Result of `board.panelize()`. Lengths are in millimetres and areas in square millimetres. */
export interface PanelizeReport {
    readonly revision: number;
    readonly units: 'mm';
    readonly columns: number;
    readonly rows: number;
    readonly boardCount: number;
    readonly gapX: number;
    readonly gapY: number;
    readonly rail: number;
    readonly boardSize: { readonly width: number; readonly height: number };
    readonly panelSize: { readonly width: number; readonly height: number };
    readonly panelOutline: {
        readonly from: readonly [number, number];
        readonly to: readonly [number, number];
    };
    readonly boardArea: number;
    readonly panelArea: number;
    readonly utilizationPercent: number;
    readonly breakaway: PanelBreakaway;
    readonly toolingHoles: readonly PanelToolingHole[];
    readonly toolingHoleCount: number;
    readonly tab: PanelTabStyle;
    readonly seams: readonly PanelSeam[];
    readonly tabs: readonly PanelTab[];
    readonly tabCount: number;
    readonly placements: readonly PanelPlacement[];
}

/** Render targets declared for preview surfaces. */
export type RenderFormat = 'svg' | 'png' | 'pdf';

/** Fabrication, assembly and exchange targets `board.export()` declares. */
export type ExportTarget =
    | 'kicad_pcb'
    | 'gbr'
    | 'drl'
    | 'drill_map'
    | 'pos'
    | 'placement'
    | 'bom'
    | 'testpoints'
    | 'ipc_d_356'
    | 'step'
    | 'ipc2581'
    | 'pdf'
    | 'svg'
    | 'png';

/** What the installed engine can do, per format. */
export interface FormatCapability {
    readonly import: SupportLevel;
    readonly edit: SupportLevel;
    readonly export: SupportLevel;
    /** Codes reported when a `partial` path loses something. */
    readonly lossCodes: readonly string[];
}

/** Result of `OpenPCB.capabilities()`. Read this instead of assuming a method works. */
export interface BoardCapabilities {
    readonly engine: { readonly package: string; readonly version: string };
    readonly project: {
        readonly import: SupportLevel;
        readonly edit: SupportLevel;
        readonly export: SupportLevel;
    };
    readonly formats: Readonly<Record<BoardFormat, FormatCapability>>;
    readonly checks: Readonly<Record<InspectCheck, SupportLevel>>;
    readonly renderers: Readonly<Record<RenderFormat, SupportLevel>>;
    readonly exportTargets: Readonly<Record<ExportTarget, SupportLevel>>;
    /** Whether each via kind can be placed and exported. */
    readonly vias: Readonly<Record<ViaKind, SupportLevel>>;
    /** Whether differential pairs can be declared and checked. */
    readonly differentialPairs: SupportLevel;
    /** Whether back-drill information can be stored and exported. */
    readonly backDrill: SupportLevel;
    /** Whether each teardrop shape can be generated. */
    readonly teardrops: Readonly<Record<'linear' | 'arc', SupportLevel>>;
    /** Whether design-rule values can be read and edited. */
    readonly designRules: SupportLevel;
    /** Whether Gerber solder-mask layers are written. */
    readonly solderMask: SupportLevel;
    /** Whether Gerber solder-paste stencil layers are written. */
    readonly solderPaste: SupportLevel;
    /** Whether Gerber files carry X2 file, aperture and polarity attributes. */
    readonly gerberX2: SupportLevel;
    /** Whether findings can be waived and their severity overridden. */
    readonly drcPolicy: SupportLevel;
    /** Whether `board.route()` ships in this engine. */
    readonly routing: SupportLevel;
    /** Whether copper pour regions can be declared, previewed and written as Gerber regions. */
    readonly zones: SupportLevel;
    /** Whether silkscreen text items can be declared and written to KiCad and the preview. */
    readonly texts: SupportLevel;
    /** Whether a copper pour area and perimeter report can be produced. */
    readonly zoneReport: SupportLevel;
    /** Whether a copper pour connectivity and isolated-island report can be produced. */
    readonly zoneAudit: SupportLevel;
    /** Whether a fabrication file manifest with consistency findings can be produced. */
    readonly fabManifest: SupportLevel;
    /** Whether a drill tool and hole report can be produced. */
    readonly drillReport: SupportLevel;
    /** Whether copper keepout regions can be declared and checked. */
    readonly keepouts: SupportLevel;
    /** Whether stitching via fences can be generated along the board outline. */
    readonly viaStitching: SupportLevel;
    /** Whether plated routed slots can be declared and written to drill data. */
    readonly slots: SupportLevel;
    /** Whether a copper area and balance report can be produced. */
    readonly copperBalance: SupportLevel;
    /** Whether a routed copper length report per declared net can be produced. */
    readonly netLengths: SupportLevel;
    /** Whether a net class, width and clearance conformance report can be produced. */
    readonly netAudit: SupportLevel;
    /** Whether a rectangular board can be arrayed into a panel with seams or tabs. */
    readonly panelization: SupportLevel;
    /** Whether a polygonal board outline can be validated, measured and offset. */
    readonly polygonOutline: SupportLevel;
    /** Whether net classes with clearance and width targets are available. */
    readonly netClasses: SupportLevel;
    /** Whether `board.route()` can follow per-net class width, clearance and via targets. */
    readonly routingNetClasses: SupportLevel;
    /** Which path styles `board.route()` can produce. */
    readonly routingStyles: Readonly<Record<RouteStyle, SupportLevel>>;
    /** Whether declared test points can be reported. */
    readonly testPoints: SupportLevel;
    /** Whether per-net test point coverage can be reported. */
    readonly testPointCoverage: SupportLevel;
    /** Whether fiducial marks can be declared and managed. */
    readonly fiducials: SupportLevel;
    /** Whether a fiducial assembly report can be produced. */
    readonly fiducialReport: SupportLevel;
    /** Whether a physical stackup can be declared and read. */
    readonly stackup: SupportLevel;
    /** Whether microstrip and stripline impedance and width conversion are available. */
    readonly impedance: SupportLevel;
    /** Whether a target-impedance trace-width table can be produced. */
    readonly impedanceTable: SupportLevel;
    /** Whether a per-net and per-pair impedance report can be produced. */
    readonly impedanceReport: SupportLevel;
}

/** Options of `OpenPCB.create()`. */
export interface BoardCreateOptions {
    readonly title?: string;
    readonly template?: string;
}

/**
 * Input of `OpenPCB.open()`.
 *
 * Exactly one of `path` or `project` must be present, so the engine never
 * guesses whether a string is a file name or serialized content.
 */
export type BoardInput =
    | { readonly path: string; readonly intent?: BoardIntent }
    | { readonly project: string; readonly intent?: BoardIntent };

/** One layer of a board summary. */
export interface BoardSummaryLayer {
    readonly id: LayerId;
    readonly index: number;
    readonly title: string;
    readonly kind: LayerKind;
    readonly visible: boolean;
    readonly locked: boolean;
    readonly objects: number;
}

/** Object counts of one board. */
export interface BoardSummaryCounts {
    readonly nets: number;
    readonly footprints: number;
    readonly tracks: number;
    readonly vias: number;
    readonly slots: number;
    readonly zones: number;
    readonly keepouts: number;
    readonly texts: number;
    readonly netClasses: number;
    readonly testPoints: number;
    readonly fiducials: number;
    readonly differentialPairs: number;
    readonly waivers: number;
}

/** One waived inspection finding. */
export interface BoardWaiver {
    readonly id: ObjectId;
    /** Finding code the waiver silences. */
    readonly code: string;
    /** Object id the waiver is narrowed to; absent means every object. */
    readonly object?: ObjectId;
    /** Why the finding was accepted. */
    readonly reason?: string;
}

/** One declared net class and its routing targets, in board units. */
export interface BoardNetClass {
    readonly id: ObjectId;
    readonly name: string;
    readonly width?: number;
    readonly clearance?: number;
    readonly viaDiameter?: number;
    readonly viaDrill?: number;
    readonly nets: readonly string[];
}

/** Summary of one board at its committed revision. */
export interface BoardSummary {
    readonly id: BoardId;
    readonly revision: number;
    readonly title: string | null;
    readonly units: BoardUnits;
    readonly outline: BoardOutline | null;
    /** Design-rule values the checks and writers read. */
    readonly rules: BoardRules;
    /** Physical stackup, when one has been declared. */
    readonly stackup: BoardStackup | null;
    /** Declared waivers. */
    readonly waivers: readonly BoardWaiver[];
    /** Severity overrides keyed by finding code. */
    readonly severityOverrides: Readonly<Record<string, Severity>>;
    /** Declared net classes. */
    readonly netClasses: readonly BoardNetClass[];
    readonly layers: readonly BoardSummaryLayer[];
    readonly counts: BoardSummaryCounts;
    /** Whether {@link Board.undo} can restore a snapshot. */
    readonly canUndo: boolean;
    /** Whether {@link Board.redo} can reapply a snapshot. */
    readonly canRedo: boolean;
}

/** One finding of an inspection run. */
export interface InspectIssue {
    readonly code: string;
    readonly severity: Severity;
    readonly revision: number;
    readonly layerId?: LayerId;
    readonly objectId?: ObjectId;
    readonly recoverable: boolean;
    readonly message: string;
    /** True when a declared waiver silenced this finding. */
    readonly waived?: boolean;
    /** Waiver that matched the finding. */
    readonly waiverId?: ObjectId;
    /** Reason carried by the matching waiver. */
    readonly waiverReason?: string;
}

/** Result of `board.inspect()`. Rules only, never an aesthetic score. */
export interface InspectReport {
    readonly revision: number;
    readonly checkerVersion: string;
    /** True when no non-waived error remains. */
    readonly passed: boolean;
    /** Findings that remain after waivers. */
    readonly active: number;
    /** Findings silenced by a matching waiver. */
    readonly waived: number;
    readonly issues: readonly InspectIssue[];
}

/** Options of `board.inspect()`. */
export interface InspectOptions {
    readonly checks?: readonly InspectCheck[];
}

/** Options of `board.save()` (native document, not an exchange format). */
export interface SaveOptions {
    readonly target?: string;
    readonly overwrite?: boolean;
}

/** Result of `board.save()`. */
export interface SaveReport {
    readonly path: string;
    readonly revision: number;
    readonly bytes: number;
}

/** Options of `board.export()`. */
export interface ExportOptions {
    readonly format: ExportTarget;
    readonly target: string;
    readonly overwrite?: boolean;
}

/** One file written by a multi-file export such as Gerber. */
export interface ExportArtifact {
    readonly name: string;
    readonly bytes: number;
}

/** Result of `board.export()`. Reports what the target actually kept. */
export interface ExportReport {
    readonly revision: number;
    readonly format: ExportTarget;
    /** File path, or the directory for multi-file targets such as `gbr`. */
    readonly path: string;
    /** Total bytes written; for a directory, the sum of its artifacts. */
    readonly bytes: number;
    /** False for projections such as PDF that cannot be reopened for editing. */
    readonly editable: boolean;
    /** Set by directory targets; each file the writer produced. */
    readonly artifacts?: readonly ExportArtifact[];
}

/** One construct an import could not map into the native document. */
export interface ImportLoss {
    /** Stable code, also listed in the format's `lossCodes`. */
    readonly code: string;
    /** Human-readable detail naming the source construct. */
    readonly message: string;
}

/** Result of `OpenPCB.importKicadPcb()`: the board, the source path and the loss ledger. */
export interface KicadImportResult {
    /** The imported document. The session owns its handle until it is closed. */
    readonly board: Board;
    readonly path: string;
    readonly bytes: number;
    readonly losses: readonly ImportLoss[];
    readonly lossCodes: readonly string[];
    /** Present when the import was written to `target`. */
    readonly saved?: SaveReport;
}

/** Options of `OpenPCB.importKicadPcb()`. */
export interface KicadImportOptions {
    /** KiCad PCB file to read. */
    readonly path: string;
    /** Native `.opcb` document to write after import; the board keeps it as its target. */
    readonly target?: string;
    readonly overwrite?: boolean;
}

/** Exchange board formats the engine can read into an editable document. */
export type BoardImportFormat = 'kicad_pcb' | 'ipc2581';

/** Options of `OpenPCB.importBoard()`. */
export interface BoardImportOptions {
    /** KiCad PCB or IPC-2581 file to read. The engine detects the format from the content. */
    readonly path: string;
    /** Native `.opcb` document to write after import; the board keeps it as its target. */
    readonly target?: string;
    readonly overwrite?: boolean;
}

/** Result of `OpenPCB.importBoard()`: the board, the detected format and the loss ledger. */
export interface BoardImportResult extends KicadImportResult {
    /** Format the engine detected and read. */
    readonly format: BoardImportFormat;
}

/** Low-level result of the native import operation: handle, document summary and loss ledger. */
export interface ImportReport {
    readonly handle: number;
    readonly format: BoardImportFormat;
    readonly path: string;
    readonly bytes: number;
    readonly revision: number;
    readonly editable: true;
    readonly document: BoardSummary;
    readonly losses: readonly ImportLoss[];
    readonly lossCodes: readonly string[];
}

/** Opaque public shape of one created board object. */
export type BoardObject = Readonly<Record<string, unknown>>;

/** Options of `board.addFootprint()`. */
export interface AddFootprintOptions {
    /** Reference designator; must be unique on the board. */
    readonly reference: string;
    readonly value?: string;
    readonly footprint?: string;
    readonly x?: number;
    readonly y?: number;
    readonly rotation?: number;
    /** Layer id; defaults to `F.Cu`. */
    readonly layer?: string;
}

/** Input of `board.addPad()`. */
export interface AddPadInput {
    /** Footprint id that owns the pad, for example `fp-1`. */
    readonly footprint: ObjectId;
    /** Pad number or name, unique within the footprint. */
    readonly number: string;
    /** Pad centre, X, in board units. */
    readonly x: number;
    /** Pad centre, Y, in board units. */
    readonly y: number;
    /** Pad width, in board units. */
    readonly w: number;
    /** Pad height, in board units. */
    readonly h: number;
    /** Declared net name, when assigned. */
    readonly net?: string;
    /** Finished circular drill diameter, in board units. */
    readonly drill?: number;
}

/** Options of `board.addTrack()`. */
export interface AddTrackOptions {
    /** Declared net name, when assigned. */
    readonly net?: string;
    readonly layer: string;
    readonly width: number;
    readonly from: readonly [number, number];
    readonly to: readonly [number, number];
}

/** Options of `board.addVia()`. */
export interface AddViaOptions {
    /** Declared net name, when assigned. */
    readonly net?: string;
    readonly at: readonly [number, number];
    readonly diameter: number;
    readonly drill: number;
    /**
     * Copper layer the span starts on. Omit both span fields for a through via;
     * set both for a blind or buried via.
     */
    readonly fromLayer?: string;
    /** Copper layer the span ends on; must sit below `fromLayer` in the stack. */
    readonly toLayer?: string;
    /**
     * Explicit via type. Only `micro` is accepted: the span must bridge two
     * adjacent copper layers with one end on `F.Cu` or `B.Cu`. Omit it to let
     * the span decide through, blind or buried.
     */
    readonly kind?: 'micro';
}

/** Options of `board.addSlot()`. */
export interface AddSlotOptions {
    /** Declared net name, when assigned. */
    readonly net?: string;
    /** One end of the slot centreline. */
    readonly from: readonly [number, number];
    /** The other end of the slot centreline. */
    readonly to: readonly [number, number];
    /** Tool diameter, equal to the cut width; greater than zero. */
    readonly width: number;
}

/** Options of `board.updateSlot()`. At least one editable field is required. */
export interface UpdateSlotOptions {
    /** Slot id to update, for example `slot-1`. */
    readonly id: ObjectId;
    /** New declared net name. */
    readonly net?: string;
    /** New first endpoint of the slot centreline. */
    readonly from?: readonly [number, number];
    /** New second endpoint of the slot centreline. */
    readonly to?: readonly [number, number];
    /**
     * New tool diameter, equal to the cut width, in millimetres. It changes the
     * Excellon and drill-map input only; no other geometry is recomputed.
     */
    readonly width?: number;
}

/** Options of `board.stitchVias()`. */
export interface StitchViasOptions {
    /** Declared net the stitching vias join. */
    readonly net: string;
    /** Target spacing between adjacent vias along the perimeter. */
    readonly spacing: number;
    /** Inset from the board edge; defaults to 0.5. */
    readonly inset?: number;
    /** Via copper diameter; defaults to 0.6. */
    readonly diameter?: number;
    /** Via finished drill; defaults to 0.3. */
    readonly drill?: number;
}

/** Options of `board.addZone()`. */
export interface AddZoneOptions {
    /** Copper layer id that receives the pour. */
    readonly layer: string;
    /** Declared net the pour belongs to, when assigned. */
    readonly net?: string;
    /** Closed polygon in board units, in drawing order; at least three points. */
    readonly points: readonly (readonly [number, number])[];
}

/** Options of `board.updateZone()`. At least one field is required. */
export interface UpdateZoneOptions {
    readonly id: ObjectId;
    /** New copper layer id. */
    readonly layer?: string;
    /** New declared net name. */
    readonly net?: string;
    /** New closed polygon in board units; at least three points. */
    readonly points?: readonly (readonly [number, number])[];
}

/** One silkscreen text item. */
export interface BoardText {
    readonly id: ObjectId;
    readonly text: string;
    readonly layer: LayerId;
    /** Anchor position in board units. */
    readonly at: readonly [number, number];
    /** Rotation in degrees, counter-clockwise. */
    readonly rotation: number;
    /** Glyph height in board units. */
    readonly size: number;
    /** Stroke thickness in board units. */
    readonly thickness: number;
}

/** Options of `board.addText()`. */
export interface AddTextOptions {
    /** Text content; must not be blank. */
    readonly text: string;
    /** Silkscreen layer id, for example `F.SilkS`. */
    readonly layer: string;
    /** Anchor position in board units. */
    readonly at: readonly [number, number];
    /** Rotation in degrees; defaults to 0. */
    readonly rotation?: number;
    /** Glyph height in board units; defaults to 1. */
    readonly size?: number;
    /** Stroke thickness in board units; defaults to 0.15. */
    readonly thickness?: number;
}

/** Options of `board.updateText()`. At least one field is required. */
export interface UpdateTextOptions {
    readonly id: ObjectId;
    readonly text?: string;
    readonly layer?: string;
    readonly at?: readonly [number, number];
    readonly rotation?: number;
    readonly size?: number;
    readonly thickness?: number;
}

/** Result of `board.listTexts()`. Positions use the board unit. */
export interface TextListReport {
    readonly revision: number;
    readonly units: BoardUnits;
    readonly count: number;
    readonly items: readonly BoardText[];
}

/** Options of `board.addKeepout()`. */
export interface AddKeepoutOptions {
    /** Copper layer id, or `*` for every copper layer; defaults to `*`. */
    readonly layer?: string;
    /** Closed polygon in board units, in drawing order; at least three points. */
    readonly points: readonly (readonly [number, number])[];
}

/** Options of `board.updateKeepout()`. At least one field is required. */
export interface UpdateKeepoutOptions {
    /** Keepout id to update. */
    readonly id: ObjectId;
    /** New copper layer id, or `*` for every copper layer. */
    readonly layer?: string;
    /** New closed polygon in board units, in drawing order; at least three points. */
    readonly points?: readonly (readonly [number, number])[];
}

/** Options of `board.addNetClass()`. */
export interface AddNetClassOptions {
    /** Class name, unique on the board. */
    readonly name: string;
    /** Target trace width, in board units. */
    readonly width?: number;
    /** Minimum copper clearance for nets in the class, in board units. */
    readonly clearance?: number;
    /** Minimum via copper diameter, in board units. */
    readonly viaDiameter?: number;
    /** Minimum via finished drill, in board units. */
    readonly viaDrill?: number;
}

/** Options of `board.assignNetClass()`. */
export interface AssignNetClassOptions {
    /** Declared net name to assign. */
    readonly net: string;
    /** Net class id or name. */
    readonly class: string;
}

/** Options of `board.addTestPoint()`. */
export interface AddTestPointOptions {
    /** Pad key `footprint-id:pad-number` or via id. */
    readonly target: string;
}

/** Options of `board.addFiducial()`. */
export interface AddFiducialOptions {
    /** Fiducial layer: `F.Fid` or `B.Fid`. */
    readonly layer: string;
    /** Mark centre in board units. */
    readonly at: readonly [number, number];
    /** Mark diameter in board units; greater than zero. */
    readonly diameter: number;
    /** Optional human label. */
    readonly name?: string;
}

/** One fiducial mark in board units. */
export interface BoardFiducial {
    readonly id: ObjectId;
    readonly layer: string;
    /** Mark centre in board units. */
    readonly at: readonly [number, number];
    /** Mark diameter in board units. */
    readonly diameter: number;
    readonly name: string | null;
}

/** Result of `board.listFiducials()`. */
export interface FiducialListReport {
    readonly revision: number;
    readonly units: BoardUnits;
    readonly count: number;
    readonly items: readonly BoardFiducial[];
}

/** One finding of the fiducial assembly report. */
export interface FiducialIssue {
    readonly code: string;
    readonly severity: 'info' | 'warning' | 'error';
    readonly objectId: string;
    readonly message: string;
}

/** One fiducial mark measured in millimetres. */
export interface FiducialReportEntry {
    readonly id: ObjectId;
    readonly layer: string;
    /** Mark centre, in millimetres. */
    readonly at: readonly [number, number];
    /** Mark diameter, in millimetres. */
    readonly diameter: number;
    readonly name: string | null;
    /** Gap from the mark edge to the nearest board edge, in millimetres;
     *  null when the board has no outline. */
    readonly edgeGap: number | null;
}

/** Result of `board.fiducialReport()`. */
export interface FiducialReport {
    readonly revision: number;
    readonly units: 'mm';
    readonly count: number;
    /** Marks on `F.Fid`. */
    readonly front: number;
    /** Marks on `B.Fid`. */
    readonly back: number;
    /** Copper-to-edge rule the edge gaps are checked against, in millimetres. */
    readonly edgeRule: number;
    readonly status: 'pass' | 'warn' | 'error';
    readonly issues: readonly FiducialIssue[];
    readonly items: readonly FiducialReportEntry[];
}

/** Back-drill geometry of one through via, in board units. */
export interface BackDrill {
    /** Barrel depth removed from the secondary side; greater than zero. */
    readonly depth: number;
    /** Remaining stub length after back drilling; not negative. */
    readonly stub: number;
}

/** Options of `board.setViaBackDrill()`. */
export interface SetViaBackDrillOptions {
    /** Via id; it must be a through via. */
    readonly id: ObjectId;
    /** Removed barrel depth, in board units; greater than zero. */
    readonly depth?: number;
    /** Remaining stub length, in board units; not negative. */
    readonly stub?: number;
}

/** Options of `board.addNet()`. */
export interface AddNetOptions {
    readonly name: string;
}

/**
 * Options of `board.addTeardrops()`.
 *
 * Teardrops are materialised as ordinary copper tracks, so they follow every
 * export and inspection path. Only the `linear` shape is implemented.
 */
export interface AddTeardropsOptions {
    /** Teardrop shape; defaults to and only accepts `linear`. */
    readonly shape?: 'linear';
    /** Length from the pad edge to the teardrop tip, in board units. */
    readonly length?: number;
}

/**
 * Options of `board.addDifferentialPair()`.
 *
 * Lengths are in board units. Declaring a pair routes nothing; the `diffpair`
 * check compares the routed copper against these targets.
 */
export interface AddDifferentialPairOptions {
    /** Net routed as the positive half; must be declared. */
    readonly positive: string;
    /** Net routed as the negative half; must be declared and differ from `positive`. */
    readonly negative: string;
    /** Target trace width. */
    readonly width: number;
    /** Target edge-to-edge gap between the halves. */
    readonly gap: number;
    /** Largest allowed route-length difference between the halves; defaults to 0. */
    readonly maxSkew?: number;
}

/**
 * Path style of `board.route()`.
 *
 * `orthogonal` emits axis-aligned L paths only and is the default. `chamfer45`
 * joins two axis-aligned segments with a 45-degree diagonal and falls back to
 * an orthogonal path for an edge when no clear diagonal fits.
 */
export type RouteStyle = 'orthogonal' | 'chamfer45';

/**
 * Options of `board.route()`.
 *
 * Lengths are in board units. The router is deterministic: the same board and
 * the same options always produce the same copper.
 */
export interface RouteOptions {
    /** Copper layers in preference order; defaults to `["F.Cu", "B.Cu"]`. */
    readonly layers?: readonly string[];
    /** Trace width; defaults to 0.25. */
    readonly width?: number;
    /** Clearance kept from existing copper; defaults to 0.2. */
    readonly clearance?: number;
    /** Via copper diameter; defaults to 0.6. */
    readonly viaDiameter?: number;
    /** Via finished drill; defaults to 0.3. */
    readonly viaDrill?: number;
    /** Route only this declared net. */
    readonly net?: string;
    /** Path style; defaults to `orthogonal`. */
    readonly style?: RouteStyle;
    /**
     * Use each net's declared class for width, clearance and via values.
     * Defaults to false; a class value wins per field, then the argument, then
     * the built-in default.
     */
    readonly useNetClasses?: boolean;
}

/** One spanning-tree edge the router could not place. */
export interface UnroutedConnection {
    readonly net: string;
    /** Stable pad key `footprint-id:pad-number`. */
    readonly from: string;
    /** Stable pad key `footprint-id:pad-number`. */
    readonly to: string;
    readonly reason: string;
    /** Where the edge's width came from; set when `useNetClasses` is true. */
    readonly widthSource?: 'netclass' | 'argument' | 'default';
}

/** One straight copper segment of a routed spanning edge. */
export interface RouteSegment {
    readonly from: readonly [number, number];
    readonly to: readonly [number, number];
    readonly layer: LayerId;
    readonly width: number;
    /** Style this segment belongs to; `chamfer45` marks the diagonal and its axis neighbours. */
    readonly style: RouteStyle | null;
}

/** One spanning edge of the `chamfer45` report. */
export interface RouteEdge {
    readonly net: string;
    /** Stable pad key `footprint-id:pad-number`. */
    readonly from: string;
    /** Stable pad key `footprint-id:pad-number`. */
    readonly to: string;
    readonly routed: boolean;
    /** Style actually used; `null` when the edge stayed unrouted. */
    readonly style: RouteStyle | null;
    /** Why an orthogonal path replaced the diagonal; `null` when a diagonal was used. */
    readonly fallback: string | null;
    /** Reason the edge stayed unrouted; `null` when it was routed. */
    readonly reason: string | null;
    readonly segments: readonly RouteSegment[];
}

/** Result of `board.route()`. `tracks` and `vias` count appended objects. */
export interface RouteReport {
    readonly routed: number;
    readonly unrouted: readonly UnroutedConnection[];
    readonly tracks: number;
    readonly vias: number;
    /** Present only for a `chamfer45` run; the default report omits it. */
    readonly style?: RouteStyle;
    /** Present only for a `chamfer45` run; one entry per spanning edge. */
    readonly edges?: readonly RouteEdge[];
}

/** Options of `board.moveFootprint()`. At least one field is required. */
export interface MoveFootprintOptions {
    readonly id: ObjectId;
    readonly x?: number;
    readonly y?: number;
    readonly rotation?: number;
    readonly layer?: LayerId;
}

/** Options of `board.updateFootprint()`. */
export interface UpdateFootprintOptions {
    readonly id: ObjectId;
    readonly value?: string;
    readonly footprint?: string;
}

/** Input of `board.updatePad()`. At least one editable field is required. */
export interface UpdatePadInput {
    /** Footprint id that owns the pad, for example `fp-1`. */
    readonly footprint: ObjectId;
    /** Pad number or name to update. */
    readonly number: string;
    /** New pad centre, X, in board units. */
    readonly x?: number;
    /** New pad centre, Y, in board units. */
    readonly y?: number;
    /** New pad width, in board units. */
    readonly w?: number;
    /** New pad height, in board units. */
    readonly h?: number;
    /** New declared net name. */
    readonly net?: string;
    /** New finished circular drill diameter, in board units. */
    readonly drill?: number;
}

/** Options of `board.updateTrack()`. At least one editable field is required. */
export interface UpdateTrackOptions {
    /** Track id to update, for example `track-1`. */
    readonly id: ObjectId;
    /** New declared net name. */
    readonly net?: string;
    /** New copper layer id. */
    readonly layer?: string;
    /** New trace width, in board units. */
    readonly width?: number;
    /** New segment start. */
    readonly from?: readonly [number, number];
    /** New segment end. */
    readonly to?: readonly [number, number];
}

/** Options of `board.splitTrack()`. */
export interface SplitTrackOptions {
    /** Track id to split, for example `track-1`. */
    readonly id: ObjectId;
    /** Split point X in board units; must be finite. */
    readonly x: number;
    /** Split point Y in board units; must be finite. */
    readonly y: number;
}

/** Options of `board.updateVia()`. At least one editable field is required. */
export interface UpdateViaOptions {
    /** Via id to update, for example `via-1`. */
    readonly id: ObjectId;
    /** New declared net name. */
    readonly net?: string;
    /** New via centre. */
    readonly at?: readonly [number, number];
    /** New copper diameter, in board units. */
    readonly diameter?: number;
    /** New finished drill diameter, in board units. */
    readonly drill?: number;
    /** Mark the via as a micro via; its existing span must already be legal. */
    readonly kind?: 'micro';
}

/** Input of `board.deletePad()`. */
export interface DeletePadInput {
    /** Footprint id that owns the pad, for example `fp-1`. */
    readonly footprint: ObjectId;
    /** Pad number or name to delete. */
    readonly number: string;
}

/** Options of `board.addLayer()`. */
export interface AddLayerOptions {
    /** Layer name, unique on the board and used as the layer id. */
    readonly name: string;
    /** Layer kind; defaults to `copper`. */
    readonly kind?: LayerKind;
    /** CSS colour; defaults to the kind colour. */
    readonly color?: string;
}

/** Options of `board.updateLayer()`. A rename also rewrites every reference. */
export interface UpdateLayerOptions {
    readonly id: LayerId;
    readonly name?: string;
    readonly kind?: LayerKind;
    readonly color?: string;
    readonly visible?: boolean;
    readonly locked?: boolean;
}

/** Options of `board.setOutline()`. */
export interface SetOutlineOptions {
    readonly from: readonly [number, number];
    readonly to: readonly [number, number];
}

/** Options of `board.outlineGeometry()`. Polygon vertices are in board units. */
export interface OutlineGeometryOptions {
    /** Closed polygon vertices; omitted measures the stored rectangular outline. */
    readonly points?: ReadonlyArray<readonly [number, number]>;
    /** Outward offset in board units; negative moves inward. */
    readonly offset?: number;
}

/** One offset polygon produced by `board.outlineGeometry()`. Lengths are in millimetres. */
export interface OutlineOffsetReport {
    /** Signed offset distance; positive was outward. */
    readonly distance: number;
    readonly vertexCount: number;
    readonly area: number;
    readonly perimeter: number;
    readonly points: ReadonlyArray<readonly [number, number]>;
}

/** Result of `board.outlineGeometry()`. Lengths are in millimetres and areas in square millimetres. */
export interface OutlineGeometryReport {
    readonly revision: number;
    readonly units: 'mm';
    /** Whether the polygon came from the request or the stored rectangular outline. */
    readonly source: 'request' | 'outline';
    readonly vertexCount: number;
    readonly orientation: 'counter-clockwise' | 'clockwise';
    readonly area: number;
    readonly perimeter: number;
    readonly points: ReadonlyArray<readonly [number, number]>;
    /** Offset polygon; `null` when no offset was requested. */
    readonly offset: OutlineOffsetReport | null;
}

/** Options of `board.setRules()`. At least one value is required. */
export interface SetRulesOptions {
    readonly holeClearance?: number;
    readonly copperToEdge?: number;
    readonly componentToEdge?: number;
    readonly holeToEdge?: number;
    readonly solderMaskExpansion?: number;
}

/** Options of `board.setStackup()`. */
export interface SetStackupOptions {
    /** Copper foil thickness, in millimetres. */
    readonly copperThickness: number;
    /** Dielectric thickness between adjacent copper layers, in millimetres. */
    readonly dielectricHeight: number;
    /** Relative permittivity of the dielectric; at least 1. */
    readonly dielectricConstant: number;
}

/** Options of `board.addWaiver()`. */
export interface AddWaiverOptions {
    /** Finding code to silence, such as `drc/track-width-too-small`. */
    readonly code: string;
    /** Object id to narrow the waiver to; omit to cover every object. */
    readonly object?: ObjectId;
    /** Why the finding was accepted. */
    readonly reason?: string;
}

/** Options of `board.setSeverity()`. */
export interface SetSeverityOptions {
    /** Finding code whose severity is overridden. */
    readonly code: string;
    readonly severity: Severity;
}

/** Result of one editing call. */
export interface BoardEditResult {
    readonly revision: number;
    readonly board: BoardSummary;
    readonly object?: BoardObject;
    /** Set by delete operations. */
    readonly deleted?: ObjectId;
}

/** Result of `board.splitTrack()`. */
export interface SplitTrackResult extends BoardEditResult {
    readonly canUndo: boolean;
    readonly canRedo: boolean;
    /** The original track, now ending at the split point. */
    readonly first: BoardObject;
    /** The new `track-N` segment from the split point to the original end. */
    readonly second: BoardObject;
}

/** An open board document. Every operation keeps its own stable code. */
export interface Board {
    readonly id: BoardId;
    readonly revision: number;
    readonly summary: BoardSummary;
    readonly canUndo: boolean;
    readonly canRedo: boolean;
    undo(): Promise<BoardEditResult>;
    redo(): Promise<BoardEditResult>;
    inspect(options?: InspectOptions): Promise<InspectReport>;
    save(options?: SaveOptions): Promise<SaveReport>;
    export(options: ExportOptions): Promise<ExportReport>;
    addFootprint(options: AddFootprintOptions): Promise<BoardEditResult>;
    deleteFootprint(id: ObjectId): Promise<BoardEditResult>;
    moveFootprint(options: MoveFootprintOptions): Promise<BoardEditResult>;
    updateFootprint(options: UpdateFootprintOptions): Promise<BoardEditResult>;
    addPad(input: AddPadInput): Promise<BoardEditResult>;
    updatePad(input: UpdatePadInput): Promise<BoardEditResult>;
    deletePad(input: DeletePadInput): Promise<BoardEditResult>;
    addTrack(options: AddTrackOptions): Promise<BoardEditResult>;
    updateTrack(options: UpdateTrackOptions): Promise<BoardEditResult>;
    splitTrack(options: SplitTrackOptions): Promise<SplitTrackResult>;
    deleteTrack(id: ObjectId): Promise<BoardEditResult>;
    addVia(options: AddViaOptions): Promise<BoardEditResult>;
    updateVia(options: UpdateViaOptions): Promise<BoardEditResult>;
    addSlot(options: AddSlotOptions): Promise<BoardEditResult>;
    updateSlot(options: UpdateSlotOptions): Promise<BoardEditResult>;
    deleteSlot(id: ObjectId): Promise<BoardEditResult>;
    stitchVias(options: StitchViasOptions): Promise<BoardEditResult>;
    deleteVia(id: ObjectId): Promise<BoardEditResult>;
    addZone(options: AddZoneOptions): Promise<BoardEditResult>;
    updateZone(options: UpdateZoneOptions): Promise<BoardEditResult>;
    deleteZone(id: ObjectId): Promise<BoardEditResult>;
    zoneReport(): Promise<ZoneAreaReport>;
    zoneAudit(): Promise<ZoneAuditReport>;
    fabManifest(): Promise<FabManifestReport>;
    addText(options: AddTextOptions): Promise<BoardEditResult>;
    updateText(options: UpdateTextOptions): Promise<BoardEditResult>;
    deleteText(id: ObjectId): Promise<BoardEditResult>;
    listTexts(): Promise<TextListReport>;
    drillReport(): Promise<DrillReport>;
    addKeepout(options: AddKeepoutOptions): Promise<BoardEditResult>;
    updateKeepout(options: UpdateKeepoutOptions): Promise<BoardEditResult>;
    deleteKeepout(id: ObjectId): Promise<BoardEditResult>;
    keepoutReport(): Promise<KeepoutAreaReport>;
    addNetClass(options: AddNetClassOptions): Promise<BoardEditResult>;
    deleteNetClass(id: ObjectId): Promise<BoardEditResult>;
    assignNetClass(options: AssignNetClassOptions): Promise<BoardEditResult>;
    addTestPoint(options: AddTestPointOptions): Promise<BoardEditResult>;
    deleteTestPoint(id: ObjectId): Promise<BoardEditResult>;
    addFiducial(options: AddFiducialOptions): Promise<BoardEditResult>;
    deleteFiducial(id: ObjectId): Promise<BoardEditResult>;
    listFiducials(): Promise<FiducialListReport>;
    fiducialReport(): Promise<FiducialReport>;
    setViaBackDrill(options: SetViaBackDrillOptions): Promise<BoardEditResult>;
    addNet(options: AddNetOptions): Promise<BoardEditResult>;
    addTeardrops(options?: AddTeardropsOptions): Promise<BoardEditResult>;
    addDifferentialPair(options: AddDifferentialPairOptions): Promise<BoardEditResult>;
    deleteDifferentialPair(id: ObjectId): Promise<BoardEditResult>;
    route(options?: RouteOptions): Promise<RouteReport>;
    addLayer(options: AddLayerOptions): Promise<BoardEditResult>;
    updateLayer(options: UpdateLayerOptions): Promise<BoardEditResult>;
    setOutline(options: SetOutlineOptions): Promise<BoardEditResult>;
    outlineGeometry(options?: OutlineGeometryOptions): Promise<OutlineGeometryReport>;
    setRules(options: SetRulesOptions): Promise<BoardEditResult>;
    setStackup(options: SetStackupOptions): Promise<BoardEditResult>;
    impedance(options: ImpedanceOptions): Promise<ImpedanceReport>;
    impedanceWidth(options: ImpedanceWidthOptions): Promise<ImpedanceWidthReport>;
    impedanceTable(options?: ImpedanceTableOptions): Promise<ImpedanceTableReport>;
    impedanceReport(options?: ImpedanceReportOptions): Promise<ImpedanceSummaryReport>;
    copperBalance(): Promise<CopperBalanceReport>;
    netLengths(): Promise<NetLengthsReport>;
    netAudit(): Promise<NetAuditReport>;
    testpointCoverage(): Promise<TestPointCoverageReport>;
    panelize(options: PanelizeOptions): Promise<PanelizeReport>;
    addWaiver(options: AddWaiverOptions): Promise<BoardEditResult>;
    deleteWaiver(id: ObjectId): Promise<BoardEditResult>;
    setSeverity(options: SetSeverityOptions): Promise<BoardEditResult>;
    deleteSeverity(code: string): Promise<BoardEditResult>;
    list(): Promise<BoardSummary>;
    close(): Promise<void>;
}

