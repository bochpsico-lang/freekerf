import { describe, expect, it, vi } from 'vitest';

import { defaultRaster } from '../dev/fake-document';
import type { ObjectSummary } from '../gen/api';
import type { AppService } from '../service';
import type { DocumentSummary } from './document.svelte';
import { canvasSettings, ImagePreviews, MAX_PREVIEW_PX } from './images';

function summary(bounds: [number, number, number, number]): DocumentSummary {
  const image: ObjectSummary = {
    id: 1,
    kind: 'image',
    layer: 7,
    name: '',
    bounds,
    transform: [1, 0, 0, 1, 0, 0],
  };
  return {
    revision: 1,
    document_revision: 1,
    dirty: false,
    layers: [{ id: 7, visible: true } as DocumentSummary['layers'][number]],
    objects: [image],
    selection: [],
    current_layer: 7,
    can_undo: false,
    can_redo: false,
    clipboard: 0,
  };
}

describe('ImagePreviews', () => {
  it('a move is not a new size, even when max - min rounds differently', async () => {
    const dispatch = vi.fn(async (action: string) =>
      action === 'document.raster_settings' ? defaultRaster() : { png: '', width: 1, height: 1 },
    );
    const service = { dispatch } as unknown as AppService;
    const grey = { width: 1, height: 1, pixels: new Uint8Array([0]) };
    const previews = new ImagePreviews(service, async () => grey);
    const w = (1250 / 96) * 25.4;
    const x = 17.81904;
    expect(x + w - x).not.toBe(w);
    expect(await previews.read(summary([0, 0, w, 10]))).toHaveLength(1);
    const moved = await previews.read(summary([x, 0, x + w, 10]));
    expect(moved![0]!.pixels).toBe(grey.pixels);
    expect(moved![0]!.rect.x).toBe(x);
    expect(dispatch.mock.calls.filter(([a]) => a === 'document.raster_preview')).toHaveLength(1);
    // nothing changed: nothing to show again
    expect(await previews.read(summary([x, 0, x + w, 10]))).toBeNull();
  });

  it('asks for at most MAX_PREVIEW_PX on the longer side', () => {
    const s = { ...defaultRaster(), lines_per_mm: 10 };
    expect(canvasSettings(s, 50).lines_per_mm).toBe(10);
    expect(canvasSettings(s, 400).lines_per_mm).toBe(MAX_PREVIEW_PX / 400);
    expect(canvasSettings({ ...s, tool: 'dithering' }, 400)).toMatchObject({ tool: 'dithering' });
    // the other tools as grey levels at that resolution
    expect(canvasSettings({ ...s, tool: 'passthrough' }, 400)).toMatchObject({
      tool: 'line_to_line',
      lines_per_mm: MAX_PREVIEW_PX / 400,
      adjustments: { brightness: 100, contrast: 100, threshold: null },
    });
    expect(canvasSettings({ ...s, tool: 'centerline' }, 400).adjustments.threshold).toBe(50);
    expect(canvasSettings({ ...s, tool: 'vectorize' }, 400).adjustments).toEqual(s.adjustments);
  });
});
