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
    | 'diffpair'
    | 'dangling-copper';

/** Rectangular board outline in board units. */
export interface BoardOutline {
    readonly from: readonly [number, number];
    readonly to: readonly [number, number];
}

/** Layer role in the stackup. */
export type LayerKind = 'copper' | 'silkscreen' | 'soldermask' | 'assembly' | 'mechanical';

/** Coordinate unit of one board document. */
export type BoardUnits = 'mm' | 'mil';

/** Render targets declared for preview surfaces. */
export type RenderFormat = 'svg' | 'png' | 'pdf';

/** Fabrication, assembly and exchange targets `board.export()` declares. */
export type ExportTarget = 'kicad_pcb' | 'gbr' | 'drl' | 'pos' | 'bom' | 'step' | 'ipc2581' | 'pdf' | 'svg' | 'png';

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
    /** Whether `board.route()` ships in this engine. */
    readonly routing: SupportLevel;
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
    readonly differentialPairs: number;
}

/** Summary of one board at its committed revision. */
export interface BoardSummary {
    readonly id: BoardId;
    readonly revision: number;
    readonly title: string | null;
    readonly units: BoardUnits;
    readonly outline: BoardOutline | null;
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
    readonly severity: 'info' | 'warning' | 'error';
    readonly revision: number;
    readonly layerId?: LayerId;
    readonly objectId?: ObjectId;
    readonly recoverable: boolean;
    readonly message: string;
}

/** Result of `board.inspect()`. Rules only, never an aesthetic score. */
export interface InspectReport {
    readonly revision: number;
    readonly checkerVersion: string;
    readonly passed: boolean;
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
}

/** One spanning-tree edge the router could not place. */
export interface UnroutedConnection {
    readonly net: string;
    /** Stable pad key `footprint-id:pad-number`. */
    readonly from: string;
    /** Stable pad key `footprint-id:pad-number`. */
    readonly to: string;
    readonly reason: string;
}

/** Result of `board.route()`. `tracks` and `vias` count appended objects. */
export interface RouteReport {
    readonly routed: number;
    readonly unrouted: readonly UnroutedConnection[];
    readonly tracks: number;
    readonly vias: number;
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

/** Result of one editing call. */
export interface BoardEditResult {
    readonly revision: number;
    readonly board: BoardSummary;
    readonly object?: BoardObject;
    /** Set by delete operations. */
    readonly deleted?: ObjectId;
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
    addTrack(options: AddTrackOptions): Promise<BoardEditResult>;
    deleteTrack(id: ObjectId): Promise<BoardEditResult>;
    addVia(options: AddViaOptions): Promise<BoardEditResult>;
    deleteVia(id: ObjectId): Promise<BoardEditResult>;
    setViaBackDrill(options: SetViaBackDrillOptions): Promise<BoardEditResult>;
    addNet(options: AddNetOptions): Promise<BoardEditResult>;
    addDifferentialPair(options: AddDifferentialPairOptions): Promise<BoardEditResult>;
    deleteDifferentialPair(id: ObjectId): Promise<BoardEditResult>;
    route(options?: RouteOptions): Promise<RouteReport>;
    addLayer(options: AddLayerOptions): Promise<BoardEditResult>;
    updateLayer(options: UpdateLayerOptions): Promise<BoardEditResult>;
    setOutline(options: SetOutlineOptions): Promise<BoardEditResult>;
    list(): Promise<BoardSummary>;
    close(): Promise<void>;
}

