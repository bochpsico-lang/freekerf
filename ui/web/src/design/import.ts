// Importing a file into the document: the bytes cross as base64 in
// `document.import_data`, the format from the file name (SVG and DXF by colour
// layer, images on a layer of their own).

import type { ImportColors, ImportFormat, Imported } from '../gen/api';
import type { AppService } from '../service';

/** The import format of a file name, none when it cannot be imported. */
export function importFormat(name: string): ImportFormat | null {
  const ext = name.toLowerCase().split('.').pop() ?? '';
  if (ext === 'svg') return 'svg';
  if (ext === 'dxf') return 'dxf';
  if (['png', 'jpg', 'jpeg', 'bmp', 'gif'].includes(ext)) return 'image';
  return null;
}

/** Base64 of bytes (in chunks: large files overflow `String.fromCharCode(...all)`). */
export function base64(bytes: ArrayBuffer): string {
  const b = new Uint8Array(bytes);
  let s = '';
  for (let i = 0; i < b.length; i += 0x8000) s += String.fromCharCode(...b.subarray(i, i + 0x8000));
  return btoa(s);
}

/** Imports `bytes` as the file `name`: one undo step. */
export async function importFile(
  service: AppService,
  name: string,
  bytes: ArrayBuffer,
  colors: ImportColors,
): Promise<Imported> {
  const format = importFormat(name);
  if (!format) throw new Error(`cannot import ${name}`);
  return service.dispatch('document.import_data', { format, data: base64(bytes), name, colors });
}
