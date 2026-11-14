/**
 * Authoring surface of `@wae/open-pcb`.
 *
 * The session, board handle, editing operations, inspection and export
 * projections run in the shared native engine. This package owns typed
 * arguments, stable error codes and the format registry; it never keeps a
 * second board model.
 */

export { OpenPCB } from './open-pcb.js';
export { NotImplementedError, OpenPCBError, errorCodes, isOpenPCBError } from './errors.js';
export type { OpenPCBErrorCode, OpenPCBErrorDetails, OpenPCBErrorOptions } from './errors.js';
export { defaultIntentFor, boardFormats, formatFromPath, formatInfo, formatRegistry } from './formats.js';
export type {
    BoardFormat,
    BoardFormatInfo,
    BoardIntent,
    FormatFamily,
    FormatMarkup,
    FormatOpenKind,
} from './formats.js';
export { GuiError, closeGui, defaultGuiContentDir, isGuiOpen, openGui, pumpEvents, runGui } from './gui.js';
export type { GuiCloseResult, GuiOpenResult, GuiStatus, OpenGuiOptions } from './gui.js';
export type { BoardHandle } from './service.js';
export type * from './types.js';

