/**
 * Stable error surface of `@wae/open-pcb`.
 *
 * Codes are contract, not prose: callers compare them, localize around them and
 * never parse the message. A failure therefore keeps its code even when the
 * text is translated.
 */

/** Every stable code the authoring surface may report. */
export const errorCodes = [
    'session/disposed',
    'session/argument-invalid',
    'session/budget-exceeded',
    'board/not-found',
    'board/revision-conflict',
    'board/history-empty',
    'board/layer-locked',
    'object/not-found',
    'object/unsupported',
    'format/unsupported',
    'format/unsupported-version',
    'format/invalid',
    'capability/not-implemented',
] as const;

/** Union of {@link errorCodes}. */
export type OpenPCBErrorCode = (typeof errorCodes)[number];

/** Optional locators a failure can carry so callers can point at the object. */
export interface OpenPCBErrorDetails {
    readonly revision?: number;
    readonly layerId?: string;
    readonly objectId?: string;
    /** Board format involved in the failure, without the leading dot. */
    readonly format?: string;
    /** Declared capability involved in the failure, e.g. `board.export`. */
    readonly feature?: string;
}

/** Options accepted by {@link OpenPCBError}. */
export interface OpenPCBErrorOptions {
    readonly recoverable?: boolean;
    readonly details?: OpenPCBErrorDetails;
    readonly cause?: unknown;
}

/** Every failure thrown or rejected by this package. */
export class OpenPCBError extends Error {
    /** Stable, comparable code. */
    readonly code: OpenPCBErrorCode;
    /** Whether retrying with different input or state can succeed. */
    readonly recoverable: boolean;
    /** Optional locators (`revision`, `layerId`, `objectId`, `format`, `feature`). */
    readonly details: OpenPCBErrorDetails;

    constructor(code: OpenPCBErrorCode, message: string, options: OpenPCBErrorOptions = {}) {
        super(message, options.cause === undefined ? undefined : { cause: options.cause });
        this.name = 'OpenPCBError';
        this.code = code;
        this.recoverable = options.recoverable ?? false;
        this.details = Object.freeze({ ...options.details });
    }
}

/**
 * Reported when the native addon is not installed.
 *
 * Every board operation is implemented in `open-pcb-model`; without the addon
 * the SDK cannot run one and says so instead of returning an empty board.
 */
export class NotImplementedError extends OpenPCBError {
    constructor(feature: string, message = `\`${feature}\` requires the native addon, which is not installed`) {
        super('capability/not-implemented', message, { recoverable: false, details: { feature } });
        this.name = 'NotImplementedError';
    }
}

/** Narrows an unknown thrown value to {@link OpenPCBError}. */
export function isOpenPCBError(value: unknown): value is OpenPCBError {
    return value instanceof OpenPCBError;
}

