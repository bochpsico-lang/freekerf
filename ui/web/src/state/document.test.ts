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

  it('reports a failed read', async () => {
    const service = new FakeService();
    vi.spyOn(service, 'documentGeometry').mockRejectedValue(new Error('gone'));
    const doc = new DocumentProjection(service);
    await doc.refresh();
    expect(doc.error).toBe('gone');
    expect(doc.layers).toEqual([]);
  });
});
