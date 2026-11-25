/**
 * Board format family.
 *
 * PCB files split by what they describe (board layout, fabrication output,
 * assembly output, mechanical or exchange container, preview image) and by
 * container (KiCad S-expression, generic text, XML, fixed-layout vector, STEP or
 * raster). The registry below classifies extensions and fixes the default open
 * intent.
 *
 * Classification is **not** a support claim. What the installed engine can read,
 * write or export is reported by `OpenPCB.capabilities()`; an extension listed
 * here may still be `unsupported`.
 */

/** Board intent applied when the caller does not override it. */
export type BoardIntent = 'edit' | 'inspect' | 'exchange';

/** Grouping used for capability reporting and user-facing wording. */
export type FormatFamily = 'board' | 'fabrication' | 'assembly' | 'exchange' | 'image';

/** Container encoding behind an extension. `unknown` means the tool is not fixed. */
export type FormatMarkup = 'sexpr' | 'text' | 'xml' | 'json' | 'fixed' | 'raster' | 'step' | 'unknown';

/** What an extension can be consumed as. `none` means it never becomes a board. */
export type FormatOpenKind = 'board' | 'exchange' | 'none';

/** Every extension the product must classify. */
export const boardFormats = [
    'kicad_pcb',
    'gbr',
    'drl',
    'drill_map',
    'dxf',
    'pos',
    'placement',
    'bom',
    'step',
    'ipc2581',
    'svg',
    'pdf',
    'png',
] as const;

/** Union of {@link boardFormats}. */
export type BoardFormat = (typeof boardFormats)[number];

/** What a format is, independent of whether it is implemented. */
export interface BoardFormatInfo {
    readonly format: BoardFormat;
    readonly family: FormatFamily;
    readonly markup: FormatMarkup;
    /** Whether the container may carry executable macros. Such parts are never executed. */
    readonly macroEnabled: boolean;
    readonly opensAs: FormatOpenKind;
    /** Intent applied to `open()` when the extension arrives without one. */
    readonly defaultIntent: BoardIntent | null;
}

type FormatRegistryEntry = Omit<BoardFormatInfo, 'format'>;

/** Extension to classification. Values describe the format, never a capability. */
export const formatRegistry: Readonly<Record<BoardFormat, FormatRegistryEntry>> = Object.freeze({
    kicad_pcb: { family: 'board', markup: 'sexpr', macroEnabled: false, opensAs: 'board', defaultIntent: 'edit' },
    gbr: { family: 'fabrication', markup: 'text', macroEnabled: false, opensAs: 'exchange', defaultIntent: null },
    drl: { family: 'fabrication', markup: 'text', macroEnabled: false, opensAs: 'exchange', defaultIntent: null },
    drill_map: { family: 'fabrication', markup: 'xml', macroEnabled: false, opensAs: 'none', defaultIntent: null },
    dxf: { family: 'fabrication', markup: 'text', macroEnabled: false, opensAs: 'exchange', defaultIntent: null },
    pos: { family: 'assembly', markup: 'text', macroEnabled: false, opensAs: 'exchange', defaultIntent: null },
    placement: { family: 'assembly', markup: 'json', macroEnabled: false, opensAs: 'exchange', defaultIntent: null },
    bom: { family: 'assembly', markup: 'text', macroEnabled: false, opensAs: 'exchange', defaultIntent: null },
    step: { family: 'exchange', markup: 'step', macroEnabled: false, opensAs: 'exchange', defaultIntent: null },
    ipc2581: { family: 'exchange', markup: 'xml', macroEnabled: false, opensAs: 'exchange', defaultIntent: null },
    svg: { family: 'exchange', markup: 'xml', macroEnabled: false, opensAs: 'none', defaultIntent: null },
    pdf: { family: 'exchange', markup: 'fixed', macroEnabled: false, opensAs: 'none', defaultIntent: null },
    png: { family: 'image', markup: 'raster', macroEnabled: false, opensAs: 'none', defaultIntent: null },
});

/** Returns the full classification of one format. */
export function formatInfo(format: BoardFormat): BoardFormatInfo {
    return { format, ...formatRegistry[format] };
}

/** Extracts a classified extension from a path, ignoring case. */
export function formatFromPath(path: string): BoardFormat | undefined {
    const extension = path.slice(path.lastIndexOf('.') + 1).toLowerCase();
    return (boardFormats as readonly string[]).includes(extension) ? (extension as BoardFormat) : undefined;
}

/** Default open intent of a format, or `null` when it never opens as a board. */
export function defaultIntentFor(format: BoardFormat): BoardIntent | null {
    return formatRegistry[format].defaultIntent;
}
