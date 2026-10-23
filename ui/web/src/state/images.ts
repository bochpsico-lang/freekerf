// The image objects of the Design mode as they will be engraved: the bitmap of
// `document.raster_preview` on the object's bounds, for the canvas. A preview is
// read again only when the image's size or raster settings change, and at most
// {@link MAX_PREVIEW_PX} on its longer side (the job is made at full resolution).

import type { GreyImage } from '../canvas/raster';
import type { RasterImage } from '../canvas/webgl2';
import type { RasterSettings } from '../gen/api';
import type { AppService } from '../service';
import type { DocumentSummary } from './document.svelte';

/** Longest side of a canvas preview, px: lower resolutions are made as they are. */
export const MAX_PREVIEW_PX = 1000;

/** Sizes in the cache key, mm: a move changes `max - min` by a rounding error. */
const KEY_STEP_MM = 1e-3;

/**
 * The settings of a canvas preview of an image whose longer side is `longest_mm`:
 * at most {@link MAX_PREVIEW_PX}. Only grey levels and dithering take their
 * resolution from `lines_per_mm`; the other tools are shown as grey levels at that
 * resolution, which is what their bitmap looks like (pass-through: the grey image,
 * without adjustments; trace outlines: the same threshold; centre lines: black and
 * white).
 */
export function canvasSettings(s: RasterSettings, longest_mm: number): RasterSettings {
  const lines_per_mm = Math.min(s.lines_per_mm, MAX_PREVIEW_PX / longest_mm);
  if (s.tool === 'line_to_line' || s.tool === 'dithering') return { ...s, lines_per_mm };
  const adjustments =
    s.tool === 'passthrough'
      ? { ...s.adjustments, brightness: 100, contrast: 100, threshold: null }
      : s.tool === 'centerline'
        ? { ...s.adjustments, threshold: s.adjustments.threshold ?? 50 }
        : s.adjustments;
  return { ...s, tool: 'line_to_line', lines_per_mm, adjustments };
}

export class ImagePreviews {
  /** The decoded preview of each image object, and the size and settings it was made for. */
  #rasters = new Map<number, { key: string; grey: GreyImage }>();
  /** What the last result showed (ids, keys, places): unchanged images are not uploaded again. */
  #shown = '';
  #run = 0;

  constructor(
    readonly service: AppService,
    readonly decode: (png: string) => Promise<GreyImage>,
  ) {}

  /**
   * The visible image objects of `s` (`layer` = layer index); null when nothing
   * changed or a newer read started. An image without a preview is left out (its
   * outline is in the geometry).
   */
  async read(s: DocumentSummary): Promise<RasterImage[] | null> {
    const run = ++this.#run;
    const rasters = new Map<number, { key: string; grey: GreyImage }>();
    const images: RasterImage[] = [];
    const keys: string[] = [];
    for (const o of s.objects) {
      const layer = s.layers.findIndex((l) => l.id === o.layer);
      if (o.kind !== 'image' || !o.bounds || layer < 0 || s.layers[layer]!.visible === false) continue;
      const [x0, y0, x1, y1] = o.bounds;
      try {
        const settings = await this.service.dispatch('document.raster_settings', { id: o.id });
        const size = [x1 - x0, y1 - y0].map((v) => Math.round(v / KEY_STEP_MM));
        const key = JSON.stringify([size, settings]);
        let r = this.#rasters.get(o.id);
        if (r?.key !== key) {
          const p = await this.service.dispatch('document.raster_preview', {
            id: o.id,
            settings: canvasSettings(settings, Math.max(x1 - x0, y1 - y0)),
          });
          r = { key, grey: await this.decode(p.png) };
        }
        if (run !== this.#run) return null;
        rasters.set(o.id, r);
        const rect = { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
        images.push({ ...r.grey, rect, layer });
        keys.push(JSON.stringify([o.id, r.key, rect, layer]));
      } catch {
        // left out
      }
    }
    if (run !== this.#run) return null;
    this.#rasters = rasters;
    const shown = keys.join();
    if (shown === this.#shown) return null;
    this.#shown = shown;
    return images;
  }
}
