import { describe, expect, it, vi } from 'vitest';

import { FakeDocument } from '../dev/fake-document';
import { DocumentProjection } from './document.svelte';

describe('DocumentProjection', () => {
  it('follows the document event and re-reads the geometry only when it changed', async () => {
    const service = new FakeDocument();
    const geometry = vi.spyOn(service, 'geometry');
    const doc = new DocumentProjection(service);
    await doc.refresh();
    expect(doc.objects).toEqual([]);
    expect(doc.paths?.segmentCount).toBe(0);
    await service.dispatch('document.add_shape', { shape: 'rect', x: 0, y: 0, width: 10, height: 10 });
    await vi.waitFor(() => expect(doc.objects).toHaveLength(1));
    await vi.waitFor(() => expect(doc.paths?.segmentCount).toBe(4));
    expect(doc.selected.map((o) => o.id)).toEqual(doc.selection);
    const reads = geometry.mock.calls.length;
    await service.dispatch('document.select', { ids: [], mode: 'replace' });
    await vi.waitFor(() => expect(doc.selection).toEqual([]));
    expect(geometry.mock.calls.length).toBe(reads);
    expect(doc.layerColors[0]).toEqual([0x4c / 255, 0x8d / 255, 1, 1]);
    expect(doc.error).toBe('');
  });

  it('reports a failed read', async () => {
    const service = new FakeDocument();
    vi.spyOn(service, 'geometry').mockRejectedValue(new Error('gone'));
    const doc = new DocumentProjection(service);
    await doc.refresh();
    expect(doc.error).toBe('gone');
    expect(doc.layers).toEqual([]);
  });
});
