import { describe, expect, it } from 'vitest';

import { decodeToolpath, SEGMENT_BYTES } from '../canvas/toolpath';
import { parseSvg } from './fake-document';
import { FakeService } from './fake-service';

const snap = (s: FakeService) => s.dispatch('session.snapshot', {});
const box = async (s: FakeService, id: number) =>
  (await snap(s)).document.objects.find((o) => o.id === id)!.bounds!;
const rect = async (s: FakeService, x: number, y: number, w: number, h: number) =>
  (await s.dispatch('document.add_shape', { shape: 'rect', x, y, width: w, height: h })).id;

describe('the fake document (the session actions, in memory)', () => {
  it('adds shapes on the current layer, selects them, and undoes and redoes', async () => {
    const s = new FakeService();
    const changes: string[] = [];
    s.subscribe((e) => {
      if (e.event === 'session') changes.push(e.change);
    });
    const id = await rect(s, 10, 20, 40, 30);
    let d = await snap(s);
    expect(d.document.objects).toHaveLength(1);
    expect(d.document.objects[0]).toMatchObject({
      id,
      layer: d.current_layer,
      kind: 'shape',
      bounds: [10, 20, 50, 50],
    });
    expect(d.selection).toEqual([id]);
    expect(d.undo).toEqual(['add shape']);
    expect(changes).toEqual(['history', 'document', 'selection']);
    const geometry = d.document_revision;
    expect(await s.dispatch('history.undo', {})).toEqual({ step: 'add shape' });
    d = await snap(s);
    expect(d.document.objects).toHaveLength(0);
    expect(d.redo).toEqual(['add shape']);
    expect(d.document_revision).toBeGreaterThan(geometry);
    await s.dispatch('history.redo', {});
    expect((await snap(s)).document.objects).toHaveLength(1);
    expect(await s.dispatch('history.redo', {})).toEqual({ step: null });
  });

  it('transforms keep a per-object affine; align, duplicate, move and delete', async () => {
    const s = new FakeService();
    const a = await rect(s, 0, 0, 10, 10);
    const b = (
      await s.dispatch('document.add_shape', { shape: 'ellipse', x: 50, y: 20, width: 20, height: 20 })
    ).id;
    await s.dispatch('document.transform', { ids: [a], op: { scale: [2, 1], pivot: [0, 0] } });
    expect(await box(s, a)).toEqual([0, 0, 20, 10]);
    await s.dispatch('document.transform', { ids: [a], op: { rotate_deg: 90, pivot: [0, 0] } });
    const r = await box(s, a);
    expect(r[0]).toBeCloseTo(-10);
    expect(r[3]).toBeCloseTo(20);
    expect((await snap(s)).document.objects[0]!.transform[1]).toBeCloseTo(2);
    await s.dispatch('document.align', { ids: [a, b], how: 'left' });
    expect((await box(s, b))[0]).toBeCloseTo(-10);
    const { ids } = await s.dispatch('document.duplicate', { ids: [b] });
    expect((await snap(s)).selection).toEqual(ids);
    expect((await box(s, ids[0]!))[0]).toBeCloseTo((await box(s, b))[0] + 5);
    await s.dispatch('document.translate', { dx: 1, dy: 0, ids: [b] });
    await s.dispatch('document.transform', { ids: [b], op: { matrix: [1, 0, 0, 1, 0, 2] } });
    expect((await box(s, b))[1]).toBeCloseTo(22);
    await s.dispatch('document.delete', { ids: [a] });
    expect((await snap(s)).document.objects.map((o) => o.id)).toEqual([b, ids[0]]);
  });

  it('selection modes and select all', async () => {
    const s = new FakeService();
    const a = await rect(s, 0, 0, 1, 1);
    const b = await rect(s, 5, 0, 1, 1);
    await s.dispatch('document.select', { ids: [a], mode: 'add' });
    expect((await snap(s)).selection.sort()).toEqual([a, b].sort());
    await s.dispatch('document.select', { ids: [a], mode: 'toggle' });
    expect((await snap(s)).selection).toEqual([b]);
    await s.dispatch('document.select', { ids: [b], mode: 'remove' });
    expect((await snap(s)).selection).toEqual([]);
    expect(await s.dispatch('document.select_all', {})).toBe(2);
    await s.dispatch('document.clear_selection', {});
    expect((await snap(s)).selection).toEqual([]);
    // selection is not an undo step
    expect((await snap(s)).undo).toHaveLength(2);
  });

  it('colour = layer: the palette creates layers, moves objects, edits and orders layers', async () => {
    const s = new FakeService();
    const a = await rect(s, 0, 0, 1, 1);
    const { layer, created } = await s.dispatch('document.layer_for_color', { color: '#e8622c' });
    expect(created).toBe(true);
    expect(await s.dispatch('document.layer_for_color', { color: '#E8622C' })).toEqual({
      layer,
      created: false,
    });
    expect((await snap(s)).current_layer).toBe(layer);
    await s.dispatch('document.move_to_layer', { ids: [a], layer });
    await s.dispatch('document.update_layer', { id: layer, speed_mm_min: 300, visible: false });
    await s.dispatch('document.move_layer', { id: layer, to: 0 });
    const d = await snap(s);
    expect(d.document.layers[0]).toMatchObject({
      id: layer,
      color: '#E8622C',
      speed_mm_min: 300,
      visible: false,
      objects: 1,
    });
    expect(d.document.objects[0]!.layer).toBe(layer);
    // hidden layers are not drawn
    expect(decodeToolpath(await s.documentGeometry()).segmentCount).toBe(0);
    await expect(s.dispatch('document.move_to_layer', { ids: [a], layer: 99 })).rejects.toThrow(
      'no layer 99',
    );
    await expect(s.dispatch('document.update_layer', { id: layer, passes: 0 })).rejects.toThrow('passes');
    const { id } = await s.dispatch('document.add_layer', { color: '#000000' });
    await s.dispatch('document.remove_layer', { id });
    expect((await snap(s)).document.layers).toHaveLength(2);
  });

  it('geometry: power = layer index, command = object index', async () => {
    const s = new FakeService();
    await rect(s, 0, 0, 10, 10);
    const { layer } = await s.dispatch('document.layer_for_color', { color: '#10A37F' });
    await s.dispatch('document.set_current_layer', { id: layer });
    await rect(s, 20, 0, 10, 10);
    const t = decodeToolpath(await s.documentGeometry());
    expect(t.segmentCount).toBe(8);
    const last = (8 - 1) * SEGMENT_BYTES;
    expect(t.segments.getFloat32(last + 16, true)).toBe(1);
    expect(t.segments.getUint32(last + 20, true)).toBe(1);
    expect(t.bounds).toMatchObject({ x: 0, y: 0, width: 30, height: 10 });
  });

  it('stand-ins for booleans, offsets and arrays', async () => {
    const s = new FakeService();
    const a = await rect(s, 0, 0, 10, 10);
    const b = await rect(s, 5, 0, 10, 10);
    await expect(s.dispatch('document.boolean', { op: 'union', ids: [a] })).rejects.toThrow('two shapes');
    const { id } = await s.dispatch('document.boolean', { op: 'intersection', ids: [a, b] });
    expect(await box(s, id)).toEqual([5, 0, 10, 10]);
    await s.dispatch('history.undo', {});
    const u = (await s.dispatch('document.boolean', { op: 'union', ids: [a, b] })).id;
    expect(await box(s, u)).toEqual([0, 0, 15, 10]);
    await s.dispatch('history.undo', {});
    expect((await s.dispatch('document.boolean', { op: 'difference', ids: [a, b] })).id).toBeGreaterThan(b);
    await s.dispatch('history.undo', {});
    const [o] = (await s.dispatch('document.offset', { ids: [a], distance_mm: 2 })).ids;
    expect(await box(s, o!)).toEqual([-2, -2, 12, 12]);
    const { ids } = await s.dispatch('document.array', { ids: [a], columns: 2, rows: 2 });
    expect(ids).toHaveLength(3);
    expect(await box(s, ids[2]!)).toEqual([15, -15, 25, -5]);
    expect((await snap(s)).selection).toHaveLength(4);
    await expect(s.dispatch('document.array', { columns: 0, rows: 1 })).rejects.toThrow('at least 1');
  });

  it('text objects and their edits', async () => {
    const s = new FakeService();
    const { id } = await s.dispatch('document.add_text', { text: 'AB', x: 0, y: 0, height_mm: 10 });
    await s.dispatch('document.set_text', { id, text: 'ABCD', height_mm: 20 });
    const o = (await snap(s)).document.objects[0]!;
    expect(o).toMatchObject({ kind: 'text', text: { text: 'ABCD', height_mm: 20 } });
    expect(o.bounds![3]).toBeCloseTo(20);
    const r = await rect(s, 0, 0, 1, 1);
    await expect(s.dispatch('document.set_text', { id: r, text: 'x' })).rejects.toThrow('not text');
  });

  it('imports SVG keeping or unifying the colours', async () => {
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50">
      <rect x="0" y="0" width="10" height="10" stroke="#ff0000"/>
      <circle cx="50" cy="25" r="5" stroke="#0000ff"/>
      <polyline points="0,0 10,10 20,0" stroke="#ff0000"/>
      <line x1="0" y1="0" x2="5" y2="5" stroke="none"/>
    </svg>`;
    expect(parseSvg(svg).map((s) => s.color)).toEqual(['#FF0000', '#0000FF', '#FF0000', '#000000']);
    const keep = new FakeService();
    const r = await keep.dispatch('document.import_data', { format: 'svg', text: svg, name: 'a.svg' });
    expect(r.objects).toHaveLength(4);
    expect(r.layers).toHaveLength(3);
    // the SVG Y axis goes down; the table's goes up
    expect((await snap(keep)).document.objects[0]!.bounds).toEqual([0, 40, 10, 50]);
    const unify = new FakeService();
    const u = await unify.dispatch('document.import_data', {
      format: 'svg',
      data: btoa(svg),
      colors: 'unify',
    });
    expect(u.layers).toEqual([]);
    await expect(unify.dispatch('document.import_data', { format: 'dxf', text: '' })).rejects.toThrow(
      'SVG only',
    );
  });

  it('migration scan and run, and a new document', async () => {
    const s = new FakeService();
    const scan = await s.dispatch('migrate.detect', {});
    expect(scan.found).toBe(true);
    const r = await s.dispatch('migrate.run', { items: ['buttons', 'connection', 'materials'] });
    expect(r.imported).toEqual([
      { kind: 'buttons', count: 14 },
      { kind: 'connection', count: 4 },
    ]);
    expect(r.warnings).toHaveLength(1);
    s.document.migration.found = false;
    expect(await s.dispatch('migrate.detect', {})).toEqual({ found: false, path: null, items: [] });
    await rect(s, 0, 0, 1, 1);
    await s.dispatch('document.new', {});
    const d = await snap(s);
    expect(d.document.objects).toEqual([]);
    expect(d.undo).toEqual([]);
    // a shape on an empty document makes a black layer
    await rect(s, 0, 0, 1, 1);
    expect((await snap(s)).document.layers[0]!.color).toBe('#000000');
  });
});
