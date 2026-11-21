/**
 * Orthogonal native operations.
 *
 * One function performs one operation and returns data; argument parsing, help
 * text and exit codes belong to `cli.ts` and to the authoring surface. Nothing
 * here knows about a command line.
 */

import { NotImplementedError, OpenPCBError, errorCodes, type OpenPCBErrorCode } from './errors.js';
import { nativeBinding } from './native.js';
import type {
    AddDifferentialPairOptions,
    AddFootprintOptions,
    AddLayerOptions,
    AddNetOptions,
    AddTrackOptions,
    AddViaOptions,
    BoardCapabilities,
    BoardCreateOptions,
    BoardEditResult,
    BoardSummary,
    ExportReport,
    ExportTarget,
    ImportReport,
    InspectCheck,
    InspectReport,
    MoveFootprintOptions,
    RouteOptions,
    RouteReport,
    SaveReport,
    SetOutlineOptions,
    SetViaBackDrillOptions,
    UpdateFootprintOptions,
    UpdateLayerOptions,
} from './types.js';

/** Result of creating or opening a board. */
export interface BoardHandle {
    readonly handle: number;
    readonly board: BoardSummary;
}

const knownCodes = new Set<string>(errorCodes);

function binding(feature: string) {
    const native = nativeBinding();
    if (!native) {
        throw new NotImplementedError(feature);
    }
    return native;
}

function decode<T>(work: () => string): T {
    try {
        return JSON.parse(work()) as T;
    } catch (error) {
        throw translate(error);
    }
}

function translate(error: unknown): OpenPCBError {
    if (error instanceof OpenPCBError) {
        return error;
    }
    const message = error instanceof Error ? error.message : String(error);
    const separator = message.indexOf(': ');
    const code = separator > 0 ? message.slice(0, separator) : '';
    if (knownCodes.has(code)) {
        return new OpenPCBError(code as OpenPCBErrorCode, message.slice(separator + 2), { details: { feature: code } });
    }
    return new OpenPCBError('format/invalid', message, { cause: error });
}

/** Reads the capability matrix of the installed addon. */
export function capabilities(): BoardCapabilities {
    return decode<BoardCapabilities>(() => binding('capabilities').capabilities());
}

/** Creates a board and returns its handle. */
export function create(request: BoardCreateOptions): BoardHandle {
    return decode<BoardHandle>(() =>
        binding('create').board_create(JSON.stringify({ title: request.title, template: request.template })),
    );
}

/** Opens a native board document and returns its handle. */
export function open(path: string): BoardHandle {
    return decode<BoardHandle>(() => binding('open').board_open(path));
}

/** Imports one KiCad PCB or IPC-2581 file and returns its handle plus the loss ledger. */
export function importBoard(path: string): ImportReport {
    return decode<ImportReport>(() => binding('import').board_import(path));
}

/** Writes one open board to a native document path. */
export function save(handle: number, target: string, overwrite: boolean): SaveReport {
    return decode<SaveReport>(() => binding('save').board_save(handle, target, overwrite));
}

/** Runs the requested rule checks on one open board. */
export function inspect(handle: number, checks?: readonly InspectCheck[]): InspectReport {
    return decode<InspectReport>(() =>
        checks === undefined
            ? binding('inspect').board_inspect(handle)
            : binding('inspect').board_inspect(handle, JSON.stringify(checks)),
    );
}

/** Projects one open board to an exchange target. */
export function exportBoard(handle: number, format: ExportTarget, target: string): ExportReport {
    return decode<ExportReport>(() => binding('export').board_export(handle, format, target));
}

/** Releases one board handle. */
export function close(handle: number): void {
    binding('board.close').board_close(handle);
}

/** Lists one open board as a summary. */
export function listBoard(handle: number): BoardSummary {
    return decode<BoardSummary>(() => binding('board.list').board_list(handle));
}

/** Restores the previous snapshot of an open board. */
export function undo(handle: number): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.undo').board_undo(handle));
}

/** Reapplies the next snapshot of an open board. */
export function redo(handle: number): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.redo').board_redo(handle));
}

/** Adds one footprint to an open board. */
export function addFootprint(handle: number, options: AddFootprintOptions): BoardEditResult {
    return decode<BoardEditResult>(() =>
        binding('board.addFootprint').board_add_footprint(handle, JSON.stringify(options)),
    );
}

/** Deletes one footprint from an open board. */
export function deleteFootprint(handle: number, id: string): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.deleteFootprint').board_delete_footprint(handle, id));
}

/** Adds one track to an open board. */
export function addTrack(handle: number, options: AddTrackOptions): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.addTrack').board_add_track(handle, JSON.stringify(options)));
}

/** Adds one via to an open board. */
export function addVia(handle: number, options: AddViaOptions): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.addVia').board_add_via(handle, JSON.stringify(options)));
}

/** Sets the back-drill depth and remaining stub of one through via. */
export function setViaBackDrill(handle: number, options: SetViaBackDrillOptions): BoardEditResult {
    return decode<BoardEditResult>(() =>
        binding('board.setViaBackDrill').board_set_via_back_drill(handle, JSON.stringify(options)),
    );
}

/** Moves one footprint on an open board. */
export function moveFootprint(handle: number, options: MoveFootprintOptions): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.moveFootprint').board_move_footprint(handle, JSON.stringify(options)));
}

/** Updates the value or package of one footprint. */
export function updateFootprint(handle: number, options: UpdateFootprintOptions): BoardEditResult {
    return decode<BoardEditResult>(() =>
        binding('board.updateFootprint').board_update_footprint(handle, JSON.stringify(options)),
    );
}

/** Deletes one track from an open board. */
export function deleteTrack(handle: number, id: string): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.deleteTrack').board_delete_track(handle, id));
}

/** Deletes one via from an open board. */
export function deleteVia(handle: number, id: string): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.deleteVia').board_delete_via(handle, id));
}

/** Declares one net on an open board. */
export function addNet(handle: number, options: AddNetOptions): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.addNet').board_add_net(handle, JSON.stringify(options)));
}

/** Declares one differential pair on an open board. */
export function addDifferentialPair(handle: number, options: AddDifferentialPairOptions): BoardEditResult {
    return decode<BoardEditResult>(() =>
        binding('board.addDifferentialPair').board_add_differential_pair(handle, JSON.stringify(options)),
    );
}

/** Deletes one differential pair from an open board. */
export function deleteDifferentialPair(handle: number, id: string): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.deleteDifferentialPair').board_delete_differential_pair(handle, id));
}

/** Routes every net with at least two pads and reports what it could place. */
export function routeBoard(handle: number, options: RouteOptions = {}): RouteReport {
    return decode<RouteReport>(() => binding('board.route').board_route(handle, JSON.stringify(options)));
}

/** Adds one layer to an open board. */
export function addLayer(handle: number, options: AddLayerOptions): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.addLayer').board_add_layer(handle, JSON.stringify(options)));
}

/** Renames or reconfigures one layer. */
export function updateLayer(handle: number, options: UpdateLayerOptions): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.updateLayer').board_update_layer(handle, JSON.stringify(options)));
}

/** Sets the rectangular outline of an open board. */
export function setOutline(handle: number, options: SetOutlineOptions): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.setOutline').board_set_outline(handle, JSON.stringify(options)));
}

