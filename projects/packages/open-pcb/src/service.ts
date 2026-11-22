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
    AddKeepoutOptions,
    AddTeardropsOptions,
    AddTestPointOptions,
    AddLayerOptions,
    AddNetClassOptions,
    AddNetOptions,
    AddPadInput,
    AddSlotOptions,
    AddTextOptions,
    AddTrackOptions,
    AddViaOptions,
    AddWaiverOptions,
    AddZoneOptions,
    AssignNetClassOptions,
    BoardCapabilities,
    BoardCreateOptions,
    BoardEditResult,
    BoardSummary,
    CopperBalanceReport,
    DeletePadInput,
    ExportReport,
    ExportTarget,
    ImportReport,
    ImpedanceOptions,
    ImpedanceReport,
    ImpedanceTableOptions,
    ImpedanceTableReport,
    ImpedanceWidthOptions,
    ImpedanceWidthReport,
    InspectCheck,
    InspectReport,
    MoveFootprintOptions,
    OutlineGeometryOptions,
    OutlineGeometryReport,
    PanelizeOptions,
    PanelizeReport,
    RouteOptions,
    RouteReport,
    SaveReport,
    SetOutlineOptions,
    SetStackupOptions,
    StitchViasOptions,
    SetRulesOptions,
    SetSeverityOptions,
    SetViaBackDrillOptions,
    TextListReport,
    UpdateFootprintOptions,
    UpdateLayerOptions,
    UpdatePadInput,
    UpdateTextOptions,
    UpdateTrackOptions,
    UpdateViaOptions,
    UpdateZoneOptions,
    ZoneAreaReport,
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

/** Adds one pad to a footprint on an open board. */
export function addPad(handle: number, input: AddPadInput): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.addPad').board_add_pad(handle, JSON.stringify(input)));
}

/** Updates the geometry or net of one pad on an open board. */
export function updatePad(handle: number, input: UpdatePadInput): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.updatePad').board_update_pad(handle, JSON.stringify(input)));
}

/** Deletes one pad from a footprint on an open board. */
export function deletePad(handle: number, input: DeletePadInput): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.deletePad').board_delete_pad(handle, JSON.stringify(input)));
}

/** Adds one track to an open board. */
export function addTrack(handle: number, options: AddTrackOptions): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.addTrack').board_add_track(handle, JSON.stringify(options)));
}

/** Updates the net, layer, width or endpoints of one track on an open board. */
export function updateTrack(handle: number, options: UpdateTrackOptions): BoardEditResult {
    return decode<BoardEditResult>(() =>
        binding('board.updateTrack').board_update_track(handle, JSON.stringify(options)),
    );
}

/** Adds one via to an open board. */
export function addVia(handle: number, options: AddViaOptions): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.addVia').board_add_via(handle, JSON.stringify(options)));
}

/** Updates the net, position, diameter or drill of one via on an open board. */
export function updateVia(handle: number, options: UpdateViaOptions): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.updateVia').board_update_via(handle, JSON.stringify(options)));
}

/** Adds one plated routed slot to an open board. */
export function addSlot(handle: number, options: AddSlotOptions): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.addSlot').board_add_slot(handle, JSON.stringify(options)));
}

/** Deletes one slot from an open board. */
export function deleteSlot(handle: number, id: string): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.deleteSlot').board_delete_slot(handle, id));
}

/** Sets the back-drill depth and remaining stub of one through via. */
export function setViaBackDrill(handle: number, options: SetViaBackDrillOptions): BoardEditResult {
    return decode<BoardEditResult>(() =>
        binding('board.setViaBackDrill').board_set_via_back_drill(handle, JSON.stringify(options)),
    );
}

/** Generates a stitching via fence along the inset board outline. */
export function stitchVias(handle: number, options: StitchViasOptions): BoardEditResult {
    return decode<BoardEditResult>(() =>
        binding('board.stitchVias').board_stitch_vias(handle, JSON.stringify(options)),
    );
}

/** Adds one copper pour region to an open board. */
export function addZone(handle: number, options: AddZoneOptions): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.addZone').board_add_zone(handle, JSON.stringify(options)));
}

/** Deletes one copper pour region from an open board. */
export function deleteZone(handle: number, id: string): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.deleteZone').board_delete_zone(handle, id));
}

/** Updates the layer, net or polygon of one copper pour region. */
export function updateZone(handle: number, options: UpdateZoneOptions): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.updateZone').board_update_zone(handle, JSON.stringify(options)));
}

/** Reports the area and perimeter of every copper pour region. */
export function zoneReport(handle: number): ZoneAreaReport {
    return decode<ZoneAreaReport>(() => binding('board.zoneReport').board_zone_report(handle));
}

/** Adds one silkscreen text item to an open board. */
export function addText(handle: number, options: AddTextOptions): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.addText').board_add_text(handle, JSON.stringify(options)));
}

/** Updates one silkscreen text item on an open board. */
export function updateText(handle: number, options: UpdateTextOptions): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.updateText').board_update_text(handle, JSON.stringify(options)));
}

/** Deletes one silkscreen text item from an open board. */
export function deleteText(handle: number, id: string): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.deleteText').board_delete_text(handle, id));
}

/** Lists the silkscreen text items of an open board. */
export function listTexts(handle: number): TextListReport {
    return decode<TextListReport>(() => binding('board.listTexts').board_list_texts(handle));
}

/** Adds one copper keepout region to an open board. */
export function addKeepout(handle: number, options: AddKeepoutOptions): BoardEditResult {
    return decode<BoardEditResult>(() =>
        binding('board.addKeepout').board_add_keepout(handle, JSON.stringify(options)),
    );
}

/** Deletes one keepout region from an open board. */
export function deleteKeepout(handle: number, id: string): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.deleteKeepout').board_delete_keepout(handle, id));
}

/** Declares one net class on an open board. */
export function addNetClass(handle: number, options: AddNetClassOptions): BoardEditResult {
    return decode<BoardEditResult>(() =>
        binding('board.addNetClass').board_add_net_class(handle, JSON.stringify(options)),
    );
}

/** Deletes one net class from an open board. */
export function deleteNetClass(handle: number, id: string): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.deleteNetClass').board_delete_net_class(handle, id));
}

/** Assigns one declared net to a net class on an open board. */
export function assignNetClass(handle: number, options: AssignNetClassOptions): BoardEditResult {
    return decode<BoardEditResult>(() =>
        binding('board.assignNetClass').board_assign_net_class(handle, JSON.stringify(options)),
    );
}

/** Declares one test point on an existing pad or via. */
export function addTestPoint(handle: number, options: AddTestPointOptions): BoardEditResult {
    return decode<BoardEditResult>(() =>
        binding('board.addTestPoint').board_add_test_point(handle, JSON.stringify(options)),
    );
}

/** Deletes one declared test point from an open board. */
export function deleteTestPoint(handle: number, id: string): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.deleteTestPoint').board_delete_test_point(handle, id));
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

/** Adds linear teardrop copper at every pad-track junction. */
export function addTeardrops(handle: number, options: AddTeardropsOptions = {}): BoardEditResult {
    return decode<BoardEditResult>(() =>
        binding('board.addTeardrops').board_add_teardrops(handle, JSON.stringify(options)),
    );
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

/** Declares one waiver for a finding code on an open board. */
export function addWaiver(handle: number, options: AddWaiverOptions): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.addWaiver').board_add_waiver(handle, JSON.stringify(options)));
}

/** Removes one waiver from an open board. */
export function deleteWaiver(handle: number, id: string): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.deleteWaiver').board_delete_waiver(handle, id));
}

/** Overrides the severity of one finding code on an open board. */
export function setSeverity(handle: number, options: SetSeverityOptions): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.setSeverity').board_set_severity(handle, JSON.stringify(options)));
}

/** Removes one severity override from an open board. */
export function deleteSeverity(handle: number, code: string): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.deleteSeverity').board_delete_severity(handle, code));
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

/** Reports the geometry of a polygonal board outline. */
export function outlineGeometry(handle: number, options: OutlineGeometryOptions): OutlineGeometryReport {
    return decode<OutlineGeometryReport>(() =>
        binding('board.outlineGeometry').board_outline_geometry(handle, JSON.stringify(options)),
    );
}

/** Reports copper area per layer and the coverage imbalance across them. */
export function copperBalance(handle: number): CopperBalanceReport {
    return decode<CopperBalanceReport>(() => binding('board.copperBalance').board_copper_balance(handle));
}

/** Builds the panelization report of one open board. */
export function panelize(handle: number, options: PanelizeOptions): PanelizeReport {
    return decode<PanelizeReport>(() =>
        binding('board.panelize').board_panelize(handle, JSON.stringify(options)),
    );
}

/** Replaces one or more design-rule values on an open board. */
export function setRules(handle: number, options: SetRulesOptions): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.setRules').board_set_rules(handle, JSON.stringify(options)));
}

/** Declares the physical stackup used by controlled-impedance calculations. */
export function setStackup(handle: number, options: SetStackupOptions): BoardEditResult {
    return decode<BoardEditResult>(() => binding('board.setStackup').board_set_stackup(handle, JSON.stringify(options)));
}

/** Computes the single-ended impedance of one trace. */
export function impedance(handle: number, options: ImpedanceOptions): ImpedanceReport {
    return decode<ImpedanceReport>(() => binding('board.impedance').stackup_impedance(handle, JSON.stringify(options)));
}

/** Converts a target impedance to the trace width that reaches it. */
export function impedanceWidth(handle: number, options: ImpedanceWidthOptions): ImpedanceWidthReport {
    return decode<ImpedanceWidthReport>(() =>
        binding('board.impedanceWidth').stackup_width(handle, JSON.stringify(options)),
    );
}

/** Suggests the trace width for each target impedance on one stackup. */
export function impedanceTable(handle: number, options: ImpedanceTableOptions = {}): ImpedanceTableReport {
    return decode<ImpedanceTableReport>(() =>
        binding('board.impedanceTable').stackup_impedance_table(handle, JSON.stringify(options)),
    );
}

