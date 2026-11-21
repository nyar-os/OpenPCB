/**
 * `new OpenPCB(options?)` authoring session.
 *
 * The constructor is synchronous and opens nothing: no network, no disk scan,
 * no window, no file. Native initialisation and imports happen inside the async
 * methods, so a failure is always awaited and always carries a stable code.
 *
 * Every board operation lives on the returned `Board`; the session only creates,
 * opens, imports and releases documents. Sidecar history is loaded dynamically,
 * so a memory session never touches `node:fs`.
 */

import { OpenPCBError } from './errors.js';
import type { BoardIntent } from './formats.js';
import * as service from './service.js';
import type {
    AddDifferentialPairOptions,
    AddFootprintOptions,
    AddLayerOptions,
    AddNetOptions,
    AddTrackOptions,
    AddViaOptions,
    Board,
    BoardCapabilities,
    BoardCreateOptions,
    BoardEditResult,
    BoardImportOptions,
    BoardImportResult,
    BoardInput,
    BoardSummary,
    ExportOptions,
    ExportReport,
    InspectOptions,
    InspectReport,
    KicadImportOptions,
    KicadImportResult,
    MoveFootprintOptions,
    OpenPCBOptions,
    RouteOptions,
    RouteReport,
    SaveOptions,
    SaveReport,
    SetOutlineOptions,
    UpdateFootprintOptions,
    UpdateLayerOptions,
} from './types.js';

/** Sidecar surface consumed by a board; provided only in `history: 'sidecar'` sessions. */
interface SidecarHistory {
    read(file: string): string;
    record(file: string, previous: string): void;
    canUndo(file: string): boolean;
    canRedo(file: string): boolean;
    restore(file: string, direction: 'undo' | 'redo'): number | undefined;
    clear(file: string): void;
}

/** Handle ownership, so a board that reopens itself keeps the session accurate. */
interface BoardSession {
    track(handle: number): void;
    untrack(handle: number): void;
}

/** Authoring session. One instance may manage several boards. */
export class OpenPCB {
    /** BCP-47 tag used for diagnostics and default copy. */
    readonly locale: string;
    private readonly historyMode: 'memory' | 'sidecar';
    private closed = false;
    private readonly handles = new Set<number>();
    private sidecar: SidecarHistory | undefined;
    private readonly owner: BoardSession = {
        track: (handle) => {
            this.handles.add(handle);
        },
        untrack: (handle) => {
            this.handles.delete(handle);
        },
    };

    constructor(options: OpenPCBOptions = {}) {
        this.locale = options.locale ?? 'en';
        this.historyMode = options.history ?? 'memory';
    }

    /** Whether {@link dispose} has run. */
    get disposed(): boolean {
        return this.closed;
    }

    /**
     * Format, checker, renderer and export disclosure for the installed engine.
     * Read this instead of treating a method name as a capability.
     */
    async capabilities(): Promise<BoardCapabilities> {
        this.assertActive();
        return service.capabilities();
    }

    /** Creates an empty board with its own handle. */
    async create(options: BoardCreateOptions = {}): Promise<Board> {
        this.assertActive();
        assertCreateOptions(options);
        const history = await this.sidecarApi();
        const created = service.create(options);
        this.handles.add(created.handle);
        return attach(created, this.owner, history);
    }

    /** Opens a native board document or an in-memory document. */
    async open(input: BoardInput): Promise<Board> {
        this.assertActive();
        const path = openPath(input);
        const history = await this.sidecarApi();
        const opened = service.open(path);
        this.handles.add(opened.handle);
        return attach(opened, this.owner, history, path);
    }

    /**
     * Imports one exchange board file and wraps the native handle in a `Board`.
     *
     * Both KiCad PCB and IPC-2581 documents are read; the engine detects the
     * format from the content and reports it on the result. With `target` the
     * imported document is saved immediately and the board keeps that path as
     * its target, so later edits write it back in place.
     */
    async importBoard(options: BoardImportOptions): Promise<BoardImportResult> {
        this.assertActive();
        assertImportOptions(options);
        const history = await this.sidecarApi();
        const imported = service.importBoard(options.path);
        this.handles.add(imported.handle);
        const board = attach({ handle: imported.handle, board: imported.document }, this.owner, history);
        let saved: SaveReport | undefined;
        try {
            saved = options.target === undefined
                ? undefined
                : await board.save({ target: options.target, overwrite: options.overwrite ?? false });
        } catch (error) {
            await board.close();
            throw error;
        }
        return {
            board,
            path: imported.path,
            format: imported.format,
            bytes: imported.bytes,
            losses: imported.losses,
            lossCodes: imported.lossCodes,
            saved,
        };
    }

    /**
     * Imports one KiCad PCB file and wraps the native handle in a `Board`.
     *
     * Kept for callers that only read KiCad files; `importBoard` is the same
     * operation with the detected format on the result.
     */
    async importKicadPcb(options: KicadImportOptions): Promise<KicadImportResult> {
        const { format: _format, ...result } = await this.importBoard(options);
        return result;
    }

    /** Inspects an input without keeping a board handle. */
    async inspect(input: BoardInput, options?: InspectOptions): Promise<InspectReport> {
        this.assertActive();
        const opened = service.open(openPath(input));
        try {
            return service.inspect(opened.handle, options?.checks);
        } finally {
            service.close(opened.handle);
        }
    }

    /** Releases the session and every handle it owns. Idempotent. */
    async dispose(): Promise<void> {
        if (this.closed) {
            return;
        }
        this.closed = true;
        for (const handle of this.handles) {
            try {
                service.close(handle);
            } catch {
                // A handle that already vanished must not keep the rest alive.
            }
        }
        this.handles.clear();
    }

    private assertActive(): void {
        if (this.closed) {
            throw new OpenPCBError('session/disposed', 'the OpenPCB session has been disposed; create a new instance');
        }
    }

    /** Loads the sidecar module on first use, and only when the session asks for it. */
    private async sidecarApi(): Promise<SidecarHistory | undefined> {
        if (this.historyMode !== 'sidecar') {
            return undefined;
        }
        if (this.sidecar === undefined) {
            const historyModule = await import('./history.js');
            this.sidecar = {
                read: historyModule.readDocument,
                record: historyModule.record,
                canUndo: historyModule.canUndo,
                canRedo: historyModule.canRedo,
                restore: historyModule.restore,
                clear: historyModule.clearHistory,
            };
        }
        return this.sidecar;
    }
}

/**
 * Validates an open input and returns the native path it addresses.
 *
 * Nothing is opened here; `open()` and the transient `inspect()` path share the
 * same validation so both reject with the same codes.
 */
function openPath(input: BoardInput): string {
    assertBoardInput(input);
    if (!('path' in input)) {
        throw new OpenPCBError(
            'format/unsupported',
            'opening a serialized `project` needs the document reader; write it to a path and pass `path`',
            { details: { feature: 'open.project' } },
        );
    }
    const intent: BoardIntent = input.intent ?? 'edit';
    if (intent === 'exchange') {
        throw new OpenPCBError(
            'format/unsupported',
            'native documents open for editing; `exchange` needs the import pipeline',
            { details: { feature: 'open.intent.exchange' } },
        );
    }
    return input.path;
}

/**
 * Wraps a native handle in the `Board` contract.
 *
 * Every method names the handle; the native layer owns the board and reports
 * its own stable codes. The local summary is refreshed after each mutation so
 * `board.revision` and `board.summary` describe the committed state.
 *
 * In sidecar mode a mutation snapshots the file before writing it back, and
 * `undo`/`redo` swap those snapshots. Reopening the document after a restore
 * keeps the native state and the file in agreement.
 */
function attach(
    opened: service.BoardHandle,
    session: BoardSession,
    history: SidecarHistory | undefined,
    path?: string,
): Board {
    let handle = opened.handle;
    let summary: BoardSummary = opened.board;
    let target = path;
    let closed = false;

    const ensure = (): void => {
        if (closed) {
            throw new OpenPCBError('board/not-found', 'the board has been closed');
        }
    };
    const view = (): BoardSummary => {
        const sidecar = history;
        const file = target;
        if (sidecar === undefined || file === undefined) {
            return summary;
        }
        return { ...summary, canUndo: sidecar.canUndo(file), canRedo: sidecar.canRedo(file) };
    };
    const adopt = (result: BoardEditResult): BoardEditResult => {
        summary = result.board;
        return result;
    };

    /** Applies one native mutation, then persists it and records the sidecar step. */
    const commit = (apply: (handle: number) => BoardEditResult): BoardEditResult => {
        const sidecar = history;
        const file = target;
        if (sidecar === undefined || file === undefined) {
            return adopt(apply(handle));
        }
        const previous = sidecar.read(file);
        const result = apply(handle);
        service.save(handle, file, true);
        sidecar.record(file, previous);
        summary = service.listBoard(handle);
        return { ...result, revision: summary.revision, board: view() };
    };

    /** Swaps one sidecar snapshot, reopens the file and refreshes the handle. */
    const restore = (direction: 'undo' | 'redo'): BoardEditResult => {
        const sidecar = history;
        const file = target;
        if (sidecar === undefined || file === undefined) {
            throw new OpenPCBError('board/not-found', 'the board has no document path to restore');
        }
        if (sidecar.restore(file, direction) === undefined) {
            throw new OpenPCBError(
                'board/history-empty',
                direction === 'undo'
                    ? 'there is no earlier revision to restore'
                    : 'there is no later revision to restore',
            );
        }
        session.untrack(handle);
        service.close(handle);
        try {
            const reopened = service.open(file);
            handle = reopened.handle;
            summary = reopened.board;
            session.track(handle);
        } catch (error) {
            closed = true;
            throw error;
        }
        return { revision: summary.revision, board: view() };
    };

    return {
        get id() {
            return summary.id;
        },
        get revision() {
            return summary.revision;
        },
        get summary() {
            return view();
        },
        get canUndo() {
            return view().canUndo;
        },
        get canRedo() {
            return view().canRedo;
        },
        async undo(): Promise<BoardEditResult> {
            ensure();
            if (history !== undefined && target !== undefined) {
                return restore('undo');
            }
            return adopt(service.undo(handle));
        },
        async redo(): Promise<BoardEditResult> {
            ensure();
            if (history !== undefined && target !== undefined) {
                return restore('redo');
            }
            return adopt(service.redo(handle));
        },
        async inspect(options?: InspectOptions): Promise<InspectReport> {
            ensure();
            return service.inspect(handle, options?.checks);
        },
        async save(options: SaveOptions = {}): Promise<SaveReport> {
            ensure();
            const destination = options.target ?? target;
            if (!destination) {
                throw new OpenPCBError(
                    'session/argument-invalid',
                    '`target` is required because this board has not been written to a path yet',
                );
            }
            const report = service.save(handle, destination, options.overwrite ?? false);
            if (history !== undefined && destination !== target) {
                // A sidecar beside a new target describes a file this board did
                // not write, so it must not be restored into it.
                history.clear(destination);
            }
            target = destination;
            summary = service.listBoard(handle);
            return report;
        },
        async export(options: ExportOptions): Promise<ExportReport> {
            ensure();
            return service.exportBoard(handle, options.format, options.target);
        },
        async addFootprint(options: AddFootprintOptions): Promise<BoardEditResult> {
            ensure();
            return commit((current) => service.addFootprint(current, options));
        },
        async deleteFootprint(id: string): Promise<BoardEditResult> {
            ensure();
            return commit((current) => service.deleteFootprint(current, id));
        },
        async moveFootprint(options: MoveFootprintOptions): Promise<BoardEditResult> {
            ensure();
            return commit((current) => service.moveFootprint(current, options));
        },
        async updateFootprint(options: UpdateFootprintOptions): Promise<BoardEditResult> {
            ensure();
            return commit((current) => service.updateFootprint(current, options));
        },
        async addTrack(options: AddTrackOptions): Promise<BoardEditResult> {
            ensure();
            return commit((current) => service.addTrack(current, options));
        },
        async deleteTrack(id: string): Promise<BoardEditResult> {
            ensure();
            return commit((current) => service.deleteTrack(current, id));
        },
        async addVia(options: AddViaOptions): Promise<BoardEditResult> {
            ensure();
            return commit((current) => service.addVia(current, options));
        },
        async deleteVia(id: string): Promise<BoardEditResult> {
            ensure();
            return commit((current) => service.deleteVia(current, id));
        },
        async addNet(options: AddNetOptions): Promise<BoardEditResult> {
            ensure();
            return commit((current) => service.addNet(current, options));
        },
        async addDifferentialPair(options: AddDifferentialPairOptions): Promise<BoardEditResult> {
            ensure();
            return commit((current) => service.addDifferentialPair(current, options));
        },
        async deleteDifferentialPair(id: string): Promise<BoardEditResult> {
            ensure();
            return commit((current) => service.deleteDifferentialPair(current, id));
        },
        async route(options: RouteOptions = {}): Promise<RouteReport> {
            ensure();
            const sidecar = history;
            const file = target;
            if (sidecar === undefined || file === undefined) {
                const report = service.routeBoard(handle, options);
                summary = service.listBoard(handle);
                return report;
            }
            const previous = sidecar.read(file);
            const report = service.routeBoard(handle, options);
            if (report.tracks + report.vias > 0) {
                service.save(handle, file, true);
                sidecar.record(file, previous);
            }
            summary = service.listBoard(handle);
            return report;
        },
        async addLayer(options: AddLayerOptions): Promise<BoardEditResult> {
            ensure();
            return commit((current) => service.addLayer(current, options));
        },
        async updateLayer(options: UpdateLayerOptions): Promise<BoardEditResult> {
            ensure();
            return commit((current) => service.updateLayer(current, options));
        },
        async setOutline(options: SetOutlineOptions): Promise<BoardEditResult> {
            ensure();
            return commit((current) => service.setOutline(current, options));
        },
        async list(): Promise<BoardSummary> {
            ensure();
            summary = service.listBoard(handle);
            return view();
        },
        async close(): Promise<void> {
            if (closed) {
                return;
            }
            closed = true;
            session.untrack(handle);
            service.close(handle);
        },
    };
}

function assertCreateOptions(options: BoardCreateOptions): void {
    if (options.title !== undefined && (typeof options.title !== 'string' || options.title.trim().length === 0)) {
        throw new OpenPCBError('session/argument-invalid', '`title` must be a non-empty string');
    }
    if (options.template !== undefined && (typeof options.template !== 'string' || options.template.trim().length === 0)) {
        throw new OpenPCBError('session/argument-invalid', '`template` must be a non-empty string');
    }
}

function assertImportOptions(options: BoardImportOptions): void {
    if (typeof options.path !== 'string' || options.path.length === 0) {
        throw new OpenPCBError('session/argument-invalid', '`path` must be a non-empty string');
    }
    if (options.target !== undefined && (typeof options.target !== 'string' || options.target.length === 0)) {
        throw new OpenPCBError('session/argument-invalid', '`target` must be a non-empty string when present');
    }
}

function assertBoardInput(input: BoardInput): void {
    const candidate = input as unknown as Record<string, unknown>;
    const present = (['path', 'project'] as const).filter((key) => candidate[key] !== undefined);
    if (present.length !== 1) {
        throw new OpenPCBError(
            'session/argument-invalid',
            'BoardInput must contain exactly one of `path` or `project`',
        );
    }
    const value = candidate[present[0]];
    if (typeof value !== 'string' || value.length === 0) {
        throw new OpenPCBError('session/argument-invalid', `\`${present[0]}\` must be a non-empty string`);
    }
}
