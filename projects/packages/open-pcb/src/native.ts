/**
 * Native addon discovery.
 *
 * The addon only maps values: board semantics, inspection and export stay in
 * the shared engine. An absent addon is a declarative state, so every call
 * reports `capability/not-implemented` instead of inventing a board.
 */

import { createRequire } from 'node:module';

const requireNative = createRequire(import.meta.url);

/** Orthogonal exports of the built addon. Every call returns a JSON string. */
export interface NativeBinding {
    version(): string;
    capabilities(): string;
    board_create(requestJson: string): string;
    board_open(path: string): string;
    board_import(path: string): string;
    board_save(handle: number, path: string, overwrite: boolean): string;
    board_inspect(handle: number, checksJson?: string): string;
    board_export(handle: number, format: string, path: string): string;
    board_close(handle: number): boolean;
    board_add_footprint(handle: number, requestJson: string): string;
    board_delete_footprint(handle: number, id: string): string;
    board_add_track(handle: number, requestJson: string): string;
    board_add_via(handle: number, requestJson: string): string;
    board_set_via_back_drill(handle: number, requestJson: string): string;
    board_route(handle: number, optionsJson: string): string;
    board_add_net(handle: number, requestJson: string): string;
    board_undo(handle: number): string;
    board_redo(handle: number): string;
    board_move_footprint(handle: number, requestJson: string): string;
    board_update_footprint(handle: number, requestJson: string): string;
    board_delete_track(handle: number, id: string): string;
    board_delete_via(handle: number, id: string): string;
    board_add_differential_pair(handle: number, requestJson: string): string;
    board_delete_differential_pair(handle: number, id: string): string;
    board_add_layer(handle: number, requestJson: string): string;
    board_update_layer(handle: number, requestJson: string): string;
    board_set_outline(handle: number, requestJson: string): string;
    board_list(handle: number): string;
    openGui(options?: { title?: string; contentDir?: string; width?: number; height?: number }): string;
    pumpEvents(): string;
    closeGui(): string;
    isGuiOpen(): boolean;
}

let cached: NativeBinding | null | undefined;

/** Loads the addon once, or returns `undefined` when it is not installed. */
export function nativeBinding(): NativeBinding | undefined {
    if (cached === undefined) {
        cached = load();
    }
    return cached ?? undefined;
}

function load(): NativeBinding | null {
    for (const candidate of candidates()) {
        try {
            return requireNative(candidate) as NativeBinding;
        } catch (error) {
            if (candidate === explicitPath()) {
                throw error;
            }
            if (!isModuleNotFound(error, candidate)) {
                throw error;
            }
        }
    }
    return null;
}

function candidates(): string[] {
    const explicit = explicitPath();
    if (explicit) {
        return [explicit];
    }
    return [`@wae/open-pcb-${process.platform}-${process.arch}`];
}

function explicitPath(): string | undefined {
    const value = process.env.OPEN_PCB_NATIVE;
    return value !== undefined && value.length > 0 ? value : undefined;
}

function isModuleNotFound(error: unknown, candidate: string): boolean {
    if (typeof error !== 'object' || error === null) {
        return false;
    }
    const code = (error as { code?: unknown }).code;
    // Node prints the resolved path, which uses the platform separator on
    // Windows. Normalize it so a linked platform package without a built
    // addon is still treated as "not installed" instead of a fatal error.
    const message = (error as Error).message.replace(/\\/g, '/');
    return code === 'MODULE_NOT_FOUND' && message.includes(candidate);
}

