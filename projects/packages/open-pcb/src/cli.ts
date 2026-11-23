#!/usr/bin/env node
/**
 * Command-line assembly only.
 *
 * Every command parses options, calls one session or document method and prints
 * the result. Board logic, inspection, export and sidecar history stay in the
 * SDK; help text and errors come from `@wae/commander`, so the CLI does not grow
 * its own parser.
 */

import { existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { createCli } from '@wae/commander';

import { isOpenPCBError } from './errors.js';
import { runGui } from './gui.js';
import { OpenPCB } from './open-pcb.js';
import type {
    Board,
    BoardCapabilities,
    BoardRules,
    BoardSummary,
    ExportTarget,
    ImpedanceStructure,
    InspectCheck,
    InspectReport,
    LayerKind,
    PanelTabStyle,
    RouteStyle,
    Severity,
} from './types.js';

const here = path.dirname(fileURLToPath(import.meta.url));

/**
 * Locates the shipped `locales/` directory.
 *
 * The compiled entry sits at `dist/cli.js` in a build and at
 * `dist-test/src/cli.js` in the test build, so the package root is found by
 * walking up to the directory that holds `locales/locales.json`.
 */
function localesDirectory(): string {
    let directory = here;
    while (directory !== path.dirname(directory)) {
        const candidate = path.join(directory, 'locales');
        if (existsSync(path.join(candidate, 'locales.json'))) {
            return candidate;
        }
        directory = path.dirname(directory);
    }
    return path.join(here, '..', 'locales');
}

const cli = createCli('open-pcb').locales(localesDirectory()).intro('cli.intro');

// Exported for contract audits that import the built entry to inspect the
// command tree. Nothing runs until the file is executed as the program entry.
export { cli };

/** One session per invocation; documents live in it until the command ends. */
const app = new OpenPCB({ history: 'sidecar' });

interface ParsedOptions {
    readonly _?: readonly string[];
    readonly [key: string]: unknown;
}

const text = (options: ParsedOptions, key: string): string | undefined =>
    typeof options[key] === 'string' ? (options[key] as string) : undefined;

const flag = (options: ParsedOptions, key: string): boolean => options[key] === true || options[key] === 'true';

/** Parses an explicit `true`/`false` option; anything else is invalid. */
const booleanOption = (raw: string | undefined): boolean | undefined => {
    if (raw === 'true' || raw === '1') {
        return true;
    }
    if (raw === 'false' || raw === '0') {
        return false;
    }
    return undefined;
};

const input = (options: ParsedOptions): string | undefined => options._?.[0];

/** Parses one numeric option; a non-numeric value is reported as invalid. */
function numberOption(raw: string | undefined): number | undefined {
    if (raw === undefined) {
        return undefined;
    }
    const value = Number(raw);
    return Number.isFinite(value) ? value : undefined;
}

/** Parses one positive integer count; anything else is invalid. */
function countOption(raw: string | undefined): number | undefined {
    if (raw === undefined) {
        return undefined;
    }
    const value = Number(raw);
    return Number.isInteger(value) && value > 0 ? value : undefined;
}

/** Parses `x,y` into a coordinate pair. */
function pair(raw: string | undefined): [number, number] | undefined {
    if (raw === undefined) {
        return undefined;
    }
    const parts = raw.split(',').map((part) => part.trim());
    if (parts.length !== 2) {
        return undefined;
    }
    const first = Number(parts[0]);
    const second = Number(parts[1]);
    return Number.isFinite(first) && Number.isFinite(second) ? [first, second] : undefined;
}

/** Parses `x,y x,y ...` into a polygon with at least three vertices. */
function polygon(raw: string | undefined): [number, number][] | undefined {
    if (raw === undefined) {
        return undefined;
    }
    const parts = raw.split(/[;\s]+/).filter((part) => part.length > 0);
    if (parts.length < 3) {
        return undefined;
    }
    const points: [number, number][] = [];
    for (const part of parts) {
        const point = pair(part);
        if (point === undefined) {
            return undefined;
        }
        points.push(point);
    }
    return points;
}

/** Opens one document for the duration of one command. */
async function withBoard<T>(file: string, apply: (board: Board) => Promise<T>): Promise<T> {
    const board = await app.open({ path: file });
    try {
        return await apply(board);
    } finally {
        await board.close();
    }
}

/** Export targets the CLI accepts, mirroring the declared capability matrix. */
const exportTargets: readonly ExportTarget[] = [
    'kicad_pcb',
    'gbr',
    'drl',
    'drill_map',
    'pos',
    'placement',
    'bom',
    'testpoints',
    'ipc_d_356',
    'step',
    'ipc2581',
    'pdf',
    'svg',
    'png',
];

/** Layer kinds the model accepts. */
const layerKinds: readonly LayerKind[] = ['copper', 'silkscreen', 'soldermask', 'assembly', 'mechanical'];

function asExportTarget(value: string | undefined): ExportTarget | undefined {
    return value !== undefined && (exportTargets as readonly string[]).includes(value) ? (value as ExportTarget) : undefined;
}

function supportedNames(map: Readonly<Record<string, string>>): string {
    const names = Object.entries(map)
        .filter(([, level]) => level === 'supported')
        .map(([name]) => name);
    return names.length > 0 ? names.join(', ') : 'none';
}

function describeCapabilities(matrix: BoardCapabilities): string {
    const formats = Object.entries(matrix.formats)
        .filter(([, entry]) => entry.export === 'supported')
        .map(([name]) => name);
    return [
        `engine: ${matrix.engine.package} ${matrix.engine.version}`,
        `project: import=${matrix.project.import} edit=${matrix.project.edit} export=${matrix.project.export}`,
        `formats with export: ${formats.length > 0 ? formats.join(', ') : 'none'}`,
        `export targets: ${supportedNames(matrix.exportTargets)}`,
        `checks: ${supportedNames(matrix.checks)}`,
        `renderers: ${supportedNames(matrix.renderers)}`,
    ].join('\n');
}

function describeBoard(board: BoardSummary): string {
    const title = board.title ?? 'untitled';
    const counts = board.counts;
    const lines = [
        `board ${board.id} "${title}" revision=${board.revision} units=${board.units} layers=${board.layers.length}`
            + ` nets=${counts.nets} footprints=${counts.footprints} tracks=${counts.tracks} vias=${counts.vias}`
            + ` zones=${counts.zones} keepouts=${counts.keepouts} diffpairs=${counts.differentialPairs}`,
    ];
    for (const layer of board.layers) {
        lines.push(`  [${layer.index}] ${layer.id} ${layer.title} (${layer.kind}, ${layer.objects} objects)`);
    }
    return lines.join('\n');
}

function describeReport(report: InspectReport): string {
    const lines = [
        `inspect revision=${report.revision} checker=${report.checkerVersion} passed=${report.passed}`
            + ` active=${report.active} waived=${report.waived}`,
    ];
    for (const issue of report.issues) {
        lines.push(`  ${issue.severity} ${issue.code} ${issue.layerId ?? '-'} ${issue.message}${issue.waived ? ' (waived)' : ''}`);
    }
    return lines.join('\n');
}

function describeRules(rules: BoardRules): string {
    return [
        `holeClearance=${rules.holeClearance}`,
        `copperToEdge=${rules.copperToEdge}`,
        `componentToEdge=${rules.componentToEdge}`,
        `holeToEdge=${rules.holeToEdge}`,
        `solderMaskExpansion=${rules.solderMaskExpansion}`,
    ].join(' ');
}

cli.command('capabilities', 'cli.cmd.capabilities').actionCtx(async (_options, _args, ctx) => {
    const matrix = await app.capabilities();
    ctx.emitResult(matrix, describeCapabilities(matrix));
    return 0;
});

cli
    .command('create', 'cli.cmd.create')
    .option('--out <file>', 'cli.opt.out')
    .option('--title <title>', 'cli.opt.title')
    .option('--template <name>', 'cli.opt.template')
    .option('--overwrite', 'cli.opt.overwrite')
    .actionCtx(async (options, _args, ctx) => {
        const out = text(options, 'out');
        if (out === undefined) {
            ctx.fail('open-pcb.cli.out-required', { message: '`--out <file>` is required' });
            return 2;
        }
        const created = await app.create({ title: text(options, 'title'), template: text(options, 'template') });
        try {
            const report = await created.save({ target: out, overwrite: flag(options, 'overwrite') });
            const board = created.summary;
            ctx.emitResult(
                { ...report, board },
                `created ${out} revision=${report.revision} layers=${board.layers.length}`,
            );
        } finally {
            await created.close();
        }
        return 0;
    });

cli.command('open', 'cli.cmd.open').actionCtx(async (options, _args, ctx) => {
    const file = input(options);
    if (file === undefined) {
        ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
        return 2;
    }
    await withBoard(file, async (board) => {
        const summary = board.summary;
        ctx.emitResult(summary, describeBoard(summary));
        return 0;
    });
    return 0;
});

cli
    .command('import', 'cli.cmd.import')
    .option('--out <file>', 'cli.opt.import-out')
    .option('--overwrite', 'cli.opt.overwrite')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a KiCad PCB or IPC-2581 file is required' });
            return 2;
        }
        const out = text(options, 'out');
        if (out === undefined) {
            ctx.fail('open-pcb.cli.out-required', { message: '`--out <file>` is required' });
            return 2;
        }
        const imported = await app.importBoard({
            path: file,
            target: out,
            overwrite: flag(options, 'overwrite'),
        });
        try {
            const document = imported.board.summary;
            const counts = document.counts;
            const saved = imported.saved ?? { path: out, revision: document.revision, bytes: imported.bytes };
            const revision = saved.revision;
            ctx.emitResult(
                {
                    ...saved,
                    format: imported.format,
                    document,
                    losses: imported.losses,
                    lossCodes: imported.lossCodes,
                },
                `imported ${imported.format} ${file} to ${out} revision=${revision}`
                    + ` nets=${counts.nets} footprints=${counts.footprints} tracks=${counts.tracks} vias=${counts.vias}`
                    + ` losses=${imported.losses.length}`,
            );
        } finally {
            await imported.board.close();
        }
        return 0;
    });

cli.command('list', 'cli.cmd.list').actionCtx(async (options, _args, ctx) => {
    const file = input(options);
    if (file === undefined) {
        ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
        return 2;
    }
    await withBoard(file, async (board) => {
        const summary = await board.list();
        ctx.emitResult(summary, describeBoard(summary));
        return 0;
    });
    return 0;
});

for (const [name, description] of [
    ['undo', 'cli.cmd.undo'],
    ['redo', 'cli.cmd.redo'],
] as const) {
    cli.command(name, description).actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        const board = await app.open({ path: file });
        try {
            const result = name === 'undo' ? await board.undo() : await board.redo();
            ctx.emitResult(result.board, `${name} revision=${result.revision}`);
            return 0;
        } catch (error) {
            if (isOpenPCBError(error) && error.code === 'board/history-empty') {
                ctx.fail('board/history-empty', { message: error.message });
                return 2;
            }
            throw error;
        } finally {
            await board.close();
        }
    });
}

const footprint = cli.command('footprint', 'cli.cmd.footprint');
footprint
    .command('add', 'cli.cmd.footprint-add')
    .option('--ref <ref>', 'cli.opt.ref')
    .option('--value <value>', 'cli.opt.value')
    .option('--footprint <name>', 'cli.opt.footprint')
    .option('--x <mm>', 'cli.opt.x')
    .option('--y <mm>', 'cli.opt.y')
    .option('--rotation <degrees>', 'cli.opt.rotation')
    .option('--layer <layer>', 'cli.opt.layer')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const reference = text(options, 'ref');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (reference === undefined || reference.length === 0) {
            ctx.fail('open-pcb.cli.ref-required', { message: '`--ref <ref>` is required' });
            return 2;
        }
        const x = numberOption(text(options, 'x'));
        const y = numberOption(text(options, 'y'));
        const rotation = numberOption(text(options, 'rotation'));
        if (text(options, 'x') !== undefined && x === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--x` must be a finite number' });
            return 2;
        }
        if (text(options, 'y') !== undefined && y === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--y` must be a finite number' });
            return 2;
        }
        if (text(options, 'rotation') !== undefined && rotation === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--rotation` must be a finite number' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.addFootprint({
                reference,
                value: text(options, 'value'),
                footprint: text(options, 'footprint'),
                x,
                y,
                rotation,
                layer: text(options, 'layer'),
            });
            ctx.emitResult(result, `added ${result.object?.id ?? 'footprint'} ${reference}`);
            return 0;
        });
        return 0;
    });

footprint
    .command('delete', 'cli.cmd.footprint-delete')
    .option('--id <id>', 'cli.opt.id')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const id = text(options, 'id');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (id === undefined || id.length === 0) {
            ctx.fail('open-pcb.cli.id-required', { message: '`--id <id>` is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.deleteFootprint(id);
            ctx.emitResult(result, `deleted ${result.deleted ?? id}`);
            return 0;
        });
        return 0;
    });

footprint
    .command('move', 'cli.cmd.footprint-move')
    .option('--id <id>', 'cli.opt.id')
    .option('--x <mm>', 'cli.opt.x')
    .option('--y <mm>', 'cli.opt.y')
    .option('--rotation <degrees>', 'cli.opt.rotation')
    .option('--layer <layer>', 'cli.opt.layer')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const id = text(options, 'id');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (id === undefined || id.length === 0) {
            ctx.fail('open-pcb.cli.id-required', { message: '`--id <id>` is required' });
            return 2;
        }
        const x = numberOption(text(options, 'x'));
        const y = numberOption(text(options, 'y'));
        const rotation = numberOption(text(options, 'rotation'));
        if (text(options, 'x') !== undefined && x === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--x` must be a finite number' });
            return 2;
        }
        if (text(options, 'y') !== undefined && y === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--y` must be a finite number' });
            return 2;
        }
        if (text(options, 'rotation') !== undefined && rotation === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--rotation` must be a finite number' });
            return 2;
        }
        if (x === undefined && y === undefined && rotation === undefined && text(options, 'layer') === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: 'a move needs `--x`, `--y`, `--rotation` or `--layer`' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.moveFootprint({ id, x, y, rotation, layer: text(options, 'layer') });
            ctx.emitResult(result, `moved ${id}`);
            return 0;
        });
        return 0;
    });

footprint
    .command('update', 'cli.cmd.footprint-update')
    .option('--id <id>', 'cli.opt.id')
    .option('--value <value>', 'cli.opt.value')
    .option('--footprint <name>', 'cli.opt.footprint')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const id = text(options, 'id');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (id === undefined || id.length === 0) {
            ctx.fail('open-pcb.cli.id-required', { message: '`--id <id>` is required' });
            return 2;
        }
        if (text(options, 'value') === undefined && text(options, 'footprint') === undefined) {
            ctx.fail('open-pcb.cli.ref-required', { message: '`--value` or `--footprint` is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.updateFootprint({
                id,
                value: text(options, 'value'),
                footprint: text(options, 'footprint'),
            });
            ctx.emitResult(result, `updated ${id}`);
            return 0;
        });
        return 0;
    });

const pad = footprint.command('pad', 'cli.cmd.footprint-pad');
pad.command('add', 'cli.cmd.footprint-pad-add')
    .option('--footprint <id>', 'cli.opt.footprint-id')
    .option('--number <n>', 'cli.opt.pad-number')
    .option('--x <mm>', 'cli.opt.x')
    .option('--y <mm>', 'cli.opt.y')
    .option('--w <mm>', 'cli.opt.w')
    .option('--h <mm>', 'cli.opt.h')
    .option('--net <name>', 'cli.opt.net')
    .option('--drill <mm>', 'cli.opt.pad-drill')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const footprintId = text(options, 'footprint');
        const padNumber = text(options, 'number');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (footprintId === undefined || footprintId.length === 0) {
            ctx.fail('open-pcb.cli.footprint-required', { message: '`--footprint <id>` is required' });
            return 2;
        }
        if (padNumber === undefined || padNumber.length === 0) {
            ctx.fail('open-pcb.cli.number-required', { message: '`--number <n>` is required' });
            return 2;
        }
        const x = numberOption(text(options, 'x'));
        const y = numberOption(text(options, 'y'));
        const w = numberOption(text(options, 'w'));
        const h = numberOption(text(options, 'h'));
        const drill = numberOption(text(options, 'drill'));
        if (x === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--x <mm>` must be a finite number' });
            return 2;
        }
        if (y === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--y <mm>` must be a finite number' });
            return 2;
        }
        if (w === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--w <mm>` must be a finite number' });
            return 2;
        }
        if (h === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--h <mm>` must be a finite number' });
            return 2;
        }
        if (text(options, 'drill') !== undefined && drill === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--drill <mm>` must be a finite number' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.addPad({
                footprint: footprintId,
                number: padNumber,
                x,
                y,
                w,
                h,
                net: text(options, 'net'),
                drill,
            });
            ctx.emitResult(result, `added pad ${footprintId}:${padNumber}`);
            return 0;
        });
        return 0;
    });

pad.command('update', 'cli.cmd.footprint-pad-update')
    .option('--footprint <id>', 'cli.opt.footprint-id')
    .option('--number <n>', 'cli.opt.pad-number')
    .option('--x <mm>', 'cli.opt.x')
    .option('--y <mm>', 'cli.opt.y')
    .option('--w <mm>', 'cli.opt.w')
    .option('--h <mm>', 'cli.opt.h')
    .option('--net <name>', 'cli.opt.net')
    .option('--drill <mm>', 'cli.opt.pad-drill')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const footprintId = text(options, 'footprint');
        const padNumber = text(options, 'number');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (footprintId === undefined || footprintId.length === 0) {
            ctx.fail('open-pcb.cli.footprint-required', { message: '`--footprint <id>` is required' });
            return 2;
        }
        if (padNumber === undefined || padNumber.length === 0) {
            ctx.fail('open-pcb.cli.number-required', { message: '`--number <n>` is required' });
            return 2;
        }
        const x = numberOption(text(options, 'x'));
        const y = numberOption(text(options, 'y'));
        const w = numberOption(text(options, 'w'));
        const h = numberOption(text(options, 'h'));
        const drill = numberOption(text(options, 'drill'));
        if (text(options, 'x') !== undefined && x === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--x <mm>` must be a finite number' });
            return 2;
        }
        if (text(options, 'y') !== undefined && y === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--y <mm>` must be a finite number' });
            return 2;
        }
        if (text(options, 'w') !== undefined && w === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--w <mm>` must be a finite number' });
            return 2;
        }
        if (text(options, 'h') !== undefined && h === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--h <mm>` must be a finite number' });
            return 2;
        }
        if (text(options, 'drill') !== undefined && drill === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--drill <mm>` must be a finite number' });
            return 2;
        }
        if (
            x === undefined &&
            y === undefined &&
            w === undefined &&
            h === undefined &&
            text(options, 'net') === undefined &&
            drill === undefined
        ) {
            ctx.fail('open-pcb.cli.number-invalid', { message: 'an update needs `--x`, `--y`, `--w`, `--h`, `--net` or `--drill`' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.updatePad({
                footprint: footprintId,
                number: padNumber,
                x,
                y,
                w,
                h,
                net: text(options, 'net'),
                drill,
            });
            ctx.emitResult(result, `updated pad ${footprintId}:${padNumber}`);
            return 0;
        });
        return 0;
    });

pad.command('delete', 'cli.cmd.footprint-pad-delete')
    .option('--footprint <id>', 'cli.opt.footprint-id')
    .option('--number <n>', 'cli.opt.pad-number')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const footprintId = text(options, 'footprint');
        const padNumber = text(options, 'number');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (footprintId === undefined || footprintId.length === 0) {
            ctx.fail('open-pcb.cli.footprint-required', { message: '`--footprint <id>` is required' });
            return 2;
        }
        if (padNumber === undefined || padNumber.length === 0) {
            ctx.fail('open-pcb.cli.number-required', { message: '`--number <n>` is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.deletePad({ footprint: footprintId, number: padNumber });
            ctx.emitResult(result, `deleted pad ${footprintId}:${result.deleted ?? padNumber}`);
            return 0;
        });
        return 0;
    });

const track = cli.command('track', 'cli.cmd.track');
track
    .command('add', 'cli.cmd.track-add')
    .option('--net <name>', 'cli.opt.net')
    .option('--layer <layer>', 'cli.opt.layer')
    .option('--from <x,y>', 'cli.opt.from')
    .option('--to <x,y>', 'cli.opt.to')
    .option('--width <mm>', 'cli.opt.width')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        const layer = text(options, 'layer');
        const from = pair(text(options, 'from'));
        const to = pair(text(options, 'to'));
        const width = numberOption(text(options, 'width'));
        if (layer === undefined || layer.length === 0) {
            ctx.fail('open-pcb.cli.layer-required', { message: '`--layer <layer>` is required' });
            return 2;
        }
        if (from === undefined) {
            ctx.fail('open-pcb.cli.point-required', { message: '`--from <x,y>` needs two numbers' });
            return 2;
        }
        if (to === undefined) {
            ctx.fail('open-pcb.cli.point-required', { message: '`--to <x,y>` needs two numbers' });
            return 2;
        }
        if (width === undefined || width <= 0) {
            ctx.fail('open-pcb.cli.width-required', { message: '`--width <mm>` must be greater than zero' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.addTrack({ net: text(options, 'net'), layer, from, to, width });
            ctx.emitResult(result, `added ${result.object?.id ?? 'track'} on ${layer}`);
            return 0;
        });
        return 0;
    });

track
    .command('update', 'cli.cmd.track-update')
    .option('--id <id>', 'cli.opt.id')
    .option('--net <name>', 'cli.opt.net')
    .option('--layer <layer>', 'cli.opt.layer')
    .option('--width <mm>', 'cli.opt.width')
    .option('--from <x,y>', 'cli.opt.from')
    .option('--to <x,y>', 'cli.opt.to')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const id = text(options, 'id');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (id === undefined || id.length === 0) {
            ctx.fail('open-pcb.cli.id-required', { message: '`--id <id>` is required' });
            return 2;
        }
        const net = text(options, 'net');
        const layer = text(options, 'layer');
        const widthText = text(options, 'width');
        const width = numberOption(widthText);
        if (widthText !== undefined && width === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--width <mm>` must be a finite number' });
            return 2;
        }
        const fromText = text(options, 'from');
        const from = pair(fromText);
        if (fromText !== undefined && from === undefined) {
            ctx.fail('open-pcb.cli.point-required', { message: '`--from <x,y>` needs two numbers' });
            return 2;
        }
        const toText = text(options, 'to');
        const to = pair(toText);
        if (toText !== undefined && to === undefined) {
            ctx.fail('open-pcb.cli.point-required', { message: '`--to <x,y>` needs two numbers' });
            return 2;
        }
        if (net === undefined && layer === undefined && width === undefined && from === undefined && to === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', {
                message: 'an update needs `--net`, `--layer`, `--width`, `--from` or `--to`',
            });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.updateTrack({ id, net, layer, width, from, to });
            ctx.emitResult(result, `updated ${result.object?.id ?? id}`);
            return 0;
        });
        return 0;
    });

track
    .command('split', 'cli.cmd.track-split')
    .option('--id <id>', 'cli.opt.id')
    .option('--x <mm>', 'cli.opt.x')
    .option('--y <mm>', 'cli.opt.y')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const id = text(options, 'id');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (id === undefined || id.length === 0) {
            ctx.fail('open-pcb.cli.id-required', { message: '`--id <id>` is required' });
            return 2;
        }
        const xText = text(options, 'x');
        const x = numberOption(xText);
        if (xText === undefined || x === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--x <mm>` must be a finite number' });
            return 2;
        }
        const yText = text(options, 'y');
        const y = numberOption(yText);
        if (yText === undefined || y === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--y <mm>` must be a finite number' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.splitTrack({ id, x, y });
            ctx.emitResult(result, `split ${id} into ${result.first.id} and ${result.second.id}`);
            return 0;
        });
        return 0;
    });

track
    .command('delete', 'cli.cmd.track-delete')
    .option('--id <id>', 'cli.opt.id')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const id = text(options, 'id');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (id === undefined || id.length === 0) {
            ctx.fail('open-pcb.cli.id-required', { message: '`--id <id>` is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.deleteTrack(id);
            ctx.emitResult(result, `deleted ${result.deleted ?? id}`);
            return 0;
        });
        return 0;
    });

const via = cli.command('via', 'cli.cmd.via');
via
    .command('add', 'cli.cmd.via-add')
    .option('--net <name>', 'cli.opt.net')
    .option('--at <x,y>', 'cli.opt.at')
    .option('--diameter <mm>', 'cli.opt.diameter')
    .option('--drill <mm>', 'cli.opt.drill')
    .option('--from-layer <layer>', 'cli.opt.from-layer')
    .option('--to-layer <layer>', 'cli.opt.to-layer')
    .option('--type <type>', 'cli.opt.type')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        const at = pair(text(options, 'at'));
        const diameter = numberOption(text(options, 'diameter'));
        const drill = numberOption(text(options, 'drill'));
        if (at === undefined) {
            ctx.fail('open-pcb.cli.point-required', { message: '`--at <x,y>` needs two numbers' });
            return 2;
        }
        if (diameter === undefined || diameter <= 0) {
            ctx.fail('open-pcb.cli.diameter-required', { message: '`--diameter <mm>` must be greater than zero' });
            return 2;
        }
        if (drill === undefined || drill <= 0) {
            ctx.fail('open-pcb.cli.drill-required', { message: '`--drill <mm>` must be greater than zero' });
            return 2;
        }
        const kind = text(options, 'type');
        if (kind !== undefined && kind !== 'micro') {
            ctx.fail('open-pcb.cli.type-invalid', { message: '`--type` accepts only `micro`' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.addVia({
                net: text(options, 'net'),
                at,
                diameter,
                drill,
                fromLayer: text(options, 'from-layer'),
                toLayer: text(options, 'to-layer'),
                kind,
            });
            ctx.emitResult(result, `added ${result.object?.id ?? 'via'} at ${at[0]},${at[1]}`);
            return 0;
        });
        return 0;
    });

via
    .command('update', 'cli.cmd.via-update')
    .option('--id <id>', 'cli.opt.id')
    .option('--net <name>', 'cli.opt.net')
    .option('--at <x,y>', 'cli.opt.at')
    .option('--diameter <mm>', 'cli.opt.diameter')
    .option('--drill <mm>', 'cli.opt.drill')
    .option('--type <type>', 'cli.opt.type')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const id = text(options, 'id');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (id === undefined || id.length === 0) {
            ctx.fail('open-pcb.cli.id-required', { message: '`--id <id>` is required' });
            return 2;
        }
        const net = text(options, 'net');
        const atText = text(options, 'at');
        const at = pair(atText);
        if (atText !== undefined && at === undefined) {
            ctx.fail('open-pcb.cli.point-required', { message: '`--at <x,y>` needs two numbers' });
            return 2;
        }
        const diameterText = text(options, 'diameter');
        const diameter = numberOption(diameterText);
        if (diameterText !== undefined && diameter === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--diameter <mm>` must be a finite number' });
            return 2;
        }
        const drillText = text(options, 'drill');
        const drill = numberOption(drillText);
        if (drillText !== undefined && drill === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--drill <mm>` must be a finite number' });
            return 2;
        }
        const kind = text(options, 'type');
        if (kind !== undefined && kind !== 'micro') {
            ctx.fail('open-pcb.cli.type-invalid', { message: '`--type` accepts only `micro`' });
            return 2;
        }
        if (net === undefined && at === undefined && diameter === undefined && drill === undefined && kind === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', {
                message: 'an update needs `--net`, `--at`, `--diameter`, `--drill` or `--type`',
            });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.updateVia({ id, net, at, diameter, drill, kind });
            ctx.emitResult(result, `updated ${result.object?.id ?? id}`);
            return 0;
        });
        return 0;
    });

via
    .command('delete', 'cli.cmd.via-delete')
    .option('--id <id>', 'cli.opt.id')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const id = text(options, 'id');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (id === undefined || id.length === 0) {
            ctx.fail('open-pcb.cli.id-required', { message: '`--id <id>` is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.deleteVia(id);
            ctx.emitResult(result, `deleted ${result.deleted ?? id}`);
            return 0;
        });
        return 0;
    });

via
    .command('backdrill', 'cli.cmd.via-backdrill')
    .option('--id <id>', 'cli.opt.id')
    .option('--depth <mm>', 'cli.opt.depth')
    .option('--stub <mm>', 'cli.opt.stub')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const id = text(options, 'id');
        const depth = numberOption(text(options, 'depth'));
        const stub = numberOption(text(options, 'stub'));
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (id === undefined || id.length === 0) {
            ctx.fail('open-pcb.cli.id-required', { message: '`--id <id>` is required' });
            return 2;
        }
        if (text(options, 'depth') !== undefined && (depth === undefined || depth <= 0)) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--depth <mm>` must be greater than zero' });
            return 2;
        }
        if (text(options, 'stub') !== undefined && (stub === undefined || stub < 0)) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--stub <mm>` must not be negative' });
            return 2;
        }
        if (depth === undefined && stub === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--depth <mm>` or `--stub <mm>` is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.setViaBackDrill({ id, depth, stub });
            ctx.emitResult(result, `back drill ${id} depth=${depth ?? '-'} stub=${stub ?? '-'}`);
            return 0;
        });
        return 0;
    });

const slot = cli.command('slot', 'cli.cmd.slot');
slot
    .command('add', 'cli.cmd.slot-add')
    .option('--net <name>', 'cli.opt.net')
    .option('--from <x,y>', 'cli.opt.from')
    .option('--to <x,y>', 'cli.opt.to')
    .option('--width <mm>', 'cli.opt.slot-width')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const from = pair(text(options, 'from'));
        const to = pair(text(options, 'to'));
        const width = numberOption(text(options, 'width'));
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (from === undefined) {
            ctx.fail('open-pcb.cli.point-required', { message: '`--from <x,y>` needs two numbers' });
            return 2;
        }
        if (to === undefined) {
            ctx.fail('open-pcb.cli.point-required', { message: '`--to <x,y>` needs two numbers' });
            return 2;
        }
        if (width === undefined || width <= 0) {
            ctx.fail('open-pcb.cli.width-required', { message: '`--width <mm>` must be greater than zero' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.addSlot({ net: text(options, 'net'), from, to, width });
            ctx.emitResult(result, `added ${result.object?.id ?? 'slot'} ${from[0]},${from[1]} to ${to[0]},${to[1]}`);
            return 0;
        });
        return 0;
    });

slot
    .command('update', 'cli.cmd.slot-update')
    .option('--id <id>', 'cli.opt.id')
    .option('--net <name>', 'cli.opt.net')
    .option('--from <x,y>', 'cli.opt.from')
    .option('--to <x,y>', 'cli.opt.to')
    .option('--width <mm>', 'cli.opt.slot-width')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const id = text(options, 'id');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (id === undefined || id.length === 0) {
            ctx.fail('open-pcb.cli.id-required', { message: '`--id <id>` is required' });
            return 2;
        }
        const net = text(options, 'net');
        const fromText = text(options, 'from');
        const from = pair(fromText);
        if (fromText !== undefined && from === undefined) {
            ctx.fail('open-pcb.cli.point-required', { message: '`--from <x,y>` needs two numbers' });
            return 2;
        }
        const toText = text(options, 'to');
        const to = pair(toText);
        if (toText !== undefined && to === undefined) {
            ctx.fail('open-pcb.cli.point-required', { message: '`--to <x,y>` needs two numbers' });
            return 2;
        }
        const widthText = text(options, 'width');
        const width = numberOption(widthText);
        if (widthText !== undefined && width === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--width <mm>` must be a finite number' });
            return 2;
        }
        if (net === undefined && from === undefined && to === undefined && width === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', {
                message: 'an update needs `--net`, `--from`, `--to` or `--width`',
            });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.updateSlot({ id, net, from, to, width });
            ctx.emitResult(result, `updated ${result.object?.id ?? id}`);
            return 0;
        });
        return 0;
    });

slot
    .command('delete', 'cli.cmd.slot-delete')
    .option('--id <id>', 'cli.opt.id')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const id = text(options, 'id');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (id === undefined || id.length === 0) {
            ctx.fail('open-pcb.cli.id-required', { message: '`--id <id>` is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.deleteSlot(id);
            ctx.emitResult(result, `deleted ${result.deleted ?? id}`);
            return 0;
        });
        return 0;
    });

const zone = cli.command('zone', 'cli.cmd.zone');
zone
    .command('add', 'cli.cmd.zone-add')
    .option('--layer <layer>', 'cli.opt.layer')
    .option('--net <name>', 'cli.opt.net')
    .option('--points <list>', 'cli.opt.points')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const layer = text(options, 'layer');
        const points = polygon(text(options, 'points'));
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (layer === undefined || layer.length === 0) {
            ctx.fail('open-pcb.cli.layer-required', { message: '`--layer <layer>` is required' });
            return 2;
        }
        if (points === undefined) {
            ctx.fail('open-pcb.cli.point-required', {
                message: '`--points` needs at least three `x,y` points separated by spaces or semicolons',
            });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.addZone({ layer, net: text(options, 'net'), points });
            ctx.emitResult(result, `added ${result.object?.id ?? 'zone'} on ${layer}`);
            return 0;
        });
        return 0;
    });

zone
    .command('delete', 'cli.cmd.zone-delete')
    .option('--id <id>', 'cli.opt.id')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const id = text(options, 'id');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (id === undefined || id.length === 0) {
            ctx.fail('open-pcb.cli.id-required', { message: '`--id <id>` is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.deleteZone(id);
            ctx.emitResult(result, `deleted ${result.deleted ?? id}`);
            return 0;
        });
        return 0;
    });

zone
    .command('update', 'cli.cmd.zone-update')
    .option('--id <id>', 'cli.opt.id')
    .option('--layer <layer>', 'cli.opt.layer')
    .option('--net <name>', 'cli.opt.net')
    .option('--points <list>', 'cli.opt.points')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const id = text(options, 'id');
        const layer = text(options, 'layer');
        const net = text(options, 'net');
        const points = polygon(text(options, 'points'));
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (id === undefined || id.length === 0) {
            ctx.fail('open-pcb.cli.id-required', { message: '`--id <id>` is required' });
            return 2;
        }
        if (layer === undefined && net === undefined && text(options, 'points') === undefined) {
            ctx.fail('open-pcb.cli.zone-update-empty', {
                message: 'a zone update needs `--layer`, `--net` or `--points`',
            });
            return 2;
        }
        if (text(options, 'points') !== undefined && points === undefined) {
            ctx.fail('open-pcb.cli.point-required', {
                message: '`--points` needs at least three `x,y` points separated by spaces or semicolons',
            });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.updateZone({ id, layer, net, points });
            ctx.emitResult(result, `updated ${result.object?.id ?? id}`);
            return 0;
        });
        return 0;
    });

zone
    .command('list', 'cli.cmd.zone-list')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const report = await board.zoneReport();
            const layers = report.layers.map((entry) => `${entry.layer}=${entry.area}mm2`).join(' ') || 'none';
            ctx.emitResult(report, `zones=${report.count} total=${report.totalArea}mm2 ${layers}`);
            return 0;
        });
        return 0;
    });

const textItem = cli.command('text', 'cli.cmd.text');
textItem
    .command('add', 'cli.cmd.text-add')
    .option('--text <text>', 'cli.opt.text')
    .option('--layer <layer>', 'cli.opt.layer')
    .option('--at <x,y>', 'cli.opt.at')
    .option('--rotation <deg>', 'cli.opt.rotation')
    .option('--size <mm>', 'cli.opt.text-size')
    .option('--thickness <mm>', 'cli.opt.text-thickness')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const content = text(options, 'text');
        const layer = text(options, 'layer');
        const at = pair(text(options, 'at'));
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (content === undefined || content.length === 0) {
            ctx.fail('open-pcb.cli.text-required', { message: '`--text <text>` is required' });
            return 2;
        }
        if (layer === undefined || layer.length === 0) {
            ctx.fail('open-pcb.cli.layer-required', { message: '`--layer <layer>` is required' });
            return 2;
        }
        if (at === undefined) {
            ctx.fail('open-pcb.cli.point-required', { message: '`--at` needs an `x,y` position' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.addText({
                text: content,
                layer,
                at,
                rotation: numberOption(text(options, 'rotation')),
                size: numberOption(text(options, 'size')),
                thickness: numberOption(text(options, 'thickness')),
            });
            ctx.emitResult(result, `added ${result.object?.id ?? 'text'} on ${layer}`);
            return 0;
        });
        return 0;
    });

textItem
    .command('update', 'cli.cmd.text-update')
    .option('--id <id>', 'cli.opt.id')
    .option('--text <text>', 'cli.opt.text')
    .option('--layer <layer>', 'cli.opt.layer')
    .option('--at <x,y>', 'cli.opt.at')
    .option('--rotation <deg>', 'cli.opt.rotation')
    .option('--size <mm>', 'cli.opt.text-size')
    .option('--thickness <mm>', 'cli.opt.text-thickness')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const id = text(options, 'id');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (id === undefined || id.length === 0) {
            ctx.fail('open-pcb.cli.id-required', { message: '`--id <id>` is required' });
            return 2;
        }
        const update = {
            id,
            text: text(options, 'text'),
            layer: text(options, 'layer'),
            at: pair(text(options, 'at')),
            rotation: numberOption(text(options, 'rotation')),
            size: numberOption(text(options, 'size')),
            thickness: numberOption(text(options, 'thickness')),
        };
        if (
            update.text === undefined
            && update.layer === undefined
            && update.at === undefined
            && update.rotation === undefined
            && update.size === undefined
            && update.thickness === undefined
        ) {
            ctx.fail('open-pcb.cli.text-update-empty', {
                message: 'a text update needs `--text`, `--layer`, `--at`, `--rotation`, `--size` or `--thickness`',
            });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.updateText(update);
            ctx.emitResult(result, `updated ${result.object?.id ?? id}`);
            return 0;
        });
        return 0;
    });

textItem
    .command('delete', 'cli.cmd.text-delete')
    .option('--id <id>', 'cli.opt.id')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const id = text(options, 'id');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (id === undefined || id.length === 0) {
            ctx.fail('open-pcb.cli.id-required', { message: '`--id <id>` is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.deleteText(id);
            ctx.emitResult(result, `deleted ${result.deleted ?? id}`);
            return 0;
        });
        return 0;
    });

textItem
    .command('list', 'cli.cmd.text-list')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const report = await board.listTexts();
            ctx.emitResult(report, `texts=${report.count}`);
            return 0;
        });
        return 0;
    });

const drill = cli.command('drill', 'cli.cmd.drill');
drill
    .command('report', 'cli.cmd.drill-report')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const report = await board.drillReport();
            ctx.emitResult(
                report,
                `holes=${report.holeCount} slots=${report.slotCount} tools=${report.tools.length} backDrilled=${report.backDrilled}`,
            );
            return 0;
        });
        return 0;
    });

const keepout = cli.command('keepout', 'cli.cmd.keepout');
keepout
    .command('add', 'cli.cmd.keepout-add')
    .option('--layer <layer>', 'cli.opt.layer')
    .option('--points <list>', 'cli.opt.points')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const points = polygon(text(options, 'points'));
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (points === undefined) {
            ctx.fail('open-pcb.cli.point-required', {
                message: '`--points` needs at least three `x,y` points separated by spaces or semicolons',
            });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.addKeepout({ layer: text(options, 'layer'), points });
            ctx.emitResult(result, `added ${result.object?.id ?? 'keepout'}`);
            return 0;
        });
        return 0;
    });

keepout
    .command('delete', 'cli.cmd.keepout-delete')
    .option('--id <id>', 'cli.opt.id')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const id = text(options, 'id');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (id === undefined || id.length === 0) {
            ctx.fail('open-pcb.cli.id-required', { message: '`--id <id>` is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.deleteKeepout(id);
            ctx.emitResult(result, `deleted ${result.deleted ?? id}`);
            return 0;
        });
        return 0;
    });

keepout
    .command('update', 'cli.cmd.keepout-update')
    .option('--id <id>', 'cli.opt.id')
    .option('--layer <layer>', 'cli.opt.layer')
    .option('--points <list>', 'cli.opt.points')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const id = text(options, 'id');
        const layer = text(options, 'layer');
        const points = polygon(text(options, 'points'));
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (id === undefined || id.length === 0) {
            ctx.fail('open-pcb.cli.id-required', { message: '`--id <id>` is required' });
            return 2;
        }
        if (layer === undefined && text(options, 'points') === undefined) {
            ctx.fail('open-pcb.cli.keepout-update-empty', {
                message: 'a keepout update needs `--layer` or `--points`',
            });
            return 2;
        }
        if (text(options, 'points') !== undefined && points === undefined) {
            ctx.fail('open-pcb.cli.point-required', {
                message: '`--points` needs at least three `x,y` points separated by spaces or semicolons',
            });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.updateKeepout({ id, layer, points });
            ctx.emitResult(result, `updated ${result.object?.id ?? id}`);
            return 0;
        });
        return 0;
    });

keepout
    .command('list', 'cli.cmd.keepout-list')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const report = await board.keepoutReport();
            ctx.emitResult(report, `keepouts=${report.count} total=${report.totalArea}mm2`);
            return 0;
        });
        return 0;
    });

const netclass = cli.command('netclass', 'cli.cmd.netclass');
netclass
    .command('add', 'cli.cmd.netclass-add')
    .option('--name <name>', 'cli.opt.name')
    .option('--width <mm>', 'cli.opt.width')
    .option('--clearance <mm>', 'cli.opt.clearance')
    .option('--via-diameter <mm>', 'cli.opt.via-diameter')
    .option('--via-drill <mm>', 'cli.opt.via-drill')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const name = text(options, 'name');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (name === undefined || name.length === 0) {
            ctx.fail('open-pcb.cli.name-required', { message: '`--name <name>` is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.addNetClass({
                name,
                width: numberOption(text(options, 'width')),
                clearance: numberOption(text(options, 'clearance')),
                viaDiameter: numberOption(text(options, 'via-diameter')),
                viaDrill: numberOption(text(options, 'via-drill')),
            });
            ctx.emitResult(result, `declared ${result.object?.id ?? 'net class'} ${name}`);
            return 0;
        });
        return 0;
    });

netclass
    .command('delete', 'cli.cmd.netclass-delete')
    .option('--id <id>', 'cli.opt.id')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const id = text(options, 'id');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (id === undefined || id.length === 0) {
            ctx.fail('open-pcb.cli.id-required', { message: '`--id <id>` is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.deleteNetClass(id);
            ctx.emitResult(result, `deleted ${result.deleted ?? id}`);
            return 0;
        });
        return 0;
    });

netclass
    .command('assign', 'cli.cmd.netclass-assign')
    .option('--net <name>', 'cli.opt.net')
    .option('--class <name-or-id>', 'cli.opt.class')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const net = text(options, 'net');
        const netClass = text(options, 'class');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (net === undefined || net.length === 0) {
            ctx.fail('open-pcb.cli.net-required', { message: '`--net <name>` is required' });
            return 2;
        }
        if (netClass === undefined || netClass.length === 0) {
            ctx.fail('open-pcb.cli.class-required', { message: '`--class <name-or-id>` is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.assignNetClass({ net, class: netClass });
            ctx.emitResult(result, `assigned ${net} to ${netClass}`);
            return 0;
        });
        return 0;
    });

const testpoint = cli.command('testpoint', 'cli.cmd.testpoint');
testpoint
    .command('add', 'cli.cmd.testpoint-add')
    .option('--target <target>', 'cli.opt.target')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const target = text(options, 'target');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (target === undefined || target.length === 0) {
            ctx.fail('open-pcb.cli.target-required', {
                message: '`--target <footprint-id:pad-number|via-id>` is required',
            });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.addTestPoint({ target });
            ctx.emitResult(result, `declared ${result.object?.id ?? 'test point'} on ${target}`);
            return 0;
        });
        return 0;
    });

testpoint
    .command('delete', 'cli.cmd.testpoint-delete')
    .option('--id <id>', 'cli.opt.id')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const id = text(options, 'id');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (id === undefined || id.length === 0) {
            ctx.fail('open-pcb.cli.id-required', { message: '`--id <id>` is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.deleteTestPoint(id);
            ctx.emitResult(result, `deleted ${result.deleted ?? id}`);
            return 0;
        });
        return 0;
    });

const fiducial = cli.command('fiducial', 'cli.cmd.fiducial');
fiducial
    .command('add', 'cli.cmd.fiducial-add')
    .option('--layer <layer>', 'cli.opt.layer')
    .option('--at <x,y>', 'cli.opt.at')
    .option('--diameter <mm>', 'cli.opt.diameter')
    .option('--name <label>', 'cli.opt.name')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const layer = text(options, 'layer');
        const at = pair(text(options, 'at'));
        const diameter = numberOption(text(options, 'diameter'));
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (layer === undefined || layer.length === 0) {
            ctx.fail('open-pcb.cli.layer-required', { message: '`--layer <layer>` is required' });
            return 2;
        }
        if (at === undefined) {
            ctx.fail('open-pcb.cli.point-required', { message: '`--at <x,y>` needs two numbers' });
            return 2;
        }
        if (diameter === undefined || diameter <= 0) {
            ctx.fail('open-pcb.cli.diameter-required', { message: '`--diameter <mm>` must be greater than zero' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.addFiducial({ layer, at, diameter, name: text(options, 'name') });
            ctx.emitResult(result, `added ${result.object?.id ?? 'fiducial'} at ${at[0]},${at[1]}`);
            return 0;
        });
        return 0;
    });

fiducial
    .command('delete', 'cli.cmd.fiducial-delete')
    .option('--id <id>', 'cli.opt.id')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const id = text(options, 'id');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (id === undefined || id.length === 0) {
            ctx.fail('open-pcb.cli.id-required', { message: '`--id <id>` is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.deleteFiducial(id);
            ctx.emitResult(result, `deleted ${result.deleted ?? id}`);
            return 0;
        });
        return 0;
    });

fiducial
    .command('list', 'cli.cmd.fiducial-list')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const report = await board.listFiducials();
            ctx.emitResult(report, `fiducials=${report.count}`);
            return 0;
        });
        return 0;
    });

fiducial
    .command('report', 'cli.cmd.fiducial-report')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const report = await board.fiducialReport();
            ctx.emitResult(
                report,
                `fiducials=${report.count} front=${report.front} back=${report.back} status=${report.status}`,
            );
            return 0;
        });
        return 0;
    });

const net = cli.command('net', 'cli.cmd.net');
net
    .command('add', 'cli.cmd.net-add')
    .option('--name <name>', 'cli.opt.name')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const name = text(options, 'name');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (name === undefined || name.length === 0) {
            ctx.fail('open-pcb.cli.name-required', { message: '`--name <name>` is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.addNet({ name });
            ctx.emitResult(result, `added ${result.object?.id ?? 'net'} ${name}`);
            return 0;
        });
        return 0;
    });

const diffpair = cli.command('diffpair', 'cli.cmd.diffpair');
diffpair
    .command('add', 'cli.cmd.diffpair-add')
    .option('--positive <net>', 'cli.opt.positive')
    .option('--negative <net>', 'cli.opt.negative')
    .option('--width <mm>', 'cli.opt.width')
    .option('--gap <mm>', 'cli.opt.gap')
    .option('--max-skew <mm>', 'cli.opt.max-skew')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const positive = text(options, 'positive');
        const negative = text(options, 'negative');
        const width = numberOption(text(options, 'width'));
        const gap = numberOption(text(options, 'gap'));
        const maxSkew = numberOption(text(options, 'max-skew'));
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (positive === undefined || positive.length === 0 || negative === undefined || negative.length === 0) {
            ctx.fail('open-pcb.cli.name-required', { message: '`--positive <net>` and `--negative <net>` are required' });
            return 2;
        }
        if (width === undefined || width <= 0) {
            ctx.fail('open-pcb.cli.width-required', { message: '`--width <mm>` must be greater than zero' });
            return 2;
        }
        if (gap === undefined || gap <= 0) {
            ctx.fail('open-pcb.cli.gap-required', { message: '`--gap <mm>` must be greater than zero' });
            return 2;
        }
        if (text(options, 'max-skew') !== undefined && (maxSkew === undefined || maxSkew < 0)) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--max-skew <mm>` must not be negative' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.addDifferentialPair({ positive, negative, width, gap, maxSkew });
            ctx.emitResult(result, `added differential pair ${positive}/${negative}`);
            return 0;
        });
        return 0;
    });

diffpair
    .command('delete', 'cli.cmd.diffpair-delete')
    .option('--id <id>', 'cli.opt.id')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const id = text(options, 'id');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (id === undefined || id.length === 0) {
            ctx.fail('open-pcb.cli.id-required', { message: '`--id <id>` is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.deleteDifferentialPair(id);
            ctx.emitResult(result, `deleted ${result.deleted ?? id}`);
            return 0;
        });
        return 0;
    });

cli
    .command('teardrop', 'cli.cmd.teardrop')
    .option('--shape <shape>', 'cli.opt.shape')
    .option('--length <mm>', 'cli.opt.length')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const shape = text(options, 'shape') as 'linear' | undefined;
        const length = numberOption(text(options, 'length'));
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (shape !== undefined && shape !== 'linear') {
            ctx.fail('open-pcb.cli.shape-invalid', { message: 'only `--shape linear` is implemented' });
            return 2;
        }
        if (text(options, 'length') !== undefined && (length === undefined || length <= 0)) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--length <mm>` must be greater than zero' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.addTeardrops({ shape, length });
            ctx.emitResult(result, `generated teardrops for ${file}`);
            return 0;
        });
        return 0;
    });

cli
    .command('stitch', 'cli.cmd.stitch')
    .option('--net <name>', 'cli.opt.net')
    .option('--spacing <mm>', 'cli.opt.spacing')
    .option('--inset <mm>', 'cli.opt.inset')
    .option('--diameter <mm>', 'cli.opt.diameter')
    .option('--drill <mm>', 'cli.opt.drill')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const net = text(options, 'net');
        const spacing = numberOption(text(options, 'spacing'));
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (net === undefined || net.length === 0) {
            ctx.fail('open-pcb.cli.net-required', { message: '`--net <name>` is required' });
            return 2;
        }
        if (spacing === undefined || spacing <= 0) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--spacing <mm>` must be greater than zero' });
            return 2;
        }
        const inset = numberOption(text(options, 'inset'));
        const diameter = numberOption(text(options, 'diameter'));
        const drill = numberOption(text(options, 'drill'));
        if (text(options, 'inset') !== undefined && (inset === undefined || inset < 0)) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--inset <mm>` must not be negative' });
            return 2;
        }
        if (text(options, 'diameter') !== undefined && (diameter === undefined || diameter <= 0)) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--diameter <mm>` must be greater than zero' });
            return 2;
        }
        if (text(options, 'drill') !== undefined && (drill === undefined || drill <= 0)) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--drill <mm>` must be greater than zero' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.stitchVias({ net, spacing, inset, diameter, drill });
            const object = result.object ?? {};
            ctx.emitResult(
                result,
                `stitched ${String(object.vias ?? 0)} vias on ${net}`
                    + ` (${String(object.skipped ?? 0)} skipped, spacing ${String(object.spacing ?? '-')})`,
            );
            return 0;
        });
        return 0;
    });

cli
    .command('route', 'cli.cmd.route')
    .option('--width <mm>', 'cli.opt.width')
    .option('--clearance <mm>', 'cli.opt.clearance')
    .option('--via <mm>', 'cli.opt.via')
    .option('--net <name>', 'cli.opt.net')
    .option('--netclass', 'cli.opt.netclass')
    .option('--style <style>', 'cli.opt.style')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        const width = numberOption(text(options, 'width'));
        const clearance = numberOption(text(options, 'clearance'));
        const viaDiameter = numberOption(text(options, 'via'));
        if (width !== undefined && width <= 0) {
            ctx.fail('open-pcb.cli.width-required', { message: '`--width <mm>` must be greater than zero' });
            return 2;
        }
        if (clearance !== undefined && clearance <= 0) {
            ctx.fail('open-pcb.cli.clearance-required', { message: '`--clearance <mm>` must be greater than zero' });
            return 2;
        }
        if (viaDiameter !== undefined && viaDiameter <= 0) {
            ctx.fail('open-pcb.cli.via-required', { message: '`--via <mm>` must be greater than zero' });
            return 2;
        }
        const style = text(options, 'style') as RouteStyle | undefined;
        if (style !== undefined && style !== 'orthogonal' && style !== 'chamfer45') {
            ctx.fail('open-pcb.cli.style-invalid', { message: '`--style` must be orthogonal or chamfer45' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const report = await board.route({
                width,
                clearance,
                viaDiameter,
                net: text(options, 'net'),
                style,
                useNetClasses: flag(options, 'netclass'),
            });
            ctx.emitResult(
                report,
                `routed ${report.routed} connection(s): ${report.tracks} tracks, ${report.vias} vias, ${report.unrouted.length} unrouted`,
            );
            return 0;
        });
        return 0;
    });

const layer = cli.command('layer', 'cli.cmd.layer');
layer
    .command('add', 'cli.cmd.layer-add')
    .option('--name <name>', 'cli.opt.name')
    .option('--kind <kind>', 'cli.opt.kind')
    .option('--color <css>', 'cli.opt.color')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const name = text(options, 'name');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (name === undefined || name.length === 0) {
            ctx.fail('open-pcb.cli.name-required', { message: '`--name <name>` is required' });
            return 2;
        }
        const kind = text(options, 'kind') as LayerKind | undefined;
        if (kind !== undefined && !layerKinds.includes(kind)) {
            ctx.fail('open-pcb.cli.kind-invalid', { message: '`--kind` must be copper, silkscreen, soldermask, assembly or mechanical' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.addLayer({ name, kind, color: text(options, 'color') });
            ctx.emitResult(result, `added layer ${name}`);
            return 0;
        });
        return 0;
    });

layer
    .command('rename', 'cli.cmd.layer-rename')
    .option('--id <id>', 'cli.opt.id')
    .option('--name <name>', 'cli.opt.name')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const id = text(options, 'id');
        const name = text(options, 'name');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (id === undefined || id.length === 0) {
            ctx.fail('open-pcb.cli.id-required', { message: '`--id <id>` is required' });
            return 2;
        }
        if (name === undefined || name.length === 0) {
            ctx.fail('open-pcb.cli.name-required', { message: '`--name <name>` is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.updateLayer({ id, name });
            ctx.emitResult(result, `renamed ${id} to ${name}`);
            return 0;
        });
        return 0;
    });

for (const [name, description, key, option] of [
    ['visible', 'cli.cmd.layer-visible', 'visible', 'cli.opt.visible'],
    ['lock', 'cli.cmd.layer-lock', 'locked', 'cli.opt.locked'],
] as const) {
    layer
        .command(name, description)
        .option('--id <id>', 'cli.opt.id')
        .option(name === 'visible' ? '--visible <bool>' : '--locked <bool>', option)
        .actionCtx(async (options, _args, ctx) => {
            const file = input(options);
            const id = text(options, 'id');
            const value = booleanOption(text(options, key));
            if (file === undefined) {
                ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
                return 2;
            }
            if (id === undefined || id.length === 0) {
                ctx.fail('open-pcb.cli.id-required', { message: '`--id <id>` is required' });
                return 2;
            }
            if (value === undefined) {
                ctx.fail('open-pcb.cli.boolean-invalid', { message: `\`--${key}\` must be true or false` });
                return 2;
            }
            await withBoard(file, async (board) => {
                const result = key === 'visible'
                    ? await board.updateLayer({ id, visible: value })
                    : await board.updateLayer({ id, locked: value });
                ctx.emitResult(result, `layer ${id} ${key}=${value}`);
                return 0;
            });
            return 0;
        });
}

const outline = cli.command('outline', 'cli.cmd.outline');
outline
    .command('set', 'cli.cmd.outline-set')
    .option('--from <x,y>', 'cli.opt.from')
    .option('--to <x,y>', 'cli.opt.to')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const from = pair(text(options, 'from'));
        const to = pair(text(options, 'to'));
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (from === undefined) {
            ctx.fail('open-pcb.cli.point-required', { message: '`--from <x,y>` needs two numbers' });
            return 2;
        }
        if (to === undefined) {
            ctx.fail('open-pcb.cli.point-required', { message: '`--to <x,y>` needs two numbers' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.setOutline({ from, to });
            ctx.emitResult(result, `outline ${from[0]},${from[1]} to ${to[0]},${to[1]}`);
            return 0;
        });
        return 0;
    });

const rules = cli.command('rules', 'cli.cmd.rules');
rules.command('show', 'cli.cmd.rules-show').actionCtx(async (options, _args, ctx) => {
    const file = input(options);
    if (file === undefined) {
        ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
        return 2;
    }
    await withBoard(file, async (board) => {
        const summary = await board.list();
        ctx.emitResult(summary.rules, describeRules(summary.rules));
        return 0;
    });
    return 0;
});

rules
    .command('set', 'cli.cmd.rules-set')
    .option('--hole-clearance <mm>', 'cli.opt.hole-clearance')
    .option('--copper-to-edge <mm>', 'cli.opt.copper-to-edge')
    .option('--component-to-edge <mm>', 'cli.opt.component-to-edge')
    .option('--hole-to-edge <mm>', 'cli.opt.hole-to-edge')
    .option('--solder-mask-expansion <mm>', 'cli.opt.solder-mask-expansion')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        const keys = ['hole-clearance', 'copper-to-edge', 'component-to-edge', 'hole-to-edge', 'solder-mask-expansion'] as const;
        const values = new Map<string, number>();
        for (const key of keys) {
            const raw = text(options, key);
            if (raw === undefined) {
                continue;
            }
            const value = Number(raw);
            if (!Number.isFinite(value)) {
                ctx.fail('open-pcb.cli.number-invalid', { message: `\`--${key}\` must be a finite number` });
                return 2;
            }
            values.set(key, value);
        }
        if (values.size === 0) {
            ctx.fail('open-pcb.cli.rules-required', { message: 'at least one design rule option is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.setRules({
                holeClearance: values.get('hole-clearance'),
                copperToEdge: values.get('copper-to-edge'),
                componentToEdge: values.get('component-to-edge'),
                holeToEdge: values.get('hole-to-edge'),
                solderMaskExpansion: values.get('solder-mask-expansion'),
            });
            ctx.emitResult(result, `rules revision=${result.revision} ${describeRules(result.board.rules)}`);
            return 0;
        });
        return 0;
    });

const copper = cli.command('copper', 'cli.cmd.copper');
copper.command('balance', 'cli.cmd.copper-balance').actionCtx(async (options, _args, ctx) => {
    const file = input(options);
    if (file === undefined) {
        ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
        return 2;
    }
    await withBoard(file, async (board) => {
        const report = await board.copperBalance();
        const layers = report.layers
            .map((layer) =>
                layer.coveragePercent === null
                    ? `${layer.layer}=${layer.area}mm2`
                    : `${layer.layer}=${layer.area}mm2 (${layer.coveragePercent}%)`,
            )
            .join(' ');
        const imbalance = report.imbalancePercent === null ? 'n/a' : `${report.imbalancePercent}%`;
        ctx.emitResult(report, `${layers} total=${report.totalArea}mm2 imbalance=${imbalance}`);
        return 0;
    });
    return 0;
});

cli.command('lengths', 'cli.cmd.lengths').actionCtx(async (options, _args, ctx) => {
    const file = input(options);
    if (file === undefined) {
        ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
        return 2;
    }
    await withBoard(file, async (board) => {
        const report = await board.netLengths();
        const nets = report.nets.map((net) => `${net.name}=${net.length}`).join(' ') || 'none';
        const longest = report.ranking[0];
        const ranking = longest === undefined ? 'longest=none' : `longest=${longest.name}(${longest.length})`;
        ctx.emitResult(report, `nets=${report.nets.length} total=${report.totals.length} ${nets} ${ranking}`);
        return 0;
    });
    return 0;
});

function panelTabStyle(raw: string | undefined): PanelTabStyle | undefined {
    if (raw === undefined) {
        return undefined;
    }
    return raw === 'v-cut' || raw === 'mouse-bite' || raw === 'none' ? raw : undefined;
}

cli
    .command('panel', 'cli.cmd.panel')
    .option('--columns <count>', 'cli.opt.columns')
    .option('--rows <count>', 'cli.opt.rows')
    .option('--gap-x <mm>', 'cli.opt.gap-x')
    .option('--gap-y <mm>', 'cli.opt.gap-y')
    .option('--rail <mm>', 'cli.opt.rail')
    .option('--tab <style>', 'cli.opt.tab')
    .option('--tab-width <mm>', 'cli.opt.tab-width')
    .option('--tab-spacing <mm>', 'cli.opt.tab-spacing')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const columns = countOption(text(options, 'columns'));
        const rows = countOption(text(options, 'rows'));
        const gaps = { gapX: numberOption(text(options, 'gap-x')), gapY: numberOption(text(options, 'gap-y')) };
        const rail = numberOption(text(options, 'rail'));
        const tab = panelTabStyle(text(options, 'tab'));
        const tabWidth = numberOption(text(options, 'tab-width'));
        const tabSpacing = numberOption(text(options, 'tab-spacing'));
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (columns === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--columns <count>` must be a positive integer' });
            return 2;
        }
        if (rows === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--rows <count>` must be a positive integer' });
            return 2;
        }
        const lengths = { 'gap-x': gaps.gapX, 'gap-y': gaps.gapY, rail, 'tab-width': tabWidth, 'tab-spacing': tabSpacing };
        for (const key of Object.keys(lengths) as (keyof typeof lengths)[]) {
            const value = lengths[key];
            if (text(options, key) !== undefined && (value === undefined || value < 0)) {
                ctx.fail('open-pcb.cli.number-invalid', { message: `\`--${key} <mm>\` must not be negative` });
                return 2;
            }
        }
        if (text(options, 'tab') !== undefined && tab === undefined) {
            ctx.fail('open-pcb.cli.tab-invalid', { message: '`--tab` must be v-cut, mouse-bite or none' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const report = await board.panelize({
                columns,
                rows,
                gapX: gaps.gapX,
                gapY: gaps.gapY,
                rail,
                tab,
                tabWidth,
                tabSpacing,
            });
            ctx.emitResult(
                report,
                `panel ${report.columns}x${report.rows} boards=${report.boardCount}`
                    + ` size=${report.panelSize.width}x${report.panelSize.height}mm tab=${report.tab}`
                    + ` tabs=${report.tabCount} utilization=${report.utilizationPercent}%`,
            );
            return 0;
        });
        return 0;
    });

const waiver = cli.command('waiver', 'cli.cmd.waiver');
waiver
    .command('add', 'cli.cmd.waiver-add')
    .option('--code <code>', 'cli.opt.code')
    .option('--object <id>', 'cli.opt.object')
    .option('--reason <text>', 'cli.opt.reason')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const code = text(options, 'code');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (code === undefined || code.length === 0) {
            ctx.fail('open-pcb.cli.code-required', { message: '`--code <code>` is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.addWaiver({ code, object: text(options, 'object'), reason: text(options, 'reason') });
            ctx.emitResult(result, `added ${result.object?.id ?? 'waiver'} for ${code}`);
            return 0;
        });
        return 0;
    });

waiver
    .command('delete', 'cli.cmd.waiver-delete')
    .option('--id <id>', 'cli.opt.id')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const id = text(options, 'id');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (id === undefined || id.length === 0) {
            ctx.fail('open-pcb.cli.id-required', { message: '`--id <id>` is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.deleteWaiver(id);
            ctx.emitResult(result, `deleted ${result.deleted ?? id}`);
            return 0;
        });
        return 0;
    });

const severity = cli.command('severity', 'cli.cmd.severity');
severity
    .command('set', 'cli.cmd.severity-set')
    .option('--code <code>', 'cli.opt.code')
    .option('--severity <level>', 'cli.opt.severity')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const code = text(options, 'code');
        const level = text(options, 'severity');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (code === undefined || code.length === 0) {
            ctx.fail('open-pcb.cli.code-required', { message: '`--code <code>` is required' });
            return 2;
        }
        if (level !== 'info' && level !== 'warning' && level !== 'error') {
            ctx.fail('open-pcb.cli.severity-invalid', { message: '`--severity` must be info, warning or error' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.setSeverity({ code, severity: level as Severity });
            ctx.emitResult(result, `${code} severity=${level}`);
            return 0;
        });
        return 0;
    });

severity
    .command('delete', 'cli.cmd.severity-delete')
    .option('--code <code>', 'cli.opt.code')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const code = text(options, 'code');
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (code === undefined || code.length === 0) {
            ctx.fail('open-pcb.cli.code-required', { message: '`--code <code>` is required' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.deleteSeverity(code);
            ctx.emitResult(result, `removed severity override for ${code}`);
            return 0;
        });
        return 0;
    });

const stackup = cli.command('stackup', 'cli.cmd.stackup');
stackup
    .command('set', 'cli.cmd.stackup-set')
    .option('--copper-thickness <mm>', 'cli.opt.copper-thickness')
    .option('--dielectric-height <mm>', 'cli.opt.dielectric-height')
    .option('--dielectric-constant <er>', 'cli.opt.dielectric-constant')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const copperThickness = numberOption(text(options, 'copper-thickness'));
        const dielectricHeight = numberOption(text(options, 'dielectric-height'));
        const dielectricConstant = numberOption(text(options, 'dielectric-constant'));
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (copperThickness === undefined || copperThickness <= 0) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--copper-thickness <mm>` must be greater than zero' });
            return 2;
        }
        if (dielectricHeight === undefined || dielectricHeight <= 0) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--dielectric-height <mm>` must be greater than zero' });
            return 2;
        }
        if (dielectricConstant === undefined || dielectricConstant < 1) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--dielectric-constant <er>` must be at least 1' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const result = await board.setStackup({ copperThickness, dielectricHeight, dielectricConstant });
            ctx.emitResult(
                result,
                `stackup copper=${copperThickness}mm dielectric=${dielectricHeight}mm er=${dielectricConstant}`,
            );
            return 0;
        });
        return 0;
    });

stackup.command('show', 'cli.cmd.stackup-show').actionCtx(async (options, _args, ctx) => {
    const file = input(options);
    if (file === undefined) {
        ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
        return 2;
    }
    await withBoard(file, async (board) => {
        const summary = await board.list();
        ctx.emitResult(summary.stackup, summary.stackup === null ? 'stackup unset' : JSON.stringify(summary.stackup));
        return 0;
    });
    return 0;
});

function impedanceStructure(raw: string | undefined): ImpedanceStructure | undefined {
    if (raw === undefined) {
        return undefined;
    }
    return raw === 'microstrip' || raw === 'stripline' || raw === 'diffMicrostrip' || raw === 'diffStripline'
        ? raw
        : undefined;
}

function differentialStructure(structure: ImpedanceStructure | undefined): boolean {
    return structure === 'diffMicrostrip' || structure === 'diffStripline';
}

/** Parses the report structure; `auto` and an empty value select the per-layer geometry. */
function reportStructure(raw: string | undefined): ImpedanceStructure | 'auto' | undefined {
    if (raw === undefined) {
        return undefined;
    }
    return raw === 'auto' || raw.trim() === '' ? 'auto' : impedanceStructure(raw);
}

/** Parses `a,b,...` into target impedances; an all-empty list stays empty for the model. */
function impedanceTargets(raw: string | undefined): number[] | undefined {
    if (raw === undefined) {
        return undefined;
    }
    const parts = raw.split(',').map((part) => part.trim());
    if (parts.every((part) => part.length === 0)) {
        return [];
    }
    const values = parts.map((part) => Number(part));
    return values.every((value) => Number.isFinite(value)) ? values : undefined;
}

stackup
    .command('impedance', 'cli.cmd.stackup-impedance')
    .option('--structure <microstrip|stripline|diffMicrostrip|diffStripline>', 'cli.opt.structure')
    .option('--width <mm>', 'cli.opt.width')
    .option('--gap <mm>', 'cli.opt.gap')
    .option('--dielectric-height <mm>', 'cli.opt.dielectric-height')
    .option('--dielectric-constant <er>', 'cli.opt.dielectric-constant')
    .option('--copper-thickness <mm>', 'cli.opt.copper-thickness')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const width = numberOption(text(options, 'width'));
        const structure = impedanceStructure(text(options, 'structure'));
        const gap = numberOption(text(options, 'gap'));
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (text(options, 'structure') !== undefined && structure === undefined) {
            ctx.fail('open-pcb.cli.structure-invalid', {
                message: '`--structure` must be microstrip, stripline, diffMicrostrip or diffStripline',
            });
            return 2;
        }
        if (width === undefined || width <= 0) {
            ctx.fail('open-pcb.cli.width-required', { message: '`--width <mm>` must be greater than zero' });
            return 2;
        }
        if (differentialStructure(structure) && (gap === undefined || gap <= 0)) {
            ctx.fail('open-pcb.cli.gap-required', {
                message: '`--gap <mm>` is required and must be greater than zero for a differential structure',
            });
            return 2;
        }
        if (!differentialStructure(structure) && gap !== undefined) {
            ctx.fail('open-pcb.cli.gap-invalid', {
                message: '`--gap <mm>` applies only to diffMicrostrip or diffStripline',
            });
            return 2;
        }
        await withBoard(file, async (board) => {
            const report = await board.impedance({
                structure,
                width,
                gap,
                dielectricHeight: numberOption(text(options, 'dielectric-height')),
                dielectricConstant: numberOption(text(options, 'dielectric-constant')),
                copperThickness: numberOption(text(options, 'copper-thickness')),
            });
            const modes =
                report.oddImpedance === undefined
                    ? ''
                    : ` gap=${report.gap}mm odd=${report.oddImpedance}ohm even=${report.evenImpedance}ohm`;
            ctx.emitResult(report, `${report.structure} width=${report.width}mm impedance=${report.impedance}ohm${modes}`);
            return 0;
        });
        return 0;
    });

stackup
    .command('width', 'cli.cmd.stackup-width')
    .option('--structure <microstrip|stripline|diffMicrostrip|diffStripline>', 'cli.opt.structure')
    .option('--impedance <ohm>', 'cli.opt.impedance')
    .option('--gap <mm>', 'cli.opt.gap')
    .option('--dielectric-height <mm>', 'cli.opt.dielectric-height')
    .option('--dielectric-constant <er>', 'cli.opt.dielectric-constant')
    .option('--copper-thickness <mm>', 'cli.opt.copper-thickness')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const target = numberOption(text(options, 'impedance'));
        const structure = impedanceStructure(text(options, 'structure'));
        const gap = numberOption(text(options, 'gap'));
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        if (text(options, 'structure') !== undefined && structure === undefined) {
            ctx.fail('open-pcb.cli.structure-invalid', {
                message: '`--structure` must be microstrip, stripline, diffMicrostrip or diffStripline',
            });
            return 2;
        }
        if (target === undefined || target <= 0) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--impedance <ohm>` must be greater than zero' });
            return 2;
        }
        if (differentialStructure(structure) && (gap === undefined || gap <= 0)) {
            ctx.fail('open-pcb.cli.gap-required', {
                message: '`--gap <mm>` is required and must be greater than zero for a differential structure',
            });
            return 2;
        }
        if (!differentialStructure(structure) && gap !== undefined) {
            ctx.fail('open-pcb.cli.gap-invalid', {
                message: '`--gap <mm>` applies only to diffMicrostrip or diffStripline',
            });
            return 2;
        }
        await withBoard(file, async (board) => {
            const report = await board.impedanceWidth({
                structure,
                impedance: target,
                gap,
                dielectricHeight: numberOption(text(options, 'dielectric-height')),
                dielectricConstant: numberOption(text(options, 'dielectric-constant')),
                copperThickness: numberOption(text(options, 'copper-thickness')),
            });
            const modes =
                report.oddImpedance === undefined
                    ? ''
                    : ` gap=${report.gap}mm odd=${report.oddImpedance}ohm even=${report.evenImpedance}ohm`;
            ctx.emitResult(report, `${report.structure} impedance=${report.impedance}ohm width=${report.width}mm${modes}`);
            return 0;
        });
        return 0;
    });

stackup
    .command('table', 'cli.cmd.stackup-table')
    .option('--targets <ohms>', 'cli.opt.targets')
    .option('--structure <microstrip|stripline|diffMicrostrip|diffStripline>', 'cli.opt.structure')
    .option('--gap <mm>', 'cli.opt.gap')
    .option('--dielectric-height <mm>', 'cli.opt.dielectric-height')
    .option('--dielectric-constant <er>', 'cli.opt.dielectric-constant')
    .option('--copper-thickness <mm>', 'cli.opt.copper-thickness')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        const rawTargets = text(options, 'targets');
        const targets = impedanceTargets(rawTargets);
        if (rawTargets !== undefined && targets === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', {
                message: '`--targets <ohm,ohm,...>` must be a comma-separated list of numbers',
            });
            return 2;
        }
        if (text(options, 'gap') !== undefined && numberOption(text(options, 'gap')) === undefined) {
            ctx.fail('open-pcb.cli.number-invalid', { message: '`--gap <mm>` must be a finite number' });
            return 2;
        }
        await withBoard(file, async (board) => {
            const report = await board.impedanceTable({
                structure: text(options, 'structure') as ImpedanceStructure | undefined,
                targets,
                gap: numberOption(text(options, 'gap')),
                dielectricHeight: numberOption(text(options, 'dielectric-height')),
                dielectricConstant: numberOption(text(options, 'dielectric-constant')),
                copperThickness: numberOption(text(options, 'copper-thickness')),
            });
            ctx.emitResult(report, JSON.stringify(report));
            return 0;
        });
        return 0;
    });

stackup
    .command('report', 'cli.cmd.stackup-report')
    .option('--structure <auto|microstrip|stripline|diffMicrostrip|diffStripline>', 'cli.opt.report-structure')
    .option('--dielectric-height <mm>', 'cli.opt.dielectric-height')
    .option('--dielectric-constant <er>', 'cli.opt.dielectric-constant')
    .option('--copper-thickness <mm>', 'cli.opt.copper-thickness')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        const rawStructure = text(options, 'structure');
        const structure = reportStructure(rawStructure);
        if (rawStructure !== undefined && structure === undefined) {
            ctx.fail('open-pcb.cli.structure-invalid', {
                message: '`--structure` must be auto, microstrip, stripline, diffMicrostrip or diffStripline',
            });
            return 2;
        }
        await withBoard(file, async (board) => {
            const report = await board.impedanceReport({
                structure,
                dielectricHeight: numberOption(text(options, 'dielectric-height')),
                dielectricConstant: numberOption(text(options, 'dielectric-constant')),
                copperThickness: numberOption(text(options, 'copper-thickness')),
            });
            ctx.emitResult(report, JSON.stringify(report));
            return 0;
        });
        return 0;
    });

cli
    .command('inspect', 'cli.cmd.inspect')
    .option('--checks <list>', 'cli.opt.checks')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        const checks = text(options, 'checks')
            ?.split(',')
            .map((name) => name.trim())
            .filter((name) => name.length > 0) as InspectCheck[] | undefined;
        const report = await app.inspect({ path: file }, { checks });
        ctx.emitResult(report, describeReport(report));
        return report.passed ? 0 : 1;
    });

cli
    .command('export', 'cli.cmd.export')
    .option('--format <format>', 'cli.opt.format')
    .option('--out <file>', 'cli.opt.out')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        if (file === undefined) {
            ctx.fail('open-pcb.cli.file-required', { message: 'a board document is required' });
            return 2;
        }
        const format = asExportTarget(text(options, 'format'));
        if (format === undefined) {
            ctx.fail('open-pcb.cli.format-required', {
                message: '`--format` must be one of kicad_pcb, gbr, drl, drill_map, pos, placement, bom, testpoints, ipc_d_356, step, ipc2581, pdf, svg',
            });
            return 2;
        }
        const out = text(options, 'out');
        if (out === undefined) {
            ctx.fail('open-pcb.cli.out-required', { message: '`--out <file-or-directory>` is required' });
            return 2;
        }
        return withBoard(file, async (board) => {
            try {
                const report = await board.export({ format, target: out });
                const files = report.artifacts ? ` (${report.artifacts.length} files)` : '';
                ctx.emitResult(report, `exported ${format} to ${out}${files} revision=${report.revision}`);
            } catch (error) {
                // Keep the stable code visible in human output; the SDK error keeps
                // its own code field unchanged.
                const code = error && typeof error === 'object' && 'code' in error ? String(error.code) : '';
                const message = error instanceof Error ? error.message : String(error);
                throw new Error(code ? `${code}: ${message}` : message);
            }
            return 0;
        });
    });

cli.command('gui', 'cli.cmd.gui').actionCtx(async (_options, _args, ctx) => {
    try {
        const status = await runGui({});
        ctx.emitResult(status, `window ${status.state}`);
        return 0;
    } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        ctx.fail('open-pcb.cli.gui-failed', { message });
        return 1;
    }
});

const entry = process.argv[1];
if (entry !== undefined && import.meta.url === pathToFileURL(entry).href) {
    const code = await cli.parse(process.argv);
    process.exitCode = code;
}
