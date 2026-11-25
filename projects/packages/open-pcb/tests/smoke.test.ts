/**
 * Behavioural tests for the authoring surface.
 *
 * They cover the declarative shell (stable error codes, idempotent disposal,
 * synchronous argument validation, the format registry) and, when the native
 * addon is installed, a real create/edit/save/open/inspect/export round trip.
 */

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { boardFormats, formatFromPath, formatInfo, OpenPCB, OpenPCBError } from '../src/index.js';
import type { BoardInput } from '../src/index.js';
import { nativeBinding } from '../src/native.js';

const installed = (() => {
    try {
        return nativeBinding() !== undefined;
    } catch {
        return false;
    }
})();
const withoutAddon = installed ? 'a built addon is installed' : false;
const withAddon = installed ? false : 'no built addon is installed';

/** A two-pad `PWR` net `board.route()` can connect in one horizontal segment. */
async function routingBoard(pcb: OpenPCB) {
    const board = await pcb.create();
    await board.addNet({ name: 'PWR' });
    await board.addFootprint({ reference: 'U1', x: 0, y: 0 });
    await board.addPad({ footprint: 'fp-1', number: '1', x: 10, y: 10, w: 0.5, h: 0.5, net: 'PWR' });
    await board.addPad({ footprint: 'fp-1', number: '2', x: 20, y: 10, w: 0.5, h: 0.5, net: 'PWR' });
    await board.setOutline({ from: [0, 0], to: [50, 40] });
    return board;
}

/** A two-pad `PWR` net whose edge has room for an exact 45-degree diagonal. */
async function chamferBoard(pcb: OpenPCB) {
    const board = await pcb.create();
    await board.addNet({ name: 'PWR' });
    await board.addFootprint({ reference: 'U1', x: 0, y: 0 });
    await board.addPad({ footprint: 'fp-1', number: '1', x: 10, y: 10, w: 0.5, h: 0.5, net: 'PWR' });
    await board.addPad({ footprint: 'fp-1', number: '2', x: 28, y: 22, w: 0.5, h: 0.5, net: 'PWR' });
    await board.setOutline({ from: [0, 0], to: [50, 40] });
    return board;
}

test('session construction is synchronous and side-effect free', async () => {
    const pcb = new OpenPCB();
    assert.equal(pcb.locale, 'en');
    assert.equal(pcb.disposed, false);
    await pcb.dispose();
    assert.equal(pcb.disposed, true);
});

test('without an installed addon engine calls keep capability/not-implemented', { skip: withoutAddon }, async () => {
    const pcb = new OpenPCB({ locale: 'zh-CN' });
    assert.equal(pcb.locale, 'zh-CN');
    const calls: (() => Promise<unknown>)[] = [
        () => pcb.capabilities(),
        () => pcb.create({ title: 'Blinker', template: 'blank' }),
        () => pcb.open({ path: 'blinker.opcb' }),
        () => pcb.inspect({ path: 'blinker.opcb' }),
    ];
    for (const call of calls) {
        await assert.rejects(call, (error: unknown) => {
            assert.ok(error instanceof OpenPCBError);
            assert.equal(error.code, 'capability/not-implemented');
            assert.ok(error.details.feature);
            return true;
        });
    }
    await pcb.dispose();
});

test('dispose is idempotent and closes the session', async () => {
    const pcb = new OpenPCB();
    await pcb.dispose();
    await pcb.dispose();
    await assert.rejects(
        pcb.capabilities(),
        (error: unknown) => error instanceof OpenPCBError && error.code === 'session/disposed',
    );
});

test('dispose resolves after a rejected handle acquisition', { skip: withoutAddon }, async () => {
    const pcb = new OpenPCB();
    await assert.rejects(pcb.create({ title: 'Blinker', template: 'blank' }));
    await assert.rejects(pcb.open({ path: 'blinker.opcb' }));
    await pcb.dispose();
    await pcb.dispose();
    assert.equal(pcb.disposed, true);
});

test('open rejects ambiguous input before reaching the engine', async () => {
    const pcb = new OpenPCB();
    const inputs = [
        {} as unknown as BoardInput,
        { path: 'a.opcb', project: '{}' } as unknown as BoardInput,
        { project: 5 } as unknown as BoardInput,
        { path: '' } as unknown as BoardInput,
    ];
    for (const input of inputs) {
        await assert.rejects(
            pcb.open(input),
            (error: unknown) => error instanceof OpenPCBError && error.code === 'session/argument-invalid',
        );
    }
    await pcb.dispose();
});

test('format registry classifies the board format family', () => {
    const expected = ['kicad_pcb', 'gbr', 'drl', 'drill_map', 'dxf', 'pos', 'placement', 'bom', 'step', 'ipc2581', 'svg', 'pdf', 'png'];
    assert.deepEqual([...boardFormats].sort(), [...expected].sort());

    assert.equal(formatInfo('kicad_pcb').family, 'board');
    assert.equal(formatInfo('kicad_pcb').opensAs, 'board');
    assert.equal(formatInfo('kicad_pcb').defaultIntent, 'edit');
    assert.equal(formatInfo('gbr').family, 'fabrication');
    assert.equal(formatInfo('gbr').opensAs, 'exchange');
    assert.equal(formatInfo('gbr').defaultIntent, null);
    assert.equal(formatInfo('drl').family, 'fabrication');
    assert.equal(formatInfo('drill_map').family, 'fabrication');
    assert.equal(formatInfo('drill_map').opensAs, 'none');
    assert.equal(formatInfo('dxf').family, 'fabrication');
    assert.equal(formatInfo('dxf').opensAs, 'exchange');
    assert.equal(formatInfo('pos').family, 'assembly');
    assert.equal(formatInfo('placement').family, 'assembly');
    assert.equal(formatInfo('placement').markup, 'json');
    assert.equal(formatInfo('bom').family, 'assembly');
    assert.equal(formatInfo('step').markup, 'step');
    assert.equal(formatInfo('ipc2581').markup, 'xml');
    assert.equal(formatInfo('svg').opensAs, 'none');
    assert.equal(formatInfo('pdf').markup, 'fixed');
    assert.equal(formatInfo('png').family, 'image');
    assert.equal(formatInfo('png').opensAs, 'none');
    assert.equal(formatInfo('png').macroEnabled, false);

    assert.equal(formatFromPath('BLINKER.KICAD_PCB'), 'kicad_pcb');
    assert.equal(formatFromPath('BLINKER.GBR'), 'gbr');
    assert.equal(formatFromPath('board.DXF'), 'dxf');
    assert.equal(formatFromPath('blinker.opcb'), undefined);
    assert.equal(formatFromPath('no-extension'), undefined);
});

test('capability matrix reports the implemented pack', { skip: withAddon }, async () => {
    const pcb = new OpenPCB();
    const matrix = await pcb.capabilities();
    assert.equal(matrix.project.import, 'partial');
    assert.equal(matrix.project.edit, 'partial');
    assert.equal(matrix.project.export, 'partial');
    for (const format of ['gbr', 'drl', 'drill_map', 'dxf', 'bom', 'pos', 'placement', 'svg', 'pdf', 'kicad_pcb', 'ipc2581', 'png'] as const) {
        assert.equal(matrix.formats[format].export, 'supported');
    }
    for (const format of ['gbr', 'drl', 'drill_map', 'bom', 'pos', 'placement', 'svg', 'pdf', 'png'] as const) {
        assert.equal(matrix.formats[format].import, 'unsupported');
    }
    assert.equal(matrix.formats.kicad_pcb.import, 'supported');
    assert.equal(matrix.formats.dxf.import, 'supported');
    assert.ok(matrix.formats.dxf.lossCodes.includes('dxf/construct-not-imported'), 'DXF loss codes are listed');
    assert.ok(matrix.formats.kicad_pcb.lossCodes.includes('kicad/construct-not-imported'));
    assert.ok(matrix.formats.kicad_pcb.lossCodes.includes('kicad/outline-not-rectangular'));
    assert.equal(matrix.formats.ipc2581.import, 'supported');
    assert.ok(matrix.formats.ipc2581.lossCodes.includes('ipc2581/outline-not-rectangular'));
    assert.equal(matrix.formats.step.export, 'unsupported');
    assert.equal(matrix.formats.step.import, 'unsupported');
    assert.equal(matrix.checks.drc, 'supported');
    assert.equal(matrix.checks.clearance, 'supported');
    assert.equal(matrix.checks['annular-ring'], 'supported');
    assert.equal(matrix.checks['track-width'], 'supported');
    assert.equal(matrix.checks.unrouted, 'supported');
    assert.equal(matrix.checks.outline, 'supported');
    assert.equal(matrix.checks.silkscreen, 'supported');
    assert.equal(matrix.checks.courtyard, 'supported');
    assert.equal(matrix.checks.stencil, 'supported');
    assert.equal(matrix.renderers.svg, 'supported');
    assert.equal(matrix.renderers.png, 'unsupported');
    assert.equal(matrix.designRules, 'supported');
    assert.equal(matrix.solderMask, 'supported');
    assert.equal(matrix.drcPolicy, 'supported');
    assert.equal(matrix.routing, 'supported');
    assert.equal(matrix.routingNetClasses, 'supported');
    assert.deepEqual(matrix.routingStyles, { orthogonal: 'supported', chamfer45: 'supported' });
    assert.equal(matrix.texts, 'supported');
    assert.equal(matrix.zoneReport, 'supported');
    assert.equal(matrix.netLengths, 'supported');
    assert.equal(matrix.netAudit, 'supported');
    assert.equal(matrix.testPointCoverage, 'supported');
    assert.equal(matrix.drillReport, 'supported');
    assert.equal(matrix.exportTargets.drl, 'supported');
    assert.equal(matrix.exportTargets.gbr, 'supported');
    assert.equal(matrix.exportTargets.dxf, 'supported');
    assert.equal(matrix.exportTargets.layers, 'supported');
    assert.equal(matrix.exportTargets.asm, 'supported');
    assert.equal(matrix.exportTargets.pdf, 'supported');
    assert.equal(matrix.exportTargets.png, 'supported');
    assert.equal(matrix.exportTargets.kicad_pcb, 'supported');
    assert.equal(matrix.exportTargets.ipc2581, 'supported');
    assert.equal(matrix.exportTargets.step, 'unsupported');
    await pcb.dispose();
});

test('board edits, saves, reopens, inspects and exports', { skip: withAddon }, async () => {
    const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'open-pcb-sdk-'));
    try {
        const pcb = new OpenPCB();
        const board = await pcb.create({ title: 'Blinker' });
        assert.equal(board.revision, 1);
        assert.equal(board.summary.counts.nets, 0);

        await board.addNet({ name: 'VCC' });
        const footprint = await board.addFootprint({
            reference: 'U1',
            value: 'MCU',
            footprint: 'QFN32',
            x: 30,
            y: 20,
            rotation: 0,
            layer: 'F.Cu',
        });
        assert.equal(footprint.object?.id, 'fp-1');
        assert.equal(board.summary.rules.solderMaskExpansion, 0.05);
        const rules = await board.setRules({ holeClearance: 0.3, solderMaskExpansion: 0.1 });
        assert.equal(rules.board.rules.holeClearance, 0.3);
        assert.equal(rules.board.rules.solderMaskExpansion, 0.1);
        assert.equal(rules.board.rules.componentToEdge, 0.25, 'untouched rules keep their value');
        await board.addTrack({ net: 'VCC', layer: 'F.Cu', width: 0.25, from: [10, 10], to: [30, 10] });
        await board.addVia({ net: 'VCC', at: [20, 20], diameter: 0.6, drill: 0.3 });
        const lengths = await board.netLengths();
        assert.equal(lengths.report, 'open-pcb-net-lengths');
        assert.deepEqual(lengths.nets, [{ name: 'VCC', tracks: 1, length: 20, vias: 1 }]);
        assert.deepEqual(lengths.ranking, [{ name: 'VCC', length: 20 }]);
        assert.deepEqual(lengths.totals, { tracks: 1, length: 20, vias: 1 });
        await board.setOutline({ from: [0, 0], to: [100, 80] });
        assert.equal(board.canUndo, true);

        const report = await board.inspect();
        assert.equal(report.passed, true, "warnings do not fail the board");
        assert.deepEqual(
            report.issues.map((issue) => issue.code),
            ['drc/footprint-no-pads', 'silkscreen/no-reference-text', 'drc/dangling-copper', 'testpoints/net-not-covered'],
            "a footprint without pads, a missing reference legend, loose copper and an uncovered net are warnings",
        );
        const placement = await board.inspect({ checks: ['silkscreen', 'courtyard'] });
        assert.equal(placement.passed, true);
        assert.deepEqual(
            placement.issues.map((issue) => issue.code),
            ['silkscreen/no-reference-text'],
            "both new checks run alongside the existing checks",
        );
        assert.equal((await board.list()).counts.tracks, 1);

        const file = path.join(directory, 'blinker.opcb');
        const saved = await board.save({ target: file });
        assert.ok(saved.bytes > 0);
        assert.ok(fs.existsSync(file));
        assert.equal(fs.existsSync(`${file}.history.json`), false, 'the default memory session writes no sidecar');

        for (const format of ['drl', 'drill_map', 'dxf', 'bom', 'pos', 'placement', 'svg'] as const) {
            const target = path.join(directory, `blinker.${format}`);
            const exported = await board.export({ format, target });
            assert.equal(exported.format, format);
            assert.ok(exported.bytes > 0);
            assert.ok(fs.existsSync(target));
        }
        assert.match(fs.readFileSync(path.join(directory, 'blinker.drl'), 'utf8'), /M48/);
        assert.match(fs.readFileSync(path.join(directory, 'blinker.drill_map'), 'utf8'), /OpenPCB drill map/);
        assert.match(fs.readFileSync(path.join(directory, 'blinker.dxf'), 'utf8'), /AC1009/);
        assert.equal(fs.readFileSync(path.join(directory, 'blinker.dxf'), 'utf8').match(/LINE/g)?.length, 4, 'the outline is four lines');
        assert.match(fs.readFileSync(path.join(directory, 'blinker.pos'), 'utf8'), /Designator/);
        assert.match(fs.readFileSync(path.join(directory, 'blinker.placement'), 'utf8'), /"placements"/);
        assert.match(fs.readFileSync(path.join(directory, 'blinker.svg'), 'utf8'), /<svg/);

        const gerber = path.join(directory, 'gerber');
        const artifacts = await board.export({ format: 'gbr', target: gerber });
        assert.equal(artifacts.artifacts?.length, 9);
        assert.equal(artifacts.bytes, artifacts.artifacts?.reduce((total, item) => total + item.bytes, 0));
        for (const name of ['F_Cu.gbr', 'B_Cu.gbr', 'F_SilkS.gbr', 'B_SilkS.gbr', 'F_Mask.gbr', 'B_Mask.gbr', 'F_Paste.gbr', 'B_Paste.gbr', 'Edge_Cuts.gbr']) {
            const content = fs.readFileSync(path.join(gerber, name), 'utf8');
            assert.match(content, /%FSLAX46Y46\*%/);
            assert.match(content, /M02\*/);
        }
        const frontCopper = fs.readFileSync(path.join(gerber, 'F_Cu.gbr'), 'utf8');
        assert.match(frontCopper, /%TO\.N,VCC\*%/, 'copper objects declare their net');
        assert.match(frontCopper, /%TD\*%/, 'object attribute blocks are closed');
        assert.match(frontCopper, /%TA\.AperFunction,Conductor\*%/, 'file and aperture attributes stay');

        const layerViews = path.join(directory, 'layers');
        const views = await board.export({ format: 'layers', target: layerViews });
        assert.equal(views.format, 'layers');
        assert.deepEqual(views.artifacts?.map((item) => item.name), ['F_Cu.svg', 'B_Cu.svg', 'F_SilkS.svg', 'B_SilkS.svg']);
        for (const name of ['F_Cu.svg', 'B_Cu.svg', 'F_SilkS.svg', 'B_SilkS.svg']) {
            assert.match(fs.readFileSync(path.join(layerViews, name), 'utf8'), /<svg/, `${name} is an SVG view`);
        }

        const assembly = path.join(directory, 'asm');
        const sheets = await board.export({ format: 'asm', target: assembly });
        assert.equal(sheets.format, 'asm');
        assert.deepEqual(sheets.artifacts?.map((item) => item.name), ['F_Asm.svg', 'B_Asm.svg']);
        for (const name of ['F_Asm.svg', 'B_Asm.svg']) {
            assert.match(fs.readFileSync(path.join(assembly, name), 'utf8'), /Assembly drawing/, `${name} is an assembly sheet`);
        }

        const vector = await board.export({ format: 'pdf', target: path.join(directory, 'blinker.pdf') });
        assert.ok(vector.bytes > 0);
        const pdf = fs.readFileSync(path.join(directory, 'blinker.pdf'), 'utf8');
        assert.match(pdf, /^%PDF-1\./);
        assert.match(pdf, /%%EOF/);

        await board.export({ format: 'kicad_pcb', target: path.join(directory, 'blinker.kicad_pcb') });
        assert.match(fs.readFileSync(path.join(directory, 'blinker.kicad_pcb'), 'utf8'), /^\(kicad_pcb/);

        await board.export({ format: 'ipc2581', target: path.join(directory, 'blinker.ipc2581') });
        assert.match(fs.readFileSync(path.join(directory, 'blinker.ipc2581'), 'utf8'), /^<\?xml/);

        const imported = await pcb.importKicadPcb({ path: path.join(directory, 'blinker.kicad_pcb') });
        assert.equal(imported.board.summary.counts.footprints, 1);
        assert.equal(imported.board.summary.counts.tracks, 1);
        assert.equal(imported.board.summary.counts.vias, 1);
        assert.equal(imported.saved, undefined, 'an import without a target stays unsaved');
        await imported.board.close();

        const ipcImported = await pcb.importBoard({ path: path.join(directory, 'blinker.ipc2581') });
        assert.equal(ipcImported.format, 'ipc2581');
        assert.equal(ipcImported.board.summary.counts.footprints, 1);
        assert.equal(ipcImported.board.summary.counts.tracks, 1);
        assert.equal(ipcImported.board.summary.counts.vias, 1);
        assert.deepEqual(ipcImported.losses, [], 'the exported IPC-2581 document round trips without losses');
        await ipcImported.board.close();

        const dxfImported = await pcb.importBoard({ path: path.join(directory, 'blinker.dxf') });
        assert.equal(dxfImported.format, 'dxf');
        assert.deepEqual(dxfImported.losses, [], 'the exported DXF outline round trips without losses');
        assert.deepEqual(dxfImported.board.summary.outline, { from: [0, 0], to: [100, 80] }, 'the outline survives the round trip');
        await dxfImported.board.close();

        await assert.rejects(
            board.export({ format: 'step', target: path.join(directory, 'blinker.step') }),
            (error: unknown) => error instanceof OpenPCBError && error.code === 'capability/not-implemented',
        );

        const savedRevision = board.revision;
        await board.close();
        assert.equal(board.revision, savedRevision);

        const reopened = await pcb.open({ path: file });
        assert.equal(reopened.summary.counts.footprints, 1);
        assert.equal(reopened.summary.counts.vias, 1);
        const reopenedReport = await reopened.inspect({ checks: ['drc', 'connectivity'] });
        assert.equal(reopenedReport.passed, true);
        await reopened.close();
        await pcb.dispose();
    } finally {
        fs.rmSync(directory, { recursive: true, force: true });
    }
});

test('waivers and severity overrides change the inspection verdict', { skip: withAddon }, async () => {
    const pcb = new OpenPCB();
    const board = await pcb.create();
    await board.addNet({ name: 'VCC' });
    await board.addTrack({ net: 'VCC', layer: 'F.Cu', width: 0.05, from: [1, 1], to: [2, 1] });
    const checks = ['track-width'] as const;

    const before = await board.inspect({ checks });
    assert.equal(before.passed, false);
    assert.equal(before.active, 1);
    assert.equal(before.waived, 0);
    assert.equal(before.issues[0].code, 'drc/track-width-too-small');

    const downgraded = await board.setSeverity({ code: 'drc/track-width-too-small', severity: 'info' });
    assert.equal(downgraded.board.severityOverrides['drc/track-width-too-small'], 'info');
    assert.equal((await board.inspect({ checks })).passed, true, 'an info finding does not block the board');
    await board.deleteSeverity('drc/track-width-too-small');
    assert.equal((await board.inspect({ checks })).passed, false);

    const waiver = await board.addWaiver({ code: 'drc/track-width-too-small', object: 'track-1', reason: 'known stub' });
    assert.equal(waiver.object?.id, 'waiver-1');
    const report = await board.inspect({ checks });
    assert.equal(report.passed, true, 'a waived error does not block the board');
    assert.equal(report.active, 0);
    assert.equal(report.waived, 1);
    assert.equal(report.issues[0].waived, true);
    assert.equal(report.issues[0].waiverId, 'waiver-1');
    assert.equal(report.issues[0].waiverReason, 'known stub');
    assert.equal(board.summary.counts.waivers, 1);
    assert.deepEqual(board.summary.waivers[0].object, 'track-1');

    await board.deleteWaiver('waiver-1');
    assert.equal((await board.inspect({ checks })).passed, false);
    await board.close();
    await pcb.dispose();
});

test('stencil check measures paste openings and honours rules and waivers', { skip: withAddon }, async () => {
    const pcb = new OpenPCB();
    const board = await pcb.create();
    await board.addFootprint({ reference: 'U1', value: 'QFN', footprint: 'QFN', x: 0, y: 0, rotation: 0, layer: 'F.Cu' });
    await board.addPad({ footprint: 'fp-1', number: '1', x: 0, y: 0, w: 1, h: 1 });
    await board.addPad({ footprint: 'fp-1', number: '2', x: 0, y: 5, w: 0.3, h: 0.3 });
    await board.addPad({ footprint: 'fp-1', number: '3', x: 5, y: 0, w: 0.17, h: 20 });

    const checks = ['stencil'] as const;
    const report = await board.inspect({ checks });
    assert.deepEqual(
        report.issues.map((issue) => [issue.code, issue.objectId]),
        [
            ['drc/stencil-area-ratio', 'fp-1:2'],
            ['drc/stencil-aspect-ratio', 'fp-1:3'],
        ],
        'the clean pad stays quiet and both ratio failures carry their pad',
    );
    assert.equal(report.issues[0].layerId, 'F.Cu');
    assert.equal(report.issues[0].severity, 'warning');
    assert.equal(report.passed, true, 'stencil findings warn without failing the board');

    const repeated = await board.inspect({ checks });
    assert.deepEqual(repeated.issues, report.issues, 'repeated runs are identical');
    const filtered = await board.inspect({ checks: ['silkscreen'] });
    assert.equal(
        filtered.issues.some((issue) => issue.code.startsWith('drc/stencil-')),
        false,
        'the check list filters the run',
    );

    await board.addWaiver({ code: 'drc/stencil-area-ratio', object: 'fp-1:2', reason: 'prototype stencil' });
    const waived = await board.inspect({ checks });
    assert.equal(waived.issues[0].waived, true);
    assert.equal(waived.waived, 1);
    assert.equal(waived.active, 1);

    const relaxed = await board.setRules({ stencilThickness: 0.1, stencilAreaRatio: 0.5, stencilAspectRatio: 1 });
    assert.equal(relaxed.board.rules.stencilThickness, 0.1);
    assert.equal(relaxed.board.rules.stencilAreaRatio, 0.5);
    const clean = await board.inspect({ checks });
    assert.equal(clean.issues.length, 0, 'the configured stencil rules accept every opening');
    await board.close();
    await pcb.dispose();
});

test('differential impedance solves pair widths through the SDK', { skip: withAddon }, async () => {
    const pcb = new OpenPCB();
    const board = await pcb.create();
    await board.setStackup({ copperThickness: 0.035, dielectricHeight: 0.2, dielectricConstant: 4.2 });

    const pair = await board.impedance({ structure: 'diffMicrostrip', width: 0.15, gap: 0.2 });
    assert.equal(pair.structure, 'diffMicrostrip');
    assert.equal(pair.gap, 0.2);
    assert.ok(pair.oddImpedance !== undefined && pair.evenImpedance !== undefined);
    assert.ok(pair.oddImpedance < pair.evenImpedance);
    assert.ok(Math.abs(pair.impedance - 2 * pair.oddImpedance) < 1e-6, 'differential is twice the odd mode');

    const solved = await board.impedanceWidth({ structure: 'diffMicrostrip', impedance: 90, gap: 0.2 });
    const achieved = await board.impedance({ structure: 'diffMicrostrip', width: solved.width, gap: 0.2 });
    assert.ok(Math.abs(achieved.impedance - 90) < 0.02, `the solved width reaches the target: ${achieved.impedance}`);

    const table = await board.impedanceTable({ structure: 'diffStripline', targets: [80, 100], gap: 0.2 });
    assert.equal(table.gap, 0.2);
    assert.equal(table.rows.length, 2);
    assert.ok(table.rows[0].oddImpedance !== undefined && table.rows[0].evenImpedance !== undefined);

    await assert.rejects(
        board.impedance({ structure: 'diffMicrostrip', width: 0.15 }),
        (error: unknown) => error instanceof OpenPCBError && error.code === 'session/argument-invalid',
    );
    await board.close();
    await pcb.dispose();
});

test('routing follows net-class width, clearance and via targets when enabled', { skip: withAddon }, async () => {
    const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'open-pcb-netclass-'));
    try {
        const pcb = new OpenPCB();

        const viaBoard = await routingBoard(pcb);
        await viaBoard.addTrack({ layer: 'F.Cu', width: 0.25, from: [15, 0], to: [15, 30] });
        await viaBoard.addNetClass({ name: 'PWR', width: 0.4, clearance: 0.3, viaDiameter: 0.8, viaDrill: 0.4 });
        await viaBoard.assignNetClass({ net: 'PWR', class: 'PWR' });
        const viaReport = await viaBoard.route({ useNetClasses: true });
        assert.equal(viaReport.routed, 1);
        assert.equal(viaReport.vias, 2, 'the blocked top layer forces a via at each endpoint');
        const viaTarget = path.join(directory, 'netclass.kicad_pcb');
        await viaBoard.export({ format: 'kicad_pcb', target: viaTarget });
        const viaText = fs.readFileSync(viaTarget, 'utf8');
        assert.match(viaText, /\(width 0\.4\)/, 'routed tracks use the class width');
        assert.match(viaText, /\(size 0\.8\) \(drill 0\.4\)/, 'vias use the class diameter and drill');

        const renamed = await viaBoard.updateNetClass({ id: 'netclass-1', name: 'PWR-RAIL', width: 0.5 });
        assert.equal(renamed.object?.name, 'PWR-RAIL');
        assert.equal(renamed.board.netClasses[0].width, 0.5);
        assert.equal(renamed.board.netClasses[0].clearance, 0.3, 'an omitted clearance is kept');
        assert.deepEqual(renamed.board.netClasses[0].nets, ['PWR'], 'a rename keeps the assignment');
        await assert.rejects(
            viaBoard.updateNetClass({ id: 'netclass-9', width: 0.5 }),
            (error: unknown) => error instanceof OpenPCBError && error.code === 'object/not-found',
        );

        const clearanceBoard = await routingBoard(pcb);
        await clearanceBoard.addTrack({ layer: 'F.Cu', width: 0.25, from: [5, 10.55], to: [25, 10.55] });
        await clearanceBoard.addNetClass({ name: 'RF', clearance: 0.05 });
        await clearanceBoard.assignNetClass({ net: 'PWR', class: 'RF' });
        const clearanceReport = await clearanceBoard.route({
            layers: ['F.Cu'],
            width: 0.3,
            clearance: 0.8,
            useNetClasses: true,
        });
        assert.equal(clearanceReport.routed, 1, 'the class clearance clears the 0.55 gap the argument would block');
        const clearanceTarget = path.join(directory, 'clearance.kicad_pcb');
        await clearanceBoard.export({ format: 'kicad_pcb', target: clearanceTarget });
        assert.match(fs.readFileSync(clearanceTarget, 'utf8'), /\(width 0\.3\)/, 'a class without width falls back to the argument');

        await viaBoard.close();
        await clearanceBoard.close();
        await pcb.dispose();
    } finally {
        fs.rmSync(directory, { recursive: true, force: true });
    }
});

test('net-class routing falls back per field and stays off by default', { skip: withAddon }, async () => {
    const pcb = new OpenPCB();
    const raw = (report: unknown): { nets: { width: number; widthSource?: string }[] } =>
        report as { nets: { width: number; widthSource?: string }[] };

    const legacy = await routingBoard(pcb);
    await legacy.addNetClass({ name: 'BIG', width: 0.5, clearance: 0.5, viaDiameter: 0.9, viaDrill: 0.45 });
    await legacy.assignNetClass({ net: 'PWR', class: 'BIG' });
    const legacyReport = await legacy.route();
    assert.equal(legacyReport.routed, 1);
    assert.equal(raw(legacyReport).nets[0].width, undefined, 'without the option the report keeps its old shape');
    assert.equal(raw(legacyReport).nets[0].widthSource, undefined);

    const fallback = await routingBoard(pcb);
    await fallback.addNetClass({ name: 'EMPTY' });
    await fallback.assignNetClass({ net: 'PWR', class: 'EMPTY' });
    const fallbackReport = await fallback.route({ width: 0.4, useNetClasses: true });
    assert.equal(fallbackReport.routed, 1);
    assert.equal(raw(fallbackReport).nets[0].width, 0.4, 'an empty class falls back to the argument');
    assert.equal(raw(fallbackReport).nets[0].widthSource, 'argument');

    const wall = await routingBoard(pcb);
    for (const layer of ['F.Cu', 'B.Cu']) {
        await wall.addTrack({ layer, width: 0.25, from: [15, 0], to: [15, 30] });
    }
    await wall.addNetClass({ name: 'PWR', width: 0.4 });
    await wall.assignNetClass({ net: 'PWR', class: 'PWR' });
    const blocked = await wall.route({ useNetClasses: true });
    assert.equal(blocked.routed, 0);
    assert.equal(blocked.unrouted.length, 1, 'an unroutable edge is still reported');
    assert.equal(blocked.unrouted[0].widthSource, 'netclass');

    await legacy.close();
    await fallback.close();
    await wall.close();
    await pcb.dispose();
});

test('routing emits an exact 45-degree diagonal and is deterministic', { skip: withAddon }, async () => {
    const pcb = new OpenPCB();
    const first = await chamferBoard(pcb);
    const report = await first.route({ style: 'chamfer45' });
    assert.equal(report.style, 'chamfer45');
    assert.equal(report.routed, 1);
    assert.equal(report.tracks, 3, 'the edge is an axis segment, a diagonal and an axis segment');

    const edge = report.edges?.[0];
    assert.ok(edge, 'the chamfer report lists every spanning edge');
    assert.equal(edge.style, 'chamfer45');
    assert.equal(edge.fallback, null);
    assert.equal(edge.segments.length, 3);
    assert.equal(edge.segments[0].to[1], edge.segments[0].from[1], 'the first segment is axis aligned');
    assert.equal(edge.segments[2].to[0], edge.segments[2].from[0], 'the last segment is axis aligned');
    assert.deepEqual(edge.segments[0].to, edge.segments[1].from, 'the segments form one chain');
    assert.deepEqual(edge.segments[1].to, edge.segments[2].from);
    const dx = edge.segments[1].to[0] - edge.segments[1].from[0];
    const dy = edge.segments[1].to[1] - edge.segments[1].from[1];
    assert.ok(Math.abs(dx) > 0, 'the diagonal has length');
    assert.ok(Math.abs(Math.abs(dx) - Math.abs(dy)) < 1e-9, `the diagonal is exactly 45 degrees: ${dx},${dy}`);
    for (const segment of edge.segments) {
        assert.equal(segment.style, 'chamfer45');
        assert.equal(segment.layer, 'F.Cu');
        assert.ok(segment.width > 0);
    }

    const inspection = await first.inspect({ checks: ['drc', 'clearance'] });
    assert.equal(inspection.passed, true, JSON.stringify(inspection.issues));

    const second = await chamferBoard(pcb);
    const repeat = await second.route({ style: 'chamfer45' });
    assert.deepEqual(repeat, report, 'the same board and options produce the same report');

    await first.close();
    await second.close();
    await pcb.dispose();
});

test('chamfer45 falls back per edge and the default report keeps its shape', { skip: withAddon }, async () => {
    const pcb = new OpenPCB();

    const legacy = await routingBoard(pcb);
    const report = await legacy.route();
    assert.equal('style' in report, false, 'the default report adds no style field');
    assert.equal('edges' in report, false, 'the default report adds no edges field');

    const aligned = await routingBoard(pcb);
    const chamfer = await aligned.route({ style: 'chamfer45' });
    const edge = chamfer.edges?.[0];
    assert.ok(edge);
    assert.equal(edge.style, 'orthogonal', 'an axis-aligned edge has no room for a diagonal');
    assert.equal(edge.fallback, 'diagonal-length-zero');
    assert.equal(edge.segments[0].style, 'orthogonal');
    assert.equal(chamfer.tracks, 1);

    await legacy.close();
    await aligned.close();
    await pcb.dispose();
});

test('zones, silkscreen text and drill reports round trip through the SDK', { skip: withAddon }, async () => {
    const pcb = new OpenPCB();
    const board = await pcb.create({ title: 'Reports' });
    await board.addNet({ name: 'VCC' });
    await board.addVia({ net: 'VCC', at: [10, 10], diameter: 0.6, drill: 0.3 });
    await board.addSlot({ net: 'VCC', from: [1, 1], to: [6, 1], width: 0.8 });
    const zone = await board.addZone({
        layer: 'F.Cu',
        points: [
            [0, 0],
            [10, 0],
            [10, 10],
            [0, 10],
        ],
    });
    assert.equal(zone.object?.id, 'zone-1');

    const resized = await board.updateZone({
        id: 'zone-1',
        points: [
            [0, 0],
            [20, 0],
            [20, 10],
            [0, 10],
        ],
    });
    assert.equal(resized.object?.id, 'zone-1');
    const zones = await board.zoneReport();
    assert.equal(zones.count, 1);
    assert.equal(zones.totalArea, 200);
    assert.equal(zones.zones[0].perimeter, 60);
    assert.equal(zones.layers[0].layer, 'F.Cu');

    const legend = await board.addText({ text: 'REV A', layer: 'F.SilkS', at: [5, 75], size: 2, thickness: 0.2 });
    assert.equal(legend.object?.id, 'text-1');
    assert.equal(board.summary.counts.texts, 1);
    const updated = await board.updateText({ id: 'text-1', text: 'REV B', rotation: 90 });
    assert.equal(updated.object?.text, 'REV B');
    const legends = await board.listTexts();
    assert.equal(legends.count, 1);
    assert.equal(legends.items[0].rotation, 90);
    assert.deepEqual(legends.items[0].at, [5, 75]);

    const drill = await board.drillReport();
    assert.equal(drill.units, 'mm');
    assert.equal(drill.holeCount, 1);
    assert.equal(drill.slotCount, 1);
    assert.equal(drill.backDrilled, 0);
    assert.equal(drill.vias.through, 1);
    assert.equal(drill.tools.length, 2);
    assert.equal(drill.tools[0].diameter, 0.3);
    assert.equal(drill.tools[1].slots, 1);
    assert.equal(drill.holes[0].id, 'via-1');
    assert.equal(drill.slots[0].length, 5);

    assert.equal((await board.deleteText('text-1')).deleted, 'text-1');
    assert.equal((await board.deleteZone('zone-1')).deleted, 'zone-1');
    assert.equal((await board.listTexts()).count, 0);
    await board.close();
    await pcb.dispose();
});

test('pour audit and fabrication manifest report islands and planned files', { skip: withAddon }, async () => {
    const pcb = new OpenPCB();
    const board = await pcb.create({ title: 'Audit' });
    assert.equal((await board.addNet({ name: 'GND' })).object?.name, 'GND');
    await board.addNet({ name: 'VCC' });
    await board.addFootprint({ reference: 'U1', x: 5, y: 5 });
    await board.addPad({ footprint: 'fp-1', number: '1', x: 0, y: 0, w: 1, h: 1, net: 'GND' });
    await board.addVia({ net: 'GND', at: [5, 5], diameter: 0.6, drill: 0.3 });
    await board.addZone({ layer: 'F.Cu', net: 'GND', points: [[0, 0], [10, 0], [10, 10], [0, 10]] });
    await board.addZone({ layer: 'F.Cu', net: 'VCC', points: [[20, 20], [30, 20], [30, 30], [20, 30]] });
    await board.addZone({ layer: 'F.Cu', points: [[40, 40], [50, 40], [50, 50], [40, 50]] });
    await board.setOutline({ from: [0, 0], to: [60, 60] });

    const audit = await board.zoneAudit();
    assert.equal(audit.report, 'open-pcb-zone-audit');
    assert.equal(audit.zoneCount, 3);
    assert.equal(audit.connectedCount, 1);
    assert.equal(audit.isolatedCount, 1);
    assert.equal(audit.unassignedCount, 1);
    assert.equal(audit.totalArea, 300);
    assert.equal(audit.isolatedArea, 100);
    assert.equal(audit.layers[0].layer, 'F.Cu');
    assert.equal(audit.zones[0].status, 'connected');
    assert.deepEqual(audit.zones[0].anchors, { pads: 1, vias: 1, tracks: 0 });
    assert.equal(audit.zones[1].status, 'isolated');
    assert.equal(audit.zones[2].status, 'unassigned');
    assert.deepEqual(audit.issues.map((issue) => issue.code), ['zone/isolated', 'zone/unassigned-net']);

    const manifest = await board.fabManifest();
    assert.equal(manifest.report, 'open-pcb-fab-manifest');
    assert.equal(manifest.status, 'pass');
    assert.equal(manifest.outline, true);
    assert.equal(manifest.fileCount, 9);
    assert.deepEqual(manifest.copperLayers, ['F.Cu', 'B.Cu']);
    assert.equal(manifest.files[0].name, 'F_Cu.gbr');
    assert.equal(manifest.files[0].function, 'Copper,L1,Top');
    assert.equal(manifest.files[8].kind, 'profile');
    assert.equal(manifest.drill.tools, 1);
    assert.equal(manifest.drill.holes, 1);
    assert.equal(manifest.assembly.placements, 1);

    const matrix = await pcb.capabilities();
    assert.equal(matrix.zoneAudit, 'supported');
    assert.equal(matrix.fabManifest, 'supported');

    await board.close();
    await pcb.dispose();
});

test('net audit and test point coverage report class gaps and probe coverage', { skip: withAddon }, async () => {
    const pcb = new OpenPCB();
    const board = await pcb.create({ title: 'Audit' });
    for (const name of ['N1', 'N2', 'SIG']) {
        await board.addNet({ name });
    }
    await board.addNetClass({ name: 'Power', width: 0.5, clearance: 0.5 });
    await board.assignNetClass({ net: 'N1', class: 'Power' });
    await board.assignNetClass({ net: 'N2', class: 'Power' });
    await board.addTrack({ net: 'N1', layer: 'F.Cu', width: 0.2, from: [0, 0], to: [10, 0] });
    await board.addTrack({ net: 'N2', layer: 'F.Cu', width: 0.3, from: [0, 0.5], to: [10, 0.5] });
    await board.addFootprint({ reference: 'U1', x: 5, y: 5 });
    await board.addPad({ footprint: 'fp-1', number: '1', x: 0, y: 0, w: 0.8, h: 0.8, net: 'SIG' });
    await board.addVia({ net: 'N1', at: [20, 20], diameter: 0.6, drill: 0.3 });
    await board.addVia({ net: 'N2', at: [30, 30], diameter: 0.6, drill: 0.3 });
    await board.addTestPoint({ target: 'via-1' });
    await board.addTestPoint({ target: 'fp-1:1' });

    const audit = await board.netAudit();
    assert.equal(audit.report, 'open-pcb-net-audit');
    assert.equal(audit.netCount, 3);
    assert.equal(audit.classCount, 1);
    assert.equal(audit.pairCount, 1);
    assert.equal(audit.status, 'error');
    assert.deepEqual(audit.issues.map((issue) => issue.code), [
        'net/clearance-below-class',
        'net/width-below-class',
        'net/width-below-class',
    ]);
    assert.equal(audit.nets[0].class, 'Power');
    assert.equal(audit.nets[0].widthStatus, 'below');
    assert.deepEqual(audit.nets[0].width, { min: 0.2, max: 0.2 });
    assert.equal(audit.pairs[0].actual, 0.25);
    assert.equal(audit.pairs[0].required, 0.5);
    assert.equal(audit.pairs[0].ok, false);

    const coverage = await board.testpointCoverage();
    assert.equal(coverage.report, 'open-pcb-testpoint-coverage');
    assert.equal(coverage.netCount, 3);
    assert.equal(coverage.coveredCount, 2);
    assert.equal(coverage.uncoveredCount, 1);
    assert.equal(coverage.unconnectedCount, 0);
    assert.equal(coverage.probeCount, 2);
    assert.equal(coverage.coveragePercent, 66.666667);
    assert.equal(coverage.status, 'warn');
    assert.deepEqual(coverage.issues.map((issue) => issue.code), ['testpoints/net-not-covered']);
    assert.equal(coverage.nets[1].name, 'N2');
    assert.equal(coverage.nets[1].covered, false);
    assert.equal(coverage.probes[1].designator, 'U1');

    await board.close();
    await pcb.dispose();
});

test('panelization reports the grid, seams, tabs and utilization', { skip: withAddon }, async () => {
    const pcb = new OpenPCB();
    const board = await pcb.create();
    await board.setOutline({ from: [0, 0], to: [100, 80] });

    const panel = await board.panelize({ columns: 2, rows: 2, gapX: 5, gapY: 5, rail: 2, tab: 'mouse-bite' });
    assert.equal(panel.boardCount, 4);
    assert.deepEqual(panel.panelSize, { width: 209, height: 169 });
    assert.deepEqual(panel.panelOutline, { from: [-2, -2], to: [207, 167] });
    assert.equal(panel.tab, 'mouse-bite');
    assert.equal(panel.placements.length, 4);
    assert.equal(panel.seams.length, 2);
    assert.equal(panel.tabs.length, panel.tabCount);
    assert.ok(panel.tabCount > 0);
    assert.ok(panel.utilizationPercent > 90 && panel.utilizationPercent < 91);
    assert.equal(panel.breakaway.area, 3321);
    assert.equal(panel.breakaway.railArea, 1496);
    assert.equal(panel.breakaway.gapArea, 1825);
    assert.ok(Math.abs(panel.breakaway.percent - 9.4023) < 0.001, `breakaway percent: ${panel.breakaway.percent}`);
    assert.equal(panel.toolingHoleCount, 0);
    assert.deepEqual(panel.toolingHoles, []);

    const defaults = await board.panelize({ columns: 1, rows: 1 });
    assert.equal(defaults.gapX, 0);
    assert.equal(defaults.gapY, 0);
    assert.equal(defaults.rail, 0);
    assert.equal(defaults.tab, 'v-cut');
    assert.deepEqual(defaults.tabs, []);
    assert.equal(defaults.utilizationPercent, 100);
    assert.deepEqual(defaults.breakaway, { area: 0, percent: 0, railArea: 0, gapArea: 0 });

    const tooling = await board.panelize({
        columns: 2,
        rows: 2,
        rail: 3,
        toolingHole: 2,
        toolingHoleInset: 1.5,
    });
    assert.equal(tooling.toolingHoleCount, 4);
    assert.deepEqual(
        tooling.toolingHoles.map((hole) => [hole.corner, hole.x, hole.y, hole.diameter]),
        [
            ['bottom-left', -1.5, -1.5, 2],
            ['bottom-right', 201.5, -1.5, 2],
            ['top-right', 201.5, 161.5, 2],
            ['top-left', -1.5, 161.5, 2],
        ],
    );
    assert.equal(tooling.breakaway.area, 2196);
    assert.equal(tooling.breakaway.railArea, 2196);
    assert.equal(tooling.breakaway.gapArea, 0);
    assert.ok(Math.abs(tooling.breakaway.percent - 6.4218) < 0.001, `breakaway percent: ${tooling.breakaway.percent}`);
    await assert.rejects(
        board.panelize({ columns: 2, rows: 1, toolingHole: 2 }),
        (error: unknown) => error instanceof OpenPCBError && error.code === 'session/argument-invalid',
    );

    await board.close();
    await pcb.dispose();
});

test('tracks split at a point and slots update in place', { skip: withAddon }, async () => {
    const pcb = new OpenPCB();
    const board = await pcb.create();
    await board.addNet({ name: 'VCC' });
    await board.addTrack({ net: 'VCC', layer: 'F.Cu', width: 0.25, from: [0, 0], to: [10, 0] });

    const split = await board.splitTrack({ id: 'track-1', x: 4, y: 0 });
    assert.equal(split.first.id, 'track-1');
    assert.deepEqual(split.first.to, [4, 0]);
    assert.equal(split.second.id, 'track-2');
    assert.deepEqual(split.second.from, [4, 0]);
    assert.deepEqual(split.second.to, [10, 0]);
    assert.equal(split.second.net, 'VCC');
    assert.equal(split.second.width, 0.25);
    assert.equal(split.canUndo, true);
    assert.equal(split.revision, board.revision);
    assert.equal((await board.list()).counts.tracks, 2);

    await assert.rejects(
        board.splitTrack({ id: 'track-1', x: 0, y: 0 }),
        (error: unknown) => error instanceof OpenPCBError && error.code === 'session/argument-invalid',
    );
    await assert.rejects(
        board.splitTrack({ id: 'track-1', x: 2, y: 1 }),
        (error: unknown) => error instanceof OpenPCBError && error.code === 'session/argument-invalid',
    );
    await assert.rejects(
        board.splitTrack({ id: 'track-404', x: 1, y: 0 }),
        (error: unknown) => error instanceof OpenPCBError && error.code === 'object/not-found',
    );

    const undone = await board.undo();
    assert.equal(undone.board.counts.tracks, 1, 'the split is one undo step');

    await board.addSlot({ net: 'VCC', from: [0, 5], to: [5, 5], width: 0.8 });
    const updated = await board.updateSlot({ id: 'slot-1', to: [6, 5], width: 1 });
    assert.equal(updated.object?.id, 'slot-1');
    assert.equal(updated.object?.width, 1);
    assert.deepEqual(updated.object?.from, [0, 5]);
    assert.deepEqual(updated.object?.to, [6, 5]);
    await assert.rejects(
        board.updateSlot({ id: 'slot-1', to: [0, 5] }),
        (error: unknown) => error instanceof OpenPCBError && error.code === 'session/argument-invalid',
    );
    await assert.rejects(
        board.updateSlot({ id: 'slot-404', width: 1 }),
        (error: unknown) => error instanceof OpenPCBError && error.code === 'object/not-found',
    );

    await board.close();
    await pcb.dispose();
});

test('editing rejects invalid input with stable codes', { skip: withAddon }, async () => {
    const pcb = new OpenPCB();
    const board = await pcb.create();
    await board.addNet({ name: 'VCC' });
    await assert.rejects(
        board.addNet({ name: 'VCC' }),
        (error: unknown) => error instanceof OpenPCBError && error.code === 'session/argument-invalid',
    );
    await assert.rejects(
        board.addTrack({ layer: 'Missing.Cu', width: 0.25, from: [0, 0], to: [1, 0] }),
        (error: unknown) => error instanceof OpenPCBError && error.code === 'session/argument-invalid',
    );
    await assert.rejects(
        board.deleteFootprint('fp-404'),
        (error: unknown) => error instanceof OpenPCBError && error.code === 'object/not-found',
    );
    await assert.rejects(
        board.addVia({ at: [0, 0], diameter: 0.6, drill: 0 }),
        (error: unknown) => error instanceof OpenPCBError && error.code === 'session/argument-invalid',
    );
    await board.close();
    await assert.rejects(
        board.inspect(),
        (error: unknown) => error instanceof OpenPCBError && error.code === 'board/not-found',
    );
    await pcb.dispose();
});

test('undo, redo, layer locks, footprint moves, deletion and outline', { skip: withAddon }, async () => {
    const pcb = new OpenPCB();
    const board = await pcb.create();
    await board.addNet({ name: 'VCC' });
    assert.equal(board.canUndo, true);

    const undone = await board.undo();
    assert.equal(undone.board.counts.nets, 0);
    assert.equal(undone.board.canRedo, true);
    assert.equal(board.canUndo, false);
    const redone = await board.redo();
    assert.equal(redone.board.counts.nets, 1);
    assert.equal(redone.board.canUndo, true);

    const fresh = await pcb.create();
    await assert.rejects(
        fresh.undo(),
        (error: unknown) => error instanceof OpenPCBError && error.code === 'board/history-empty',
    );

    const added = await board.addLayer({ name: 'Inner1', kind: 'copper' });
    assert.equal(added.object?.id, 'Inner1');
    await assert.rejects(
        board.addLayer({ name: 'Inner1' }),
        (error: unknown) => error instanceof OpenPCBError && error.code === 'session/argument-invalid',
    );
    await board.addTrack({ layer: 'Inner1', width: 0.25, from: [1, 1], to: [2, 2] });
    const renamed = await board.updateLayer({ id: 'Inner1', name: 'Inner2' });
    assert.ok(renamed.board.layers.some((layer) => layer.id === 'Inner2'));
    await board.updateLayer({ id: 'Inner2', locked: true });
    await assert.rejects(
        board.addTrack({ layer: 'Inner2', width: 0.25, from: [0, 0], to: [1, 0] }),
        (error: unknown) => error instanceof OpenPCBError && error.code === 'board/layer-locked',
    );
    await board.updateLayer({ id: 'Inner2', locked: false, visible: false });
    assert.equal(board.summary.layers.find((layer) => layer.id === 'Inner2')?.visible, false);

    await board.addFootprint({ reference: 'U1', value: 'MCU', x: 10, y: 10 });
    const moved = await board.moveFootprint({ id: 'fp-1', x: 20, y: 30, rotation: 45 });
    assert.equal(moved.object?.x, 20);
    assert.equal(moved.object?.rotation, 45);
    const updated = await board.updateFootprint({ id: 'fp-1', value: 'SAMD21', footprint: 'TQFP32' });
    assert.equal(updated.object?.value, 'SAMD21');

    await board.setOutline({ from: [0, 0], to: [50, 40] });
    assert.equal(board.summary.outline?.to[0], 50);
    await assert.rejects(
        board.setOutline({ from: [0, 0], to: [0, 40] }),
        (error: unknown) => error instanceof OpenPCBError && error.code === 'session/argument-invalid',
    );

    await board.addVia({ at: [5, 5], diameter: 0.6, drill: 0.3 });
    assert.equal((await board.deleteTrack('track-1')).deleted, 'track-1');
    assert.equal((await board.deleteVia('via-1')).deleted, 'via-1');
    await assert.rejects(
        board.deleteTrack('track-404'),
        (error: unknown) => error instanceof OpenPCBError && error.code === 'object/not-found',
    );

    await board.close();
    await fresh.close();
    await pcb.dispose();
});

test('sidecar history persists across sessions and is cleared on a new target', { skip: withAddon }, async () => {
    const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'open-pcb-sidecar-'));
    try {
        const file = path.join(directory, 'blinker.opcb');
        const first = new OpenPCB({ history: 'sidecar' });
        const board = await first.create({ title: 'Blinker' });
        await board.save({ target: file });
        await board.addNet({ name: 'VCC' });

        const sidecar = JSON.parse(fs.readFileSync(`${file}.history.json`, 'utf8')) as {
            version: number;
            undo: string[];
            redo: string[];
        };
        assert.equal(sidecar.version, 1);
        assert.equal(sidecar.undo.length, 1, 'the pre-edit document is snapshotted');
        assert.equal(sidecar.redo.length, 0);
        await board.close();
        await first.dispose();

        const second = new OpenPCB({ history: 'sidecar' });
        const reopened = await second.open({ path: file });
        assert.equal(reopened.summary.counts.nets, 1);
        assert.equal(reopened.canUndo, true);

        const undone = await reopened.undo();
        assert.equal(undone.board.counts.nets, 0);
        assert.equal(undone.revision, 3, 'undo bumps the restored revision');
        assert.equal(undone.board.canUndo, false);
        assert.equal(undone.board.canRedo, true);
        const redone = await reopened.redo();
        assert.equal(redone.board.counts.nets, 1);
        assert.equal(redone.revision, 4);

        const copy = path.join(directory, 'copy.opcb');
        fs.writeFileSync(`${copy}.history.json`, '{"version":1,"undo":["stale"],"redo":[]}\n');
        await reopened.save({ target: copy });
        assert.equal(fs.existsSync(`${copy}.history.json`), false, 'a new target drops its stale sidecar');

        await reopened.close();
        await second.dispose();
    } finally {
        fs.rmSync(directory, { recursive: true, force: true });
    }
});

test('CLI creates, edits, inspects and exports in place', { skip: withAddon }, () => {
    const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'open-pcb-cli-'));
    const cli = fileURLToPath(new URL('../src/cli.js', import.meta.url));
    const document = path.join(directory, 'blinker.opcb');
    const run = (...args: string[]): string => {
        const result = spawnSync(process.execPath, [cli, ...args], { encoding: 'utf8' });
        assert.equal(result.status, 0, result.stderr || result.stdout);
        return result.stdout;
    };
    const runFail = (...args: string[]): string => {
        const result = spawnSync(process.execPath, [cli, ...args], { encoding: 'utf8' });
        assert.notEqual(result.status, 0, 'the command is expected to fail');
        return `${result.stdout}${result.stderr}`;
    };
    const footprintX = (): number => {
        const document = JSON.parse(fs.readFileSync(documentPath, 'utf8')) as {
            document: { footprints: { x: number }[] };
        };
        return document.document.footprints[0].x;
    };
    const documentPath = document;
    try {
        run('create', '--out', document, '--title', 'Blinker', '--template', 'blank');
        run('net', 'add', document, '--name', 'VCC');
        run(
            'footprint',
            'add',
            document,
            '--ref',
            'U1',
            '--value',
            'MCU',
            '--footprint',
            'QFN32',
            '--x',
            '30',
            '--y',
            '20',
            '--rotation',
            '0',
            '--layer',
            'F.Cu',
        );
        run('track', 'add', document, '--net', 'VCC', '--layer', 'F.Cu', '--from', '10,10', '--to', '30,10', '--width', '0.25');
        run('via', 'add', document, '--net', 'VCC', '--at', '20,20', '--diameter', '0.6', '--drill', '0.3');
        assert.match(run('lengths', document), /nets=1 total=20 VCC=20 longest=VCC\(20\)/);
        run('zone', 'add', document, '--layer', 'F.Cu', '--points', '0,0 10,0 10,10 0,10');
        run('zone', 'update', document, '--id', 'zone-1', '--points', '0,0 20,0 20,10 0,10');
        assert.match(run('zone', 'list', document), /zones=1 total=200mm2 F\.Cu=200mm2/);
        run('text', 'add', document, '--text', 'REV A', '--layer', 'F.SilkS', '--at', '5,75', '--size', '2');
        run('text', 'update', document, '--id', 'text-1', '--text', 'REV B');
        assert.match(run('text', 'list', document), /texts=1/);
        run('stackup', 'set', document, '--copper-thickness', '0.035', '--dielectric-height', '0.2', '--dielectric-constant', '4.2');
        assert.match(
            run('stackup', 'impedance', document, '--structure', 'diffMicrostrip', '--width', '0.15', '--gap', '0.2'),
            /diffMicrostrip width=0\.15mm impedance=[\d.]+ohm gap=0\.2mm odd=[\d.]+ohm even=[\d.]+ohm/,
        );
        assert.match(
            run('stackup', 'width', document, '--structure', 'diffStripline', '--impedance', '90', '--gap', '0.2'),
            /diffStripline impedance=90ohm width=[\d.]+mm gap=0\.2mm/,
        );
        const table = JSON.parse(
            run('stackup', 'table', document, '--targets', '40,50', '--structure', 'microstrip'),
        ) as { targets: number; rows: { targetImpedance: number; width: number; impedance: number }[] };
        assert.equal(table.targets, 2);
        assert.deepEqual(
            table.rows.map((row) => row.targetImpedance),
            [40, 50],
        );
        for (const row of table.rows) {
            assert.ok(row.width > 0, 'each row suggests a positive width');
            assert.ok(Math.abs(row.impedance - row.targetImpedance) < 0.02, `row reaches its target: ${row.impedance}`);
        }
        assert.deepEqual(
            JSON.parse(run('stackup', 'table', document, '--targets', '40,50', '--structure', 'microstrip')),
            table,
            'the table is deterministic',
        );
        const impedanceReport = JSON.parse(run('stackup', 'report', document)) as {
            units: string;
            structure: string;
            netCount: number;
            pairCount: number;
            nets: {
                net: string;
                tracks: number;
                widthMin: number;
                widthMax: number;
                impedanceMin: number;
                impedanceMax: number;
                structures: string[];
            }[];
        };
        assert.equal(impedanceReport.units, 'mm');
        assert.equal(impedanceReport.structure, 'auto');
        assert.equal(impedanceReport.netCount, 1);
        assert.equal(impedanceReport.pairCount, 0);
        assert.equal(impedanceReport.nets[0]!.net, 'VCC');
        assert.equal(impedanceReport.nets[0]!.tracks, 1);
        assert.equal(impedanceReport.nets[0]!.widthMin, 0.25);
        assert.equal(impedanceReport.nets[0]!.widthMax, 0.25);
        assert.deepEqual(impedanceReport.nets[0]!.structures, ['microstrip']);
        assert.ok(
            impedanceReport.nets[0]!.impedanceMin <= impedanceReport.nets[0]!.impedanceMax,
            'the impedance range is ordered',
        );
        assert.equal(
            run('stackup', 'report', document, '--structure', 'microstrip'),
            run('stackup', 'report', document, '--structure', 'microstrip'),
            'the report is deterministic',
        );
        assert.match(runFail('stackup', 'report', document, '--structure', 'twinaxial'), /--structure/);
        assert.match(runFail('stackup', 'table', document, '--targets', 'fast'), /--targets/);
        assert.match(runFail('stackup', 'table', document, '--gap', 'wide'), /--gap/);
        assert.match(
            runFail('stackup', 'impedance', document, '--structure', 'diffMicrostrip', '--width', '0.15'),
            /--gap/,
        );
        assert.match(run('drill', 'report', document), /holes=1 slots=0 tools=1 backDrilled=0/);

        run('footprint', 'move', document, '--id', 'fp-1', '--x', '40', '--y', '25', '--rotation', '90');
        assert.equal(footprintX(), 40);
        run('undo', document);
        assert.equal(footprintX(), 30);
        run('redo', document);
        assert.equal(footprintX(), 40);

        run('layer', 'add', document, '--name', 'Inner1', '--kind', 'copper');
        run('track', 'add', document, '--layer', 'Inner1', '--from', '1,1', '--to', '2,2', '--width', '0.25');
        run('layer', 'rename', document, '--id', 'Inner1', '--name', 'Inner2');
        run('layer', 'visible', document, '--id', 'Inner2', '--visible', 'false');
        run('layer', 'lock', document, '--id', 'Inner2', '--locked', 'true');
        run('layer', 'lock', document, '--id', 'Inner2', '--locked', 'false');
        run('outline', 'set', document, '--from', '0,0', '--to', '100,80');
        assert.match(
            run('panel', document, '--columns', '2', '--rows', '2', '--gap-x', '5', '--gap-y', '5', '--rail', '2', '--tab', 'mouse-bite'),
            /panel 2x2 boards=4 size=209x169mm tab=mouse-bite tabs=18 utilization=[\d.]+%/,
        );
        assert.match(runFail('panel', document, '--columns', '2', '--rows', '2', '--tab', 'castellated'), /--tab/);
        assert.match(
            run('panel', document, '--columns', '2', '--rows', '2', '--rail', '2', '--tooling-hole', '2'),
            /size=204x164mm .*breakaway=1456mm2 tooling=4/,
        );
        assert.match(runFail('panel', document, '--columns', '2', '--rows', '2', '--tooling-hole', '0'), /--tooling-hole/);
        assert.match(runFail('panel', document, '--columns', '2', '--rows', '2', '--tooling-hole', '2'), /rail/);
        assert.match(run('rules', 'show', document), /solderMaskExpansion=0.05/);
        run('rules', 'set', document, '--hole-clearance', '0.3', '--solder-mask-expansion', '0.1');
        assert.match(run('rules', 'show', document), /holeClearance=0.3 .*solderMaskExpansion=0.1/);
        run('severity', 'set', document, '--code', 'drc/footprint-no-pads', '--severity', 'info');
        run('severity', 'delete', document, '--code', 'drc/footprint-no-pads');
        run('waiver', 'add', document, '--code', 'drc/footprint-no-pads', '--object', 'fp-1', '--reason', 'mechanical');
        run('waiver', 'delete', document, '--id', 'waiver-1');

        const listed = run('list', document);
        assert.match(listed, /nets=1 footprints=1 tracks=2 vias=1/);
        const inspected = run(
            'inspect',
            document,
            '--checks',
            'drc,connectivity,clearance,annular_rings,stackup,outline,silkscreen,courtyard,netlist,hole-to-pad',
        );
        assert.match(inspected, /passed=true/);

        assert.match(run('rules', 'show', document), /stencilThickness=0\.12 stencilAreaRatio=0\.66 stencilAspectRatio=1\.5/);
        run('footprint', 'pad', 'add', document, '--footprint', 'fp-1', '--number', '1', '--x', '0', '--y', '0', '--w', '0.3', '--h', '0.3');
        const stencil = run('inspect', document, '--checks', 'stencil');
        assert.match(stencil, /drc\/stencil-area-ratio/);
        assert.doesNotMatch(stencil, /solder-mask/);
        assert.equal(stencil, run('inspect', document, '--checks', 'stencil'), 'the check output is deterministic');
        run('rules', 'set', document, '--stencil-thickness', '0.1', '--stencil-area-ratio', '0.5');
        assert.match(run('rules', 'show', document), /stencilThickness=0\.1 stencilAreaRatio=0\.5/);
        assert.match(run('inspect', document, '--checks', 'stencil'), /passed=true/);

        for (const format of ['drl', 'drill_map', 'dxf', 'bom', 'pos', 'placement', 'svg'] as const) {
            run('export', document, '--format', format, '--out', path.join(directory, `blinker.${format}`));
        }
        assert.match(fs.readFileSync(path.join(directory, 'blinker.drl'), 'utf8'), /M48/);
        assert.match(fs.readFileSync(path.join(directory, 'blinker.drill_map'), 'utf8'), /OpenPCB drill map/);
        assert.match(fs.readFileSync(path.join(directory, 'blinker.dxf'), 'utf8'), /AC1009/);
        assert.match(fs.readFileSync(path.join(directory, 'blinker.bom'), 'utf8'), /^Quantity,/);
        assert.match(fs.readFileSync(path.join(directory, 'blinker.pos'), 'utf8'), /^Designator,/);
        assert.match(fs.readFileSync(path.join(directory, 'blinker.placement'), 'utf8'), /"placements"/);
        assert.match(fs.readFileSync(path.join(directory, 'blinker.svg'), 'utf8'), /<svg/);

        const gerber = path.join(directory, 'gerber');
        run('export', document, '--format', 'gbr', '--out', gerber);
        for (const name of ['F_Cu.gbr', 'B_Cu.gbr', 'F_SilkS.gbr', 'B_SilkS.gbr', 'F_Mask.gbr', 'B_Mask.gbr', 'F_Paste.gbr', 'B_Paste.gbr', 'Edge_Cuts.gbr']) {
            const content = fs.readFileSync(path.join(gerber, name), 'utf8');
            assert.match(content, /%FSLAX46Y46\*%/, `${name} declares the coordinate format`);
            assert.match(content, /M02\*/, `${name} ends with M02`);
        }
        run('export', document, '--format', 'layers', '--out', path.join(directory, 'layers'));
        assert.ok(fs.existsSync(path.join(directory, 'layers', 'F_Cu.svg')), 'the layer view directory is written');
        run('export', document, '--format', 'asm', '--out', path.join(directory, 'asm'));
        assert.ok(fs.existsSync(path.join(directory, 'asm', 'B_Asm.svg')), 'the assembly directory is written');
        const failed = runFail('export', document, '--format', 'step', '--out', path.join(directory, 'blinker.step'));
        assert.match(failed, /capability\/not-implemented/);
        assert.equal(fs.existsSync(path.join(directory, 'blinker.step')), false);

        const kicad = path.join(directory, 'blinker.kicad_pcb');
        run('export', document, '--format', 'kicad_pcb', '--out', kicad);
        const imported = path.join(directory, 'imported.opcb');
        const result = run('import', kicad, '--out', imported);
        assert.match(result, /imported kicad_pcb .*nets=1 footprints=1 tracks=2 vias=1/);
        assert.match(run('list', imported), /nets=1 footprints=1 tracks=2 vias=1/);

        const ipc = path.join(directory, 'blinker.ipc2581');
        run('export', document, '--format', 'ipc2581', '--out', ipc);
        const ipcImported = path.join(directory, 'imported-ipc.opcb');
        assert.match(run('import', ipc, '--out', ipcImported), /imported ipc2581 .*losses=0/);
        assert.match(run('list', ipcImported), /nets=1 footprints=1 tracks=2 vias=1/);

        const dxfImported = path.join(directory, 'imported-dxf.opcb');
        assert.match(run('import', path.join(directory, 'blinker.dxf'), '--out', dxfImported), /imported dxf .*losses=0/);
        assert.match(run('list', dxfImported), /footprints=0/);

        run('footprint', 'delete', document, '--id', 'fp-1');
        assert.match(run('list', document), /footprints=0/);
    } finally {
        fs.rmSync(directory, { recursive: true, force: true });
    }
});

test('CLI splits tracks and updates slots in place', { skip: withAddon }, () => {
    const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'open-pcb-split-cli-'));
    const cli = fileURLToPath(new URL('../src/cli.js', import.meta.url));
    const document = path.join(directory, 'split.opcb');
    const run = (...args: string[]): string => {
        const result = spawnSync(process.execPath, [cli, ...args], { encoding: 'utf8' });
        assert.equal(result.status, 0, result.stderr || result.stdout);
        return result.stdout;
    };
    const runFail = (...args: string[]): string => {
        const result = spawnSync(process.execPath, [cli, ...args], { encoding: 'utf8' });
        assert.notEqual(result.status, 0, 'the command is expected to fail');
        return `${result.stdout}${result.stderr}`;
    };
    try {
        run('create', '--out', document, '--title', 'Split', '--template', 'blank');
        run('net', 'add', document, '--name', 'VCC');
        run('track', 'add', document, '--net', 'VCC', '--layer', 'F.Cu', '--from', '0,0', '--to', '10,0', '--width', '0.25');
        assert.match(run('track', 'split', document, '--id', 'track-1', '--x', '4', '--y', '0'), /split track-1 into track-1 and track-2/);
        assert.match(run('list', document), /tracks=2/);

        run('slot', 'add', document, '--net', 'VCC', '--from', '0,5', '--to', '5,5', '--width', '0.8');
        assert.match(run('slot', 'update', document, '--id', 'slot-1', '--width', '1.2', '--to', '6,5'), /updated slot-1/);
        assert.match(run('drill', 'report', document), /holes=0 slots=1 tools=1/);

        assert.match(runFail('track', 'split', document, '--id', 'track-1', '--x', '0', '--y', '0'), /must lie between the track endpoints/);
        assert.match(runFail('slot', 'update', document, '--id', 'slot-1', '--to', '0,5'), /needs two distinct endpoints/);
    } finally {
        fs.rmSync(directory, { recursive: true, force: true });
    }
});
