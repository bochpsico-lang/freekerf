// Raster previews for the canvas: the PNG of `document.raster_preview` (grey
// levels, or black and white when dithered) as the single-channel pixels of a
// `RasterImage` (0 = burn, 255 = nothing).

/** A decoded grey image: one byte per pixel, rows top to bottom. */
export interface GreyImage {
  width: number;
  height: number;
  pixels: Uint8Array;
}

/** Decodes a PNG given as base64 (in the browser: `createImageBitmap` and a 2D canvas). */
export async function decodeGreyPng(base64: string): Promise<GreyImage> {
  const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
  const bitmap = await createImageBitmap(new Blob([bytes], { type: 'image/png' }));
  try {
    const { width, height } = bitmap;
    const canvas = document.createElement('canvas');
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext('2d');
    if (!ctx) throw new Error('no 2D canvas');
    ctx.drawImage(bitmap, 0, 0);
    const rgba = ctx.getImageData(0, 0, width, height).data;
    return { width, height, pixels: greyOf(rgba) };
  } finally {
    bitmap.close();
  }
}

/** The grey level of RGBA pixels; transparent counts as white (nothing burnt). */
export function greyOf(rgba: Uint8ClampedArray): Uint8Array {
  const out = new Uint8Array(rgba.length / 4);
  for (let i = 0; i < out.length; i++) {
    const a = rgba[i * 4 + 3]! / 255;
    const grey = 0.299 * rgba[i * 4]! + 0.587 * rgba[i * 4 + 1]! + 0.114 * rgba[i * 4 + 2]!;
    out[i] = Math.round(grey * a + 255 * (1 - a));
  }
  return out;
}
