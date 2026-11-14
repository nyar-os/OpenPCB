/**
 * Sidecar undo history.
 *
 * Loaded only when a session selects `history: 'sidecar'`; the dynamic import in
 * `open-pcb.ts` keeps `node:fs` out of the default memory session. Every
 * mutation snapshots the document it is about to replace, so `undo` and `redo`
 * keep working across processes without a second board model.
 */

import fs from 'node:fs';

const HISTORY_LIMIT = 32;
const HISTORY_VERSION = 1;

interface DocumentHistory {
    readonly version: number;
    undo: string[];
    redo: string[];
}

/** Where the snapshots of one document live. */
export function historyPath(file: string): string {
    return `${file}.history.json`;
}

/** Reads one native document as text; the caller compares it with the new one. */
export function readDocument(file: string): string {
    return fs.readFileSync(file, 'utf8');
}

function readHistory(file: string): DocumentHistory {
    try {
        const parsed = JSON.parse(fs.readFileSync(historyPath(file), 'utf8')) as Partial<DocumentHistory>;
        if (parsed.version === HISTORY_VERSION && Array.isArray(parsed.undo) && Array.isArray(parsed.redo)) {
            return {
                version: HISTORY_VERSION,
                undo: parsed.undo.filter((item): item is string => typeof item === 'string'),
                redo: parsed.redo.filter((item): item is string => typeof item === 'string'),
            };
        }
    } catch {
        // No history yet, or a file this build cannot read.
    }
    return { version: HISTORY_VERSION, undo: [], redo: [] };
}

function writeAtomic(target: string, content: string): void {
    const temporary = `${target}.${process.pid}.tmp`;
    fs.writeFileSync(temporary, content);
    fs.renameSync(temporary, target);
}

function writeHistory(file: string, history: DocumentHistory): void {
    writeAtomic(historyPath(file), `${JSON.stringify(history)}\n`);
}

function push(snapshots: string[], snapshot: string): void {
    snapshots.push(snapshot);
    if (snapshots.length > HISTORY_LIMIT) {
        snapshots.splice(0, snapshots.length - HISTORY_LIMIT);
    }
}

/** Records the document as it was before a mutation and drops the redo branch. */
export function record(file: string, previous: string): void {
    const history = readHistory(file);
    push(history.undo, previous);
    history.redo.length = 0;
    writeHistory(file, history);
}

/** Whether an earlier snapshot is available. */
export function canUndo(file: string): boolean {
    return readHistory(file).undo.length > 0;
}

/** Whether a later snapshot is available. */
export function canRedo(file: string): boolean {
    return readHistory(file).redo.length > 0;
}

/** Removes the snapshots of one target, used when a board moves to a new path. */
export function clearHistory(file: string): void {
    fs.rmSync(historyPath(file), { force: true });
}

/** Reads the committed revision from a native document. */
function revisionOf(document: string): number {
    const parsed = JSON.parse(document) as { document?: { revision?: unknown } };
    const revision = parsed.document?.revision;
    return typeof revision === 'number' ? revision : 1;
}

/**
 * Swaps the document with the newest snapshot in one direction, bumps the
 * restored revision and returns it. `undefined` means that direction is empty.
 */
export function restore(file: string, direction: 'undo' | 'redo'): number | undefined {
    const history = readHistory(file);
    const source = direction === 'undo' ? history.undo : history.redo;
    const destination = direction === 'undo' ? history.redo : history.undo;
    const snapshot = source.pop();
    if (snapshot === undefined) {
        return undefined;
    }
    const current = readDocument(file);
    const restored = JSON.parse(snapshot) as { document: { revision: number } };
    restored.document.revision = revisionOf(current) + 1;
    writeAtomic(file, `${JSON.stringify(restored, null, 2)}\n`);
    push(destination, current);
    writeHistory(file, history);
    return restored.document.revision;
}
