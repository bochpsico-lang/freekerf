import { describe, expect, it } from 'vitest';

import { decodeToolpath, SEGMENT_BYTES } from '../canvas/toolpath';
import { FakeDocument, parseSvg } from './fake-document';

const box = async (d: FakeDocument, id: number) =>
  (await d.dispatch('document.get', {})).objects.find((o) => o.id === id)!.bounds!;

describe('FakeDocument (the UI contract, in memory)', () => {
  it('adds shapes on the current layer, selects them, and undoes and redoes', async () => {
    const d = new FakeDocument();
    let events = 0;
    d.subscribe(() => events++);
    const { id } = await d.dispatch('document.add_shape', {
      shape: 'rect',
      x: 10,
      y: 20,
      width: 40,
      height: 30,
    });
    let s = await d.dispatch('document.get', {});
    expect(s.objects).toHaveLength(1);
    expect(s.objects[0]).toMatchObject({
      id,
      layer: s.current_layer,
      kind: 'shape',
      bounds: [10, 20, 50, 50],
    });
    expect(s.selection).toEqual([id]);
    expect(s.can_undo).toBe(true);
    await d.dispatch('document.undo', {});
    s = await d.dispatch('document.get', {});
    expect(s.objects).toHaveLength(0);
    expect(s.can_redo).toBe(true);
    await d.dispatch('document.redo', {});
    expect((await d.dispatch('document.get', {})).objects).toHaveLength(1);
    expect(events).toBe(3);
  });

  it('transforms keep a per-object affine; align and duplicate', async () => {
    const d = new FakeDocument();
    const a = (await d.dispatch('document.add_shape', { shape: 'rect', x: 0, y: 0, width: 10, height: 10 }))
      .id;
    const b = (
      await d.dispatch('document.add_shape', { shape: 'ellipse', x: 50, y: 20, width: 20, height: 20 })
    ).id;
    await d.dispatch('document.transform', { ids: [a], op: { scale: [2, 1], pivot: [0, 0] } });
    expect(await box(d, a)).toEqual([0, 0, 20, 10]);
    await d.dispatch('document.transform', { ids: [a], op: { rotate_deg: 90, pivot: [0, 0] } });
    const r = await box(d, a);
    expect(r[0]).toBeCloseTo(-10);
    expect(r[3]).toBeCloseTo(20);
    expect((await d.dispatch('document.get', {})).objects[0]!.transform[1]).toBeCloseTo(2);
    await d.dispatch('document.align', { ids: [a, b], how: 'left' });
    expect((await box(d, b))[0]).toBeCloseTo(-10);
    const { ids } = await d.dispatch('document.duplicate', { ids: [b] });
    const s = await d.dispatch('document.get', {});
    expect(s.selection).toEqual(ids);
    expect((await box(d, ids[0]!))[0]).toBeCloseTo((await box(d, b))[0] + 5);
    await d.dispatch('document.delete', { ids: [a] });
    expect((await d.dispatch('document.get', {})).objects.map((o) => o.id)).toEqual([b, ids[0]]);
  });

  it('selection modes and select all', async () => {
    const d = new FakeDocument();
    const a = (await d.dispatch('document.add_shape', { shape: 'rect', x: 0, y: 0, width: 1, height: 1 })).id;
    const b = (await d.dispatch('document.add_shape', { shape: 'rect', x: 5, y: 0, width: 1, height: 1 })).id;
    await d.dispatch('document.select', { ids: [a], mode: 'add' });
    expect((await d.dispatch('document.get', {})).selection.sort()).toEqual([a, b].sort());
    await d.dispatch('document.select', { ids: [a], mode: 'toggle' });
    expect((await d.dispatch('document.get', {})).selection).toEqual([b]);
    await d.dispatch('document.select', { ids: [], mode: 'replace' });
    expect((await d.dispatch('document.get', {})).selection).toEqual([]);
    await d.dispatch('document.select_all', {});
    expect((await d.dispatch('document.get', {})).selection).toHaveLength(2);
    // selection is not an undo step
    expect((await d.dispatch('document.get', {})).can_redo).toBe(false);
  });

  it('colour = layer: the palette creates layers, moves objects, edits and orders layers', async () => {
    const d = new FakeDocument();
    const a = (await d.dispatch('document.add_shape', { shape: 'rect', x: 0, y: 0, width: 1, height: 1 })).id;
    const { layer } = await d.dispatch('document.layer_for_color', { color: '#e8622c' });
    expect((await d.dispatch('document.layer_for_color', { color: '#E8622C' })).layer).toBe(layer);
    await d.dispatch('document.set_layer', { ids: [a], layer });
    await d.dispatch('document.layer_update', { layer, speed_mm_min: 300, visible: false });
    await d.dispatch('document.layer_move', { layer, to: 0 });
    const s = await d.dispatch('document.get', {});
    expect(s.layers[0]).toMatchObject({ id: layer, color: '#E8622C', speed_mm_min: 300, visible: false });
    expect(s.objects[0]!.layer).toBe(layer);
    // hidden layers are not drawn
    expect(decodeToolpath(await d.geometry()).segmentCount).toBe(0);
    await expect(d.dispatch('document.set_layer', { ids: [a], layer: 99 })).rejects.toThrow('no layer 99');
  });

  it('geometry: power = layer index, command = object index', async () => {
    const d = new FakeDocument();
    await d.dispatch('document.add_shape', { shape: 'rect', x: 0, y: 0, width: 10, height: 10 });
    const { layer } = await d.dispatch('document.layer_for_color', { color: '#10A37F' });
    await d.dispatch('document.set_current_layer', { layer });
    await d.dispatch('document.add_shape', { shape: 'rect', x: 20, y: 0, width: 10, height: 10 });
    const t = decodeToolpath(await d.geometry());
    expect(t.segmentCount).toBe(8);
    const last = (8 - 1) * SEGMENT_BYTES;
    expect(t.segments.getFloat32(last + 16, true)).toBe(1);
    expect(t.segments.getUint32(last + 20, true)).toBe(1);
    expect(t.bounds).toMatchObject({ x: 0, y: 0, width: 30, height: 10 });
  });

  it('text objects and their edits', async () => {
    const d = new FakeDocument();
    const { id } = await d.dispatch('document.add_text', { text: 'AB', x: 0, y: 0, height_mm: 10 });
    await d.dispatch('document.set_text', { id, text: 'ABCD', height_mm: 20 });
    const o = (await d.dispatch('document.get', {})).objects[0]!;
    expect(o.text).toMatchObject({ text: 'ABCD', height_mm: 20 });
    expect(o.bounds![3]).toBeCloseTo(20);
    const r = (await d.dispatch('document.add_shape', { shape: 'rect', x: 0, y: 0, width: 1, height: 1 })).id;
    await expect(d.dispatch('document.set_text', { id: r, text: 'x' })).rejects.toThrow('not a text');
  });

  it('imports SVG keeping or unifying the colours', async () => {
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50">
      <rect x="0" y="0" width="10" height="10" stroke="#ff0000"/>
      <circle cx="50" cy="25" r="5" stroke="#0000ff"/>
      <polyline points="0,0 10,10 20,0" stroke="#ff0000"/>
      <line x1="0" y1="0" x2="5" y2="5" stroke="none"/>
    </svg>`;
    expect(parseSvg(svg).map((s) => s.color)).toEqual(['#FF0000', '#0000FF', '#FF0000', '#000000']);
    const bytes = new TextEncoder().encode(svg).buffer as ArrayBuffer;
    const keep = new FakeDocument();
    const r = await keep.import('a.svg', bytes, { colors: 'keep' });
    expect(r).toMatchObject({ objects: 4, layers: 3 });
    // the SVG Y axis goes down; the table's goes up
    expect((await keep.dispatch('document.get', {})).objects[0]!.bounds).toEqual([0, 40, 10, 50]);
    const unify = new FakeDocument();
    expect(await unify.import('a.svg', bytes, { colors: 'unify' })).toMatchObject({ objects: 4, layers: 0 });
  });

  it('migration scan and run', async () => {
    const d = new FakeDocument();
    const scan = await d.dispatch('migrate.detect', {});
    expect(scan.found).toBe(true);
    const r = await d.dispatch('migrate.run', { items: ['buttons', 'connection'] });
    expect(r.imported).toEqual([
      { kind: 'buttons', count: 14 },
      { kind: 'connection', count: 1 },
    ]);
    d.migration.found = false;
    expect(await d.dispatch('migrate.detect', {})).toEqual({ found: false, path: null, items: [] });
    await d.dispatch('document.new', {});
  });
});
