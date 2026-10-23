import { describe, expect, it, vi } from 'vitest';

import { FakeService } from '../dev/fake-service';
import { DocumentProjection } from './document.svelte';

describe('DocumentProjection', () => {
  it('follows the session events and re-reads the geometry only when the document changed', async () => {
    const service = new FakeService();
    const geometry = vi.spyOn(service, 'documentGeometry');
    const doc = new DocumentProjection(service);
    await doc.refresh();
    expect(doc.objects).toEqual([]);
    expect(doc.paths?.segmentCount).toBe(0);
    await service.dispatch('document.add_shape', { shape: 'rect', x: 0, y: 0, width: 10, height: 10 });
    await vi.waitFor(() => expect(doc.objects).toHaveLength(1));
    await vi.waitFor(() => expect(doc.paths?.segmentCount).toBe(4));
    expect(doc.selected.map((o) => o.id)).toEqual(doc.selection);
    const reads = geometry.mock.calls.length;
    await service.dispatch('document.clear_selection', {});
    await vi.waitFor(() => expect(doc.selection).toEqual([]));
    expect(geometry.mock.calls.length).toBe(reads);
    expect(doc.summary).toMatchObject({ can_undo: true, can_redo: false, current_layer: doc.layers[0]!.id });
    // view changes do not read the document
    const snapshots = service.calls.filter((c) => c.action === 'session.snapshot').length;
    await service.dispatch('view.set_tool', { tool: 'rect' });
    expect(service.calls.filter((c) => c.action === 'session.snapshot').length).toBe(snapshots);
    expect(doc.layerColors[0]).toEqual([0x4c / 255, 0x8d / 255, 1, 1]);
    expect(doc.error).toBe('');
  });

  it('shows the images as engraved, read again only when their size or settings change', async () => {
    const service = new FakeService();
    const decode = vi.fn(async () => ({ width: 2, height: 1, pixels: new Uint8Array([0, 255]) }));
    const doc = new DocumentProjection(service, decode);
    await doc.refresh();
    const { objects, layers } = await service.dispatch('document.import_data', {
      format: 'image',
      data: '',
      name: 'photo.png',
    });
    const id = objects[0]!;
    await vi.waitFor(() => expect(doc.images).toHaveLength(1));
    expect(doc.images[0]).toMatchObject({
      width: 2,
      height: 1,
      rect: { x: 0, y: 0, width: 40, height: 30 },
      layer: doc.layers.findIndex((l) => l.id === layers[0]),
    });
    const previews = () => service.calls.filter((c) => c.action === 'document.raster_preview').length;
    expect(previews()).toBe(1);
    // moved: the same pixels in another place
    await service.dispatch('document.translate', { ids: [id], dx: 5, dy: 0 });
    await vi.waitFor(() => expect(doc.images[0]!.rect.x).toBe(5));
    expect(previews()).toBe(1);
    // new settings: a new preview
    const settings = await service.dispatch('document.raster_settings', { id });
    await service.dispatch('document.set_raster', { id, settings: { ...settings, lines_per_mm: 5 } });
    await vi.waitFor(() => expect(previews()).toBe(2));
    // a hidden layer hides its image
    await service.dispatch('document.update_layer', { id: layers[0]!, visible: false });
    await vi.waitFor(() => expect(doc.images).toEqual([]));
  });

  it('keeps the outline of an image whose preview fails', async () => {
    const service = new FakeService();
    const doc = new DocumentProjection(service, () => Promise.reject(new Error('no canvas')));
    await doc.refresh();
    await service.dispatch('document.import_data', { format: 'image', data: '', name: 'photo.png' });
    await vi.waitFor(() => expect(doc.objects).toHaveLength(1));
    await vi.waitFor(() => expect(doc.paths?.segmentCount).toBe(4));
    expect(doc.images).toEqual([]);
    expect(doc.error).toBe('');
  });

  it('reports a failed read', async () => {
    const service = new FakeService();
    vi.spyOn(service, 'documentGeometry').mockRejectedValue(new Error('gone'));
    const doc = new DocumentProjection(service);
    await doc.refresh();
    expect(doc.error).toBe('gone');
    expect(doc.layers).toEqual([]);
  });
});
